use folderloom_desktop::Desktop;
use std::{
    fs, thread,
    time::{Duration, Instant},
};

fn idle(app: &Desktop) -> serde_json::Value {
    let until = Instant::now() + Duration::from_secs(20);
    loop {
        let state = app.snapshot().unwrap();
        if state["worker"].is_null() {
            return state;
        }
        assert!(Instant::now() < until, "worker did not finish");
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn saved_inventory_and_full_content_evidence_reopen_through_desktop() {
    let fixture = tempfile::tempdir().unwrap();
    let source = fixture.path().join(r"source\name");
    fs::create_dir(&source).unwrap();
    fs::write(source.join("one.txt"), b"same contents").unwrap();
    fs::write(source.join("two.txt"), b"same contents").unwrap();
    fs::write(source.join("unique.txt"), b"unique").unwrap();
    let data = fixture.path().join("app");
    let app = Desktop::open(data.clone()).unwrap();
    app.start(vec![source.to_str().unwrap().to_owned()])
        .unwrap();
    let scanned = idle(&app);
    assert_eq!(scanned["inventory_progress"]["state"], "complete");
    assert_eq!(
        scanned["roots"][0]["input_path"],
        fs::canonicalize(&source).unwrap().to_str().unwrap()
    );
    assert_ne!(
        scanned["roots"][0]["path"],
        scanned["roots"][0]["input_path"]
    );
    assert_eq!(scanned["entries"].as_array().unwrap().len(), 4);
    app.run_job("compare").unwrap();
    let compared = idle(&app);
    assert_eq!(compared["comparison_progress"]["full_hashes"], 2);
    assert_eq!(compared["comparison_progress"]["unique_sizes"], 1);
    drop(app);
    let reopened = Desktop::open(data).unwrap().snapshot().unwrap();
    assert_eq!(reopened["job_id"], compared["job_id"]);
    assert_eq!(reopened["comparisons"], compared["comparisons"]);
    assert_eq!(fs::read(source.join("one.txt")).unwrap(), b"same contents");
    assert_eq!(fs::read_dir(source).unwrap().count(), 3);
}

#[test]
fn pause_or_early_completion_releases_writer_and_saved_work_can_resume() {
    let fixture = tempfile::tempdir().unwrap();
    let source = fixture.path().join("source");
    fs::create_dir(&source).unwrap();
    fs::create_dir(source.join("nested")).unwrap();
    let data = fixture.path().join("app");
    let app = Desktop::open(data.clone()).unwrap();
    let started = app
        .start(vec![source.to_str().unwrap().to_owned()])
        .unwrap();
    assert_eq!(started["worker"], "scan");
    app.pause().unwrap();
    let stopped = idle(&app);
    assert!(stopped["error"].is_null(), "{stopped}");
    // Completion before the request is legitimate; scheduling is not a contract.
    assert!(matches!(
        stopped["inventory_progress"]["state"].as_str(),
        Some("paused" | "complete")
    ));
    drop(app);
    let reopened = Desktop::open(data).unwrap();
    assert_eq!(
        reopened.snapshot().unwrap()["inventory_progress"],
        stopped["inventory_progress"]
    );
    reopened.run_job("scan").unwrap();
    let completed = idle(&reopened);
    assert_eq!(completed["inventory_progress"]["state"], "complete");
    assert_eq!(completed["inventory_progress"]["entries"], 2);
}

#[test]
fn unavailable_roots_are_visible_comparison_gaps_and_stale_inventory() {
    let fixture = tempfile::tempdir().unwrap();
    let source = fixture.path().join("source");
    fs::create_dir(&source).unwrap();
    fs::write(source.join("file"), b"data").unwrap();
    let app = Desktop::open(fixture.path().join("app")).unwrap();
    app.start(vec![source.to_str().unwrap().to_owned()])
        .unwrap();
    idle(&app);
    fs::rename(&source, fixture.path().join("unavailable")).unwrap();
    app.run_job("compare").unwrap();
    let compared = idle(&app);
    assert_eq!(
        compared["comparison_progress"]["state"],
        "finished_with_errors"
    );
    assert_eq!(compared["comparison_progress"]["root_failures"], 1);
    assert_eq!(compared["root_failures"].as_array().unwrap().len(), 1);
    app.run_job("scan").unwrap();
    let scanned = idle(&app);
    assert_eq!(
        scanned["inventory_progress"]["state"],
        "finished_with_errors"
    );
    assert_eq!(scanned["inventory_progress"]["stale_entries"], 2);
    assert!(!scanned["failures"].as_array().unwrap().is_empty());
}

#[test]
#[cfg(target_os = "linux")]
fn invalid_utf8_placements_keep_distinct_keys_and_display_paths() {
    use std::{
        ffi::OsString,
        os::unix::ffi::{OsStrExt, OsStringExt},
    };
    let fixture = tempfile::tempdir().unwrap();
    let source = fixture.path().join("source");
    fs::create_dir(&source).unwrap();
    fs::write(source.join(OsString::from_vec(vec![0xff])), b"same").unwrap();
    fs::write(source.join(std::ffi::OsStr::from_bytes(b"\\xff")), b"same").unwrap();
    let app = Desktop::open(fixture.path().join("app")).unwrap();
    app.start(vec![source.to_str().unwrap().to_owned()])
        .unwrap();
    let snapshot = idle(&app);
    let entries: Vec<_> = snapshot["entries"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["entry_type"] == "file")
        .collect();
    assert_eq!(entries.len(), 2);
    assert_ne!(entries[0]["key"], entries[1]["key"]);
    assert_ne!(entries[0]["relative_path"], entries[1]["relative_path"]);
    let root = snapshot["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["relative_path"] == "")
        .unwrap();
    assert!(root["parent_key"].is_null());
    assert_eq!(entries[0]["parent_key"], root["key"]);
    assert_eq!(entries[1]["parent_key"], root["key"]);
}

#[test]
fn nested_placements_expose_parent_keys_independent_of_display_paths() {
    let fixture = tempfile::tempdir().unwrap();
    let source = fixture.path().join("source");
    let nested = source.join(r"folder\name");
    fs::create_dir_all(&nested).unwrap();
    fs::write(nested.join("file"), b"synthetic").unwrap();
    let app = Desktop::open(fixture.path().join("app")).unwrap();
    app.start(vec![source.to_str().unwrap().into()]).unwrap();
    let saved = idle(&app);
    let entries = saved["entries"].as_array().unwrap();
    let root = entries.iter().find(|e| e["relative_path"] == "").unwrap();
    let folder = entries
        .iter()
        .find(|e| e["relative_path"] == r"folder\\name")
        .unwrap();
    let file = entries.iter().find(|e| e["entry_type"] == "file").unwrap();
    assert!(root["parent_key"].is_null());
    assert_eq!(folder["parent_key"], root["key"]);
    assert_eq!(file["parent_key"], folder["key"]);
}

#[test]
fn saved_job_reopens_in_a_fresh_process() {
    const CHILD: &str = "FOLDERLOOM_SYNTHETIC_RESTART";
    if let Some(base) = std::env::var_os(CHILD) {
        let base = std::path::PathBuf::from(base);
        let app = Desktop::open(base.join("app")).unwrap();
        if std::env::var("FOLDERLOOM_RESTART_STAGE").unwrap() == "create" {
            app.start(vec![base.join("source").to_str().unwrap().into()])
                .unwrap();
            idle(&app);
            app.run_job("compare").unwrap();
            idle(&app);
        }
        let saved = app.snapshot().unwrap();
        assert_eq!(saved["inventory_progress"]["entries"], 3);
        assert_eq!(saved["comparison_progress"]["full_hashes"], 2);
        assert!(saved["worker"].is_null());
        assert!(saved["error"].is_null());
        return;
    }
    let fixture = tempfile::tempdir().unwrap();
    let source = fixture.path().join("source");
    fs::create_dir(&source).unwrap();
    fs::write(source.join("alpha"), b"same bytes").unwrap();
    fs::write(source.join("beta"), b"same bytes").unwrap();
    for stage in ["create", "reopen"] {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "saved_job_reopens_in_a_fresh_process",
                "--nocapture",
            ])
            .env(CHILD, fixture.path())
            .env("FOLDERLOOM_RESTART_STAGE", stage)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "child {stage}: {} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn reopen_in_the_same_window_reloads_saved_changes_without_starting_work() {
    let fixture = tempfile::tempdir().unwrap();
    let source = fixture.path().join("source");
    fs::create_dir(&source).unwrap();
    let data = fixture.path().join("app");
    let app = Desktop::open(data.clone()).unwrap();
    app.start(vec![source.to_str().unwrap().into()]).unwrap();
    let original = idle(&app);
    let database = data
        .join("jobs")
        .join(format!("{}.sqlite", original["job_id"].as_str().unwrap()));
    let mut external = folderloom_core::Job::resume(&database).unwrap();
    external.pause().unwrap();
    drop(external);
    let reopened = app.bootstrap().unwrap()["snapshot"].clone();
    assert_eq!(reopened["inventory_progress"]["generation"], 2);
    assert_eq!(reopened["inventory_progress"]["state"], "paused");
    assert!(reopened["worker"].is_null());
    assert!(
        reopened["inventory_progress"]["pending_directories"]
            .as_u64()
            .unwrap()
            > 0
    );
    app.run_job("scan").unwrap();
    let resumed = idle(&app);
    assert_eq!(resumed["inventory_progress"]["state"], "complete");
    assert_eq!(resumed["inventory_progress"]["entries"], 1);
}

#[test]
fn interrupted_database_can_be_recovered_and_resumed_through_desktop() {
    const CHILD: &str = "FOLDERLOOM_SYNTHETIC_HOT_JOURNAL";
    if let Some(database) = std::env::var_os(CHILD) {
        // Spill uncommitted pages, then exit without SQLite's rollback/close.
        // This crash-only table belongs to the synthetic fixture, not the Job schema.
        let connection = rusqlite::Connection::open(database).unwrap();
        connection
            .execute_batch(
                "PRAGMA cache_size=10; PRAGMA cache_spill=1;
             CREATE TABLE crashpad(payload BLOB);
             WITH RECURSIVE n(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<2000)
             INSERT INTO crashpad SELECT zeroblob(4000) FROM n;
             BEGIN IMMEDIATE; UPDATE crashpad SET payload=randomblob(4000);",
            )
            .unwrap();
        std::process::exit(0);
    }
    let fixture = tempfile::tempdir().unwrap();
    let source = fixture.path().join("source");
    fs::create_dir(&source).unwrap();
    fs::write(source.join("file"), b"synthetic contents").unwrap();
    let data = fixture.path().join("app");
    let app = Desktop::open(data.clone()).unwrap();
    app.start(vec![source.to_str().unwrap().into()]).unwrap();
    let original = idle(&app);
    drop(app);
    let database = data
        .join("jobs")
        .join(format!("{}.sqlite", original["job_id"].as_str().unwrap()));
    let child = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "interrupted_database_can_be_recovered_and_resumed_through_desktop",
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
    // A read-only observer cannot recover this journal. Only the guarded writer can.
    assert!(
        folderloom_core::Job::open(&database)
            .and_then(|job| job.progress())
            .is_err()
    );
    let reopened = Desktop::open(data.clone()).unwrap();
    let saved = reopened.bootstrap().unwrap()["snapshot"].clone();
    assert_eq!(saved["job_id"], original["job_id"]);
    assert_eq!(saved["recovery_required"], true);
    assert!(
        saved["error"]
            .as_str()
            .unwrap()
            .contains("could not be reopened")
    );
    reopened.run_job("scan").unwrap();
    let recovered = idle(&reopened);
    assert!(recovered["error"].is_null(), "{recovered}");
    assert_eq!(recovered["recovery_required"], false);
    assert_eq!(recovered["job_id"], original["job_id"]);
    assert_eq!(recovered["inventory_progress"]["state"], "complete");
    assert_eq!(recovered["inventory_progress"]["generation"], 2);
    assert_eq!(recovered["inventory_progress"]["entries"], 2);
    drop(reopened);
    assert_eq!(
        Desktop::open(data).unwrap().snapshot().unwrap()["job_id"],
        original["job_id"]
    );
    assert_eq!(
        fs::read(source.join("file")).unwrap(),
        b"synthetic contents"
    );
}
