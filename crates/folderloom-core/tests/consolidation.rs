use folderloom_core::{ConsolidationPlan, Job, Mapping, Options, PlanDecision, State};
use std::{
    fs,
    os::unix::fs::{MetadataExt, PermissionsExt, symlink},
    path::{Path, PathBuf},
};

struct Fixture {
    _temp: tempfile::TempDir,
    base: PathBuf,
    database: PathBuf,
    sources: Vec<PathBuf>,
    destination: PathBuf,
}
impl Fixture {
    fn new(count: usize) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let base = fs::canonicalize(temp.path()).unwrap();
        let destination = base.join("destination");
        fs::create_dir(&destination).unwrap();
        let sources = (0..count)
            .map(|i| {
                let p = base.join(format!("source-{i}"));
                fs::create_dir(&p).unwrap();
                p
            })
            .collect();
        Self {
            database: base.join("inventory.sqlite"),
            base,
            sources,
            destination,
            _temp: temp,
        }
    }
    fn scan(&self) {
        let mut job = Job::create(&self.database, &self.sources, Options::default()).unwrap();
        while job.step(128).unwrap().state == State::Running {}
        assert_eq!(job.progress().unwrap().state, State::Complete);
    }
    fn mappings(&self) -> Vec<Mapping> {
        Job::open(&self.database)
            .unwrap()
            .roots()
            .unwrap()
            .into_iter()
            .map(|r| Mapping {
                root_id: r.id,
                source_relative: PathBuf::new(),
                destination_relative: PathBuf::new(),
            })
            .collect()
    }
    fn plan(&self) -> ConsolidationPlan {
        let job = Job::plan(&self.database, &self.destination, &self.mappings()).unwrap();
        job.consolidation_plan().unwrap().unwrap()
    }
    fn run(&self, revision: i64) -> ConsolidationPlan {
        Job::approve_plan(&self.database, revision).unwrap();
        let mut job = Job::execute(&self.database, revision).unwrap();
        finish(&mut job)
    }
}
fn finish(job: &mut Job) -> ConsolidationPlan {
    loop {
        let p = job.execute_step(1).unwrap();
        if p.state != "running" {
            return p;
        }
    }
}

#[test]
fn durable_preview_deduplicates_only_same_target_and_preserves_collision_names() {
    let f = Fixture::new(2);
    for s in &f.sources {
        fs::write(s.join("same.txt"), b"same content").unwrap();
    }
    fs::write(f.sources[0].join("different.txt"), b"first").unwrap();
    fs::write(f.sources[1].join("different.txt"), b"second").unwrap();
    fs::create_dir(f.sources[1].join("nested")).unwrap();
    fs::write(f.sources[1].join("nested/same.txt"), b"same content").unwrap();
    fs::write(f.destination.join("different.txt"), b"destination original").unwrap();
    f.scan();
    let p = f.plan();
    assert_eq!(
        fs::read_dir(&f.destination).unwrap().count(),
        1,
        "preview must not write destination"
    );
    let same = p
        .items
        .iter()
        .find(|i| i.destination_relative == Path::new("same.txt"))
        .unwrap();
    assert_eq!(same.contributors.len(), 2);
    let targets: Vec<_> = p
        .items
        .iter()
        .filter(|i| i.action == "copy")
        .map(|i| i.destination_relative.clone())
        .collect();
    assert!(targets.contains(&PathBuf::from("different (source 1).txt")));
    assert!(targets.contains(&PathBuf::from("different (source 2).txt")));
    assert!(targets.contains(&PathBuf::from("nested/same.txt")));
    let reopened = Job::open(&f.database)
        .unwrap()
        .consolidation_plan()
        .unwrap()
        .unwrap();
    assert_eq!(reopened.revision, p.revision);
    assert_eq!(
        reopened
            .items
            .iter()
            .map(|i| &i.destination_relative)
            .collect::<Vec<_>>(),
        p.items
            .iter()
            .map(|i| &i.destination_relative)
            .collect::<Vec<_>>()
    );
    assert!(
        Job::execute(&f.database, p.revision).is_err(),
        "approval must be explicit"
    );
    let done = f.run(p.revision);
    assert_eq!(done.state, "complete", "{done:#?}");
    assert_eq!(
        fs::read(f.destination.join("same.txt")).unwrap(),
        b"same content"
    );
    assert_eq!(
        fs::read(f.destination.join("nested/same.txt")).unwrap(),
        b"same content"
    );
    assert_eq!(
        fs::read(f.destination.join("different.txt")).unwrap(),
        b"destination original"
    );
    assert_eq!(
        fs::read(f.sources[0].join("different.txt")).unwrap(),
        b"first"
    );
    assert_eq!(fs::read_dir(&f.sources[0]).unwrap().count(), 2);
}

#[test]
fn decisions_revoke_approval_and_revisions_survive_reopen() {
    let f = Fixture::new(1);
    fs::write(f.sources[0].join("a"), b"a").unwrap();
    fs::write(f.sources[0].join("b"), b"b").unwrap();
    f.scan();
    let p = f.plan();
    Job::approve_plan(&f.database, p.revision).unwrap();
    let id = p
        .items
        .iter()
        .find(|i| i.source_relative == Path::new("b"))
        .unwrap()
        .id;
    let edited = Job::update_plan(
        &f.database,
        p.revision,
        &[PlanDecision {
            item_id: id,
            included: false,
        }],
    )
    .unwrap();
    assert_eq!(edited.revision, p.revision + 1);
    assert_eq!(edited.approved_revision, None);
    assert!(Job::execute(&f.database, p.revision).is_err());
    assert!(Job::execute(&f.database, edited.revision).is_err());
    let done = f.run(edited.revision);
    assert_eq!(done.state, "complete");
    assert!(f.destination.join("a").exists());
    assert!(!f.destination.join("b").exists());
    assert_eq!(
        done.items.iter().find(|i| i.id == id).unwrap().status,
        "skipped"
    );
}

#[test]
fn wrapper_mapping_and_explicit_subfolders_preserve_separate_copies() {
    let f = Fixture::new(2);
    for s in &f.sources {
        fs::create_dir(s.join("wrapper")).unwrap();
        fs::write(s.join("wrapper/a"), b"same").unwrap();
    }
    f.scan();
    let mut mappings = f.mappings();
    mappings[0].source_relative = "wrapper".into();
    mappings[0].destination_relative = "kept/one".into();
    mappings[1].source_relative = "wrapper".into();
    mappings[1].destination_relative = "kept/two".into();
    let p = Job::plan(&f.database, &f.destination, &mappings)
        .unwrap()
        .consolidation_plan()
        .unwrap()
        .unwrap();
    let done = f.run(p.revision);
    assert_eq!(done.state, "complete", "{done:#?}");
    assert_eq!(fs::read(f.destination.join("kept/one/a")).unwrap(), b"same");
    assert_eq!(fs::read(f.destination.join("kept/two/a")).unwrap(), b"same");
    assert!(!f.destination.join("kept/one/wrapper").exists());
}

#[test]
fn changed_source_after_approval_fails_without_publishing() {
    let f = Fixture::new(1);
    fs::write(f.sources[0].join("file"), b"before").unwrap();
    f.scan();
    let p = f.plan();
    Job::approve_plan(&f.database, p.revision).unwrap();
    fs::write(f.sources[0].join("file"), b"after!").unwrap();
    let done = finish(&mut Job::execute(&f.database, p.revision).unwrap());
    assert_eq!(done.state, "finished_with_errors");
    assert!(!f.destination.join("file").exists());
    assert!(
        done.items
            .iter()
            .any(|i| i.status == "failed" && i.detail.as_deref().unwrap().contains("changed"))
    );
    assert_eq!(
        Job::open(&f.database)
            .unwrap()
            .consolidation_plan()
            .unwrap()
            .unwrap()
            .state,
        "finished_with_errors"
    );
}

#[test]
fn destination_appearing_after_approval_is_never_overwritten_or_renamed() {
    let f = Fixture::new(1);
    fs::write(f.sources[0].join("file"), b"incoming").unwrap();
    f.scan();
    let p = f.plan();
    Job::approve_plan(&f.database, p.revision).unwrap();
    fs::write(f.destination.join("file"), b"keep me").unwrap();
    let done = finish(&mut Job::execute(&f.database, p.revision).unwrap());
    assert_eq!(done.state, "finished_with_errors");
    assert_eq!(fs::read(f.destination.join("file")).unwrap(), b"keep me");
    let again = finish(&mut Job::execute(&f.database, p.revision).unwrap());
    assert_eq!(again.state, "finished_with_errors");
    assert!(!f.destination.join("file (source 1)").exists());
}

#[test]
fn identical_existing_destination_rechecked_and_not_mutated() {
    let f = Fixture::new(1);
    fs::write(f.sources[0].join("file"), b"same").unwrap();
    fs::write(f.destination.join("file"), b"same").unwrap();
    let original = fs::metadata(f.destination.join("file")).unwrap();
    f.scan();
    let p = f.plan();
    assert_eq!(
        p.items
            .iter()
            .find(|i| i.source_relative == Path::new("file"))
            .unwrap()
            .action,
        "already_present"
    );
    let done = f.run(p.revision);
    assert_eq!(done.state, "complete");
    assert_eq!(
        fs::metadata(f.destination.join("file")).unwrap().ino(),
        original.ino()
    );
    fs::write(f.destination.join("file"), b"evil").unwrap();
    let retried = finish(&mut Job::execute(&f.database, p.revision).unwrap());
    assert_eq!(retried.state, "finished_with_errors");
    assert_eq!(fs::read(f.destination.join("file")).unwrap(), b"evil");
}

#[test]
fn pause_drop_and_resume_reuses_approved_paths_and_verifies_completed_files() {
    let f = Fixture::new(1);
    fs::write(f.sources[0].join("a"), b"a").unwrap();
    fs::write(f.sources[0].join("b"), b"b").unwrap();
    f.scan();
    let p = f.plan();
    Job::approve_plan(&f.database, p.revision).unwrap();
    let mut worker = Job::execute(&f.database, p.revision).unwrap();
    worker.execute_step(2).unwrap();
    let paused = worker.pause_execution().unwrap();
    assert_eq!(paused.state, "paused");
    drop(worker);
    let reopened = Job::open(&f.database)
        .unwrap()
        .consolidation_plan()
        .unwrap()
        .unwrap();
    assert_eq!(reopened.state, "paused");
    let done = finish(&mut Job::execute(&f.database, p.revision).unwrap());
    assert_eq!(done.state, "complete", "{done:#?}");
    fs::remove_file(f.destination.join("a")).unwrap();
    let stale = finish(&mut Job::execute(&f.database, p.revision).unwrap());
    assert_eq!(stale.state, "finished_with_errors");
    assert!(
        !f.destination.join("a").exists(),
        "changed published output must require a new reviewed plan"
    );
}

#[test]
fn changed_generation_overlap_and_escape_are_rejected() {
    let f = Fixture::new(1);
    fs::create_dir(f.sources[0].join("sub")).unwrap();
    fs::write(f.sources[0].join("file"), b"a").unwrap();
    f.scan();
    let p = f.plan();
    assert!(Job::plan(&f.database, &f.sources[0].join("sub"), &f.mappings()).is_err());
    let mut mapping = f.mappings();
    mapping[0].destination_relative = "../escape".into();
    assert!(Job::plan(&f.database, &f.destination, &mapping).is_err());
    mapping[0].destination_relative = PathBuf::new();
    mapping[0].source_relative = "../source-0".into();
    assert!(Job::plan(&f.database, &f.destination, &mapping).is_err());
    let mut scan = Job::resume(&f.database).unwrap();
    while scan.step(128).unwrap().state == State::Running {}
    drop(scan);
    assert!(Job::approve_plan(&f.database, p.revision).is_err());
    assert!(Job::execute(&f.database, p.revision).is_err());
}

#[test]
fn symlinks_are_held_and_destination_ancestor_replacement_cannot_escape() {
    let f = Fixture::new(1);
    fs::create_dir(f.sources[0].join("dir")).unwrap();
    fs::write(f.sources[0].join("dir/file"), b"a").unwrap();
    symlink("dir/file", f.sources[0].join("link")).unwrap();
    fs::create_dir(f.destination.join("dir")).unwrap();
    f.scan();
    let p = f.plan();
    let link = p
        .items
        .iter()
        .find(|i| i.source_relative == Path::new("link"))
        .unwrap();
    assert_eq!(link.action, "blocked");
    assert!(!link.included);
    Job::approve_plan(&f.database, p.revision).unwrap();
    fs::remove_dir(f.destination.join("dir")).unwrap();
    let outside = f.base.join("outside");
    fs::create_dir(&outside).unwrap();
    symlink(&outside, f.destination.join("dir")).unwrap();
    let done = finish(&mut Job::execute(&f.database, p.revision).unwrap());
    assert_eq!(done.state, "finished_with_errors");
    assert_eq!(fs::read_dir(&outside).unwrap().count(), 0);
    assert!(!f.destination.join("link").exists());
}

#[test]
fn file_mode_and_modification_time_preserved_without_source_inode_alias() {
    let f = Fixture::new(1);
    let source = f.sources[0].join("file");
    fs::write(&source, b"metadata").unwrap();
    fs::set_permissions(&source, fs::Permissions::from_mode(0o640)).unwrap();
    let timestamp = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1234567890);
    fs::File::open(&source)
        .unwrap()
        .set_times(fs::FileTimes::new().set_modified(timestamp))
        .unwrap();
    f.scan();
    let p = f.plan();
    let done = f.run(p.revision);
    assert_eq!(done.state, "complete", "{done:#?}");
    let from = fs::metadata(&source).unwrap();
    let to = fs::metadata(f.destination.join("file")).unwrap();
    assert_eq!(from.mode() & 0o777, to.mode() & 0o777);
    assert_eq!(from.modified().unwrap(), to.modified().unwrap());
    assert_ne!(from.ino(), to.ino());
}

#[test]
fn scanned_destination_is_excluded_from_incoming_sources() {
    let mut f = Fixture::new(1);
    fs::write(f.sources[0].join("incoming"), b"a").unwrap();
    fs::write(f.destination.join("existing"), b"b").unwrap();
    f.sources.push(f.destination.clone());
    f.scan();
    let p = f.plan();
    assert_eq!(p.mappings.len(), 1);
    assert!(
        !p.items
            .iter()
            .any(|i| i.source_relative == Path::new("existing"))
    );
    assert_eq!(f.run(p.revision).state, "complete");
}

#[cfg(target_os = "macos")]
#[test]
fn macos_quarantine_extended_attribute_survives_copy() {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    let f = Fixture::new(1);
    let source = f.sources[0].join("file");
    fs::write(&source, b"quarantined bytes").unwrap();
    let name = CString::new("com.apple.quarantine").unwrap();
    let value = b"0081;12345678;FolderloomTest;";
    let source_c = CString::new(source.as_os_str().as_bytes()).unwrap();
    assert_eq!(
        unsafe {
            libc::setxattr(
                source_c.as_ptr(),
                name.as_ptr(),
                value.as_ptr().cast(),
                value.len(),
                0,
                0,
            )
        },
        0
    );
    f.scan();
    let p = f.plan();
    let done = f.run(p.revision);
    assert_eq!(done.state, "complete", "{done:#?}");
    let destination = CString::new(f.destination.join("file").as_os_str().as_bytes()).unwrap();
    let mut output = [0u8; 256];
    let n = unsafe {
        libc::getxattr(
            destination.as_ptr(),
            name.as_ptr(),
            output.as_mut_ptr().cast(),
            output.len(),
            0,
            0,
        )
    };
    assert_eq!(n as usize, value.len());
    assert_eq!(&output[..n as usize], value);
}

#[test]
fn case_and_unicode_aliases_receive_durable_separate_names() {
    let f = Fixture::new(2);
    fs::write(f.sources[0].join("File.txt"), b"first").unwrap();
    fs::write(f.sources[1].join("file.txt"), b"second").unwrap();
    fs::write(f.sources[0].join("é.txt"), b"same").unwrap();
    fs::write(f.sources[1].join("e\u{301}.txt"), b"same").unwrap();
    f.scan();
    let p = f.plan();
    let files: Vec<_> = p.items.iter().filter(|i| i.action == "copy").collect();
    assert_eq!(
        files.len(),
        4,
        "distinct intended spellings retain distinct placements"
    );
    assert_eq!(
        files
            .iter()
            .filter(|i| i
                .destination_relative
                .to_string_lossy()
                .contains("source 2"))
            .count(),
        2
    );
    let done = f.run(p.revision);
    assert_eq!(done.state, "complete", "{done:#?}");
    assert_eq!(fs::read_dir(&f.destination).unwrap().count(), 4);
}

#[test]
fn interrupted_copy_resumes_in_fresh_process() {
    use std::io::{Seek, SeekFrom, Write};
    use std::process::Command;
    use std::time::{Duration, Instant};
    const CHILD: &str = "FOLDERLOOM_COPY_INTERRUPT_DATABASE";
    if let Some(database) = std::env::var_os(CHILD) {
        let database = PathBuf::from(database);
        let revision = Job::open(&database)
            .unwrap()
            .consolidation_plan()
            .unwrap()
            .unwrap()
            .revision;
        let done = finish(&mut Job::execute(&database, revision).unwrap());
        assert_eq!(done.state, "complete", "{done:#?}");
        return;
    }
    let f = Fixture::new(1);
    let source = f.sources[0].join("large.bin");
    let mut file = fs::File::create(&source).unwrap();
    file.set_len(128 * 1024 * 1024).unwrap();
    file.write_all(b"synthetic start").unwrap();
    file.seek(SeekFrom::End(-20)).unwrap();
    file.write_all(b"synthetic finish").unwrap();
    file.sync_all().unwrap();
    drop(file);
    f.scan();
    let p = f.plan();
    Job::approve_plan(&f.database, p.revision).unwrap();
    let item = p.items.iter().find(|i| i.action == "copy").unwrap();
    let stage = f
        .destination
        .join(format!(".folderloom-plan-{}-item-{}.tmp", p.id, item.id));
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "interrupted_copy_resumes_in_fresh_process",
            "--nocapture",
        ])
        .env(CHILD, &f.database)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let until = Instant::now() + Duration::from_secs(20);
    loop {
        if fs::metadata(&stage).is_ok_and(|m| m.len() > 0) {
            child.kill().unwrap();
            break;
        }
        assert!(Instant::now() < until, "child did not reach staging");
        assert!(
            child.try_wait().unwrap().is_none(),
            "child completed before interruption"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    child.wait().unwrap();
    assert!(
        stage.exists(),
        "interruption left a real durable staging file"
    );
    let retry = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "interrupted_copy_resumes_in_fresh_process",
            "--nocapture",
        ])
        .env(CHILD, &f.database)
        .output()
        .unwrap();
    assert!(
        retry.status.success(),
        "{} {}",
        String::from_utf8_lossy(&retry.stdout),
        String::from_utf8_lossy(&retry.stderr)
    );
    assert!(!stage.exists());
    assert_eq!(fs::read_dir(&f.destination).unwrap().count(), 1);
    assert_eq!(
        fs::read(&source).unwrap(),
        fs::read(f.destination.join("large.bin")).unwrap()
    );
    let reopened = Job::open(&f.database)
        .unwrap()
        .consolidation_plan()
        .unwrap()
        .unwrap();
    assert_eq!(reopened.state, "complete");
    assert_eq!(reopened.revision, p.revision);
}

#[test]
fn missing_owned_stage_after_completed_publish_is_idempotently_reconciled() {
    let f = Fixture::new(1);
    fs::write(f.sources[0].join("file"), b"complete").unwrap();
    f.scan();
    let p = f.plan();
    let done = f.run(p.revision);
    assert_eq!(done.state, "complete");
    let item = done.items.iter().find(|i| i.action == "copy").unwrap();
    let metadata = fs::metadata(f.destination.join("file")).unwrap();
    // Reproduce the durable state of a crash after unlink(temp), before clearing its
    // SQLite bookkeeping. The successfully published inode and bytes remain real.
    let db = rusqlite::Connection::open(&f.database).unwrap();
    db.execute(
        "UPDATE consolidation_items SET stage=?,stage_identity=? WHERE id=?",
        rusqlite::params![
            format!(".folderloom-plan-{}-item-{}.tmp", p.id, item.id).as_bytes(),
            format!("{}:{}", metadata.dev(), metadata.ino()),
            item.id
        ],
    )
    .unwrap();
    drop(db);
    let recovered = finish(&mut Job::execute(&f.database, p.revision).unwrap());
    assert_eq!(recovered.state, "complete", "{recovered:#?}");
    assert_eq!(fs::read_dir(&f.destination).unwrap().count(), 1);
}
