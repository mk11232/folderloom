use folderloom_core::{ComparisonStage, ComparisonState, Job, State};
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    process::{Child, Command, Stdio},
};

struct RunningChild(Child);

impl Drop for RunningChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn saved_status_exposes_changed_sources_and_invalidated_comparison_generations() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    fs::create_dir(&source).unwrap();
    fs::write(source.join("first"), b"synthetic pair").unwrap();
    fs::write(source.join("second"), b"synthetic pair").unwrap();
    let db = temp.path().join("job.sqlite");
    let exe = env!("CARGO_BIN_EXE_folderloom");
    assert!(
        Command::new(exe)
            .arg("scan")
            .arg(&db)
            .arg(&source)
            .output()
            .unwrap()
            .status
            .success()
    );
    assert!(
        Command::new(exe)
            .arg("compare")
            .arg(&db)
            .output()
            .unwrap()
            .status
            .success()
    );
    let generation = Job::open(&db).unwrap().progress().unwrap().generation;
    fs::write(source.join("first"), b"changed synthetic content").unwrap();
    let saved = Command::new(exe)
        .arg("comparison")
        .arg(&db)
        .output()
        .unwrap();
    assert!(saved.status.success());
    assert!(
        String::from_utf8_lossy(&saved.stdout)
            .contains("comparison checkpoint generation=1 state=Complete")
    );
    let checked = Command::new(exe).arg("compare").arg(&db).output().unwrap();
    assert_eq!(checked.status.code(), Some(2));
    let gaps = Command::new(exe)
        .arg("comparison")
        .arg(&db)
        .output()
        .unwrap();
    let output = String::from_utf8_lossy(&gaps.stdout);
    assert!(output.contains("state=FinishedWithErrors"), "{output}");
    let changed = output
        .lines()
        .find(|line| line.contains("path=\"first\""))
        .unwrap();
    assert!(changed.contains("stage=Failed"), "{changed}");
    assert!(changed.contains("full_hash=None"), "{changed}");
    assert!(changed.contains("failure=Some"), "{changed}");
    assert!(
        Command::new(exe)
            .arg("resume")
            .arg(&db)
            .output()
            .unwrap()
            .status
            .success()
    );
    let status = Command::new(exe).arg("status").arg(&db).output().unwrap();
    let output = String::from_utf8_lossy(&status.stdout);
    assert!(output.contains("state=Invalidated"), "{output}");
    let stale = Command::new(exe)
        .arg("comparison")
        .arg(&db)
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&stale.stdout).contains("stale=true"));
    let observer = Job::open(&db).unwrap();
    assert_eq!(observer.progress().unwrap().generation, generation + 1);
    assert!(observer.comparisons().unwrap().iter().all(|r| r.stale));
    drop(observer);
    assert!(
        Command::new(exe)
            .arg("compare")
            .arg(&db)
            .output()
            .unwrap()
            .status
            .success()
    );
    let observer = Job::open(&db).unwrap();
    assert_eq!(
        observer.comparison_progress().unwrap().state,
        ComparisonState::Complete
    );
    assert_eq!(observer.comparison_progress().unwrap().unique_sizes, 2);
    assert_eq!(observer.inventory().unwrap().len(), 3);
}

#[test]
fn killed_comparison_preserves_completed_evidence_and_resumes_unfinished_hash() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    fs::create_dir(&source).unwrap();
    for name in ["a-small", "b-small"] {
        fs::write(source.join(name), b"already completed synthetic content").unwrap();
    }
    for name in ["c-large", "d-large"] {
        fs::write(source.join(name), vec![42; 128 * 1024]).unwrap();
    }
    let db = temp.path().join("job.sqlite");
    let exe = env!("CARGO_BIN_EXE_folderloom");
    assert!(
        Command::new(exe)
            .arg("scan")
            .arg(&db)
            .arg(&source)
            .output()
            .unwrap()
            .status
            .success()
    );
    let generation = Job::open(&db).unwrap().progress().unwrap().generation;
    let mut child = RunningChild(
        Command::new(exe)
            .arg("compare")
            .arg(&db)
            .args(["--byte-budget", "4096", "--interactive"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let mut reader = BufReader::new(child.0.stdout.take().unwrap());
    let mut completed = Vec::new();
    let mut unfinished = None;
    for _ in 0..200 {
        let mut line = String::new();
        assert!(reader.read_line(&mut line).unwrap() > 0);
        let observer = Job::open(&db).unwrap();
        let records = observer.comparisons().unwrap();
        completed = records
            .iter()
            .filter(|r| r.stage == ComparisonStage::FullHash)
            .map(|r| (r.relative_path.clone(), r.full_hash.clone()))
            .collect();
        unfinished = records.into_iter().find(|r| {
            r.stage == ComparisonStage::Hashing && r.bytes_read > 0 && r.bytes_read < r.size
        });
        if !completed.is_empty() && unfinished.is_some() {
            break;
        }
        assert!(line.contains("state=Running"), "{line}");
        child
            .0
            .stdin
            .as_mut()
            .unwrap()
            .write_all(b"next\n")
            .unwrap();
    }
    let unfinished = unfinished.expect("a full hash must have a durable incomplete checkpoint");
    assert!(!completed.is_empty());
    assert!(unfinished.sample_hash.is_some());
    assert!(unfinished.full_hash.is_none());
    let competitor = Command::new(exe).arg("compare").arg(&db).output().unwrap();
    assert!(!competitor.status.success());
    child.0.kill().unwrap();
    child.0.wait().unwrap();

    let saved = Command::new(exe)
        .arg("comparison")
        .arg(&db)
        .output()
        .unwrap();
    assert!(saved.status.success());
    let output = String::from_utf8_lossy(&saved.stdout);
    assert!(output.contains("stage=Hashing"), "{output}");
    assert!(output.contains("full_hash=None"), "{output}");
    let observer = Job::open(&db).unwrap();
    assert_eq!(
        observer.comparison_progress().unwrap().state,
        ComparisonState::Running
    );
    assert_eq!(observer.progress().unwrap().generation, generation);
    let saved_records = observer.comparisons().unwrap();
    for (path, hash) in &completed {
        assert_eq!(
            saved_records
                .iter()
                .find(|r| &r.relative_path == path)
                .unwrap()
                .full_hash,
            *hash
        );
    }
    let saved_unfinished = saved_records
        .iter()
        .find(|r| r.relative_path == unfinished.relative_path)
        .unwrap();
    assert_eq!(saved_unfinished.bytes_read, unfinished.bytes_read);
    assert!(saved_unfinished.full_hash.is_none());
    drop(observer);

    let mut paused = RunningChild(
        Command::new(exe)
            .arg("compare")
            .arg(&db)
            .args(["--byte-budget", "4096", "--interactive"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let mut line = String::new();
    let mut pause_reader = BufReader::new(paused.0.stdout.take().unwrap());
    pause_reader.read_line(&mut line).unwrap();
    paused
        .0
        .stdin
        .take()
        .unwrap()
        .write_all(b"pause\n")
        .unwrap();
    assert!(paused.0.wait().unwrap().success());
    assert_eq!(
        Job::open(&db).unwrap().comparison_progress().unwrap().state,
        ComparisonState::Paused
    );
    let resumed = Command::new(exe).arg("compare").arg(&db).output().unwrap();
    assert!(
        resumed.status.success(),
        "{}",
        String::from_utf8_lossy(&resumed.stderr)
    );
    let observer = Job::open(&db).unwrap();
    assert_eq!(observer.progress().unwrap().generation, generation);
    assert_eq!(
        observer.comparison_progress().unwrap().state,
        ComparisonState::Complete
    );
    assert_eq!(observer.comparison_progress().unwrap().full_hashes, 4);
    let records = observer.comparisons().unwrap();
    for (path, hash) in completed {
        assert_eq!(
            records
                .iter()
                .find(|r| r.relative_path == path)
                .unwrap()
                .full_hash,
            hash
        );
    }
    let large: Vec<_> = records.iter().filter(|r| r.size == 128 * 1024).collect();
    assert_eq!(large.len(), 2);
    assert_eq!(large[0].full_hash, large[1].full_hash);
    assert!(large[0].full_hash.is_some());
}

#[test]
fn comparison_commands_save_content_evidence_and_retain_separate_placements() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    fs::create_dir_all(source.join("nested")).unwrap();
    fs::write(source.join("first"), b"same synthetic content").unwrap();
    fs::write(source.join("nested/second"), b"same synthetic content").unwrap();
    fs::write(source.join("unique"), b"unique").unwrap();
    let db = temp.path().join("job.sqlite");
    let exe = env!("CARGO_BIN_EXE_folderloom");
    let scan = Command::new(exe)
        .arg("scan")
        .arg(&db)
        .arg(&source)
        .output()
        .unwrap();
    assert!(scan.status.success());
    let compared = Command::new(exe)
        .arg("compare")
        .arg(&db)
        .args(["--byte-budget", "4096"])
        .output()
        .unwrap();
    assert!(
        compared.status.success(),
        "{}",
        String::from_utf8_lossy(&compared.stderr)
    );
    let saved = Command::new(exe)
        .arg("comparison")
        .arg(&db)
        .output()
        .unwrap();
    assert!(saved.status.success());
    let output = String::from_utf8_lossy(&saved.stdout);
    assert!(output.contains("state=Complete"), "{output}");
    assert!(output.contains(&source.display().to_string()), "{output}");
    for path in ["first", "nested/second", "unique"] {
        assert!(output.contains(path), "{output}");
    }
    assert!(output.contains("blake3"), "{output}");
    assert!(
        output.contains("not authority for copying or deletion"),
        "{output}"
    );
    assert_eq!(Job::open(&db).unwrap().inventory().unwrap().len(), 5);
}

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

#[test]
fn cli_reopens_root_gaps_separately_from_file_and_inventory_evidence() {
    let temp = tempfile::tempdir().unwrap();
    let empty = temp.path().join("empty");
    let good = temp.path().join("good");
    fs::create_dir(&empty).unwrap();
    fs::create_dir(&good).unwrap();
    for name in ["a", "b"] {
        fs::write(good.join(name), b"abc").unwrap();
    }
    let db = temp.path().join("job.sqlite");
    let exe = env!("CARGO_BIN_EXE_folderloom");
    assert!(
        Command::new(exe)
            .arg("scan")
            .arg(&db)
            .arg(&empty)
            .arg(&good)
            .output()
            .unwrap()
            .status
            .success()
    );
    fs::remove_dir(&empty).unwrap();
    assert_eq!(
        Command::new(exe)
            .arg("compare")
            .arg(&db)
            .output()
            .unwrap()
            .status
            .code(),
        Some(2)
    );
    let before = fs::read(&db).unwrap();
    for command in ["status", "comparison"] {
        let saved = Command::new(exe).arg(command).arg(&db).output().unwrap();
        assert!(saved.status.success());
        let output = String::from_utf8_lossy(&saved.stdout);
        assert!(output.contains("state=FinishedWithErrors"), "{output}");
        assert!(output.contains("full_hashes=2"), "{output}");
        assert!(output.contains("failures=1 root_failures=1"), "{output}");
        let gap = output
            .lines()
            .find(|line| line.starts_with("comparison root failure "))
            .expect("saved root gap details");
        assert!(gap.contains("generation=1 root=1 stale=false"), "{gap}");
        assert!(
            output.contains(&format!(
                "path={:?}",
                fs::canonicalize(temp.path()).unwrap().join("empty")
            )),
            "{output}"
        );
    }
    assert_eq!(fs::read(&db).unwrap(), before);
    let job = Job::open(&db).unwrap();
    assert_eq!(job.comparisons().unwrap().len(), 2);
    assert_eq!(job.progress().unwrap().state, State::Complete);
    assert!(job.failures().unwrap().is_empty());
}

#[test]
fn interactive_scan_and_compare_share_whitespace_eof_and_invalid_command_behavior() {
    for command in ["scan", "compare"] {
        for (input, expected_code, checkpoints) in [
            (" next \n pause \n", 0, 3),
            (" \n", 0, 2),
            ("", 0, 2),
            ("invalid\n", 1, 1),
        ] {
            let temp = tempfile::tempdir().unwrap();
            let source = temp.path().join("source");
            fs::create_dir(&source).unwrap();
            for name in ["a", "b", "c", "d"] {
                fs::write(source.join(name), b"abc").unwrap();
            }
            let db = temp.path().join("job.sqlite");
            let exe = env!("CARGO_BIN_EXE_folderloom");
            let mut runner = Command::new(exe);
            runner.arg(command).arg(&db).arg("--interactive");
            if command == "compare" {
                assert!(
                    Command::new(exe)
                        .arg("scan")
                        .arg(&db)
                        .arg(&source)
                        .output()
                        .unwrap()
                        .status
                        .success()
                );
                runner.args(["--byte-budget", "1"]);
            } else {
                runner.arg(&source).args(["--batch-size", "1"]);
            }
            let mut child = runner
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            child
                .stdin
                .take()
                .unwrap()
                .write_all(input.as_bytes())
                .unwrap();
            let output = child.wait_with_output().unwrap();
            assert_eq!(
                output.status.code(),
                Some(expected_code),
                "command={command}, input={input:?}"
            );
            assert_eq!(
                String::from_utf8_lossy(&output.stdout)
                    .matches("checkpoint")
                    .count(),
                checkpoints
            );
            let job = Job::open(&db).unwrap();
            if command == "compare" {
                assert_eq!(
                    job.comparison_progress().unwrap().state,
                    ComparisonState::Paused
                );
            } else {
                assert_eq!(job.progress().unwrap().state, State::Paused);
            }
            if expected_code == 1 {
                assert!(String::from_utf8_lossy(&output.stderr).contains("expected next or pause"));
            }
        }
    }
}
