//! Desktop adapter: one background writer, saved checkpoints, and read-only snapshots.
use folderloom_core::{ComparisonState, Job, Options, State};
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
    snapshot: Mutex<Value>,
    pause: AtomicBool,
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
        let manifest = fs::read(&path)
            .map_err(err)
            .and_then(|bytes| serde_json::from_slice::<Manifest>(&bytes).map_err(err));
        let (manifest, configuration_error) = match manifest {
            Ok(manifest) => (manifest, None),
            Err(message) => (
                Manifest {
                    title: "Folderloom".into(),
                    description: "Select local source folders to inventory and compare.".into(),
                    roots: vec![],
                },
                Some(format!(
                    "Demo configuration unavailable: {message}. Choose local folders to begin."
                )),
            ),
        };
        Ok(
            json!({"manifest":manifest,"manifest_path":display_path(&path),"configuration_error":configuration_error,"snapshot":self.reopen()?}),
        )
    }
    /// Reopen durable state only while idle. The same gate reserves worker starts.
    pub fn reopen(&self) -> Result<Value> {
        let mut cached = self.inner.snapshot.lock().map_err(err)?;
        if cached["worker"].is_null() {
            *cached = read_saved(&self.inner.data);
        }
        Ok(cached.clone())
    }
    pub fn snapshot(&self) -> Result<Value> {
        self.inner.snapshot.lock().map(|s| s.clone()).map_err(err)
    }
    /// Start a deliberately selected job. All filesystem work happens on the worker.
    pub fn start(&self, roots: Vec<String>) -> Result<Value> {
        if roots.is_empty() {
            return Err("Select at least one source folder.".into());
        }
        self.launch("scan", Some(roots))
    }
    pub fn run_job(&self, mode: &str) -> Result<Value> {
        if !matches!(mode, "scan" | "compare") {
            return Err("Unknown job operation.".into());
        }
        self.launch(mode, None)
    }
    pub fn pause(&self) -> Result<Value> {
        let mut snapshot = self.inner.snapshot.lock().map_err(err)?;
        if !snapshot["worker"].is_null() {
            self.inner.pause.store(true, Ordering::Release);
            snapshot["pause_requested"] = json!(true);
        }
        Ok(snapshot.clone())
    }
    fn launch(&self, mode: &str, roots: Option<Vec<String>>) -> Result<Value> {
        let mut snapshot = self.inner.snapshot.lock().map_err(err)?;
        if !snapshot["worker"].is_null() {
            return Err(
                "A job is already running. Pause it before starting another operation.".into(),
            );
        }
        let id = if roots.is_some() {
            format!(
                "{}-{}",
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_err(err)?
                    .as_nanos(),
                std::process::id()
            )
        } else {
            snapshot["job_id"]
                .as_str()
                .ok_or("Start a scan before resuming or comparing.")?
                .to_owned()
        };
        self.inner.pause.store(false, Ordering::Release);
        snapshot["worker"] = json!(mode);
        snapshot["pause_requested"] = json!(false);
        snapshot["error"] = Value::Null;
        let initial = snapshot.clone();
        let app = self.clone();
        let mode = mode.to_owned();
        // Reserve the worker under the mutex before spawn; concurrent IPC cannot admit a second writer.
        let spawned = std::thread::Builder::new()
            .name("folderloom-job".into())
            .spawn(move || {
                let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    app.work(&id, &mode, roots)
                }))
                .unwrap_or_else(|_| {
                    Err("Worker stopped unexpectedly. Reopen and resume the saved job.".into())
                });
                if let Ok(mut state) = app.inner.snapshot.lock() {
                    state["worker"] = Value::Null;
                    state["pause_requested"] = json!(false);
                    if let Err(message) = outcome {
                        state["error"] = json!(message);
                    }
                }
            });
        if let Err(e) = spawned {
            snapshot["worker"] = Value::Null;
            return Err(err(e));
        }
        Ok(initial)
    }
    fn publish(&self, job: &Job, id: &str, mode: &str) -> Result<()> {
        let mut next = job_snapshot(job, id)?;
        next["worker"] = json!(mode);
        next["pause_requested"] = json!(self.inner.pause.load(Ordering::Acquire));
        *self.inner.snapshot.lock().map_err(err)? = next;
        Ok(())
    }
    fn work(&self, id: &str, mode: &str, roots: Option<Vec<String>>) -> Result<()> {
        let database = self.inner.data.join("jobs").join(format!("{id}.sqlite"));
        let mut job = if let Some(roots) = roots {
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
        } else if mode == "scan" {
            Job::resume(&database).map_err(err)?
        } else {
            Job::compare(&database).map_err(err)?
        };
        self.publish(&job, id, mode)?;
        loop {
            if self.inner.pause.load(Ordering::Acquire) {
                if mode == "scan" {
                    job.pause().map_err(err)?;
                } else {
                    job.pause_comparison().map_err(err)?;
                }
                self.publish(&job, id, mode)?;
                break;
            }
            let running = if mode == "scan" {
                job.step(128).map_err(err)?.state == State::Running
            } else {
                job.compare_step(1024 * 1024).map_err(err)?.state == ComparisonState::Running
            };
            self.publish(&job, id, mode)?;
            if !running {
                break;
            }
        }
        Ok(())
    }
}
fn read_saved(data: &Path) -> Value {
    let mut snapshot = empty_snapshot();
    match fs::read_to_string(data.join("current-job")) {
        Ok(id) => match valid_id(id.trim()).and_then(|id| {
            let job = Job::open(&data.join("jobs").join(format!("{id}.sqlite"))).map_err(err)?;
            job_snapshot(&job, id)
        }) {
            Ok(saved) => snapshot = saved,
            Err(message) => {
                snapshot["error"] = json!(format!("Saved job could not be reopened: {message}"))
            }
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => snapshot["error"] = json!(format!("Saved job pointer could not be read: {e}")),
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
fn empty_snapshot() -> Value {
    json!({"job_id":null,"worker":null,"pause_requested":false,"error":null,"roots":[],"inventory_progress":null,"comparison_progress":null,"entries":[],"comparisons":[],"failures":[],"root_failures":[]})
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
fn job_snapshot(job: &Job, id: &str) -> Result<Value> {
    let p = job.progress().map_err(err)?;
    let c = job.comparison_progress().map_err(err)?;
    Ok(json!({
        "job_id":id,"worker":null,"pause_requested":false,"error":null,
        "roots":job.roots().map_err(err)?.into_iter().map(|r|json!({"id":r.id,"path":display_path(&r.path),"input_path":r.path.to_str()})).collect::<Vec<_>>(),
        "inventory_progress":{"state":state(p.state),"generation":p.generation,"entries":p.entries,"stale_entries":p.stale_entries,"pending_directories":p.pending_directories,"failed_directories":p.failed_directories,"failures":p.failures},
        "comparison_progress":{"state":state(c.state),"generation":c.generation,"files":c.files,"pending_files":c.pending_files,"full_hashes":c.full_hashes,"unique_sizes":c.unique_sizes,"unique_samples":c.unique_samples,"failures":c.failures,"root_failures":c.root_failures,"bytes_read":c.bytes_read},
        "entries":job.inventory().map_err(err)?.into_iter().map(|e|json!({"key":key(e.root_id,&e.relative_path),"root_id":e.root_id,"relative_path":display_path(&e.relative_path),"entry_type":state(e.entry_type),"size":e.size,"modified_ns":e.modified_ns,"link_target":e.link_target.as_deref().map(display_path),"excluded":e.excluded,"stale":e.stale,"directory_complete":e.directory_complete})).collect::<Vec<_>>(),
        "comparisons":job.comparisons().map_err(err)?.into_iter().map(|e|json!({"key":key(e.root_id,&e.relative_path),"root_id":e.root_id,"relative_path":display_path(&e.relative_path),"generation":e.generation,"size":e.size,"stage":state(e.stage),"algorithm":e.algorithm,"sample_format":e.sample_format,"sample_hash":e.sample_hash,"full_hash":e.full_hash,"bytes_read":e.bytes_read,"failure":e.failure,"stale":e.stale})).collect::<Vec<_>>(),
        "failures":job.failures().map_err(err)?.into_iter().map(|f|json!({"root_id":f.root_id,"relative_path":display_path(&f.relative_path),"message":f.message,"generation":f.generation})).collect::<Vec<_>>(),
        "root_failures":job.comparison_root_failures().map_err(err)?.into_iter().map(|f|json!({"root_id":f.root_id,"generation":f.generation,"message":f.message,"stale":f.stale})).collect::<Vec<_>>()
    }))
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
                desktop_pause
            ])
            .run(tauri::generate_context!())
            .expect("Folderloom desktop could not start");
    }
}
#[cfg(feature = "desktop")]
pub use shell::run;
