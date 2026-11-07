use super::*;
use std::io::{Read, Seek, SeekFrom};
use std::os::fd::{AsRawFd, FromRawFd};

const ALGORITHM: &str = "blake3";
const SAMPLE_FORMAT: &str = "edge-v1-4096";
const EDGE: u64 = 4096;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComparisonState {
    NotStarted,
    Running,
    Paused,
    Complete,
    FinishedWithErrors,
    Invalidated,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComparisonStage {
    PendingSample,
    Sampling,
    Sampled,
    PendingFull,
    Hashing,
    FullHash,
    UniqueSize,
    UniqueSample,
    Failed,
}
impl ComparisonStage {
    fn parse(s: &str) -> Self {
        match s {
            "pending_sample" => Self::PendingSample,
            "sampling" => Self::Sampling,
            "sampled" => Self::Sampled,
            "pending_full" => Self::PendingFull,
            "hashing" => Self::Hashing,
            "full" => Self::FullHash,
            "unique_size" => Self::UniqueSize,
            "unique_sample" => Self::UniqueSample,
            _ => Self::Failed,
        }
    }
}
#[derive(Debug, Clone)]
pub struct Comparison {
    pub root_id: i64,
    pub relative_path: PathBuf,
    pub generation: i64,
    pub size: u64,
    pub stage: ComparisonStage,
    pub algorithm: String,
    pub sample_format: String,
    pub sample_hash: Option<String>,
    pub full_hash: Option<String>,
    pub bytes_read: u64,
    pub failure: Option<String>,
    pub stale: bool,
}
/// A saved root-level comparison gap, separate from inventory and file evidence.
#[derive(Debug, Clone)]
pub struct ComparisonRootFailure {
    pub root_id: i64,
    pub generation: i64,
    pub message: String,
    pub stale: bool,
}
#[derive(Debug, Clone)]
pub struct ComparisonProgress {
    pub state: ComparisonState,
    pub generation: i64,
    pub files: usize,
    pub pending_files: usize,
    pub full_hashes: usize,
    pub unique_sizes: usize,
    pub unique_samples: usize,
    /// Total current-generation file and root failures.
    pub failures: usize,
    pub root_failures: usize,
    pub bytes_read: u64,
}
pub(super) struct ActiveRead {
    record: Comparison,
    file: fs::File,
    hasher: blake3::Hasher,
    offset: u64,
    full: bool,
}
pub(super) fn create_schema(db: &Connection) -> Result<()> {
    db.execute_batch("CREATE TABLE comparison_job (id INTEGER PRIMARY KEY CHECK(id=1),generation INTEGER,state TEXT,bytes_read INTEGER NOT NULL DEFAULT 0);
    CREATE TABLE comparison_root_failures (root INTEGER,generation INTEGER,message TEXT,PRIMARY KEY(root,generation));
    CREATE TABLE comparisons (root INTEGER,path BLOB,generation INTEGER,size INTEGER,stage TEXT,algorithm TEXT,sample_format TEXT,sample_hash TEXT,full_hash TEXT,bytes_read INTEGER NOT NULL DEFAULT 0,failure TEXT,PRIMARY KEY(root,path,generation));")?;
    Ok(())
}
pub(super) fn migrate(db: &Connection) -> Result<()> {
    let version: i64 = db.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version == 1 {
        db.execute_batch("BEGIN IMMEDIATE; ALTER TABLE entries ADD COLUMN observation TEXT;")?;
        let result =
            create_schema(db).and_then(|()| Ok(db.execute_batch("PRAGMA user_version=2; COMMIT")?));
        if result.is_err() {
            db.execute_batch("ROLLBACK")?;
        }
        result?;
    }
    Ok(())
}
impl Job {
    /// Start/resume a comparison without changing the inventory generation. Old v1
    /// inventories are readable but need a rescan to supply file identity evidence.
    pub fn compare(database: &Path) -> Result<Self> {
        let roots = preflight_roots(database)?;
        let database = safe_database(database, &roots)?;
        let lock = lock_database(&database)?;
        let mut job = Self {
            db: Connection::open(database)?,
            active: None,
            comparison_active: None,
            _lock: Some(lock),
        };
        job.db.execute_batch("PRAGMA synchronous=FULL")?;
        migrate(&job.db)?;
        let p = job.progress()?;
        if !matches!(p.state, State::Complete | State::FinishedWithErrors) {
            return Err(Error::Invalid(
                "finish the saved inventory before comparison".into(),
            ));
        }
        job.db.execute_batch("BEGIN IMMEDIATE")?;
        let result = job.prepare_comparison(p.generation);
        match result {
            Ok(()) => job.db.execute_batch("COMMIT")?,
            Err(e) => {
                job.db.execute_batch("ROLLBACK")?;
                return Err(e);
            }
        }
        Ok(job)
    }
    fn prepare_comparison(&mut self, generation: i64) -> Result<()> {
        self.db.execute("INSERT INTO comparison_job(id,generation,state) VALUES(1,?,'running') ON CONFLICT(id) DO UPDATE SET bytes_read=CASE WHEN generation=excluded.generation THEN bytes_read ELSE 0 END,generation=excluded.generation,state='running'",[generation])?;
        self.db.execute("INSERT OR IGNORE INTO comparisons(root,path,generation,size,stage,algorithm,sample_format) SELECT root,path,?,size,'pending_sample',?,? FROM entries WHERE kind='file'",params![generation,ALGORITHM,SAMPLE_FORMAT])?;
        self.db.execute("UPDATE comparisons SET stage=CASE WHEN stage='sampling' THEN 'pending_sample' ELSE 'pending_full' END,bytes_read=0 WHERE generation=? AND stage IN ('sampling','hashing')",[generation])?;
        self.validate_comparison_roots(generation)?;
        for record in self.comparisons()?.into_iter().filter(|r| !r.stale) {
            if record.stage == ComparisonStage::Failed {
                continue;
            }
            if let Err(e) = self.open_observed(&record) {
                self.comparison_failure(&record, &e.to_string())?;
            }
        }
        self.db.execute("UPDATE comparisons SET stage='unique_size' WHERE generation=? AND stage='pending_sample' AND size IN (SELECT size FROM comparisons WHERE generation=? GROUP BY size HAVING count(*)=1)",params![generation,generation])?;
        Ok(())
    }
    pub fn comparison_progress(&self) -> Result<ComparisonProgress> {
        let generation = self.progress()?.generation;
        let version: i64 = self.db.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        let saved: Option<(i64, String, u64)> = if version == 1 {
            None
        } else {
            self.db
                .query_row(
                    "SELECT generation,state,bytes_read FROM comparison_job",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get::<_, i64>(2)? as u64)),
                )
                .optional()?
        };
        let (state, total) = match saved {
            None => (ComparisonState::NotStarted, 0),
            Some((g, s, n)) => (
                if g != generation {
                    ComparisonState::Invalidated
                } else {
                    match s.as_str() {
                        "running" => ComparisonState::Running,
                        "paused" => ComparisonState::Paused,
                        "complete" => ComparisonState::Complete,
                        "errors" => ComparisonState::FinishedWithErrors,
                        _ => ComparisonState::Invalidated,
                    }
                },
                n,
            ),
        };
        let mut p = ComparisonProgress {
            state,
            generation,
            files: 0,
            pending_files: 0,
            full_hashes: 0,
            unique_sizes: 0,
            unique_samples: 0,
            failures: 0,
            root_failures: 0,
            bytes_read: total,
        };
        for r in self.comparisons()?.into_iter().filter(|r| !r.stale) {
            p.files += 1;
            match r.stage {
                ComparisonStage::FullHash => p.full_hashes += 1,
                ComparisonStage::UniqueSize => p.unique_sizes += 1,
                ComparisonStage::UniqueSample => p.unique_samples += 1,
                ComparisonStage::Failed => p.failures += 1,
                _ => p.pending_files += 1,
            }
        }
        p.root_failures = self
            .comparison_root_failures()?
            .iter()
            .filter(|r| !r.stale)
            .count();
        p.failures += p.root_failures;
        Ok(p)
    }
    /// Saved root gaps only; reading this does not inspect source paths.
    pub fn comparison_root_failures(&self) -> Result<Vec<ComparisonRootFailure>> {
        let version: i64 = self.db.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version == 1 {
            return Ok(vec![]);
        }
        Ok(self.db.prepare("SELECT root,f.generation,message,f.generation!=j.generation FROM comparison_root_failures f CROSS JOIN job j ORDER BY f.generation,root")?.query_map([], |r| Ok(ComparisonRootFailure { root_id: r.get(0)?, generation: r.get(1)?, message: r.get(2)?, stale: r.get(3)? }))?.collect::<std::result::Result<_,_>>()?)
    }
    fn validate_comparison_roots(&self, generation: i64) -> Result<()> {
        for root in self.roots()? {
            let validation = (|| -> Result<()> {
                let directory = self.open_comparison_root(root.id)?;
                let saved: Option<String> = self.db.query_row(
                    "SELECT observation FROM entries WHERE root=? AND path=? AND generation=? AND kind='directory'",
                    params![root.id, bytes(Path::new("")), generation],
                    |r| r.get(0),
                ).optional()?.flatten();
                // Legacy inventories may lack stamps; do not invent observations.
                if let Some(saved) = saved
                    && directory_stamp(&directory.metadata()?) != saved
                {
                    return Err(Error::Invalid(
                        "source root directory changed since inventory; rescan required".into(),
                    ));
                }
                Ok(())
            })();
            if let Err(e) = validation {
                self.db.execute("INSERT OR IGNORE INTO comparison_root_failures(root,generation,message) VALUES(?,?,?)", params![root.id,generation,e.to_string()])?;
            }
        }
        Ok(())
    }
    /// Saved observations only; stale records are retained across inventory generations.
    pub fn comparisons(&self) -> Result<Vec<Comparison>> {
        let version: i64 = self.db.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version == 1 {
            return Ok(vec![]);
        }
        Ok(self.db.prepare("SELECT root,path,c.generation,size,stage,algorithm,sample_format,sample_hash,full_hash,bytes_read,failure,c.generation!=j.generation FROM comparisons c CROSS JOIN job j ORDER BY c.generation,root,path")?.query_map([],|r|Ok(Comparison{root_id:r.get(0)?,relative_path:path(r.get(1)?),generation:r.get(2)?,size:r.get::<_,i64>(3)? as u64,stage:ComparisonStage::parse(&r.get::<_,String>(4)?),algorithm:r.get(5)?,sample_format:r.get(6)?,sample_hash:r.get(7)?,full_hash:r.get(8)?,bytes_read:r.get::<_,i64>(9)? as u64,failure:r.get(10)?,stale:r.get(11)?}))?.collect::<std::result::Result<_,_>>()?)
    }
    fn comparison_writer(&self) -> Result<()> {
        if self._lock.is_none() {
            Err(Error::Invalid(
                "read-only job; use compare to start a comparison writer".into(),
            ))
        } else {
            Ok(())
        }
    }
    pub fn pause_comparison(&mut self) -> Result<ComparisonProgress> {
        self.comparison_writer()?;
        self.db.execute(
            "UPDATE comparison_job SET state='paused' WHERE state='running'",
            [],
        )?;
        self.comparison_active = None;
        self.comparison_progress()
    }
    fn comparison_failure(&self, r: &Comparison, message: &str) -> Result<()> {
        self.db.execute("UPDATE comparisons SET stage='failed',sample_hash=NULL,full_hash=NULL,failure=? WHERE root=? AND path=? AND generation=?",params![message,r.root_id,bytes(&r.relative_path),r.generation])?;
        Ok(())
    }
    fn expected_observation(&self, r: &Comparison) -> Result<String> {
        let (stamp, generation, kind, excluded): (Option<String>, i64, String, bool) =
            self.db.query_row(
                "SELECT observation,generation,kind,excluded FROM entries WHERE root=? AND path=?",
                params![r.root_id, bytes(&r.relative_path)],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )?;
        if generation != r.generation || kind != "file" || excluded {
            return Err(Error::Invalid(
                "stale or excluded inventory observation; rescan required".into(),
            ));
        }
        stamp.ok_or_else(|| {
            Error::Invalid("inventory predates file identity evidence; rescan required".into())
        })
    }
    pub(super) fn check_file(&self, r: &Comparison, file: &fs::File) -> Result<()> {
        let meta = file.metadata()?;
        if !meta.is_file()
            || meta.len() != r.size
            || directory_stamp(&meta) != self.expected_observation(r)?
        {
            return Err(Error::Invalid(
                "source file changed identity, type, size or timestamps since inventory".into(),
            ));
        }
        Ok(())
    }
    fn open_comparison_root(&self, root_id: i64) -> Result<fs::File> {
        let root = self.validate_root(root_id)?;
        // Every component is opened relative to an owned directory descriptor.
        // NOFOLLOW rejects leaf and ancestor links; NONBLOCK prevents FIFO hangs.
        let mut dir = fs::File::open("/")?;
        for component in root.components() {
            if let std::path::Component::Normal(name) = component {
                dir = open_at(&dir, name, true)?;
            }
        }
        let expected_root: String =
            self.db
                .query_row("SELECT identity FROM roots WHERE id=?", [root_id], |row| {
                    row.get(0)
                })?;
        if identity(&dir.metadata()?) != expected_root {
            return Err(Error::Invalid("source root changed identity".into()));
        }
        Ok(dir)
    }
    pub(super) fn open_observed(&self, r: &Comparison) -> Result<fs::File> {
        self.expected_observation(r)?;
        let mut dir = self.open_comparison_root(r.root_id)?;
        let parts: Vec<_> = r.relative_path.components().collect();
        let mut relative = PathBuf::new();
        for (i, part) in parts.iter().enumerate() {
            let std::path::Component::Normal(name) = part else {
                return Err(Error::Invalid("invalid relative source path".into()));
            };
            let last = i + 1 == parts.len();
            let file = open_at(&dir, name, !last)?;
            if last {
                self.check_file(r, &file)?;
                return Ok(file);
            }
            relative.push(name);
            let saved:Option<String>=self.db.query_row("SELECT observation FROM entries WHERE root=? AND path=? AND generation=? AND kind='directory'",params![r.root_id,bytes(&relative),r.generation],|row|row.get(0)).optional()?.flatten();
            let actual = directory_stamp(&file.metadata()?);
            if saved.as_deref() != Some(actual.as_str()) {
                return Err(Error::Invalid(
                    "source ancestor changed since inventory".into(),
                ));
            }
            dir = file;
        }
        Err(Error::Invalid("file placement has empty path".into()))
    }
    /// At most `byte_budget` content bytes are read (capped at 8 MiB per step).
    /// Incomplete hashes are memory-only and restart from zero after reopening.
    pub fn compare_step(&mut self, byte_budget: usize) -> Result<ComparisonProgress> {
        self.comparison_writer()?;
        if byte_budget == 0 {
            return Err(Error::Invalid("byte budget must be positive".into()));
        }
        if self.comparison_progress()?.state != ComparisonState::Running {
            return self.comparison_progress();
        }
        if self.comparison_active.is_none() {
            let next = self.comparisons()?.into_iter().find(|r| {
                !r.stale
                    && matches!(
                        r.stage,
                        ComparisonStage::PendingSample | ComparisonStage::PendingFull
                    )
            });
            if let Some(r) = next {
                match self.open_observed(&r) {
                    Ok(file) => {
                        let full = r.stage == ComparisonStage::PendingFull;
                        let mut hasher = blake3::Hasher::new();
                        if !full {
                            hasher.update(b"folderloom:edge-v1-4096\0");
                            hasher.update(&r.size.to_le_bytes());
                        }
                        self.comparison_active = Some(ActiveRead {
                            record: r,
                            file,
                            hasher,
                            offset: 0,
                            full,
                        });
                    }
                    Err(e) => {
                        self.comparison_failure(&r, &e.to_string())?;
                        return self.comparison_progress();
                    }
                }
            } else {
                let generation = self.progress()?.generation;
                self.db.execute("UPDATE comparisons SET stage=CASE WHEN (SELECT count(*) FROM comparisons b WHERE b.generation=comparisons.generation AND b.size=comparisons.size AND b.sample_hash=comparisons.sample_hash AND b.stage!='failed')>1 THEN 'pending_full' ELSE 'unique_sample' END WHERE generation=? AND stage='sampled'",[generation])?;
                let p = self.comparison_progress()?;
                if p.pending_files == 0 {
                    // Final validation catches roots or completed placements lost while unrelated work ran.
                    self.validate_comparison_roots(generation)?;
                    for r in self
                        .comparisons()?
                        .into_iter()
                        .filter(|r| !r.stale && r.stage != ComparisonStage::Failed)
                    {
                        if let Err(e) = self.open_observed(&r) {
                            self.comparison_failure(&r, &e.to_string())?;
                        }
                    }
                    let p = self.comparison_progress()?;
                    self.db.execute(
                        "UPDATE comparison_job SET state=?",
                        [
                            if p.failures > 0 || self.progress()?.state == State::FinishedWithErrors
                            {
                                "errors"
                            } else {
                                "complete"
                            },
                        ],
                    )?;
                }
                return self.comparison_progress();
            }
        }
        let mut active = self.comparison_active.take().unwrap();
        let result = (|| -> Result<usize> {
            self.check_file(&active.record, &active.file)?;
            self.open_observed(&active.record)?;
            let total = if active.full {
                active.record.size
            } else {
                active.record.size.min(EDGE * 2)
            };
            let mut remaining = byte_budget.min(8 * 1024 * 1024);
            let mut read = 0;
            let mut buffer = [0u8; 64 * 1024];
            while remaining > 0 && active.offset < total {
                let logical = active.offset;
                let (offset, segment) = if active.full || active.record.size <= EDGE * 2 {
                    (logical, total - logical)
                } else if logical < EDGE {
                    (logical, EDGE - logical)
                } else {
                    (active.record.size - EDGE + logical - EDGE, total - logical)
                };
                active.file.seek(SeekFrom::Start(offset))?;
                let n = remaining.min(buffer.len()).min(segment as usize);
                let got = active.file.read(&mut buffer[..n])?;
                if got == 0 {
                    return Err(Error::Invalid(
                        "source ended before observed file size".into(),
                    ));
                }
                active.hasher.update(&buffer[..got]);
                active.offset += got as u64;
                remaining -= got;
                read += got;
            }
            self.check_file(&active.record, &active.file)?;
            self.open_observed(&active.record)?;
            Ok(read)
        })();
        match result {
            Err(e) => self.comparison_failure(&active.record, &e.to_string())?,
            Ok(read) => {
                let total = if active.full {
                    active.record.size
                } else {
                    active.record.size.min(EDGE * 2)
                };
                let done = active.offset == total;
                let digest = done.then(|| active.hasher.finalize().to_hex().to_string());
                let stage = match (active.full, done) {
                    (true, true) => "full",
                    (true, false) => "hashing",
                    (false, true) => "sampled",
                    (false, false) => "sampling",
                };
                self.db.execute_batch("BEGIN IMMEDIATE")?;
                let commit = (|| -> Result<()> {
                    self.db.execute("UPDATE comparisons SET stage=?,bytes_read=?,sample_hash=COALESCE(?,sample_hash),full_hash=? WHERE root=? AND path=? AND generation=?",params![stage,active.offset as i64,if active.full{None}else{digest.as_deref()},if active.full{digest.as_deref()}else{None},active.record.root_id,bytes(&active.record.relative_path),active.record.generation])?;
                    self.db.execute(
                        "UPDATE comparison_job SET bytes_read=bytes_read+?",
                        [read as i64],
                    )?;
                    Ok(())
                })();
                match commit {
                    Ok(()) => self.db.execute_batch("COMMIT")?,
                    Err(e) => {
                        self.db.execute_batch("ROLLBACK")?;
                        return Err(e);
                    }
                }
                if !done {
                    self.comparison_active = Some(active);
                }
            }
        }
        self.comparison_progress()
    }
}
fn open_at(parent: &fs::File, name: &std::ffi::OsStr, directory: bool) -> Result<fs::File> {
    let name = std::ffi::CString::new(name.as_bytes())
        .map_err(|_| Error::Invalid("source path contains NUL".into()))?;
    let flags = libc::O_RDONLY
        | libc::O_CLOEXEC
        | libc::O_NOFOLLOW
        | libc::O_NONBLOCK
        | if directory { libc::O_DIRECTORY } else { 0 };
    // SAFETY: parent stays alive for openat; name is NUL terminated. A successful
    // descriptor is uniquely owned and immediately transferred into File.
    let fd = unsafe { libc::openat(parent.as_raw_fd(), name.as_ptr(), flags) };
    if fd < 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(unsafe { fs::File::from_raw_fd(fd) })
}
