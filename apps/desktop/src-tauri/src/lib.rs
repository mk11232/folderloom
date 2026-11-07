//! Desktop adapter: one background writer, saved checkpoints, and read-only snapshots.
use folderloom_core::{ComparisonState, Job, Mapping, Options, PlanDecision, State};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    fs,
    os::unix::ffi::OsStrExt,
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
            Self::Scan(_) => WorkerMode::Scan,
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

impl Desktop {
    pub fn open(data: PathBuf) -> Result<Self> {
        fs::create_dir_all(data.join("jobs")).map_err(err)?;
        let snapshot = read_saved(&data);
        Ok(Self {
            inner: Arc::new(Inner {
                data,
                snapshot: Mutex::new(snapshot),
                pause: AtomicBool::new(false),
            }),
        })
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
        let database = self.inner.data.join("jobs").join(format!("{id}.sqlite"));
        match decisions {
            Some(decisions) => Job::update_plan(&database, revision, &decisions),
            None => Job::approve_plan(&database, revision),
        }
        .map_err(err)?;
        let job = Job::open(&database).map_err(err)?;
        *cached = job_snapshot(&job, id)?;
        cached.payload()
    }
    pub fn execute(&self, revision: i64) -> Result<Value> {
        self.launch(Work::Execute(revision))
    }
    pub fn destination(&self) -> Result<PathBuf> {
        let cached = self.inner.snapshot.lock().map_err(err)?;
        let id = cached.control.job_id.as_deref().ok_or("No saved job.")?;
        let job =
            Job::open(&self.inner.data.join("jobs").join(format!("{id}.sqlite"))).map_err(err)?;
        Ok(job
            .consolidation_plan()
            .map_err(err)?
            .ok_or("No saved merge plan.")?
            .destination)
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
        let mode = work.mode();
        let id = if matches!(&work, Work::Scan(Some(_))) {
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
        let mut next = job_snapshot(job, id)?;
        next.control.worker = Some(mode);
        next.control.pause_requested = self.inner.pause.load(Ordering::Acquire);
        *self.inner.snapshot.lock().map_err(err)? = next;
        Ok(())
    }
    fn work(&self, id: &str, work: Work) -> Result<()> {
        let mode = work.mode();
        let database = self.inner.data.join("jobs").join(format!("{id}.sqlite"));
        let mut job = match work {
            Work::Scan(Some(roots)) => {
                let roots: Vec<_> = roots.into_iter().map(PathBuf::from).collect();
                let job = Job::create(&database, &roots, Options::default()).map_err(err)?;
                // The core performs source/database safety checks before any saved selection is published.
                fs::write(self.inner.data.join("current-job.next"), id).map_err(err)?;
                fs::rename(
                    self.inner.data.join("current-job.next"),
                    self.inner.data.join("current-job"),
                )
                .map_err(err)?;
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
fn read_saved(data: &Path) -> Snapshot {
    let mut snapshot = empty_snapshot();
    match fs::read_to_string(data.join("current-job")) {
        Ok(id) => match valid_id(id.trim()).and_then(|id| {
            // Inspection is read-only and may fail while SQLite needs writer recovery.
            // Keep only a validated pointer so an explicit Resume can use core guards.
            snapshot.control.job_id = Some(id.to_owned());
            snapshot.control.recovery_required = true;
            let job = Job::open(&data.join("jobs").join(format!("{id}.sqlite"))).map_err(err)?;
            job_snapshot(&job, id)
        }) {
            Ok(saved) => snapshot = saved,
            Err(message) => {
                snapshot.control.error = Some(format!("Saved job could not be reopened: {message}"))
            }
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => {
            snapshot.control.error = Some(format!("Saved job pointer could not be read: {e}"))
        }
    }
    snapshot
}
fn valid_id(id: &str) -> Result<&str> {
    if id.is_empty() || !id.bytes().all(|b| b.is_ascii_digit() || b == b'-') {
        Err("Invalid saved job identifier.".into())
    } else {
        Ok(id)
    }
}
fn empty_snapshot() -> Snapshot {
    Snapshot {
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
fn job_snapshot(job: &Job, id: &str) -> Result<Snapshot> {
    let p = job.progress().map_err(err)?;
    let c = job.comparison_progress().map_err(err)?;
    let plan = job.consolidation_plan().map_err(err)?.map(|plan| json!({
        "id":plan.id,"revision":plan.revision,"approved_revision":plan.approved_revision,
        "destination":display_path(&plan.destination),"destination_input":plan.destination.to_str(),
        "generation":plan.generation,"state":plan.state,
        "mappings":plan.mappings.iter().map(|m|json!({"root_id":m.root_id,"source_key":key(m.root_id,&m.source_relative),"source_relative":display_path(&m.source_relative),"destination_relative":display_path(&m.destination_relative),"destination_input":m.destination_relative.to_str()})).collect::<Vec<_>>(),
        "items":plan.items.iter().map(|i|json!({"id":i.id,"root_id":i.root_id,"source_key":key(i.root_id,&i.source_relative),"source_relative":display_path(&i.source_relative),"destination_relative":display_path(&i.destination_relative),"action":i.action,"included":i.included,"status":i.status,"detail":i.detail,"hash":i.hash,"contributors":i.contributors.iter().map(|c|json!({"root_id":c.root_id,"source_relative":display_path(&c.source_relative)})).collect::<Vec<_>>()})).collect::<Vec<_>>()
    }));
    Ok(Snapshot {
        control: Control {
            job_id: Some(id.to_owned()),
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
