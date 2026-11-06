use folderloom_core::{Job, State};
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    process::{Command, Stdio},
};

#[test]
fn killed_runner_reopens_without_false_completion_then_pauses_and_resumes() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    fs::create_dir_all(source.join("nested")).unwrap();
    for i in 0..20 {
        fs::write(source.join(format!("file-{i}")), b"synthetic").unwrap();
    }
    fs::write(source.join("nested/child"), b"child").unwrap();
    let db = temp.path().join("job.sqlite");
    let exe = env!("CARGO_BIN_EXE_folderloom");
    let mut child = Command::new(exe)
        .arg("scan")
        .arg(&db)
        .arg(&source)
        .args(["--batch-size", "1", "--interactive"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut line = String::new();
    BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    assert!(line.contains("state=Running"), "{line}");
    let observer = Job::open(&db).unwrap();
    assert_eq!(observer.progress().unwrap().state, State::Running);
    assert!(
        !observer
            .inventory()
            .unwrap()
            .iter()
            .find(|e| e.relative_path.as_os_str().is_empty())
            .unwrap()
            .directory_complete
    );
    drop(observer);
    child.kill().unwrap();
    child.wait().unwrap();
    let status = Command::new(exe).arg("status").arg(&db).output().unwrap();
    assert!(status.status.success());
    assert!(String::from_utf8_lossy(&status.stdout).contains("state=Running"));
    let mut child = Command::new(exe)
        .arg("resume")
        .arg(&db)
        .args(["--batch-size", "1", "--interactive"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    line.clear();
    let mut reader = BufReader::new(child.stdout.take().unwrap());
    reader.read_line(&mut line).unwrap();
    child.stdin.take().unwrap().write_all(b"pause\n").unwrap();
    assert!(child.wait().unwrap().success());
    assert_eq!(
        Job::open(&db).unwrap().progress().unwrap().state,
        State::Paused
    );
    let resumed = Command::new(exe).arg("resume").arg(&db).output().unwrap();
    assert!(
        resumed.status.success(),
        "{}",
        String::from_utf8_lossy(&resumed.stderr)
    );
    let job = Job::open(&db).unwrap();
    assert_eq!(job.progress().unwrap().state, State::Complete);
    assert_eq!(job.inventory().unwrap().len(), 23);
    assert_eq!(job.progress().unwrap().stale_entries, 0);
    let reopened = Command::new(exe)
        .arg("inventory")
        .arg(&db)
        .output()
        .unwrap();
    assert!(reopened.status.success());
    assert!(String::from_utf8_lossy(&reopened.stdout).contains("nested/child"));
}
