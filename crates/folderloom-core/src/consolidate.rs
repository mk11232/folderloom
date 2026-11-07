//! Reviewed copy-only plans. Paths are stored as native bytes, never display strings.
use super::*;
use serde::{Deserialize, Serialize};
#[cfg(not(target_os = "macos"))]
use std::os::unix::fs::PermissionsExt;
use std::{
    collections::BTreeMap,
    ffi::{CString, OsStr, OsString},
    io::{Read, Seek, SeekFrom, Write},
    os::fd::{AsRawFd, FromRawFd},
    path::Component,
};
use unicode_normalization::UnicodeNormalization;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mapping {
    pub root_id: i64,
    pub source_relative: PathBuf,
    pub destination_relative: PathBuf,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanDecision {
    pub item_id: i64,
    pub included: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanContributor {
    pub root_id: i64,
    pub source_relative: PathBuf,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanItem {
    pub id: i64,
    pub root_id: i64,
    pub source_relative: PathBuf,
    pub destination_relative: PathBuf,
    pub action: String,
    pub included: bool,
    pub status: String,
    pub detail: Option<String>,
    pub hash: Option<String>,
    pub contributors: Vec<PlanContributor>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsolidationPlan {
    pub id: i64,
    pub revision: i64,
    pub approved_revision: Option<i64>,
    pub destination: PathBuf,
    pub generation: i64,
    pub state: String,
    pub mappings: Vec<Mapping>,
    pub items: Vec<PlanItem>,
}
#[cfg(target_os = "macos")]
const FILE_METADATA: &str = "Verified content; macOS file metadata copied, including permissions, timestamps, ACLs and extended attributes (including quarantine).";
#[cfg(not(target_os = "macos"))]
const FILE_METADATA: &str = "Verified content, Unix permissions and modification time. Metadata exception: ownership, ACLs, extended attributes and creation time are not copied.";
type PlannedDestination = (i64, String, Option<String>, PathBuf);
#[derive(Default)]
struct PlacementEvidence<'a> {
    hash: Option<&'a str>,
    destination_stamp: Option<&'a str>,
    detail: Option<&'a str>,
}
const DIRECTORY_METADATA: &str = "Directory placement retained; directory permissions, timestamps, ownership, ACLs and extended attributes are not copied.";

fn schema(db: &Connection) -> Result<()> {
    db.execute_batch("CREATE TABLE IF NOT EXISTS consolidation_plans (
      id INTEGER PRIMARY KEY, revision INTEGER NOT NULL, approved_revision INTEGER, destination BLOB NOT NULL,
      destination_identity TEXT NOT NULL, generation INTEGER NOT NULL, state TEXT NOT NULL);
    CREATE TABLE IF NOT EXISTS consolidation_mappings (plan INTEGER, position INTEGER, root INTEGER, source BLOB, destination BLOB);
    CREATE TABLE IF NOT EXISTS consolidation_items (id INTEGER PRIMARY KEY, plan INTEGER NOT NULL, root INTEGER NOT NULL,
      source BLOB NOT NULL, destination BLOB NOT NULL, action TEXT NOT NULL, included INTEGER NOT NULL,
      status TEXT NOT NULL, detail TEXT, hash TEXT, destination_stamp TEXT, stage BLOB, stage_identity TEXT);
    CREATE TABLE IF NOT EXISTS consolidation_contributors (item INTEGER, root INTEGER, source BLOB, PRIMARY KEY(item,root,source));
    CREATE TABLE IF NOT EXISTS consolidation_directories (plan INTEGER, path BLOB, identity TEXT NOT NULL, PRIMARY KEY(plan,path));
    CREATE TABLE IF NOT EXISTS consolidation_decisions (plan INTEGER, revision INTEGER, item INTEGER, included INTEGER, event TEXT NOT NULL);")?;
    Ok(())
}
fn invalid(message: &str) -> Error {
    Error::Invalid(message.into())
}
fn relative(p: &Path) -> Result<()> {
    if p.components().any(|c| !matches!(c, Component::Normal(_))) {
        return Err(invalid(
            "mapping paths must be relative and contain no dot or parent components",
        ));
    }
    Ok(())
}
fn c_name(name: &OsStr) -> Result<CString> {
    CString::new(name.as_bytes()).map_err(|_| invalid("path contains NUL"))
}
fn at(parent: &fs::File, name: &OsStr, directory: bool) -> Result<fs::File> {
    let name = c_name(name)?;
    let flags = libc::O_RDONLY
        | libc::O_NOFOLLOW
        | libc::O_CLOEXEC
        | libc::O_NONBLOCK
        | if directory { libc::O_DIRECTORY } else { 0 };
    // SAFETY: the parent is owned and name is NUL-terminated; successful fd is transferred once.
    let fd = unsafe { libc::openat(parent.as_raw_fd(), name.as_ptr(), flags) };
    if fd < 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(unsafe { fs::File::from_raw_fd(fd) })
}
fn absolute_directory(p: &Path) -> Result<fs::File> {
    if !p.is_absolute() {
        return Err(invalid(
            "destination must be an absolute existing directory",
        ));
    }
    let mut dir = fs::File::open("/")?;
    for part in p.components() {
        match part {
            Component::RootDir => {}
            Component::Normal(name) => dir = at(&dir, name, true)?,
            _ => {
                return Err(invalid(
                    "absolute directory path contains unsupported components",
                ));
            }
        }
    }
    Ok(dir)
}
fn missing(e: &Error) -> bool {
    matches!(e, Error::Io(io) if io.kind() == std::io::ErrorKind::NotFound)
}
fn hash_file(file: &mut fs::File) -> Result<String> {
    let before = file.metadata()?;
    if !before.is_file() {
        return Err(invalid("only regular files can be copied or deduplicated"));
    }
    file.seek(SeekFrom::Start(0))?;
    let mut hasher = blake3::Hasher::new();
    let mut buffer = [0u8; 128 * 1024];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    if directory_stamp(&before) != directory_stamp(&file.metadata()?) {
        return Err(invalid("file changed while hashing"));
    }
    Ok(hasher.finalize().to_hex().to_string())
}

#[cfg(target_os = "macos")]
fn quarantine(file: &fs::File) -> Result<Option<Vec<u8>>> {
    let name = c"com.apple.quarantine";
    // SAFETY: file owns its descriptor; the name is a static C string.
    let size = unsafe {
        libc::fgetxattr(
            file.as_raw_fd(),
            name.as_ptr(),
            std::ptr::null_mut(),
            0,
            0,
            0,
        )
    };
    if size < 0 {
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() == Some(libc::ENOATTR) {
            return Ok(None);
        }
        return Err(error.into());
    }
    let mut value = vec![0u8; size as usize];
    let read = unsafe {
        libc::fgetxattr(
            file.as_raw_fd(),
            name.as_ptr(),
            value.as_mut_ptr().cast(),
            value.len(),
            0,
            0,
        )
    };
    if read < 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    if read as usize != value.len() {
        return Err(invalid("quarantine metadata changed while reading"));
    }
    Ok(Some(value))
}
#[cfg(target_os = "macos")]
fn preserve_quarantine(source: &fs::File, stage: &fs::File) -> Result<()> {
    let expected = quarantine(source)?;
    if let Some(value) = &expected {
        // fcopyfile may normalize quarantine attributes. Preserve the original
        // security marker exactly, then verify it before any destination publish.
        let name = c"com.apple.quarantine";
        if unsafe {
            libc::fsetxattr(
                stage.as_raw_fd(),
                name.as_ptr(),
                value.as_ptr().cast(),
                value.len(),
                0,
                0,
            )
        } != 0
        {
            return Err(std::io::Error::last_os_error().into());
        }
    }
    if quarantine(stage)? != expected {
        return Err(invalid(
            "quarantine preservation failed; copy was not published",
        ));
    }
    Ok(())
}

// A conservative portable collision key. Different spellings still retain separate
// placements via suffixes; they are never silently collapsed by this key.
fn name_key(path: &Path) -> Vec<u8> {
    match path.to_str() {
        Some(text) => text
            .nfd()
            .flat_map(char::to_lowercase)
            .collect::<String>()
            .into_bytes(),
        None => bytes(path).iter().map(u8::to_ascii_lowercase).collect(),
    }
}
fn suffixed(path: &Path, root: i64, number: usize, directory: bool) -> PathBuf {
    let name = path.file_name().unwrap_or(OsStr::new("folder"));
    let mut result = OsString::from(if directory {
        name
    } else {
        Path::new(name).file_stem().unwrap_or(name)
    });
    result.push(format!(
        " (source {root}{})",
        if number == 1 {
            String::new()
        } else {
            format!("-{number}")
        }
    ));
    if !directory && let Some(ext) = path.extension() {
        result.push(".");
        result.push(ext);
    }
    path.with_file_name(result)
}

impl Job {
    fn consolidation_writer(database: &Path) -> Result<Self> {
        let protected = preflight_roots(database)?;
        let database = safe_database(database, &protected)?;
        let lock = lock_database(&database)?;
        let job = Self {
            db: Connection::open(database)?,
            active: None,
            comparison_active: None,
            _lock: Some(lock),
        };
        job.db.execute_batch("PRAGMA synchronous=FULL")?;
        compare::migrate(&job.db)?;
        schema(&job.db)?;
        Ok(job)
    }
    /// Prepare a durable preview from explicitly selected directory mappings. This
    /// hashes every incoming regular file afresh and performs no destination writes.
    pub fn plan(database: &Path, destination: &Path, mappings: &[Mapping]) -> Result<Self> {
        let job = Self::consolidation_writer(database)?;
        let progress = job.progress()?;
        if progress.state != State::Complete || progress.stale_entries != 0 {
            return Err(invalid(
                "finish a clean inventory before planning consolidation",
            ));
        }
        // Validate the supplied path before canonicalization, which would hide links.
        let destination_fd = absolute_directory(destination)?;
        let destination = fs::canonicalize(destination)?;
        let mut accepted = Vec::new();
        let mut incoming = Vec::<PathBuf>::new();
        for mapping in mappings {
            relative(&mapping.source_relative)?;
            relative(&mapping.destination_relative)?;
            let root = job.validate_root(mapping.root_id)?;
            if root == destination {
                continue;
            } // The selected primary is never incoming.
            let source = root.join(&mapping.source_relative);
            let directory = job.source_directory(mapping.root_id, &mapping.source_relative)?;
            if !directory.metadata()?.is_dir() {
                return Err(invalid("mapping source must be a directory"));
            }
            if root.starts_with(&destination) || destination.starts_with(&root) {
                return Err(invalid("destination and incoming sources must not overlap"));
            }
            if incoming
                .iter()
                .any(|p| source.starts_with(p) || p.starts_with(&source))
            {
                return Err(invalid("incoming directory mappings must not overlap"));
            }
            incoming.push(source);
            accepted.push(mapping.clone());
        }
        if accepted.is_empty() {
            return Err(invalid("select at least one incoming source directory"));
        }
        // Storage cannot be inside the destination, including when it is not an inventory root.
        safe_database(database, std::slice::from_ref(&destination))?;
        job.db.execute_batch("BEGIN IMMEDIATE")?;
        let result = (|| -> Result<()> {
            let revision: i64 = job.db.query_row(
                "SELECT COALESCE(MAX(revision),0)+1 FROM consolidation_plans",
                [],
                |r| r.get(0),
            )?;
            job.db.execute("INSERT INTO consolidation_plans(revision,destination,destination_identity,generation,state) VALUES(?,?,?,?,'proposed')",params![revision,bytes(&destination),identity(&destination_fd.metadata()?),progress.generation])?;
            let plan = job.db.last_insert_rowid();
            job.remember_directory(plan, Path::new(""), &destination_fd)?;
            let entries = job.inventory()?;
            for (position, mapping) in accepted.iter().enumerate() {
                job.db.execute(
                    "INSERT INTO consolidation_mappings VALUES (?,?,?,?,?)",
                    params![
                        plan,
                        position as i64,
                        mapping.root_id,
                        bytes(&mapping.source_relative),
                        bytes(&mapping.destination_relative)
                    ],
                )?;
                let mut selected: Vec<_> = entries
                    .iter()
                    .filter(|e| {
                        e.root_id == mapping.root_id
                            && e.relative_path.starts_with(&mapping.source_relative)
                    })
                    .collect();
                selected.sort_by(|a, b| {
                    a.relative_path
                        .components()
                        .count()
                        .cmp(&b.relative_path.components().count())
                        .then(a.relative_path.cmp(&b.relative_path))
                });
                let mut remapped = BTreeMap::<PathBuf, PathBuf>::new();
                let mapped_root = selected
                    .iter()
                    .find(|e| e.relative_path == mapping.source_relative)
                    .ok_or_else(|| invalid("mapped source directory is missing from inventory"))?;
                let mut prefix = PathBuf::new();
                for part in mapping
                    .destination_relative
                    .parent()
                    .unwrap_or(Path::new(""))
                    .components()
                {
                    prefix.push(part);
                    let prior = job.planned_destination(plan, &prefix)?;
                    if let Some((_, action, _, spelling)) = &prior
                        && (action != "create_directory" || spelling != &prefix)
                    {
                        return Err(invalid(
                            "mapping target parent conflicts with another planned name; choose a distinct target subfolder",
                        ));
                    }
                    let planned = prior.is_some();
                    if !planned {
                        let existing = job.destination_entry(plan, &destination_fd, &prefix)?;
                        let stamp = if let Some(dir) = existing {
                            if !dir.metadata()?.is_dir() {
                                return Err(invalid("mapping target parent is not a directory"));
                            }
                            job.remember_directory(plan, &prefix, &dir)?;
                            Some(identity(&dir.metadata()?))
                        } else {
                            None
                        };
                        job.insert_item(
                            plan,
                            mapped_root,
                            &prefix,
                            "create_directory",
                            PlacementEvidence {
                                destination_stamp: stamp.as_deref(),
                                detail: Some(DIRECTORY_METADATA),
                                ..PlacementEvidence::default()
                            },
                        )?;
                    }
                }
                for entry in selected {
                    let suffix = entry
                        .relative_path
                        .strip_prefix(&mapping.source_relative)
                        .map_err(|_| invalid("invalid mapping prefix"))?;
                    let desired = if suffix.as_os_str().is_empty() {
                        mapping.destination_relative.clone()
                    } else {
                        let parent = entry.relative_path.parent().unwrap();
                        remapped
                            .get(parent)
                            .ok_or_else(|| invalid("source directory missing from inventory"))?
                            .join(entry.relative_path.file_name().unwrap())
                    };
                    let directory = entry.entry_type == EntryType::Directory;
                    if directory {
                        job.source_directory(entry.root_id, &entry.relative_path)?;
                    }
                    if entry.excluded && directory {
                        remapped.insert(entry.relative_path.clone(), desired);
                        continue;
                    }
                    let (hash, problem) = match entry.entry_type {
                        EntryType::File => match job.source_hash(entry.root_id, &entry.relative_path) {
                            Ok(h) => (Some(h), None), Err(e) => (None, Some(e.to_string())),
                        },
                        EntryType::Directory => (None, None),
                        _ => (None, Some("Links and unsupported filesystem items are held; no copy is attempted.".into())),
                    };
                    if let Some(problem) = problem {
                        job.insert_item(
                            plan,
                            entry,
                            &desired,
                            "blocked",
                            PlacementEvidence {
                                detail: Some(&problem),
                                ..PlacementEvidence::default()
                            },
                        )?;
                        continue;
                    }
                    let mut candidate = desired.clone();
                    let mut number = 0;
                    loop {
                        let planned = job.planned_destination(plan, &candidate)?;
                        if let Some((item, action, existing_hash, spelling)) = planned {
                            if spelling == candidate
                                && ((directory && action == "create_directory")
                                    || (!directory && hash == existing_hash && hash.is_some()))
                            {
                                job.add_contributor(item, entry.root_id, &entry.relative_path)?;
                                if directory {
                                    remapped.insert(entry.relative_path.clone(), candidate);
                                }
                                break;
                            }
                        } else {
                            let existing =
                                job.destination_entry(plan, &destination_fd, &candidate)?;
                            match existing {
                                None => {
                                    if candidate.as_os_str().is_empty() {
                                        return Err(invalid("destination root is missing"));
                                    }
                                    job.insert_item(
                                        plan,
                                        entry,
                                        &candidate,
                                        if directory {
                                            "create_directory"
                                        } else {
                                            "copy"
                                        },
                                        PlacementEvidence {
                                            hash: hash.as_deref(),
                                            ..PlacementEvidence::default()
                                        },
                                    )?;
                                    if directory {
                                        remapped.insert(entry.relative_path.clone(), candidate);
                                    }
                                    break;
                                }
                                Some(mut file) => {
                                    let meta = file.metadata()?;
                                    if directory && meta.is_dir() {
                                        job.remember_directory(plan, &candidate, &file)?;
                                        job.insert_item(
                                            plan,
                                            entry,
                                            &candidate,
                                            "create_directory",
                                            PlacementEvidence {
                                                destination_stamp: Some(&identity(&meta)),
                                                ..PlacementEvidence::default()
                                            },
                                        )?;
                                        remapped.insert(entry.relative_path.clone(), candidate);
                                        break;
                                    }
                                    if !directory
                                        && meta.is_file()
                                        && hash.as_deref() == Some(hash_file(&mut file)?.as_str())
                                    {
                                        job.insert_item(plan,entry,&candidate,"already_present",PlacementEvidence { hash: hash.as_deref(), destination_stamp: Some(&directory_stamp(&meta)), detail: Some("Existing destination content matches; its metadata is retained.") })?;
                                        break;
                                    }
                                }
                            }
                        }
                        number += 1;
                        if number > 10000 {
                            return Err(invalid("too many destination name collisions"));
                        }
                        candidate = suffixed(&desired, entry.root_id, number, directory);
                    }
                }
            }
            Ok(())
        })();
        match result {
            Ok(()) => job.db.execute_batch("COMMIT")?,
            Err(e) => {
                job.db.execute_batch("ROLLBACK")?;
                return Err(e);
            }
        }
        Ok(job)
    }
    fn planned_destination(
        &self,
        plan: i64,
        candidate: &Path,
    ) -> Result<Option<PlannedDestination>> {
        let rows: Vec<(i64,String,Option<String>,Vec<u8>)> = self.db.prepare("SELECT id,action,hash,destination FROM consolidation_items WHERE plan=? AND action!='blocked' ORDER BY id")?
            .query_map([plan], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?)))?
            .collect::<std::result::Result<_,_>>()?;
        let key = name_key(candidate);
        Ok(rows
            .into_iter()
            .find_map(|(id, action, hash, destination)| {
                let destination = path(destination);
                (name_key(&destination) == key).then_some((id, action, hash, destination))
            }))
    }
    fn insert_item(
        &self,
        plan: i64,
        entry: &Entry,
        dest: &Path,
        action: &str,
        evidence: PlacementEvidence<'_>,
    ) -> Result<()> {
        let PlacementEvidence {
            hash,
            destination_stamp: stamp,
            detail,
        } = evidence;
        let detail = detail.or(if action == "create_directory" {
            Some(DIRECTORY_METADATA)
        } else {
            None
        });
        self.db.execute("INSERT INTO consolidation_items(plan,root,source,destination,action,included,status,detail,hash,destination_stamp) VALUES(?,?,?,?,?,?,'pending',?,?,?)",params![plan,entry.root_id,bytes(&entry.relative_path),bytes(dest),action,action!="blocked",detail,hash,stamp])?;
        self.add_contributor(
            self.db.last_insert_rowid(),
            entry.root_id,
            &entry.relative_path,
        )
    }
    fn add_contributor(&self, item: i64, root: i64, source: &Path) -> Result<()> {
        self.db.execute(
            "INSERT OR IGNORE INTO consolidation_contributors VALUES (?,?,?)",
            params![item, root, bytes(source)],
        )?;
        Ok(())
    }
    fn source_record(&self, root: i64, relative: &Path) -> Result<Comparison> {
        let (size, generation): (i64, i64) = self.db.query_row(
            "SELECT size,generation FROM entries WHERE root=? AND path=? AND kind='file'",
            params![root, bytes(relative)],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        if generation != self.progress()?.generation {
            return Err(invalid("stale source inventory"));
        }
        Ok(Comparison {
            root_id: root,
            relative_path: relative.into(),
            generation,
            size: size as u64,
            stage: ComparisonStage::PendingFull,
            algorithm: "blake3".into(),
            sample_format: String::new(),
            sample_hash: None,
            full_hash: None,
            bytes_read: 0,
            failure: None,
            stale: false,
        })
    }
    fn source_hash(&self, root: i64, relative: &Path) -> Result<String> {
        let record = self.source_record(root, relative)?;
        let mut file = self.open_observed(&record)?;
        let hash = hash_file(&mut file)?;
        self.check_file(&record, &file)?;
        self.open_observed(&record)?;
        Ok(hash)
    }
    fn source_directory(&self, root: i64, relative: &Path) -> Result<fs::File> {
        let base = self.validate_root(root)?;
        let mut file = absolute_directory(&base)?;
        let mut current = PathBuf::new();
        for part in std::iter::once(None).chain(relative.components().map(Some)) {
            if let Some(Component::Normal(name)) = part {
                file = at(&file, name, true)?;
                current.push(name);
            } else if part.is_some() {
                return Err(invalid("invalid source directory component"));
            }
            let expected: Option<String>=self.db.query_row("SELECT observation FROM entries WHERE root=? AND path=? AND generation=(SELECT generation FROM job) AND kind='directory'",params![root,bytes(&current)],|r|r.get(0)).optional()?.flatten();
            if expected.as_deref() != Some(directory_stamp(&file.metadata()?).as_str()) {
                return Err(invalid(
                    "source directory changed since inventory; rescan required",
                ));
            }
        }
        Ok(file)
    }
    fn remember_directory(&self, plan: i64, relative: &Path, dir: &fs::File) -> Result<()> {
        let actual = identity(&dir.metadata()?);
        let saved: Option<String> = self
            .db
            .query_row(
                "SELECT identity FROM consolidation_directories WHERE plan=? AND path=?",
                params![plan, bytes(relative)],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(saved) = saved {
            if saved != actual {
                return Err(invalid("destination directory changed identity"));
            }
        } else {
            self.db.execute(
                "INSERT INTO consolidation_directories VALUES(?,?,?)",
                params![plan, bytes(relative), actual],
            )?;
        }
        Ok(())
    }
    // Planning observes existing ancestors without creating any filesystem objects.
    fn destination_entry(
        &self,
        plan: i64,
        root: &fs::File,
        relative: &Path,
    ) -> Result<Option<fs::File>> {
        let mut dir = root.try_clone()?;
        let mut current = PathBuf::new();
        let parts: Vec<_> = relative.components().collect();
        for (i, part) in parts.iter().enumerate() {
            let Component::Normal(name) = part else {
                return Err(invalid("invalid destination path"));
            };
            current.push(name);
            match at(&dir, name, i + 1 < parts.len()) {
                Ok(file) => {
                    if i + 1 < parts.len() {
                        self.remember_directory(plan, &current, &file)?;
                    }
                    dir = file;
                }
                Err(e) if missing(&e) => return Ok(None),
                Err(e) => return Err(e),
            }
        }
        Ok(Some(dir))
    }
    /// Read the latest saved preview/result without touching source or destination files.
    pub fn consolidation_plan(&self) -> Result<Option<ConsolidationPlan>> {
        let exists: bool = self.db.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='consolidation_plans')",
            [],
            |r| r.get(0),
        )?;
        if !exists {
            return Ok(None);
        }
        let mut plan=self.db.query_row("SELECT id,revision,approved_revision,destination,generation,state FROM consolidation_plans ORDER BY id DESC LIMIT 1",[],|r|Ok(ConsolidationPlan{id:r.get(0)?,revision:r.get(1)?,approved_revision:r.get(2)?,destination:path(r.get(3)?),generation:r.get(4)?,state:r.get(5)?,mappings:vec![],items:vec![]})).optional()?;
        if let Some(p) = &mut plan {
            p.mappings=self.db.prepare("SELECT root,source,destination FROM consolidation_mappings WHERE plan=? ORDER BY position")?.query_map([p.id],|r|Ok(Mapping{root_id:r.get(0)?,source_relative:path(r.get(1)?),destination_relative:path(r.get(2)?)}))?.collect::<std::result::Result<_,_>>()?;
            p.items=self.db.prepare("SELECT id,root,source,destination,action,included,status,detail,hash FROM consolidation_items WHERE plan=? ORDER BY id")?.query_map([p.id],|r|Ok(PlanItem{id:r.get(0)?,root_id:r.get(1)?,source_relative:path(r.get(2)?),destination_relative:path(r.get(3)?),action:r.get(4)?,included:r.get(5)?,status:r.get(6)?,detail:r.get(7)?,hash:r.get(8)?,contributors:vec![]}))?.collect::<std::result::Result<_,_>>()?;
            for item in &mut p.items {
                item.contributors=self.db.prepare("SELECT root,source FROM consolidation_contributors WHERE item=? ORDER BY root,source")?.query_map([item.id],|r|Ok(PlanContributor{root_id:r.get(0)?,source_relative:path(r.get(1)?)}))?.collect::<std::result::Result<_,_>>()?;
            }
        }
        Ok(plan)
    }
    fn require_plan(&self, revision: Option<i64>) -> Result<ConsolidationPlan> {
        let plan = self
            .consolidation_plan()?
            .ok_or_else(|| invalid("prepare a consolidation plan first"))?;
        if revision.is_some_and(|r| r != plan.revision) {
            return Err(invalid("plan revision changed; review the current preview"));
        }
        if plan.generation != self.progress()?.generation {
            return Err(invalid(
                "inventory generation changed; create a new consolidation plan",
            ));
        }
        Ok(plan)
    }
    pub fn update_plan(
        database: &Path,
        revision: i64,
        decisions: &[PlanDecision],
    ) -> Result<ConsolidationPlan> {
        let job = Self::consolidation_writer(database)?;
        let plan = job.require_plan(Some(revision))?;
        if !matches!(plan.state.as_str(), "proposed" | "approved") {
            return Err(invalid(
                "a started plan cannot be edited; create a new preview",
            ));
        }
        for d in decisions {
            let item = plan
                .items
                .iter()
                .find(|i| i.id == d.item_id)
                .ok_or_else(|| invalid("decision item is not in this plan"))?;
            if d.included && item.action == "blocked" {
                return Err(invalid("unsupported items cannot be approved for copying"));
            }
        }
        job.db.execute_batch("BEGIN IMMEDIATE")?;
        let result = (|| -> Result<()> {
            for d in decisions {
                job.db.execute(
                    "UPDATE consolidation_items SET included=? WHERE plan=? AND id=?",
                    params![d.included, plan.id, d.item_id],
                )?;
                job.db.execute(
                    "INSERT INTO consolidation_decisions VALUES(?,?,?,?,'decision')",
                    params![plan.id, revision + 1, d.item_id, d.included],
                )?;
            }
            job.db.execute("UPDATE consolidation_plans SET revision=revision+1,approved_revision=NULL,state='proposed' WHERE id=?",[plan.id])?;
            Ok(())
        })();
        match result {
            Ok(()) => job.db.execute_batch("COMMIT")?,
            Err(e) => {
                job.db.execute_batch("ROLLBACK")?;
                return Err(e);
            }
        }
        job.require_plan(None)
    }
    pub fn approve_plan(database: &Path, revision: i64) -> Result<ConsolidationPlan> {
        let job = Self::consolidation_writer(database)?;
        let plan = job.require_plan(Some(revision))?;
        if !matches!(plan.state.as_str(), "proposed" | "approved") {
            return Err(invalid("only an unstarted plan can be approved"));
        }
        if !plan.items.iter().any(|i| i.included) {
            return Err(invalid(
                "include at least one supported placement before approval",
            ));
        }
        // A declined directory must not be silently recreated by an included child.
        for item in plan.items.iter().filter(|i| i.included) {
            if plan.items.iter().any(|d| {
                d.action == "create_directory"
                    && !d.included
                    && item
                        .destination_relative
                        .starts_with(&d.destination_relative)
            }) {
                return Err(invalid(
                    "an included placement depends on an excluded directory; include that directory or exclude its children",
                ));
            }
        }
        job.db.execute_batch("BEGIN IMMEDIATE")?;
        job.db.execute(
            "UPDATE consolidation_plans SET approved_revision=revision,state='approved' WHERE id=?",
            [plan.id],
        )?;
        job.db.execute(
            "INSERT INTO consolidation_decisions(plan,revision,event) VALUES(?,?,'approved')",
            params![plan.id, revision],
        )?;
        job.db.execute_batch("COMMIT")?;
        job.require_plan(None)
    }
    pub fn execute(database: &Path, revision: i64) -> Result<Self> {
        let job = Self::consolidation_writer(database)?;
        let plan = job.require_plan(Some(revision))?;
        if plan.approved_revision != Some(revision) {
            return Err(invalid(
                "explicit approval of this exact plan revision is required",
            ));
        }
        job.destination_root(&plan)?;
        job.db.execute_batch("BEGIN IMMEDIATE")?;
        job.db.execute("UPDATE consolidation_items SET status=CASE WHEN included=0 THEN 'skipped' ELSE 'pending' END WHERE plan=?",[plan.id])?;
        job.db.execute(
            "UPDATE consolidation_plans SET state='running' WHERE id=?",
            [plan.id],
        )?;
        job.db.execute_batch("COMMIT")?;
        Ok(job)
    }
    pub fn pause_execution(&mut self) -> Result<ConsolidationPlan> {
        if self._lock.is_none() {
            return Err(invalid("read-only job cannot pause execution"));
        }
        let plan = self.require_plan(None)?;
        self.db.execute(
            "UPDATE consolidation_plans SET state='paused' WHERE id=? AND state='running'",
            [plan.id],
        )?;
        self.require_plan(None)
    }
    fn destination_root(&self, plan: &ConsolidationPlan) -> Result<fs::File> {
        let file = absolute_directory(&plan.destination)?;
        let expected: String = self.db.query_row(
            "SELECT destination_identity FROM consolidation_plans WHERE id=?",
            [plan.id],
            |r| r.get(0),
        )?;
        if identity(&file.metadata()?) != expected {
            return Err(invalid(
                "destination root changed identity; create a new plan",
            ));
        }
        Ok(file)
    }
    fn execution_parent(&self, plan: &ConsolidationPlan, relative: &Path) -> Result<fs::File> {
        let mut dir = self.destination_root(plan)?;
        let mut current = PathBuf::new();
        for part in relative.components() {
            let Component::Normal(name) = part else {
                return Err(invalid("invalid destination parent"));
            };
            current.push(name);
            dir = at(&dir, name, true)?;
            let expected: Option<String> = self
                .db
                .query_row(
                    "SELECT identity FROM consolidation_directories WHERE plan=? AND path=?",
                    params![plan.id, bytes(&current)],
                    |r| r.get(0),
                )
                .optional()?;
            if expected.as_deref() != Some(identity(&dir.metadata()?).as_str()) {
                return Err(invalid("destination ancestor changed or was not approved"));
            }
        }
        Ok(dir)
    }
    /// Complete at most `budget` file/directory checkpoints. A single file copy and
    /// full verification finishes before a cooperative pause; partial bytes are not
    /// published. Crash recovery recognizes only a saved owned staging inode.
    pub fn execute_step(&mut self, budget: usize) -> Result<ConsolidationPlan> {
        if self._lock.is_none() {
            return Err(invalid("read-only job cannot execute a plan"));
        }
        if budget == 0 {
            return Err(invalid("execution budget must be positive"));
        }
        let plan = self.require_plan(None)?;
        if plan.state != "running" {
            return Ok(plan);
        }
        for item in plan
            .items
            .iter()
            .filter(|i| i.included && i.status == "pending")
            .take(budget.min(128))
        {
            let result = self.execute_item(&plan, item);
            match result {
                Ok(detail) => {
                    self.db.execute(
                        "UPDATE consolidation_items SET status='verified',detail=? WHERE id=?",
                        params![detail, item.id],
                    )?;
                    // Keep owned staging evidence until verified status is durable.
                    if let Err(error) = self.cleanup_stage(&plan, item) {
                        self.db.execute("UPDATE consolidation_items SET status='failed',detail=? WHERE id=?",params![format!("Content verified; owned staging cleanup needs attention: {error}"),item.id])?;
                    }
                }
                Err(e) => {
                    self.db.execute(
                        "UPDATE consolidation_items SET status='failed',detail=? WHERE id=?",
                        params![e.to_string(), item.id],
                    )?;
                }
            }
        }
        let current = self.require_plan(None)?;
        if !current
            .items
            .iter()
            .any(|i| i.included && i.status == "pending")
        {
            let state = if current
                .items
                .iter()
                .any(|i| i.status == "failed" || i.action == "blocked")
            {
                "finished_with_errors"
            } else {
                "complete"
            };
            self.db.execute(
                "UPDATE consolidation_plans SET state=? WHERE id=?",
                params![state, plan.id],
            )?;
        }
        self.require_plan(None)
    }
    fn execute_item(&self, plan: &ConsolidationPlan, item: &PlanItem) -> Result<String> {
        // Revalidate every contributor, including content collapsed at the same target.
        for c in &item.contributors {
            if item.action == "create_directory" {
                self.source_directory(c.root_id, &c.source_relative)?;
            } else if self.source_hash(c.root_id, &c.source_relative)?.as_str()
                != item.hash.as_deref().unwrap_or("")
            {
                return Err(invalid(
                    "source content changed since this plan; create a new plan",
                ));
            }
        }
        let expected: Option<String> = self.db.query_row(
            "SELECT destination_stamp FROM consolidation_items WHERE id=?",
            [item.id],
            |r| r.get(0),
        )?;
        if item.destination_relative.as_os_str().is_empty() {
            self.destination_root(plan)?;
            return Ok(DIRECTORY_METADATA.into());
        }
        let parent_path = item.destination_relative.parent().unwrap_or(Path::new(""));
        let parent = self.execution_parent(plan, parent_path)?;
        let name = item
            .destination_relative
            .file_name()
            .ok_or_else(|| invalid("destination filename missing"))?;
        if item.action == "create_directory" {
            let existing = at(&parent, name, true);
            let dir = match existing {
                Ok(dir) => {
                    let remembered:Option<String>=self.db.query_row("SELECT identity FROM consolidation_directories WHERE plan=? AND path=?",params![plan.id,bytes(&item.destination_relative)],|r|r.get(0)).optional()?;
                    if expected.as_deref().or(remembered.as_deref())
                        != Some(identity(&dir.metadata()?).as_str())
                    {
                        return Err(invalid(
                            "destination directory appeared or changed after preview",
                        ));
                    }
                    dir
                }
                Err(e) if missing(&e) && expected.is_none() => {
                    let name = c_name(name)?;
                    // SAFETY: directory fd and C name are live; mkdirat never replaces an entry.
                    if unsafe { libc::mkdirat(parent.as_raw_fd(), name.as_ptr(), 0o755) } != 0 {
                        return Err(std::io::Error::last_os_error().into());
                    }
                    parent.sync_all()?;
                    at(&parent, OsStr::from_bytes(name.as_bytes()), true)?
                }
                Err(e) => return Err(e),
            };
            self.remember_directory(plan.id, &item.destination_relative, &dir)?;
            self.confirm_placement(plan, item, &dir)?;
            return Ok(DIRECTORY_METADATA.into());
        }
        if item.action == "already_present" {
            let mut destination = at(&parent, name, false)?;
            if expected.as_deref() != Some(directory_stamp(&destination.metadata()?).as_str()) {
                return Err(invalid("existing destination changed since preview"));
            }
            if Some(hash_file(&mut destination)?) != item.hash {
                return Err(invalid(
                    "existing destination content changed since preview",
                ));
            }
            self.confirm_placement(plan, item, &destination)?;
            return Ok("Fresh full BLAKE3 verification matches all contributors; existing destination metadata retained.".into());
        }
        if item.action != "copy" {
            return Err(invalid("unsupported placement cannot be executed"));
        }
        if let Some(expected) = &expected {
            let mut target = at(&parent, name, false)?;
            if identity(&target.metadata()?) != *expected
                || Some(hash_file(&mut target)?) != item.hash
            {
                return Err(invalid(
                    "previously published destination changed; create a new plan",
                ));
            }
            self.confirm_placement(plan, item, &target)?;
            return Ok(FILE_METADATA.into());
        }
        let record = self.source_record(item.root_id, &item.source_relative)?;
        let mut source = self.open_observed(&record)?;
        let source_meta = source.metadata()?;
        let (stage_name, mut stage) = self.staging_file(plan, item, &parent)?;
        // An interrupted publish is accepted only when target still aliases our saved staging inode.
        let target = at(&parent, name, false);
        if let Ok(mut target) = target {
            if identity(&target.metadata()?) != identity(&stage.metadata()?)
                || Some(hash_file(&mut target)?) != item.hash
            {
                return Err(invalid(
                    "destination appeared after preview; nothing was overwritten",
                ));
            }
            self.check_file(&record, &source)?;
            self.confirm_placement(plan, item, &target)?;
            self.db.execute(
                "UPDATE consolidation_items SET destination_stamp=? WHERE id=?",
                params![identity(&target.metadata()?), item.id],
            )?;
            return Ok(FILE_METADATA.into());
        } else if let Err(e) = target
            && !missing(&e)
        {
            return Err(e);
        }
        if stage.metadata()?.nlink() != 1 {
            return Err(invalid(
                "staging file has an unexpected hard-link alias; nothing was modified",
            ));
        }
        stage.set_len(0)?;
        stage.seek(SeekFrom::Start(0))?;
        source.seek(SeekFrom::Start(0))?;
        std::io::copy(&mut source, &mut stage)?;
        stage.flush()?;
        self.check_file(&record, &source)?;
        self.open_observed(&record)?;
        if Some(hash_file(&mut stage)?) != item.hash {
            return Err(invalid("staged file failed full content verification"));
        }
        #[cfg(target_os = "macos")]
        {
            // SAFETY: both descriptors are owned and refer to the reviewed source
            // and exclusively created staging file. Metadata only; no content execution.
            if unsafe {
                libc::fcopyfile(
                    source.as_raw_fd(),
                    stage.as_raw_fd(),
                    std::ptr::null_mut(),
                    libc::COPYFILE_METADATA,
                )
            } != 0
            {
                return Err(invalid(&format!(
                    "metadata preservation failed; copy was not published: {}",
                    std::io::Error::last_os_error()
                )));
            }
        }
        #[cfg(target_os = "macos")]
        preserve_quarantine(&source, &stage)?;
        #[cfg(not(target_os = "macos"))]
        {
            stage.set_permissions(fs::Permissions::from_mode(source_meta.mode() & 0o777))?;
            stage.set_times(fs::FileTimes::new().set_modified(source_meta.modified()?))?;
        }
        let stage_meta = stage.metadata()?;
        if stage_meta.mode() & 0o777 != source_meta.mode() & 0o777
            || stage_meta.modified()? != source_meta.modified()?
        {
            return Err(invalid(
                "metadata verification failed; copy was not published",
            ));
        }
        stage.sync_all()?;
        // Validate paths again immediately before publishing through the pinned parent.
        let checked = self.execution_parent(plan, parent_path)?;
        if identity(&checked.metadata()?) != identity(&parent.metadata()?) {
            return Err(invalid("destination parent moved during copy"));
        }
        self.check_file(&record, &source)?;
        self.open_observed(&record)?;
        let stage_c = c_name(&stage_name)?;
        let target_c = c_name(name)?;
        // SAFETY: both relative C names and owned directory fd are live. linkat is
        // atomic and refuses any existing target; it cannot overwrite a user file.
        if unsafe {
            libc::linkat(
                parent.as_raw_fd(),
                stage_c.as_ptr(),
                parent.as_raw_fd(),
                target_c.as_ptr(),
                0,
            )
        } != 0
        {
            return Err(std::io::Error::last_os_error().into());
        }
        parent.sync_all()?;
        let mut published = at(&parent, name, false)?;
        if identity(&published.metadata()?) != identity(&stage.metadata()?)
            || Some(hash_file(&mut published)?) != item.hash
        {
            return Err(invalid("published file failed verification"));
        }
        self.check_file(&record, &source)?;
        self.confirm_placement(plan, item, &published)?;
        self.db.execute(
            "UPDATE consolidation_items SET destination_stamp=? WHERE id=?",
            params![identity(&published.metadata()?), item.id],
        )?;
        Ok(FILE_METADATA.into())
    }
    fn confirm_placement(
        &self,
        plan: &ConsolidationPlan,
        item: &PlanItem,
        observed: &fs::File,
    ) -> Result<()> {
        let parent = self.execution_parent(
            plan,
            item.destination_relative.parent().unwrap_or(Path::new("")),
        )?;
        let current = at(
            &parent,
            item.destination_relative
                .file_name()
                .ok_or_else(|| invalid("destination filename missing"))?,
            observed.metadata()?.is_dir(),
        )?;
        if directory_stamp(&current.metadata()?) != directory_stamp(&observed.metadata()?) {
            return Err(invalid(
                "exact destination placement changed during verification",
            ));
        }
        for contributor in &item.contributors {
            if item.action == "create_directory" {
                self.source_directory(contributor.root_id, &contributor.source_relative)?;
            } else {
                self.open_observed(
                    &self.source_record(contributor.root_id, &contributor.source_relative)?,
                )?;
            }
        }
        Ok(())
    }
    fn staging_file(
        &self,
        plan: &ConsolidationPlan,
        item: &PlanItem,
        parent: &fs::File,
    ) -> Result<(OsString, fs::File)> {
        let (saved, expected): (Option<Vec<u8>>, Option<String>) = self.db.query_row(
            "SELECT stage,stage_identity FROM consolidation_items WHERE id=?",
            [item.id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        if let Some(name) = saved {
            let name = OsString::from_vec(name);
            let read = at(parent, &name, false)?;
            if expected.as_deref() != Some(identity(&read.metadata()?).as_str()) {
                return Err(invalid(
                    "staging file changed identity; held for manual review",
                ));
            }
            if read.metadata()?.nlink() > 1 {
                return Ok((name, read));
            }
            let name_c = c_name(&name)?;
            // SAFETY: open existing stage with no-follow; compare identity before writes.
            let fd = unsafe {
                libc::openat(
                    parent.as_raw_fd(),
                    name_c.as_ptr(),
                    libc::O_RDWR | libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK,
                )
            };
            if fd < 0 {
                return Err(std::io::Error::last_os_error().into());
            }
            let file = unsafe { fs::File::from_raw_fd(fd) };
            if identity(&file.metadata()?) != identity(&read.metadata()?) {
                return Err(invalid("staging file changed while opening"));
            }
            return Ok((name, file));
        }
        let name = OsString::from(format!(".folderloom-plan-{}-item-{}.tmp", plan.id, item.id));
        let name_c = c_name(&name)?;
        // SAFETY: exclusive creation cannot touch an existing filesystem object.
        let fd = unsafe {
            libc::openat(
                parent.as_raw_fd(),
                name_c.as_ptr(),
                libc::O_RDWR | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                0o600,
            )
        };
        if fd < 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        let file = unsafe { fs::File::from_raw_fd(fd) };
        self.db.execute(
            "UPDATE consolidation_items SET stage=?,stage_identity=? WHERE id=?",
            params![name.as_bytes(), identity(&file.metadata()?), item.id],
        )?;
        parent.sync_all()?;
        Ok((name, file))
    }
    fn cleanup_stage(&self, plan: &ConsolidationPlan, item: &PlanItem) -> Result<()> {
        let (name, expected): (Option<Vec<u8>>, Option<String>) = self.db.query_row(
            "SELECT stage,stage_identity FROM consolidation_items WHERE id=?",
            [item.id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let Some(name) = name else {
            return Ok(());
        };
        let parent = self.execution_parent(
            plan,
            item.destination_relative.parent().unwrap_or(Path::new("")),
        )?;
        let file = match at(&parent, OsStr::from_bytes(&name), false) {
            Ok(file) => file,
            Err(error) if missing(&error) => {
                self.db.execute(
                    "UPDATE consolidation_items SET stage=NULL,stage_identity=NULL WHERE id=?",
                    [item.id],
                )?;
                return Ok(());
            }
            Err(error) => return Err(error),
        };
        if expected.as_deref() != Some(identity(&file.metadata()?).as_str()) {
            return Err(invalid("owned staging file changed before cleanup"));
        }
        let name = c_name(OsStr::from_bytes(&name))?;
        // SAFETY: only the saved owned staging entry is unlinked, never a source path.
        if unsafe { libc::unlinkat(parent.as_raw_fd(), name.as_ptr(), 0) } != 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        parent.sync_all()?;
        self.db.execute(
            "UPDATE consolidation_items SET stage=NULL,stage_identity=NULL WHERE id=?",
            [item.id],
        )?;
        Ok(())
    }
}
