use super::*;

fn fixture(mode: WorkerMode) -> (tempfile::TempDir, Desktop) {
    let directory = tempfile::tempdir().unwrap();
    let app = Desktop::open(directory.path().join("app")).unwrap();
    app.inner.snapshot.lock().unwrap().control.worker = Some(mode);
    (directory, app)
}
fn finish_worker(app: &Desktop, failure: Option<&str>) {
    let mut snapshot = app.inner.snapshot.lock().unwrap();
    snapshot.control.worker = None;
    snapshot.control.error = failure.map(str::to_owned);
}

#[test]
fn quit_waits_for_worker_release_in_every_phase_and_blocks_new_work() {
    for mode in [
        WorkerMode::Scan,
        WorkerMode::Compare,
        WorkerMode::Plan,
        WorkerMode::Execute,
    ] {
        let (_directory, app) = fixture(mode);
        assert!(!app.request_quit().unwrap());
        let request = app.quit_status().unwrap().request;
        assert_eq!(app.confirm_quit(request).unwrap().phase, QuitPhase::Waiting);
        assert!(app.inner.pause.load(Ordering::Acquire));
        assert!(!app.finish_quit(request).unwrap());
        assert!(!app.request_quit().unwrap());
        assert_eq!(app.quit_status().unwrap().request, request);
        assert!(
            app.run_job("scan")
                .unwrap_err()
                .contains("Keep the app open")
        );
        finish_worker(&app, None);
        assert!(app.finish_quit(request).unwrap());
        assert!(app.request_quit().unwrap());
        assert!(app.run_job("scan").is_err());
    }
}

#[test]
fn cancelling_pending_quit_defeats_late_ack_and_old_request_cannot_affect_new_quit() {
    let (_directory, app) = fixture(WorkerMode::Compare);
    assert!(!app.request_quit().unwrap());
    let old = app.quit_status().unwrap().request;
    app.confirm_quit(old).unwrap();
    assert_eq!(app.cancel_quit(old).unwrap().phase, QuitPhase::Idle);
    assert!(
        app.inner.pause.load(Ordering::Acquire),
        "do not retract a pause already being saved"
    );
    assert!(!app.request_quit().unwrap());
    let new = app.quit_status().unwrap().request;
    assert_ne!(old, new);
    assert_eq!(app.confirm_quit(old).unwrap().phase, QuitPhase::Confirming);
    assert_eq!(app.cancel_quit(old).unwrap().phase, QuitPhase::Confirming);
    finish_worker(&app, None);
    assert!(!app.finish_quit(old).unwrap());
    app.confirm_quit(new).unwrap();
    app.cancel_quit(new).unwrap();
    assert!(!app.finish_quit(new).unwrap());
    assert_eq!(app.quit_status().unwrap().phase, QuitPhase::Idle);
}

#[test]
fn keep_working_does_not_request_pause_and_worker_finishing_does_not_accept_dialog() {
    let (_directory, app) = fixture(WorkerMode::Scan);
    app.request_quit().unwrap();
    let request = app.quit_status().unwrap().request;
    assert!(!app.inner.pause.load(Ordering::Acquire));
    finish_worker(&app, None);
    assert!(
        !app.request_quit().unwrap(),
        "an open confirmation still requires an answer"
    );
    assert!(!app.finish_quit(request).unwrap());
    assert_eq!(app.cancel_quit(request).unwrap().phase, QuitPhase::Idle);
    assert!(!app.inner.pause.load(Ordering::Acquire));
}

#[test]
fn worker_failure_keeps_app_open_instead_of_claiming_saved_pause() {
    let (_directory, app) = fixture(WorkerMode::Execute);
    app.request_quit().unwrap();
    let request = app.quit_status().unwrap().request;
    app.confirm_quit(request).unwrap();
    finish_worker(&app, Some("Synthetic checkpoint failure"));
    assert!(!app.finish_quit(request).unwrap());
    let status = app.quit_status().unwrap();
    assert_eq!(status.phase, QuitPhase::Failed);
    assert_eq!(
        status.error.as_deref(),
        Some("Synthetic checkpoint failure")
    );
    assert!(!app.request_quit().unwrap());
    app.cancel_quit(request).unwrap();
    // A later explicit quit is allowed once the user has seen the stopped job.
    assert!(app.request_quit().unwrap());
}

#[test]
fn idle_exit_reserves_admission_before_a_new_worker_can_start() {
    let directory = tempfile::tempdir().unwrap();
    let app = Desktop::open(directory.path().join("app")).unwrap();
    assert!(app.request_quit().unwrap());
    assert_eq!(app.quit_status().unwrap().phase, QuitPhase::Exiting);
    assert!(
        app.start(vec![directory.path().display().to_string()])
            .is_err()
    );
    assert_eq!(app.cancel_quit(0).unwrap().phase, QuitPhase::Exiting);
}

#[test]
fn acknowledged_scan_quit_reopens_and_resumes_saved_inventory() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("source");
    fs::create_dir(&source).unwrap();
    for index in 0..512 {
        fs::write(source.join(format!("file-{index}")), b"synthetic").unwrap();
    }
    let data = directory.path().join("app");
    let app = Desktop::open(data.clone()).unwrap();
    app.start(vec![source.display().to_string()]).unwrap();
    assert!(!app.request_quit().unwrap());
    let request = app.quit_status().unwrap().request;
    app.confirm_quit(request).unwrap();
    let until = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while !app.finish_quit(request).unwrap() {
        assert!(std::time::Instant::now() < until);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let saved = app.snapshot().unwrap();
    assert!(saved["error"].is_null(), "{saved}");
    drop(app);
    let reopened = Desktop::open(data).unwrap();
    assert_eq!(reopened.snapshot().unwrap()["job_id"], saved["job_id"]);
    assert_eq!(
        reopened.snapshot().unwrap()["inventory_progress"],
        saved["inventory_progress"]
    );
    reopened.run_job("scan").unwrap();
    while reopened.snapshot().unwrap()["worker"].is_string() {
        assert!(std::time::Instant::now() < until);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let complete = reopened.snapshot().unwrap();
    assert_eq!(complete["inventory_progress"]["state"], "complete");
    assert_eq!(complete["inventory_progress"]["entries"], 513);
    assert_eq!(fs::read(source.join("file-0")).unwrap(), b"synthetic");
}

#[test]
fn damaged_state_does_not_trap_native_quit() {
    for poison_quit in [false, true] {
        let (_directory, app) = fixture(WorkerMode::Scan);
        let broken = app.clone();
        let _ = std::thread::spawn(move || {
            if poison_quit {
                let _guard = broken.inner.quit.lock().unwrap();
                panic!("synthetic quit lock failure");
            } else {
                let _guard = broken.inner.snapshot.lock().unwrap();
                panic!("synthetic snapshot lock failure");
            }
        })
        .join();
        assert!(app.request_quit().is_err());
        assert!(app.native_quit_allowed());
    }
}
