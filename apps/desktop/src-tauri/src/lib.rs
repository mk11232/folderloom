//! Desktop adapter: one background writer, saved checkpoints, and read-only snapshots.
use folderloom_core::{ComparisonState, Job, Mapping, Options, PlanDecision, State};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    fs,
    io::{Read, Write},
    os::unix::{ffi::OsStrExt, fs::MetadataExt},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

type Result<T> = std::result::Result<T, String>;
fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

#[derive(Clone)]
pub struct Desktop {
    inner: Arc<Inner>,
}
struct Inner {
    data: PathBuf,
    snapshot: Mutex<Snapshot>,
    pause: AtomicBool,
}
#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum WorkerMode {
    Scan,
    Compare,
    Plan,
    Execute,
}

#[derive(Deserialize)]
pub struct MappingInput {
    pub source_key: String,
    pub destination_relative: String,
}

enum Work {
    Create(JobSetup),
    Scan(Option<Vec<String>>),
    Compare,
    Plan {
        destination: String,
        mappings: Vec<MappingInput>,
    },
    Execute(i64),
}
impl Work {
    fn mode(&self) -> WorkerMode {
        match self {
            Self::Create(_) | Self::Scan(_) => WorkerMode::Scan,
            Self::Compare => WorkerMode::Compare,
            Self::Plan { .. } => WorkerMode::Plan,
            Self::Execute(_) => WorkerMode::Execute,
        }
    }
}

#[derive(Clone, Default, Serialize)]
struct Control {
    job_id: Option<String>,
    worker: Option<WorkerMode>,
    pause_requested: bool,
    recovery_required: bool,
    error: Option<String>,
}

#[derive(Clone, Serialize)]
struct Snapshot {
    setup: Option<JobSetup>,
    #[serde(flatten)]
    control: Control,
    #[serde(flatten)]
    observations: Value,
}
impl Snapshot {
    fn payload(&self) -> Result<Value> {
        serde_json::to_value(self).map_err(err)
    }
}

#[derive(Default, Serialize, Deserialize)]
pub struct Manifest {
    title: String,
    description: String,
    roots: Vec<Source>,
}
#[derive(Serialize, Deserialize)]
struct Source {
    label: String,
    path: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct JobSource {
    pub label: String,
    pub path: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct JobSetup {
    pub name: String,
    pub sources: Vec<JobSource>,
    pub destination: String,
}
#[derive(Clone, Debug, Serialize)]
pub struct JobSummary {
    pub id: String,
    pub name: String,
    pub created_ms: u64,
    pub updated_ms: u64,
    pub source_count: usize,
    pub entry_count: usize,
    pub state: String,
    pub stage: String,
    pub destination: Option<String>,
    pub error: Option<String>,
    pub active: bool,
}
#[derive(Serialize, Deserialize)]
struct JobMetadata {
    setup: JobSetup,
    created_ms: u64,
}

impl Desktop {
    pub fn open(data: PathBuf) -> Result<Self> {
        fs::create_dir_all(data.join("jobs")).map_err(err)?;
        let data = fs::canonicalize(data).map_err(err)?;
        validate_jobs_directory(&data)?;
        let snapshot = read_saved(&data);
        Ok(Self {
            inner: Arc::new(Inner {
                data,
                snapshot: Mutex::new(snapshot),
                pause: AtomicBool::new(false),
            }),
        })
    }
    /// Compact saved-job rows. Listing does not load inventory or comparison arrays.
    pub fn jobs(&self) -> Result<Vec<JobSummary>> {
        let cached = self.inner.snapshot.lock().map_err(err)?;
        validate_jobs_directory(&self.inner.data)?;
        let mut rows = Vec::new();
        for entry in fs::read_dir(self.inner.data.join("jobs")).map_err(err)? {
            let entry = entry.map_err(err)?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            let Some(id) = name.strip_suffix(".sqlite") else {
                continue;
            };
            if valid_id(id).is_err() {
                continue;
            }
            let selected = cached.control.job_id.as_deref() == Some(id);
            let active = selected && cached.control.worker.is_some();
            let mut row = job_summary(&self.inner.data, id, active);
            if selected {
                if let Some(worker) = cached.control.worker {
                    row.state = match worker {
                        WorkerMode::Scan => "scanning",
                        WorkerMode::Compare => "comparing",
                        WorkerMode::Plan => "planning",
                        WorkerMode::Execute => "copying",
                    }
                    .into();
                }
                if cached.control.error.is_some() {
                    row.error = cached.control.error.clone();
                }
            }
            rows.push(row);
        }
        rows.sort_by(|a, b| b.updated_ms.cmp(&a.updated_ms).then(b.id.cmp(&a.id)));
        Ok(rows)
    }
    pub fn open_job(&self, job_id: String) -> Result<Value> {
        let mut cached = self.inner.snapshot.lock().map_err(err)?;
        if cached.control.worker.is_some() {
            return Err(
                "Wait for the current operation to finish before opening another job.".into(),
            );
        }
        existing_database(&self.inner.data, &job_id)?;
        let next = read_job_snapshot(&self.inner.data, &job_id);
        save_pointer(&self.inner.data, &job_id)?;
        *cached = next;
        cached.payload()
    }
    pub fn create_job(&self, setup: JobSetup) -> Result<Value> {
        self.launch(Work::Create(setup))
    }
    /// Drafts intentionally accept incomplete wizard values. Sources are not touched.
    pub fn save_draft(&self, setup: JobSetup) -> Result<()> {
        let gate = self.inner.snapshot.lock().map_err(err)?;
        if gate.control.worker.is_some() {
            return Err(
                "Wait for the current operation to finish before saving a new-job draft.".into(),
            );
        }
        validate_setup_size(&setup)?;
        atomic_write(
            &self.inner.data.join("new-job-draft.json"),
            &serde_json::to_vec(&setup).map_err(err)?,
        )
    }
    pub fn draft(&self) -> Result<Option<JobSetup>> {
        let _gate = self.inner.snapshot.lock().map_err(err)?;
        read_json(&self.inner.data.join("new-job-draft.json"))
    }
    pub fn bootstrap(&self) -> Result<Value> {
        let path = self.inner.data.join("demo.json");
        let manifest = match fs::read(&path) {
            Ok(bytes) => serde_json::from_slice::<Manifest>(&bytes)
                .map(Some)
                .map_err(err),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(err(e)),
        };
        let (manifest, configuration_error) = match manifest {
            Ok(manifest) => (manifest, None),
            Err(message) => (
                None,
                Some(format!(
                    "Demo configuration unavailable: {message}. Choose local folders to begin."
                )),
            ),
        };
        let manifest = manifest.unwrap_or_else(|| Manifest {
            title: "Folderloom".into(),
            description: "Select local source folders to inventory and compare.".into(),
            roots: vec![],
        });
        Ok(
            json!({"manifest":manifest,"manifest_path":display_path(&path),"configuration_error":configuration_error,"snapshot":self.reopen()?}),
        )
    }
    /// Reopen durable state only while idle. The same gate reserves worker starts.
    pub fn reopen(&self) -> Result<Value> {
        let mut cached = self.inner.snapshot.lock().map_err(err)?;
        if cached.control.worker.is_none() {
            *cached = read_saved(&self.inner.data);
        }
        cached.payload()
    }
    pub fn snapshot(&self) -> Result<Value> {
        self.inner.snapshot.lock().map_err(err)?.payload()
    }
    /// Start a deliberately selected job. All filesystem work happens on the worker.
    pub fn start(&self, roots: Vec<String>) -> Result<Value> {
        if roots.is_empty() {
            return Err("Select at least one source folder.".into());
        }
        self.launch(Work::Scan(Some(roots)))
    }
    pub fn run_job(&self, mode: &str) -> Result<Value> {
        let mode = match mode {
            "scan" => Work::Scan(None),
            "compare" => Work::Compare,
            _ => return Err("Unknown job operation.".into()),
        };
        self.launch(mode)
    }
    pub fn plan(&self, destination: String, mappings: Vec<MappingInput>) -> Result<Value> {
        self.launch(Work::Plan {
            destination,
            mappings,
        })
    }
    pub fn decide(&self, revision: i64, decisions: Vec<PlanDecision>) -> Result<Value> {
        self.edit_plan(revision, Some(decisions))
    }
    pub fn approve(&self, revision: i64) -> Result<Value> {
        self.edit_plan(revision, None)
    }
    fn edit_plan(&self, revision: i64, decisions: Option<Vec<PlanDecision>>) -> Result<Value> {
        let mut cached = self.inner.snapshot.lock().map_err(err)?;
        if cached.control.worker.is_some() {
            return Err("Wait for the current operation to finish before editing the plan.".into());
        }
        let id = cached
            .control
            .job_id
            .as_deref()
            .ok_or("Start a scan first.")?;
        let database = existing_database(&self.inner.data, id)?;
        match decisions {
            Some(decisions) => Job::update_plan(&database, revision, &decisions),
            None => Job::approve_plan(&database, revision),
        }
        .map_err(err)?;
        let job = Job::open(&database).map_err(err)?;
        *cached = job_snapshot(&job, id, &self.inner.data)?;
        cached.payload()
    }
    pub fn execute(&self, revision: i64) -> Result<Value> {
        self.launch(Work::Execute(revision))
    }
    pub fn destination(&self) -> Result<PathBuf> {
        let cached = self.inner.snapshot.lock().map_err(err)?;
        let id = cached.control.job_id.as_deref().ok_or("No saved job.")?;
        let job = Job::open(&existing_database(&self.inner.data, id)?).map_err(err)?;
        if let Some(plan) = job.consolidation_plan().map_err(err)? {
            return Ok(plan.destination);
        }
        cached
            .setup
            .as_ref()
            .map(|s| PathBuf::from(&s.destination))
            .ok_or_else(|| "No primary destination is saved.".into())
    }
    pub fn pause(&self) -> Result<Value> {
        let mut snapshot = self.inner.snapshot.lock().map_err(err)?;
        if snapshot.control.worker.is_some() {
            self.inner.pause.store(true, Ordering::Release);
            snapshot.control.pause_requested = true;
        }
        snapshot.payload()
    }
    fn launch(&self, work: Work) -> Result<Value> {
        let mut snapshot = self.inner.snapshot.lock().map_err(err)?;
        if snapshot.control.worker.is_some() {
            return Err(
                "A job is already running. Pause it before starting another operation.".into(),
            );
        }
        let work = match work {
            Work::Create(setup) => Work::Create(normalize_setup(setup, &self.inner.data)?),
            other => other,
        };
        let mode = work.mode();
        let creating = matches!(&work, Work::Create(_) | Work::Scan(Some(_)));
        let id = if creating {
            format!(
                "{}-{}",
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_err(err)?
                    .as_nanos(),
                std::process::id()
            )
        } else {
            snapshot
                .control
                .job_id
                .as_deref()
                .ok_or("Start a scan before resuming or comparing.")?
                .to_owned()
        };
        if creating {
            *snapshot = empty_snapshot();
            snapshot.control.job_id = Some(id.clone());
            if let Work::Create(setup) = &work {
                snapshot.setup = Some(setup.clone());
            }
        } else {
            existing_database(&self.inner.data, &id)?;
        }
        self.inner.pause.store(false, Ordering::Release);
        snapshot.control.worker = Some(mode);
        snapshot.control.pause_requested = false;
        snapshot.control.error = None;
        let initial = snapshot.payload()?;
        let app = self.clone();
        // Reserve the worker under the mutex before spawn; concurrent IPC cannot admit a second writer.
        let spawned =
            std::thread::Builder::new()
                .name("folderloom-job".into())
                .spawn(move || {
                    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        app.work(&id, work)
                    }))
                    .unwrap_or_else(|_| {
                        Err("Worker stopped unexpectedly. Reopen and resume the saved job.".into())
                    });
                    if let Ok(mut state) = app.inner.snapshot.lock() {
                        state.control.worker = None;
                        state.control.pause_requested = false;
                        if let Err(message) = outcome {
                            state.control.error = Some(message);
                        }
                    }
                });
        if let Err(e) = spawned {
            snapshot.control.worker = None;
            return Err(err(e));
        }
        Ok(initial)
    }
    fn publish(&self, job: &Job, id: &str, mode: WorkerMode) -> Result<()> {
        let mut next = job_snapshot(job, id, &self.inner.data)?;
        next.control.worker = Some(mode);
        next.control.pause_requested = self.inner.pause.load(Ordering::Acquire);
        *self.inner.snapshot.lock().map_err(err)? = next;
        Ok(())
    }
    fn work(&self, id: &str, work: Work) -> Result<()> {
        let mode = work.mode();
        validate_jobs_directory(&self.inner.data)?;
        valid_id(id)?;
        let database = self.inner.data.join("jobs").join(format!("{id}.sqlite"));
        if !matches!(&work, Work::Create(_) | Work::Scan(Some(_))) {
            existing_database(&self.inner.data, id)?;
        }
        let mut job = match work {
            Work::Create(setup) => {
                let roots: Vec<_> = setup
                    .sources
                    .iter()
                    .map(|source| PathBuf::from(&source.path))
                    .collect();
                let job = Job::create(&database, &roots, Options::default()).map_err(err)?;
                let metadata = JobMetadata {
                    setup,
                    created_ms: now_ms()?,
                };
                atomic_write(
                    &metadata_path(&self.inner.data, id),
                    &serde_json::to_vec(&metadata).map_err(err)?,
                )?;
                save_pointer(&self.inner.data, id)?;
                match fs::remove_file(self.inner.data.join("new-job-draft.json")) {
                    Ok(()) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => return Err(err(e)),
                }
                job
            }
            Work::Scan(Some(roots)) => {
                let roots: Vec<_> = roots.into_iter().map(PathBuf::from).collect();
                let job = Job::create(&database, &roots, Options::default()).map_err(err)?;
                // The core performs source/database safety checks before any saved selection is published.
                save_pointer(&self.inner.data, id)?;
                job
            }
            Work::Scan(None) => Job::resume(&database).map_err(err)?,
            Work::Compare => Job::compare(&database).map_err(err)?,
            Work::Plan {
                destination,
                mappings,
            } => {
                let saved = Job::open(&database).map_err(err)?;
                let entries = saved.inventory().map_err(err)?;
                let mappings = mappings
                    .into_iter()
                    .map(|input| {
                        let entry = entries
                            .iter()
                            .find(|entry| {
                                key(entry.root_id, &entry.relative_path) == input.source_key
                            })
                            .ok_or("Selected folder is not in the saved inventory.")?;
                        Ok(Mapping {
                            root_id: entry.root_id,
                            source_relative: entry.relative_path.clone(),
                            destination_relative: PathBuf::from(input.destination_relative),
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;
                drop(saved);
                Job::plan(&database, Path::new(&destination), &mappings).map_err(err)?
            }
            Work::Execute(revision) => Job::execute(&database, revision).map_err(err)?,
        };
        self.publish(&job, id, mode)?;
        if matches!(mode, WorkerMode::Plan) {
            return Ok(());
        }
        loop {
            if self.inner.pause.load(Ordering::Acquire) {
                match mode {
                    WorkerMode::Scan => {
                        job.pause().map_err(err)?;
                    }
                    WorkerMode::Compare => {
                        job.pause_comparison().map_err(err)?;
                    }
                    WorkerMode::Execute => {
                        job.pause_execution().map_err(err)?;
                    }
                    WorkerMode::Plan => {}
                }
                self.publish(&job, id, mode)?;
                break;
            }
            let running = match mode {
                WorkerMode::Scan => job.step(128).map_err(err)?.state == State::Running,
                WorkerMode::Compare => {
                    job.compare_step(1024 * 1024).map_err(err)?.state == ComparisonState::Running
                }
                WorkerMode::Execute => job.execute_step(1).map_err(err)?.state == "running",
                WorkerMode::Plan => false,
            };
            self.publish(&job, id, mode)?;
            if !running {
                break;
            }
        }
        Ok(())
    }
}

fn now_ms() -> Result<u64> {
    u64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(err)?
            .as_millis(),
    )
    .map_err(err)
}
fn millis(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|t| u64::try_from(t.as_millis()).ok())
        .unwrap_or(0)
}
fn validate_jobs_directory(data: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(data.join("jobs")).map_err(err)?;
    if !metadata.is_dir() {
        return Err("Saved jobs directory must be a real directory, not a link.".into());
    }
    Ok(())
}
fn existing_database(data: &Path, id: &str) -> Result<PathBuf> {
    valid_id(id)?;
    validate_jobs_directory(data)?;
    let database = data.join("jobs").join(format!("{id}.sqlite"));
    let metadata = fs::symlink_metadata(&database).map_err(err)?;
    if !metadata.is_file() || metadata.nlink() != 1 {
        return Err("Saved job database must be a regular file without link aliases.".into());
    }
    Ok(database)
}
fn read_regular(path: &Path) -> std::io::Result<Vec<u8>> {
    let before = fs::symlink_metadata(path)?;
    if !before.is_file() || before.nlink() != 1 || before.len() > 1024 * 1024 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "saved settings must be a regular file without link aliases and under 1 MiB",
        ));
    }
    let mut file = fs::File::open(path)?;
    let actual = file.metadata()?;
    if before.dev() != actual.dev() || before.ino() != actual.ino() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "saved settings changed while opening",
        ));
    }
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    Ok(bytes)
}
fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<Option<T>> {
    match read_regular(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).map(Some).map_err(err),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(err(error)),
    }
}
fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().ok_or("Settings need a parent directory.")?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("Invalid settings filename.")?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(err)?
        .as_nanos();
    let temporary = parent.join(format!(".{name}.{stamp}-{}.tmp", std::process::id()));
    let result = (|| -> Result<()> {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(err)?;
        file.write_all(bytes).map_err(err)?;
        file.sync_all().map_err(err)?;
        fs::rename(&temporary, path).map_err(err)?;
        fs::File::open(parent)
            .and_then(|dir| dir.sync_all())
            .map_err(err)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}
fn save_pointer(data: &Path, id: &str) -> Result<()> {
    valid_id(id)?;
    atomic_write(&data.join("current-job"), id.as_bytes())
}
fn metadata_path(data: &Path, id: &str) -> PathBuf {
    data.join("jobs").join(format!("{id}.json"))
}
fn validate_setup_size(setup: &JobSetup) -> Result<()> {
    if setup.name.len() > 256
        || setup.sources.len() > 512
        || setup.destination.len() > 16384
        || setup
            .sources
            .iter()
            .any(|s| s.path.len() > 16384 || s.label.len() > 1024)
        || serde_json::to_vec(setup).map_err(err)?.len() > 1024 * 1024
    {
        return Err("Job setup is too large.".into());
    }
    Ok(())
}
fn normalize_setup(mut setup: JobSetup, data: &Path) -> Result<JobSetup> {
    validate_setup_size(&setup)?;
    setup.name = setup.name.trim().to_owned();
    if setup.name.is_empty() {
        return Err("Name this job before starting the scan.".into());
    }
    if setup.destination.is_empty() {
        return Err("Choose an existing primary destination before scanning.".into());
    }
    let destination = fs::canonicalize(&setup.destination)
        .map_err(|e| format!("Primary destination is unavailable: {e}"))?;
    if !destination.is_dir() {
        return Err("Primary destination must be an existing folder.".into());
    }
    if data.starts_with(&destination) || destination.starts_with(data.join("jobs")) {
        return Err(
            "Primary destination must be separate from Folderloom's saved-job storage.".into(),
        );
    }
    let mut sources = Vec::<JobSource>::new();
    let mut paths = Vec::<PathBuf>::new();
    for source in setup.sources {
        let path = fs::canonicalize(&source.path)
            .map_err(|e| format!("Source folder is unavailable: {e}"))?;
        if !path.is_dir() {
            return Err("Every source must be an existing folder.".into());
        }
        if path == destination {
            continue;
        }
        if path.starts_with(&destination) || destination.starts_with(&path) {
            return Err("Primary destination and incoming source folders must not overlap.".into());
        }
        if data.starts_with(&path) || path.starts_with(data.join("jobs")) {
            return Err("Sources must be separate from Folderloom's saved-job storage.".into());
        }
        if paths.contains(&path) {
            continue;
        }
        if paths
            .iter()
            .any(|other| path.starts_with(other) || other.starts_with(&path))
        {
            return Err("Incoming source folders must not overlap each other.".into());
        }
        let label = if source.label.trim().is_empty() {
            path.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "Source".into())
        } else {
            source.label.trim().to_owned()
        };
        sources.push(JobSource {
            label,
            path: path
                .to_str()
                .ok_or("Selected source path cannot be represented by the folder picker.")?
                .into(),
        });
        paths.push(path);
    }
    if sources.is_empty() {
        return Err(
            "Choose at least one incoming source other than the primary destination.".into(),
        );
    }
    setup.sources = sources;
    setup.destination = destination
        .to_str()
        .ok_or("Selected destination path cannot be represented by the folder picker.")?
        .into();
    Ok(setup)
}
fn derived_setup(job: &Job, id: &str, destination: Option<&Path>) -> Result<Option<JobSetup>> {
    let roots = job.roots().map_err(err)?;
    let mut sources = Vec::new();
    for root in roots {
        let Some(path) = root.path.to_str() else {
            return Ok(None);
        };
        sources.push(JobSource {
            label: root
                .path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "Source".into()),
            path: path.into(),
        });
    }
    let destination = match destination {
        Some(path) => match path.to_str() {
            Some(path) => path.into(),
            None => return Ok(None),
        },
        None => String::new(),
    };
    Ok(Some(JobSetup {
        name: format!("Scan {id}"),
        sources,
        destination,
    }))
}
fn job_summary(data: &Path, id: &str, active: bool) -> JobSummary {
    let database = data.join("jobs").join(format!("{id}.sqlite"));
    let fs_metadata = fs::symlink_metadata(&database).ok();
    let created_ms = id
        .split('-')
        .next()
        .and_then(|part| part.parse::<u128>().ok())
        .and_then(|n| u64::try_from(n / 1_000_000).ok())
        .filter(|n| *n > 0)
        .or_else(|| {
            fs_metadata
                .as_ref()
                .and_then(|m| m.created().ok())
                .map(millis)
        })
        .unwrap_or(0);
    let updated_ms = fs_metadata
        .as_ref()
        .and_then(|m| m.modified().ok())
        .map(millis)
        .unwrap_or(created_ms);
    let mut row = JobSummary {
        id: id.into(),
        name: format!("Scan {id}"),
        created_ms,
        updated_ms,
        source_count: 0,
        entry_count: 0,
        state: "unavailable".into(),
        stage: "inventory".into(),
        destination: None,
        error: None,
        active,
    };
    match read_json::<JobMetadata>(&metadata_path(data, id)) {
        Ok(Some(metadata)) => {
            row.name = metadata.setup.name;
            row.source_count = metadata.setup.sources.len();
            row.destination = Some(metadata.setup.destination);
            row.created_ms = metadata.created_ms;
        }
        Ok(None) => {}
        Err(message) => row.error = Some(format!("Saved job setup could not be read: {message}")),
    }
    let inspection = (|| -> Result<()> {
        let job = Job::open(&existing_database(data, id)?).map_err(err)?;
        let progress = job.progress().map_err(err)?;
        row.state = state(progress.state);
        row.entry_count = progress.entries;
        let roots = job.roots().map_err(err)?;
        row.source_count = roots.len();
        if row.name == format!("Scan {id}") {
            let names: Vec<_> = roots
                .iter()
                .filter_map(|r| r.path.file_name())
                .map(|n| n.to_string_lossy().into_owned())
                .collect();
            if !names.is_empty() {
                row.name = format!("Scan — {}", names.join(", "));
            }
        }
        if let Some(plan) = job.consolidation_plan().map_err(err)? {
            row.stage = if plan.generation != progress.generation
                || matches!(plan.state.as_str(), "proposed" | "approved")
            {
                "plan"
            } else {
                "results"
            }
            .into();
            row.state = if plan.generation != progress.generation {
                "needs_replan".into()
            } else {
                plan.state
            };
            row.destination = Some(display_path(&plan.destination));
        } else {
            let comparison = job.comparison_progress().map_err(err)?;
            row.state = match comparison.state {
                ComparisonState::Running => "comparison_running".into(),
                ComparisonState::Paused => "comparison_paused".into(),
                ComparisonState::FinishedWithErrors => "comparison_finished_with_errors".into(),
                ComparisonState::Complete => "comparison_complete".into(),
                _ => row.state.clone(),
            };
        }
        Ok(())
    })();
    if let Err(message) = inspection {
        row.error = Some(format!("Saved job could not be read: {message}"));
    }
    row
}

fn read_job_snapshot(data: &Path, id: &str) -> Snapshot {
    let saved = existing_database(data, id)
        .and_then(|database| Job::open(&database).map_err(err))
        .and_then(|job| job_snapshot(&job, id, data));
    match saved {
        Ok(snapshot) => snapshot,
        Err(message) => {
            let mut snapshot = empty_snapshot();
            snapshot.control.job_id = Some(id.to_owned());
            snapshot.control.recovery_required = true;
            snapshot.control.error = Some(format!("Saved job could not be reopened: {message}"));
            snapshot.setup = read_json::<JobMetadata>(&metadata_path(data, id))
                .ok()
                .flatten()
                .map(|m| m.setup);
            snapshot
        }
    }
}
fn read_saved(data: &Path) -> Snapshot {
    match read_regular(&data.join("current-job")).and_then(|bytes| {
        String::from_utf8(bytes)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }) {
        Ok(id) => match valid_id(id.trim()) {
            Ok(id) => read_job_snapshot(data, id),
            Err(message) => {
                let mut snapshot = empty_snapshot();
                snapshot.control.error = Some(message);
                snapshot
            }
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => empty_snapshot(),
        Err(e) => {
            let mut snapshot = empty_snapshot();
            snapshot.control.error = Some(format!("Saved job pointer could not be read: {e}"));
            snapshot
        }
    }
}
fn valid_id(id: &str) -> Result<&str> {
    if id.is_empty() || id.len() > 100 || !id.bytes().all(|b| b.is_ascii_digit() || b == b'-') {
        Err("Invalid saved job identifier.".into())
    } else {
        Ok(id)
    }
}
fn empty_snapshot() -> Snapshot {
    Snapshot {
        setup: None,
        control: Control::default(),
        observations: json!({"roots":[],"inventory_progress":null,"comparison_progress":null,"entries":[],"comparisons":[],"failures":[],"root_failures":[],"plan":null}),
    }
}
fn state(value: impl std::fmt::Debug) -> String {
    let value = format!("{value:?}");
    let mut result = String::new();
    for (i, c) in value.chars().enumerate() {
        if i > 0 && c.is_uppercase() {
            result.push('_');
        }
        result.extend(c.to_lowercase());
    }
    result
}
fn key(root: i64, path: &Path) -> String {
    let mut key = format!("{root}:");
    for byte in path.as_os_str().as_bytes() {
        use std::fmt::Write;
        write!(key, "{byte:02x}").expect("string write");
    }
    key
}
// Escapes are display-only. Native path bytes remain distinct in opaque placement keys.
fn display_path(path: &Path) -> String {
    let bytes = path.as_os_str().as_bytes();
    let mut result = String::new();
    let mut remaining = bytes;
    while !remaining.is_empty() {
        match std::str::from_utf8(remaining) {
            Ok(text) => {
                result.push_str(&text.replace('\\', "\\\\"));
                break;
            }
            Err(error) => {
                let valid = error.valid_up_to();
                result.push_str(
                    &std::str::from_utf8(&remaining[..valid])
                        .expect("valid prefix")
                        .replace('\\', "\\\\"),
                );
                let invalid = error.error_len().unwrap_or(remaining.len() - valid);
                for byte in &remaining[valid..valid + invalid] {
                    use std::fmt::Write;
                    write!(result, "\\x{byte:02x}").expect("string write");
                }
                remaining = &remaining[valid + invalid..];
            }
        }
    }
    result
}
fn job_snapshot(job: &Job, id: &str, data: &Path) -> Result<Snapshot> {
    let p = job.progress().map_err(err)?;
    let c = job.comparison_progress().map_err(err)?;
    let plan = job.consolidation_plan().map_err(err)?.map(|plan| json!({
        "id":plan.id,"revision":plan.revision,"approved_revision":plan.approved_revision,
        "destination":display_path(&plan.destination),"destination_input":plan.destination.to_str(),
        "generation":plan.generation,"state":plan.state,
        "mappings":plan.mappings.iter().map(|m|json!({"root_id":m.root_id,"source_key":key(m.root_id,&m.source_relative),"source_relative":display_path(&m.source_relative),"destination_relative":display_path(&m.destination_relative),"destination_input":m.destination_relative.to_str()})).collect::<Vec<_>>(),
        "items":plan.items.iter().map(|i|json!({"id":i.id,"root_id":i.root_id,"source_key":key(i.root_id,&i.source_relative),"source_relative":display_path(&i.source_relative),"destination_relative":display_path(&i.destination_relative),"action":i.action,"included":i.included,"status":i.status,"detail":i.detail,"hash":i.hash,"contributors":i.contributors.iter().map(|c|json!({"root_id":c.root_id,"source_relative":display_path(&c.source_relative)})).collect::<Vec<_>>()})).collect::<Vec<_>>()
    }));
    let metadata = read_json::<JobMetadata>(&metadata_path(data, id));
    let metadata_error = metadata
        .as_ref()
        .err()
        .map(|message| format!("Saved job setup could not be read: {message}"));
    let setup = match metadata.ok().flatten() {
        Some(metadata) => Some(metadata.setup),
        None if plan
            .as_ref()
            .is_some_and(|p| p["destination_input"].is_null()) =>
        {
            None
        }
        None => {
            let destination = plan
                .as_ref()
                .and_then(|p| p["destination_input"].as_str())
                .map(Path::new);
            derived_setup(job, id, destination)?
        }
    };
    Ok(Snapshot {
        setup,
        control: Control {
            job_id: Some(id.to_owned()),
            error: metadata_error,
            ..Control::default()
        },
        observations: json!({
        "plan":plan,
        "roots":job.roots().map_err(err)?.into_iter().map(|r|json!({"id":r.id,"path":display_path(&r.path),"input_path":r.path.to_str()})).collect::<Vec<_>>(),
        "inventory_progress":{"state":state(p.state),"generation":p.generation,"entries":p.entries,"stale_entries":p.stale_entries,"pending_directories":p.pending_directories,"failed_directories":p.failed_directories,"failures":p.failures},
        "comparison_progress":{"state":state(c.state),"generation":c.generation,"files":c.files,"pending_files":c.pending_files,"full_hashes":c.full_hashes,"unique_sizes":c.unique_sizes,"unique_samples":c.unique_samples,"failures":c.failures,"root_failures":c.root_failures,"bytes_read":c.bytes_read},
        "entries":job.inventory().map_err(err)?.into_iter().map(|e|json!({"key":key(e.root_id,&e.relative_path),"parent_key":e.relative_path.parent().map(|parent|key(e.root_id,parent)),"root_id":e.root_id,"relative_path":display_path(&e.relative_path),"entry_type":state(e.entry_type),"size":e.size,"modified_ns":e.modified_ns,"link_target":e.link_target.as_deref().map(display_path),"excluded":e.excluded,"stale":e.stale,"directory_complete":e.directory_complete})).collect::<Vec<_>>(),
        "comparisons":job.comparisons().map_err(err)?.into_iter().map(|e|json!({"key":key(e.root_id,&e.relative_path),"root_id":e.root_id,"relative_path":display_path(&e.relative_path),"generation":e.generation,"size":e.size,"stage":state(e.stage),"algorithm":e.algorithm,"sample_format":e.sample_format,"sample_hash":e.sample_hash,"full_hash":e.full_hash,"bytes_read":e.bytes_read,"failure":e.failure,"stale":e.stale})).collect::<Vec<_>>(),
        "failures":job.failures().map_err(err)?.into_iter().map(|f|json!({"root_id":f.root_id,"relative_path":display_path(&f.relative_path),"message":f.message,"generation":f.generation})).collect::<Vec<_>>(),
        "root_failures":job.comparison_root_failures().map_err(err)?.into_iter().map(|f|json!({"root_id":f.root_id,"generation":f.generation,"message":f.message,"stale":f.stale})).collect::<Vec<_>>()
        }),
    })
}

#[cfg(feature = "desktop")]
mod shell {
    use super::*;
    use tauri::Manager;
    #[tauri::command]
    async fn desktop_jobs(app: tauri::State<'_, Desktop>) -> Result<Vec<JobSummary>> {
        let app = app.inner().clone();
        tauri::async_runtime::spawn_blocking(move || app.jobs())
            .await
            .map_err(err)?
    }
    #[tauri::command]
    async fn desktop_open_job(app: tauri::State<'_, Desktop>, job_id: String) -> Result<Value> {
        let app = app.inner().clone();
        tauri::async_runtime::spawn_blocking(move || app.open_job(job_id))
            .await
            .map_err(err)?
    }
    #[tauri::command]
    async fn desktop_create_job(app: tauri::State<'_, Desktop>, setup: JobSetup) -> Result<Value> {
        let app = app.inner().clone();
        tauri::async_runtime::spawn_blocking(move || app.create_job(setup))
            .await
            .map_err(err)?
    }
    #[tauri::command]
    async fn desktop_save_draft(app: tauri::State<'_, Desktop>, setup: JobSetup) -> Result<()> {
        let app = app.inner().clone();
        tauri::async_runtime::spawn_blocking(move || app.save_draft(setup))
            .await
            .map_err(err)?
    }
    #[tauri::command]
    async fn desktop_draft(app: tauri::State<'_, Desktop>) -> Result<Option<JobSetup>> {
        let app = app.inner().clone();
        tauri::async_runtime::spawn_blocking(move || app.draft())
            .await
            .map_err(err)?
    }
    #[tauri::command]
    async fn desktop_bootstrap(app: tauri::State<'_, Desktop>) -> Result<Value> {
        let app = app.inner().clone();
        tauri::async_runtime::spawn_blocking(move || app.bootstrap())
            .await
            .map_err(err)?
    }
    #[tauri::command]
    async fn desktop_snapshot(app: tauri::State<'_, Desktop>) -> Result<Value> {
        app.snapshot()
    }
    #[tauri::command]
    async fn desktop_start(
        app: tauri::State<'_, Desktop>,
        root_paths: Vec<String>,
    ) -> Result<Value> {
        app.start(root_paths)
    }
    #[tauri::command]
    async fn desktop_run(app: tauri::State<'_, Desktop>, mode: String) -> Result<Value> {
        app.run_job(&mode)
    }
    #[tauri::command]
    async fn desktop_pause(app: tauri::State<'_, Desktop>) -> Result<Value> {
        app.pause()
    }
    #[tauri::command]
    async fn desktop_plan(
        app: tauri::State<'_, Desktop>,
        destination: String,
        mappings: Vec<MappingInput>,
    ) -> Result<Value> {
        app.plan(destination, mappings)
    }
    #[tauri::command]
    async fn desktop_decide(
        app: tauri::State<'_, Desktop>,
        revision: i64,
        decisions: Vec<PlanDecision>,
    ) -> Result<Value> {
        let app = app.inner().clone();
        tauri::async_runtime::spawn_blocking(move || app.decide(revision, decisions))
            .await
            .map_err(err)?
    }
    #[tauri::command]
    async fn desktop_approve(app: tauri::State<'_, Desktop>, revision: i64) -> Result<Value> {
        let app = app.inner().clone();
        tauri::async_runtime::spawn_blocking(move || app.approve(revision))
            .await
            .map_err(err)?
    }
    #[tauri::command]
    async fn desktop_execute(app: tauri::State<'_, Desktop>, revision: i64) -> Result<Value> {
        app.execute(revision)
    }
    #[tauri::command]
    async fn desktop_open_destination(app: tauri::State<'_, Desktop>) -> Result<()> {
        let path = app.destination()?;
        #[cfg(target_os = "macos")]
        let command = "open";
        #[cfg(not(target_os = "macos"))]
        let command = "xdg-open";
        std::process::Command::new(command)
            .arg(path)
            .spawn()
            .map_err(err)?;
        Ok(())
    }
    pub fn run() {
        tauri::Builder::default()
            .plugin(tauri_plugin_dialog::init())
            .setup(|app| {
                let data = app.path().app_data_dir()?;
                app.manage(Desktop::open(data).map_err(std::io::Error::other)?);
                Ok(())
            })
            .invoke_handler(tauri::generate_handler![
                desktop_bootstrap,
                desktop_jobs,
                desktop_open_job,
                desktop_create_job,
                desktop_save_draft,
                desktop_draft,
                desktop_snapshot,
                desktop_start,
                desktop_run,
                desktop_pause,
                desktop_plan,
                desktop_decide,
                desktop_approve,
                desktop_execute,
                desktop_open_destination
            ])
            .run(tauri::generate_context!())
            .expect("Folderloom desktop could not start");
    }
}
#[cfg(feature = "desktop")]
pub use shell::run;
