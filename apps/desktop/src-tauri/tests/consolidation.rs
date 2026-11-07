use folderloom_core::PlanDecision;
use folderloom_desktop::{Desktop, MappingInput};
use serde_json::Value;
use std::{
    fs, thread,
    time::{Duration, Instant},
};

fn idle(app: &Desktop) -> Value {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let snapshot = app.snapshot().unwrap();
        if snapshot["worker"].is_null() {
            return snapshot;
        }
        assert!(Instant::now() < deadline, "worker did not finish");
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn review_reopen_approve_copy_and_results_through_shared_desktop_interface() {
    let fixture = tempfile::tempdir().unwrap();
    let source = fixture.path().join("source");
    let destination = fixture.path().join("primary");
    let data = fixture.path().join("app");
    fs::create_dir_all(source.join(r"wrapper\name")).unwrap();
    fs::create_dir(&destination).unwrap();
    let content = source.join(r"wrapper\name");
    fs::write(content.join("same.txt"), b"same").unwrap();
    fs::write(destination.join("same.txt"), b"same").unwrap();
    fs::write(content.join("new.txt"), b"new bytes").unwrap();
    fs::write(content.join("leave.txt"), b"skip me").unwrap();
    let app = Desktop::open(data.clone()).unwrap();
    app.start(vec![source.to_str().unwrap().into()]).unwrap();
    let inventory = idle(&app);
    let key = inventory["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["relative_path"] == r"wrapper\\name")
        .unwrap()["key"]
        .as_str()
        .unwrap();
    app.plan(
        fs::canonicalize(&destination)
            .unwrap()
            .to_str()
            .unwrap()
            .into(),
        vec![MappingInput {
            source_key: key.into(),
            destination_relative: String::new(),
        }],
    )
    .unwrap();
    let planned = idle(&app);
    assert!(planned["error"].is_null(), "{planned}");
    assert_eq!(planned["plan"]["mappings"][0]["source_key"], key);
    let revision = planned["plan"]["revision"].as_i64().unwrap();
    let leave = planned["plan"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["destination_relative"] == "leave.txt")
        .unwrap()["id"]
        .as_i64()
        .unwrap();
    let edited = app
        .decide(
            revision,
            vec![PlanDecision {
                item_id: leave,
                included: false,
            }],
        )
        .unwrap();
    let revised = edited["plan"]["revision"].as_i64().unwrap();
    assert!(revised > revision);
    assert!(app.approve(revision).is_err());
    assert!(!destination.join("new.txt").exists());
    drop(app);
    let app = Desktop::open(data.clone()).unwrap();
    assert_eq!(app.snapshot().unwrap()["plan"], edited["plan"]);
    app.approve(revised).unwrap();
    app.execute(revised).unwrap();
    let completed = idle(&app);
    assert!(completed["error"].is_null(), "{completed}");
    assert_eq!(completed["plan"]["state"], "complete", "{completed}");
    assert_eq!(fs::read(destination.join("new.txt")).unwrap(), b"new bytes");
    assert_eq!(fs::read(destination.join("same.txt")).unwrap(), b"same");
    assert!(!destination.join("leave.txt").exists());
    assert_eq!(fs::read(content.join("leave.txt")).unwrap(), b"skip me");
    assert_eq!(fs::read_dir(&content).unwrap().count(), 3);
    assert_eq!(
        app.destination().unwrap(),
        fs::canonicalize(destination).unwrap()
    );
    drop(app);
    let reopened = Desktop::open(data).unwrap();
    assert_eq!(reopened.snapshot().unwrap()["plan"], completed["plan"]);
}

#[test]
fn unknown_mapping_identity_cannot_turn_display_paths_into_copy_authority() {
    let fixture = tempfile::tempdir().unwrap();
    let source = fixture.path().join("source");
    let destination = fixture.path().join("primary");
    fs::create_dir(&source).unwrap();
    fs::create_dir(&destination).unwrap();
    fs::write(source.join("file"), b"data").unwrap();
    let app = Desktop::open(fixture.path().join("app")).unwrap();
    app.start(vec![source.to_str().unwrap().into()]).unwrap();
    idle(&app);
    app.plan(
        fs::canonicalize(&destination)
            .unwrap()
            .to_str()
            .unwrap()
            .into(),
        vec![MappingInput {
            source_key: "1:2e2e2f657363617065".into(),
            destination_relative: String::new(),
        }],
    )
    .unwrap();
    let refused = idle(&app);
    assert!(
        refused["error"]
            .as_str()
            .unwrap()
            .contains("not in the saved inventory")
    );
    assert!(refused["plan"].is_null());
    assert_eq!(fs::read_dir(destination).unwrap().count(), 0);
}
