//! Durable inventory and comparison jobs. No source mutation operations are exposed.
mod compare;
pub use compare::{Comparison, ComparisonProgress, ComparisonStage, ComparisonState};
use rusqlite::{Connection, OptionalExtension, params};
use std::os::unix::{
    ffi::{OsStrExt, OsStringExt},
    fs::MetadataExt,
};
use std::{
    fs,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Invalid(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Database(#[from] rusqlite::Error),
}
pub type Result<T> = std::result::Result<T, Error>;
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum EmptyFolders {
    #[default]
    Include,
    Ignore,
}
#[derive(Debug, Default, Clone, Copy)]
pub struct Options {
    pub empty_folders: EmptyFolders,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Running,
    Paused,
    Complete,
    FinishedWithErrors,
}
#[derive(Debug, Clone)]
pub struct Progress {
    pub state: State,
    pub generation: i64,
    pub entries: usize,
    pub stale_entries: usize,
    pub pending_directories: usize,
    pub failed_directories: usize,
    pub failures: usize,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntryType {
    File,
    Directory,
    Symlink,
    Other,
}
#[derive(Debug, Clone)]
pub struct Entry {
    pub root_id: i64,
    pub relative_path: PathBuf,
    pub entry_type: EntryType,
    pub size: u64,
    pub modified_ns: Option<String>,
    pub link_target: Option<PathBuf>,
    pub excluded: bool,
    pub stale: bool,
    pub directory_complete: bool,
}
#[derive(Debug, Clone)]
pub struct Root {
    pub id: i64,
    pub path: PathBuf,
}
#[derive(Debug, Clone)]
pub struct Failure {
    pub root_id: i64,
    pub relative_path: PathBuf,
    pub message: String,
    pub generation: i64,
}
struct Active {
    root: i64,
    relative: PathBuf,
    iter: fs::ReadDir,
    count: usize,
    stamp: String,
}
/// A single writer owns a job. Read-only handles inspect durable checkpoints.
pub struct Job {
    db: Connection,
    active: Option<Active>,
    comparison_active: Option<compare::ActiveRead>,
    _lock: Option<fs::File>,
}
fn bytes(path: &Path) -> &[u8] {
    path.as_os_str().as_bytes()
}
fn path(bytes: Vec<u8>) -> PathBuf {
    std::ffi::OsString::from_vec(bytes).into()
}
fn identity(meta: &fs::Metadata) -> String {
    format!("{}:{}", meta.dev(), meta.ino())
}
fn modified(meta: &fs::Metadata) -> Option<String> {
    meta.modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|t| t.as_nanos().to_string())
}
impl Job {
    pub fn create(database: &Path, roots: &[PathBuf], options: Options) -> Result<Self> {
        if roots.is_empty() {
            return Err(Error::Invalid("select at least one root".into()));
        }
        let roots: Vec<_> = roots
            .iter()
            .map(fs::canonicalize)
            .collect::<std::io::Result<_>>()?;
        if roots.iter().collect::<std::collections::HashSet<_>>().len() != roots.len() {
            return Err(Error::Invalid("duplicate source root selection".into()));
        }
        for root in &roots {
            if !fs::metadata(root)?.is_dir() {
                return Err(Error::Invalid("source must be a directory".into()));
            }
        }
        let database = safe_database(database, &roots)?;
        let lock = lock_database(&database)?;
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&database)?;
        let db = Connection::open(&database)?;
        db.execute_batch("PRAGMA journal_mode=DELETE; PRAGMA synchronous=FULL; BEGIN IMMEDIATE; PRAGMA user_version=2;
          CREATE TABLE job (id INTEGER PRIMARY KEY CHECK(id=1), state TEXT, generation INTEGER, ignore_empty INTEGER);
          CREATE TABLE roots (id INTEGER PRIMARY KEY, path BLOB UNIQUE, identity TEXT);
          CREATE TABLE entries (root INTEGER, path BLOB, kind TEXT, size INTEGER, modified TEXT, observation TEXT, target BLOB, excluded INTEGER DEFAULT 0, generation INTEGER, PRIMARY KEY(root,path));
          CREATE TABLE directories (root INTEGER, path BLOB, state TEXT, stamp TEXT, PRIMARY KEY(root,path));
          CREATE TABLE failures (root INTEGER, path BLOB, message TEXT, generation INTEGER);")?;
        compare::create_schema(&db)?;
        db.execute(
            "INSERT INTO job VALUES (1,'running',1,?)",
            [options.empty_folders == EmptyFolders::Ignore],
        )?;
        let mut job = Self {
            db,
            active: None,
            comparison_active: None,
            _lock: Some(lock),
        };
        for root in roots {
            let meta = fs::metadata(&root)?;
            if !meta.is_dir() {
                return Err(Error::Invalid("source must be a directory".into()));
            }
            job.db.execute(
                "INSERT INTO roots(path,identity) VALUES (?,?)",
                params![bytes(&root), identity(&meta)],
            )?;
            let id = job.db.last_insert_rowid();
            job.observe(id, Path::new(""), &root)?;
        }
        job.db.execute_batch("COMMIT")?;
        Ok(job)
    }
    pub fn open(database: &Path) -> Result<Self> {
        let roots = preflight_roots(database)?;
        let database = safe_database(database, &roots)?;
        Ok(Self {
            db: Connection::open_with_flags(database, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?,
            active: None,
            comparison_active: None,
            _lock: None,
        })
    }
    /// Revalidate every selected root and repeat directory discovery in a new generation.
    /// Prior observations remain visible as stale until observed again.
    pub fn resume(database: &Path) -> Result<Self> {
        let protected = preflight_roots(database)?;
        let database = safe_database(database, &protected)?;
        let lock = lock_database(&database)?;
        let mut job = Self {
            db: Connection::open(&database)?,
            active: None,
            comparison_active: None,
            _lock: Some(lock),
        };
        compare::migrate(&job.db)?;
        let roots = job.roots()?;
        job.db.execute_batch(
            "PRAGMA synchronous=FULL; BEGIN IMMEDIATE;
            UPDATE job SET generation=generation+1,state='running'; DELETE FROM directories; UPDATE comparison_job SET state='invalidated';",
        )?;
        for root in roots {
            if let Err(e) = job.validate_root(root.id) {
                job.record_failure(root.id, Path::new(""), &e.to_string())?;
                job.db.execute(
                    "INSERT INTO directories VALUES (?,?,'failed',NULL)",
                    params![root.id, bytes(Path::new(""))],
                )?;
            } else {
                job.observe(root.id, Path::new(""), &root.path)?;
            }
        }
        job.db.execute_batch("COMMIT")?;
        Ok(job)
    }
    /// Pause after the last committed batch. Resuming starts a fresh validation pass.
    pub fn pause(&mut self) -> Result<Progress> {
        self.db
            .execute("UPDATE job SET state='paused' WHERE state='running'", [])?;
        self.active = None;
        self.progress()
    }
    fn validate_root(&self, root: i64) -> Result<PathBuf> {
        let (base, expected): (Vec<u8>, String) =
            self.db
                .query_row("SELECT path,identity FROM roots WHERE id=?", [root], |r| {
                    Ok((r.get(0)?, r.get(1)?))
                })?;
        let base = path(base);
        let meta = fs::symlink_metadata(&base)?;
        if !meta.is_dir() || identity(&meta) != expected {
            return Err(Error::Invalid("selected source root changed identity or type; create a new job to accept its replacement".into()));
        }
        Ok(base)
    }
    fn directory_path(&self, root: i64, relative: &Path) -> Result<PathBuf> {
        let mut full = self.validate_root(root)?;
        for part in relative.components() {
            full.push(part);
            if !fs::symlink_metadata(&full)?.is_dir() {
                return Err(Error::Invalid(
                    "directory changed type; symlinks are not followed".into(),
                ));
            }
        }
        Ok(full)
    }
    fn record_failure(&self, root: i64, relative: &Path, message: &str) -> Result<()> {
        self.db.execute(
            "INSERT INTO failures VALUES (?,?,?,(SELECT generation FROM job))",
            params![root, bytes(relative), message],
        )?;
        Ok(())
    }
    fn fail_directory(&self, root: i64, relative: &Path, message: &str) -> Result<()> {
        self.record_failure(root, relative, message)?;
        self.db.execute(
            "UPDATE directories SET state='failed' WHERE root=? AND path=?",
            params![root, bytes(relative)],
        )?;
        let mut prefix = bytes(relative).to_vec();
        if !prefix.is_empty() {
            prefix.push(b'/');
        }
        self.db.execute("UPDATE entries SET generation=0,excluded=0 WHERE root=? AND (path=? OR substr(path,1,?)=?)",params![root,bytes(relative),prefix.len() as i64,prefix])?;
        Ok(())
    }
    pub fn roots(&self) -> Result<Vec<Root>> {
        Ok(self
            .db
            .prepare("SELECT id,path FROM roots ORDER BY id")?
            .query_map([], |r| {
                Ok(Root {
                    id: r.get(0)?,
                    path: path(r.get(1)?),
                })
            })?
            .collect::<std::result::Result<_, _>>()?)
    }
    pub fn progress(&self) -> Result<Progress> {
        let (state, generation): (String, i64) =
            self.db
                .query_row("SELECT state,generation FROM job", [], |r| {
                    Ok((r.get(0)?, r.get(1)?))
                })?;
        let count = |sql: &str| -> Result<usize> {
            Ok(self.db.query_row(sql, [], |r| r.get::<_, i64>(0))? as usize)
        };
        Ok(Progress {
            state: match state.as_str() {
                "running" => State::Running,
                "paused" => State::Paused,
                "complete" => State::Complete,
                _ => State::FinishedWithErrors,
            },
            generation,
            entries: count("SELECT count(*) FROM entries")?,
            stale_entries: count(
                "SELECT count(*) FROM entries WHERE generation != (SELECT generation FROM job)",
            )?,
            pending_directories: count("SELECT count(*) FROM directories WHERE state='pending'")?,
            failed_directories: count("SELECT count(*) FROM directories WHERE state='failed'")?,
            failures: count(
                "SELECT count(*) FROM failures WHERE generation=(SELECT generation FROM job)",
            )?,
        })
    }
    pub fn inventory(&self) -> Result<Vec<Entry>> {
        Ok(self.db.prepare("SELECT e.root,e.path,e.kind,e.size,e.modified,e.target,e.excluded,e.generation != j.generation,COALESCE(d.state='complete' AND e.generation=j.generation,0) FROM entries e CROSS JOIN job j LEFT JOIN directories d ON d.root=e.root AND d.path=e.path ORDER BY e.root,e.path")?.query_map([], |r| {
            let kind: String = r.get(2)?;
            Ok(Entry { root_id:r.get(0)?, relative_path:path(r.get(1)?), entry_type:match kind.as_str() { "file"=>EntryType::File,"directory"=>EntryType::Directory,"symlink"=>EntryType::Symlink,_=>EntryType::Other },size:r.get::<_, i64>(3)? as u64,modified_ns:r.get(4)?,link_target:r.get::<_,Option<Vec<u8>>>(5)?.map(path),excluded:r.get(6)?,stale:r.get(7)?,directory_complete:r.get(8)? })
        })?.collect::<std::result::Result<_,_>>()?)
    }
    pub fn failures(&self) -> Result<Vec<Failure>> {
        Ok(self
            .db
            .prepare("SELECT root,path,message,generation FROM failures ORDER BY rowid")?
            .query_map([], |r| {
                Ok(Failure {
                    root_id: r.get(0)?,
                    relative_path: path(r.get(1)?),
                    message: r.get(2)?,
                    generation: r.get(3)?,
                })
            })?
            .collect::<std::result::Result<_, _>>()?)
    }
    fn observe(&mut self, root: i64, relative: &Path, full: &Path) -> Result<()> {
        let meta = fs::symlink_metadata(full)?;
        let kind = if meta.is_symlink() {
            "symlink"
        } else if meta.is_dir() {
            "directory"
        } else if meta.is_file() {
            "file"
        } else {
            "other"
        };
        let target = if meta.is_symlink() {
            Some(fs::read_link(full)?)
        } else {
            None
        };
        self.db.execute("INSERT INTO entries(root,path,kind,size,modified,observation,target,generation) VALUES (?,?,?,?,?,?,?,(SELECT generation FROM job)) ON CONFLICT(root,path) DO UPDATE SET kind=excluded.kind,size=excluded.size,modified=excluded.modified,observation=excluded.observation,target=excluded.target,generation=excluded.generation,excluded=0",params![root,bytes(relative),kind,i64::try_from(meta.len()).map_err(|_| Error::Invalid("file size exceeds supported range".into()))?,modified(&meta),directory_stamp(&meta),target.as_deref().map(bytes)])?;
        if meta.is_dir() {
            self.db.execute(
                "INSERT OR IGNORE INTO directories VALUES (?,?,'pending',?)",
                params![root, bytes(relative), directory_stamp(&meta)],
            )?;
        }
        Ok(())
    }
    /// Commit at most `budget` discovered children. Directory completion requires EOF.
    pub fn step(&mut self, budget: usize) -> Result<Progress> {
        if self._lock.is_none() {
            return Err(Error::Invalid(
                "read-only job; use resume to start a writer".into(),
            ));
        }
        if budget == 0 {
            return Err(Error::Invalid("batch size must be positive".into()));
        }
        let budget = budget.min(4096);
        if self.progress()?.state != State::Running {
            return self.progress();
        }
        if self.active.is_none() {
            let next:Option<(i64,Vec<u8>)> = self.db.query_row("SELECT root,path FROM directories WHERE state='pending' ORDER BY root,length(path),path LIMIT 1",[],|r| Ok((r.get(0)?,r.get(1)?))).optional()?;
            if let Some((root, relative)) = next {
                let relative = path(relative);
                let opened = (|| -> Result<_> {
                    let full = self.directory_path(root, &relative)?;
                    let stamp = directory_stamp(&fs::symlink_metadata(&full)?);
                    let expected: String = self.db.query_row(
                        "SELECT stamp FROM directories WHERE root=? AND path=?",
                        params![root, bytes(&relative)],
                        |r| r.get(0),
                    )?;
                    if stamp != expected {
                        return Err(Error::Invalid("directory changed since discovery".into()));
                    }
                    Ok((fs::read_dir(&full)?, stamp))
                })();
                match opened {
                    Ok((iter, stamp)) => {
                        self.active = Some(Active {
                            root,
                            relative,
                            iter,
                            count: 0,
                            stamp,
                        })
                    }
                    Err(e) => {
                        self.fail_directory(root, &relative, &e.to_string())?;
                        return self.progress();
                    }
                }
            } else {
                for root in self.roots()? {
                    if let Err(e) = self.validate_root(root.id) {
                        self.fail_directory(root.id, Path::new(""), &e.to_string())?;
                    }
                }
                let p = self.progress()?;
                let state = if p.failures > 0 || p.stale_entries > 0 || p.failed_directories > 0 {
                    "errors"
                } else {
                    "complete"
                };
                self.db.execute("UPDATE job SET state=?", [state])?;
                return self.progress();
            }
        }
        self.db.execute_batch("BEGIN IMMEDIATE")?;
        let result = (|| -> Result<()> {
            let mut active = self.active.take().unwrap();
            let validation = self
                .directory_path(active.root, &active.relative)
                .and_then(|p| Ok(directory_stamp(&fs::symlink_metadata(p)?)));
            if validation
                .as_ref()
                .map(|stamp| stamp != &active.stamp)
                .unwrap_or(true)
            {
                self.fail_directory(
                    active.root,
                    &active.relative,
                    "directory changed or became unavailable during enumeration",
                )?;
                return Ok(());
            }
            for _ in 0..budget {
                match active.iter.next() {
                    Some(Ok(child)) => {
                        let relative = active.relative.join(child.file_name());
                        match self.observe(active.root, &relative, &child.path()) {
                            Ok(()) => {}
                            Err(e @ Error::Io(_)) => {
                                self.record_failure(active.root, &relative, &e.to_string())?
                            }
                            Err(e) => return Err(e),
                        }
                        active.count += 1;
                    }
                    Some(Err(e)) => {
                        self.fail_directory(active.root, &active.relative, &e.to_string())?;
                        return Ok(());
                    }
                    None => {
                        let current = self
                            .directory_path(active.root, &active.relative)
                            .and_then(|p| Ok(directory_stamp(&fs::symlink_metadata(p)?)));
                        if current
                            .as_ref()
                            .map(|stamp| stamp != &active.stamp)
                            .unwrap_or(true)
                        {
                            self.fail_directory(
                                active.root,
                                &active.relative,
                                "directory changed during enumeration",
                            )?;
                        } else {
                            self.db.execute(
                                "UPDATE directories SET state='complete' WHERE root=? AND path=?",
                                params![active.root, bytes(&active.relative)],
                            )?;
                            if active.count == 0 {
                                self.db.execute("UPDATE entries SET excluded=(SELECT ignore_empty FROM job) WHERE root=? AND path=?",params![active.root,bytes(&active.relative)])?;
                            }
                        }
                        return Ok(());
                    }
                }
            }
            self.active = Some(active);
            Ok(())
        })();
        match result {
            Ok(()) => self.db.execute_batch("COMMIT")?,
            Err(e) => {
                self.db.execute_batch("ROLLBACK")?;
                self.active = None;
                return Err(e);
            }
        }
        self.progress()
    }
}

// Resolve existing ancestors before creating anything. A missing parent is an error;
// creating directories implicitly would expand the source-write safety boundary.
fn safe_database(database: &Path, roots: &[PathBuf]) -> Result<PathBuf> {
    let database = if database.exists() {
        fs::canonicalize(database)?
    } else {
        let absolute = std::path::absolute(database)?;
        let parent = fs::canonicalize(
            absolute
                .parent()
                .ok_or_else(|| Error::Invalid("database needs a parent".into()))?,
        )?;
        parent.join(
            absolute
                .file_name()
                .ok_or_else(|| Error::Invalid("database needs a filename".into()))?,
        )
    };
    for root in roots {
        if database.starts_with(root) {
            return Err(Error::Invalid(
                "inventory storage must be outside every source root".into(),
            ));
        }
    }
    if let Ok(meta) = fs::metadata(&database)
        && meta.nlink() != 1
    {
        return Err(Error::Invalid(
            "database must not have hard-link aliases".into(),
        ));
    }
    // SQLite journal names must not redirect writes to a different location.
    for suffix in ["-journal", "-wal", "-shm", ".lock"] {
        let mut name = database.as_os_str().to_os_string();
        name.push(suffix);
        match fs::symlink_metadata(Path::new(&name)) {
            Ok(meta) if !meta.is_file() || meta.nlink() != 1 => {
                return Err(Error::Invalid("unsafe database sidecar alias".into()));
            }
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(database)
}

fn directory_stamp(meta: &fs::Metadata) -> String {
    format!(
        "{}:{:?}:{}:{}",
        identity(meta),
        modified(meta),
        meta.ctime(),
        meta.ctime_nsec()
    )
}

// The roots table is committed at creation and never subsequently modified. Read
// only that immutable configuration without journal recovery before checking the
// storage boundary. Ordinary inventory readers then use SQLite locking, not this
// special connection. A hot journal is recovered only by the validated writer.
fn preflight_roots(database: &Path) -> Result<Vec<PathBuf>> {
    let absolute = fs::canonicalize(database)?;
    let encoded: String = bytes(&absolute)
        .iter()
        .map(|b| format!("%{b:02X}"))
        .collect();
    let db = Connection::open_with_flags(
        format!("file:{encoded}?immutable=1"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
    )?;
    let version: i64 = db.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version != 1 && version != 2 {
        return Err(Error::Invalid(format!(
            "unsupported inventory schema version {version}"
        )));
    }
    let mut roots: Vec<PathBuf> = db
        .prepare("SELECT path FROM roots")?
        .query_map([], |r| Ok(path(r.get(0)?)))?
        .collect::<std::result::Result<_, _>>()?;
    if roots.is_empty() {
        return Err(Error::Invalid("incomplete job configuration".into()));
    }
    let aliases: Vec<_> = roots
        .iter()
        .filter_map(|p| fs::canonicalize(p).ok())
        .collect();
    roots.extend(aliases);
    Ok(roots)
}

fn lock_database(database: &Path) -> Result<fs::File> {
    let mut lock_path = database.as_os_str().to_os_string();
    lock_path.push(".lock");
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(lock_path)?;
    fs2::FileExt::try_lock_exclusive(&lock)?;
    Ok(lock)
}
