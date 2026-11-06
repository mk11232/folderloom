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
    let source = fixture.path().join("source");
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
fn pause_releases_writer_and_another_desktop_can_resume_saved_work() {
    let fixture = tempfile::tempdir().unwrap();
    let source = fixture.path().join("source");
    fs::create_dir(&source).unwrap();
    // A selected wide tree has enough real enumeration work for a pending pause.
    for i in 0..128 {
        fs::create_dir(source.join(format!("folder-{i}"))).unwrap();
    }
    let data = fixture.path().join("app");
    let app = Desktop::open(data.clone()).unwrap();
    let started = app
        .start(vec![source.to_str().unwrap().to_owned()])
        .unwrap();
    assert_eq!(started["worker"], "scan");
    assert!(app.run_job("scan").unwrap_err().contains("already running"));
    assert!(
        app.start(vec![source.to_str().unwrap().to_owned()])
            .unwrap_err()
            .contains("already running")
    );
    app.pause().unwrap();
    let paused = idle(&app);
    assert_eq!(paused["inventory_progress"]["state"], "paused");
    assert!(
        paused["inventory_progress"]["pending_directories"]
            .as_u64()
            .unwrap()
            > 0
    );
    drop(app);
    let reopened = Desktop::open(data).unwrap();
    assert_eq!(
        reopened.snapshot().unwrap()["inventory_progress"]["state"],
        "paused"
    );
    reopened.run_job("scan").unwrap();
    let completed = idle(&reopened);
    assert_eq!(completed["inventory_progress"]["state"], "complete");
    assert_eq!(completed["inventory_progress"]["entries"], 129);
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
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};
    let fixture = tempfile::tempdir().unwrap();
    let source = fixture.path().join("source");
    fs::create_dir(&source).unwrap();
    fs::write(source.join(OsString::from_vec(vec![0xff])), b"same").unwrap();
    fs::write(source.join("\\xff"), b"same").unwrap();
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
