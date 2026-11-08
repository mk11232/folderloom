use folderloom_desktop::{Desktop, JobSetup, JobSource, MappingInput};
use serde_json::Value;
use std::{
    fs,
    path::PathBuf,
    thread,
    time::{Duration, Instant},
};

fn idle(app: &Desktop) -> Value {
    let until = Instant::now() + Duration::from_secs(20);
    loop {
        let snapshot = app.snapshot().unwrap();
        if snapshot["worker"].is_null() {
            return snapshot;
        }
        assert!(Instant::now() < until, "worker did not finish: {snapshot}");
        thread::sleep(Duration::from_millis(5));
    }
}
struct Fixture {
    _temp: tempfile::TempDir,
    data: PathBuf,
    source: PathBuf,
    destination: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let base = fs::canonicalize(temp.path()).unwrap();
        let source = base.join("incoming");
        let destination = base.join("primary");
        fs::create_dir(&source).unwrap();
        fs::create_dir(&destination).unwrap();
        fs::write(source.join("file.txt"), b"synthetic source").unwrap();
        Self {
            data: base.join("app"),
            source,
            destination,
            _temp: temp,
        }
    }
    fn setup(&self, name: &str) -> JobSetup {
        JobSetup {
            name: name.into(),
            sources: vec![JobSource {
                label: "Incoming".into(),
                path: self.source.to_str().unwrap().into(),
            }],
            destination: self.destination.to_str().unwrap().into(),
        }
    }
    fn create(&self, app: &Desktop, name: &str) -> Value {
        app.create_job(self.setup(name)).unwrap();
        let snapshot = idle(app);
        assert!(snapshot["error"].is_null(), "{snapshot}");
        snapshot
    }
}

#[test]
fn multiple_named_jobs_reopen_older_plan_and_setup_after_restart() {
    let f = Fixture::new();
    let app = Desktop::open(f.data.clone()).unwrap();
    let first = f.create(&app, "First review");
    let first_id = first["job_id"].as_str().unwrap().to_owned();
    let key = first["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["relative_path"] == "")
        .unwrap()["key"]
        .as_str()
        .unwrap();
    app.plan(
        f.destination.to_str().unwrap().into(),
        vec![MappingInput {
            source_key: key.into(),
            destination_relative: String::new(),
        }],
    )
    .unwrap();
    let planned = idle(&app);
    assert!(planned["error"].is_null(), "{planned}");
    let revision = planned["plan"]["revision"].clone();
    let initial = app.create_job(f.setup("Second review")).unwrap();
    assert_ne!(initial["job_id"], first["job_id"]);
    assert_eq!(initial["setup"]["name"], "Second review");
    assert!(initial["plan"].is_null());
    assert!(initial["entries"].as_array().unwrap().is_empty());
    let second = idle(&app);
    assert!(second["error"].is_null(), "{second}");
    let rows = app.jobs().unwrap();
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|r| !r.active));
    let first_row = rows.iter().find(|r| r.id == first_id).unwrap();
    assert_eq!(first_row.name, "First review");
    assert_eq!(first_row.stage, "plan");
    assert_eq!(first_row.source_count, 1);
    assert_eq!(first_row.entry_count, 2);
    assert!(first_row.created_ms > 0);
    assert!(first_row.updated_ms > 0);
    drop(app);
    let app = Desktop::open(f.data.clone()).unwrap();
    assert_eq!(app.snapshot().unwrap()["job_id"], second["job_id"]);
    let opened = app.open_job(first_id.clone()).unwrap();
    assert_eq!(opened["plan"]["revision"], revision);
    assert_eq!(opened["setup"]["name"], "First review");
    assert_eq!(
        opened["setup"]["destination"],
        f.destination.to_str().unwrap()
    );
    assert!(opened["worker"].is_null());
    drop(app);
    assert_eq!(
        Desktop::open(f.data.clone()).unwrap().snapshot().unwrap()["job_id"],
        first_id
    );
    assert_eq!(
        fs::read(f.source.join("file.txt")).unwrap(),
        b"synthetic source"
    );
    assert_eq!(fs::read_dir(&f.destination).unwrap().count(), 0);
}

#[test]
fn draft_source_removal_is_durable_without_changing_saved_job_or_files() {
    let f = Fixture::new();
    let app = Desktop::open(f.data.clone()).unwrap();
    let saved = f.create(&app, "Saved setup");
    let mut draft = f.setup("Unfinished review");
    draft.destination.clear();
    draft.sources.push(JobSource {
        label: "Primary candidate".into(),
        path: f.destination.to_str().unwrap().into(),
    });
    app.save_draft(draft.clone()).unwrap();
    draft.sources.remove(0);
    app.save_draft(draft.clone()).unwrap();
    drop(app);
    let app = Desktop::open(f.data.clone()).unwrap();
    assert_eq!(app.draft().unwrap(), Some(draft));
    assert_eq!(app.snapshot().unwrap()["setup"], saved["setup"]);
    assert_eq!(
        fs::read(f.source.join("file.txt")).unwrap(),
        b"synthetic source"
    );
    assert!(f.source.is_dir());
    assert!(f.destination.is_dir());
}

#[test]
fn create_requires_disjoint_primary_and_persists_it_before_scan() {
    let f = Fixture::new();
    let app = Desktop::open(f.data.clone()).unwrap();
    let mut setup = f.setup("  Named scan  ");
    setup.sources.push(JobSource {
        label: "Primary".into(),
        path: f.destination.to_str().unwrap().into(),
    });
    app.save_draft(setup.clone()).unwrap();
    let initial = app.create_job(setup).unwrap();
    assert_eq!(initial["setup"]["name"], "Named scan");
    assert_eq!(initial["setup"]["sources"].as_array().unwrap().len(), 1);
    let saved = idle(&app);
    assert_eq!(saved["roots"].as_array().unwrap().len(), 1);
    assert_eq!(
        saved["setup"]["destination"],
        f.destination.to_str().unwrap()
    );
    assert_eq!(app.destination().unwrap(), f.destination);
    assert!(app.draft().unwrap().is_none());
    drop(app);
    let app = Desktop::open(f.data.clone()).unwrap();
    assert_eq!(app.snapshot().unwrap()["setup"], saved["setup"]);
    let mut bad = f.setup("Missing primary");
    bad.destination.clear();
    assert!(app.create_job(bad).is_err());
    assert_eq!(app.snapshot().unwrap()["job_id"], saved["job_id"]);
    let nested = f.source.join("nested");
    fs::create_dir(&nested).unwrap();
    let mut bad = f.setup("Overlapping");
    bad.destination = nested.to_str().unwrap().into();
    assert!(app.create_job(bad).is_err());
    assert_eq!(app.jobs().unwrap().len(), 1);
}

#[test]
fn invalid_ids_and_linked_databases_are_rejected_and_corruption_stays_visible() {
    use std::os::unix::fs::symlink;
    let f = Fixture::new();
    let app = Desktop::open(f.data.clone()).unwrap();
    let original = f.create(&app, "Valid job");
    for id in ["../outside", "1/../../outside", "", "not-an-id", "1.sqlite"] {
        assert!(app.open_job(id.into()).is_err(), "{id}");
    }
    assert_eq!(app.snapshot().unwrap()["job_id"], original["job_id"]);
    fs::write(f.data.join("jobs/123-4.sqlite"), b"not a database").unwrap();
    symlink(f.source.join("file.txt"), f.data.join("jobs/123-5.sqlite")).unwrap();
    let rows = app.jobs().unwrap();
    assert_eq!(rows.len(), 3);
    assert!(
        rows.iter()
            .find(|r| r.id == "123-4")
            .unwrap()
            .error
            .is_some()
    );
    assert!(
        rows.iter()
            .find(|r| r.id == "123-5")
            .unwrap()
            .error
            .is_some()
    );
    assert!(app.open_job("123-5".into()).is_err());
    let corrupt = app.open_job("123-4".into()).unwrap();
    assert_eq!(corrupt["job_id"], "123-4");
    assert_eq!(corrupt["recovery_required"], true);
    assert!(
        corrupt["error"]
            .as_str()
            .unwrap()
            .contains("could not be reopened")
    );
    assert!(corrupt["entries"].as_array().unwrap().is_empty());
    drop(app);
    let app = Desktop::open(f.data.clone()).unwrap();
    assert_eq!(app.snapshot().unwrap()["job_id"], "123-4");
    assert!(app.snapshot().unwrap()["error"].is_string());
    assert_eq!(
        fs::read(f.source.join("file.txt")).unwrap(),
        b"synthetic source"
    );
}

#[test]
fn active_writer_gate_prevents_switch_create_and_draft_lost_updates() {
    let f = Fixture::new();
    let app = Desktop::open(f.data.clone()).unwrap();
    let first = f.create(&app, "First");
    let first_id = first["job_id"].as_str().unwrap();
    let second = f.create(&app, "Second");
    let second_id = second["job_id"].as_str().unwrap();
    let lock = rusqlite::Connection::open(f.data.join("jobs").join(format!("{second_id}.sqlite")))
        .unwrap();
    lock.execute_batch("BEGIN EXCLUSIVE").unwrap();
    // The worker cannot finish its database update until this test releases the
    // exclusive SQLite lock, so these gate assertions do not race a tiny scan.
    let started = app.run_job("scan").unwrap();
    assert_eq!(started["worker"], "scan");
    assert!(
        app.open_job(first_id.into())
            .unwrap_err()
            .contains("current operation")
    );
    assert!(
        app.create_job(f.setup("Must not start"))
            .unwrap_err()
            .contains("already running")
    );
    assert!(
        app.save_draft(f.setup("Must not lose draft"))
            .unwrap_err()
            .contains("current operation")
    );
    assert!(
        app.set_job_archived(second_id.into(), true)
            .unwrap_err()
            .contains("current operation")
    );
    assert!(
        app.delete_job(second_id.into())
            .unwrap_err()
            .contains("current operation")
    );
    assert!(app.clear_draft().unwrap_err().contains("current operation"));
    lock.execute_batch("COMMIT").unwrap();
    drop(lock);
    let saved = idle(&app);
    assert!(saved["error"].is_null(), "{saved}");
    assert_eq!(saved["job_id"], second_id);
    assert_eq!(app.jobs().unwrap().len(), 2);
    assert!(app.jobs().unwrap().iter().all(|r| !r.active));
}

#[test]
fn legacy_jobs_have_derived_setup_and_invalid_metadata_is_explicit() {
    let f = Fixture::new();
    let app = Desktop::open(f.data.clone()).unwrap();
    app.start(vec![f.source.to_str().unwrap().into()]).unwrap();
    let saved = idle(&app);
    let id = saved["job_id"].as_str().unwrap();
    assert_eq!(
        saved["setup"]["sources"][0]["path"],
        f.source.to_str().unwrap()
    );
    assert_eq!(saved["setup"]["destination"], "");
    assert_eq!(app.jobs().unwrap()[0].name, "Scan — incoming");
    fs::write(
        f.data.join("jobs").join(format!("{id}.json")),
        b"broken setup",
    )
    .unwrap();
    let row = app.jobs().unwrap().remove(0);
    assert!(row.error.unwrap().contains("setup"));
    let reopened = app.open_job(id.into()).unwrap();
    assert_eq!(reopened["entries"], saved["entries"]);
    assert!(reopened["error"].as_str().unwrap().contains("setup"));
}

#[test]
fn older_hot_journal_job_can_be_selected_and_recovered() {
    const CHILD: &str = "FOLDERLOOM_JOBS_HOT_JOURNAL";
    if let Some(database) = std::env::var_os(CHILD) {
        let connection = rusqlite::Connection::open(database).unwrap();
        connection.execute_batch("PRAGMA cache_size=10; PRAGMA cache_spill=1; CREATE TABLE crashpad(payload BLOB); WITH RECURSIVE n(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<2000) INSERT INTO crashpad SELECT zeroblob(4000) FROM n; BEGIN IMMEDIATE; UPDATE crashpad SET payload=randomblob(4000);").unwrap();
        std::process::exit(0);
    }
    let f = Fixture::new();
    let app = Desktop::open(f.data.clone()).unwrap();
    let first = f.create(&app, "Older interrupted job");
    let first_id = first["job_id"].as_str().unwrap();
    let second = f.create(&app, "Current job");
    let database = f.data.join("jobs").join(format!("{first_id}.sqlite"));
    let child = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "older_hot_journal_job_can_be_selected_and_recovered",
            "--nocapture",
        ])
        .env(CHILD, &database)
        .output()
        .unwrap();
    assert!(
        child.status.success(),
        "{}",
        String::from_utf8_lossy(&child.stderr)
    );
    assert_eq!(app.snapshot().unwrap()["job_id"], second["job_id"]);
    let selected = app.open_job(first_id.into()).unwrap();
    assert_eq!(selected["recovery_required"], true);
    assert!(selected["error"].is_string());
    assert_eq!(selected["setup"]["name"], "Older interrupted job");
    app.run_job("scan").unwrap();
    let recovered = idle(&app);
    assert!(recovered["error"].is_null(), "{recovered}");
    assert_eq!(recovered["recovery_required"], false);
    assert_eq!(recovered["job_id"], first_id);
    assert_eq!(recovered["inventory_progress"]["state"], "complete");
    assert_eq!(app.jobs().unwrap().len(), 2);
    assert_eq!(
        fs::read(f.source.join("file.txt")).unwrap(),
        b"synthetic source"
    );
}

#[test]
fn samples_under_app_data_are_allowed_but_database_storage_is_not() {
    let f = Fixture::new();
    let app = Desktop::open(f.data.clone()).unwrap();
    let source = f.data.join("samples/source");
    let destination = f.data.join("samples/primary");
    fs::create_dir_all(&source).unwrap();
    fs::create_dir_all(&destination).unwrap();
    fs::write(source.join("safe.txt"), b"safe sample").unwrap();
    let setup = JobSetup {
        name: "App data samples".into(),
        sources: vec![JobSource {
            label: "Sample".into(),
            path: source.to_str().unwrap().into(),
        }],
        destination: destination.to_str().unwrap().into(),
    };
    app.create_job(setup.clone()).unwrap();
    let scanned = idle(&app);
    assert!(scanned["error"].is_null(), "{scanned}");
    let private_subdir = f.data.join("jobs/private");
    fs::create_dir(&private_subdir).unwrap();
    for prohibited in [&f.data, &f.data.join("jobs"), &private_subdir] {
        let mut bad = setup.clone();
        bad.sources[0].path = prohibited.to_str().unwrap().into();
        assert!(app.create_job(bad).is_err());
        let mut bad = setup.clone();
        bad.destination = prohibited.to_str().unwrap().into();
        assert!(app.create_job(bad).is_err());
    }
    assert_eq!(fs::read(source.join("safe.txt")).unwrap(), b"safe sample");
}

#[test]
fn job_rows_distinguish_comparison_checkpoints_and_invalidated_plans() {
    let f = Fixture::new();
    let app = Desktop::open(f.data.clone()).unwrap();
    let first = f.create(&app, "Progress states");
    let id = first["job_id"].as_str().unwrap();
    let database = f.data.join("jobs").join(format!("{id}.sqlite"));
    let mut compare = folderloom_core::Job::compare(&database).unwrap();
    compare.pause_comparison().unwrap();
    drop(compare);
    assert_eq!(app.jobs().unwrap()[0].state, "comparison_paused");
    let key = first["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["relative_path"] == "")
        .unwrap()["key"]
        .as_str()
        .unwrap();
    app.plan(
        f.destination.to_str().unwrap().into(),
        vec![MappingInput {
            source_key: key.into(),
            destination_relative: String::new(),
        }],
    )
    .unwrap();
    assert!(idle(&app)["error"].is_null());
    app.run_job("scan").unwrap();
    assert!(idle(&app)["error"].is_null());
    let row = app.jobs().unwrap().remove(0);
    assert_eq!(row.state, "needs_replan");
    assert_eq!(row.stage, "plan");
}

#[test]
fn archive_and_restore_survive_restart_without_changing_job_or_files() {
    let f = Fixture::new();
    let app = Desktop::open(f.data.clone()).unwrap();
    let saved = f.create(&app, "Finished review");
    let id = saved["job_id"].as_str().unwrap().to_owned();
    let database = f.data.join("jobs").join(format!("{id}.sqlite"));
    let before = fs::read(&database).unwrap();
    assert!(!app.jobs().unwrap()[0].archived);
    app.set_job_archived(id.clone(), true).unwrap();
    drop(app);
    let app = Desktop::open(f.data.clone()).unwrap();
    let row = app.jobs().unwrap().remove(0);
    assert!(row.archived);
    assert_eq!(row.name, "Finished review");
    assert_eq!(
        app.open_job(id.clone()).unwrap()["entries"],
        saved["entries"]
    );
    app.set_job_archived(id.clone(), false).unwrap();
    drop(app);
    let app = Desktop::open(f.data.clone()).unwrap();
    assert!(!app.jobs().unwrap()[0].archived);
    assert_eq!(fs::read(database).unwrap(), before);
    assert_eq!(
        fs::read(f.source.join("file.txt")).unwrap(),
        b"synthetic source"
    );
    assert_eq!(fs::read_dir(&f.destination).unwrap().count(), 0);
    assert!(app.set_job_archived("../outside".into(), true).is_err());
    assert!(app.set_job_archived("123-999".into(), true).is_err());
    // Legacy jobs without setup metadata can also be retired.
    fs::remove_file(f.data.join("jobs").join(format!("{id}.json"))).unwrap();
    app.set_job_archived(id, true).unwrap();
    assert!(app.jobs().unwrap()[0].archived);
}

#[test]
fn delete_archived_job_removes_only_its_records_and_clears_selected_job() {
    let f = Fixture::new();
    let app = Desktop::open(f.data.clone()).unwrap();
    let other = f.create(&app, "Keep this job");
    let selected = f.create(&app, "Disposable job");
    let id = selected["job_id"].as_str().unwrap().to_owned();
    fs::write(f.destination.join("copied.txt"), b"keep copied output").unwrap();
    app.save_draft(f.setup("Keep draft")).unwrap();
    assert!(app.delete_job(id.clone()).unwrap_err().contains("Archive"));
    assert!(app.delete_job("../outside".into()).is_err());
    app.set_job_archived(id.clone(), true).unwrap();
    let next = app.delete_job(id.clone()).unwrap();
    assert!(next["job_id"].is_null());
    assert!(fs::read_dir(f.data.join("jobs")).unwrap().all(|entry| {
        !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(&id)
    }));
    assert!(!f.data.join("jobs").join(format!("{id}.sqlite")).exists());
    drop(app);
    let app = Desktop::open(f.data.clone()).unwrap();
    assert!(app.snapshot().unwrap()["job_id"].is_null());
    assert_eq!(app.jobs().unwrap().len(), 1);
    assert_eq!(app.jobs().unwrap()[0].id, other["job_id"].as_str().unwrap());
    assert_eq!(app.draft().unwrap().unwrap().name, "Keep draft");
    assert_eq!(
        fs::read(f.source.join("file.txt")).unwrap(),
        b"synthetic source"
    );
    assert_eq!(
        fs::read(f.destination.join("copied.txt")).unwrap(),
        b"keep copied output"
    );
}

#[test]
fn delete_rejects_linked_companion_without_removing_any_records() {
    use std::os::unix::fs::symlink;
    let f = Fixture::new();
    let app = Desktop::open(f.data.clone()).unwrap();
    let saved = f.create(&app, "Archived job");
    let id = saved["job_id"].as_str().unwrap().to_owned();
    app.set_job_archived(id.clone(), true).unwrap();
    let metadata = f.data.join("jobs").join(format!("{id}.json"));
    fs::remove_file(&metadata).unwrap();
    symlink(f.source.join("file.txt"), &metadata).unwrap();
    assert!(app.delete_job(id.clone()).is_err());
    assert!(f.data.join("jobs").join(format!("{id}.sqlite")).exists());
    assert_eq!(app.snapshot().unwrap()["job_id"], id);
    assert_eq!(
        fs::read(f.source.join("file.txt")).unwrap(),
        b"synthetic source"
    );
}

#[test]
fn delete_rejects_external_writer_and_preserves_other_selected_job() {
    let f = Fixture::new();
    let app = Desktop::open(f.data.clone()).unwrap();
    let old = f.create(&app, "Old job");
    let id = old["job_id"].as_str().unwrap().to_owned();
    let selected = f.create(&app, "Selected job");
    app.set_job_archived(id.clone(), true).unwrap();
    let database = f.data.join("jobs").join(format!("{id}.sqlite"));
    let writer = folderloom_core::Job::resume(&database).unwrap();
    assert!(
        app.delete_job(id.clone())
            .unwrap_err()
            .contains("another process")
    );
    assert!(database.exists());
    drop(writer);
    app.delete_job(id).unwrap();
    assert_eq!(app.snapshot().unwrap()["job_id"], selected["job_id"]);
    drop(app);
    let reopened = Desktop::open(f.data.clone()).unwrap();
    assert_eq!(reopened.snapshot().unwrap()["job_id"], selected["job_id"]);
}

#[test]
fn cancelling_setup_is_durable_and_does_not_remove_started_jobs_or_files() {
    let f = Fixture::new();
    let app = Desktop::open(f.data.clone()).unwrap();
    let saved = f.create(&app, "Keep job");
    app.save_draft(f.setup("Cancelled setup")).unwrap();
    app.clear_draft().unwrap();
    app.clear_draft().unwrap();
    drop(app);
    let app = Desktop::open(f.data.clone()).unwrap();
    assert!(app.draft().unwrap().is_none());
    assert_eq!(app.jobs().unwrap().len(), 1);
    assert_eq!(app.snapshot().unwrap()["job_id"], saved["job_id"]);
    assert_eq!(
        fs::read(f.source.join("file.txt")).unwrap(),
        b"synthetic source"
    );
    app.save_draft(JobSetup {
        name: "  ".into(),
        sources: vec![],
        destination: String::new(),
    })
    .unwrap();
    assert!(app.draft().unwrap().is_none());
    drop(app);
    assert!(Desktop::open(f.data).unwrap().draft().unwrap().is_none());
}
