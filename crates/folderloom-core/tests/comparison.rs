use folderloom_core::{ComparisonStage, ComparisonState, Job, Options, State};
use std::{fs, path::Path};
fn scan(db: &Path, source: &Path) {
    let mut job = Job::create(db, &[source.to_owned()], Options::default()).unwrap();
    while job.step(64).unwrap().state == State::Running {}
}
fn finish(job: &mut Job) {
    for _ in 0..10000 {
        if job.compare_step(1024).unwrap().state != ComparisonState::Running {
            return;
        }
    }
    panic!("did not finish");
}
#[test]
fn identical_placements_have_full_evidence_and_unique_files_stay_in_inventory() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    fs::create_dir(&source).unwrap();
    fs::write(source.join("a"), b"abc").unwrap();
    fs::write(source.join("b"), b"abc").unwrap();
    fs::write(source.join("unique"), b"unique").unwrap();
    let db = temp.path().join("job.sqlite");
    scan(&db, &source);
    let mut job = Job::compare(&db).unwrap();
    finish(&mut job);
    assert_eq!(job.inventory().unwrap().len(), 4);
    let records = job.comparisons().unwrap();
    assert_eq!(records.len(), 3);
    assert_eq!(records[0].stage, ComparisonStage::FullHash);
    assert_eq!(
        records[0].full_hash.as_deref(),
        Some("6437b3ac38465133ffb63b75273a8db548c558465d79db03fd359c6cd5bd9d85")
    );
    assert_eq!(records[0].full_hash, records[1].full_hash);
    assert_eq!(records[2].stage, ComparisonStage::UniqueSize);
    assert!(records[2].full_hash.is_none());
    assert_eq!(
        job.comparison_progress().unwrap().state,
        ComparisonState::Complete
    );
}

#[test]
fn matching_edges_never_prove_equal_middle_and_reads_are_byte_bounded() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    fs::create_dir(&source).unwrap();
    let a = vec![b'a'; 20000];
    let mut b = a.clone();
    b[10000] = b'b';
    fs::write(source.join("a"), a).unwrap();
    fs::write(source.join("b"), b).unwrap();
    let db = temp.path().join("job.sqlite");
    scan(&db, &source);
    let mut job = Job::compare(&db).unwrap();
    let mut previous = 0;
    for _ in 0..10000 {
        let p = job.compare_step(127).unwrap();
        assert!(p.bytes_read - previous <= 127);
        previous = p.bytes_read;
        if p.state != ComparisonState::Running {
            break;
        }
    }
    let records = job.comparisons().unwrap();
    assert_eq!(records[0].sample_hash, records[1].sample_hash);
    assert_ne!(records[0].full_hash, records[1].full_hash);
    assert!(records.iter().all(|r| r.stage == ComparisonStage::FullHash));
    assert_eq!(previous, 56384); // 2 * (8192 sample bytes + 20000 full bytes)
}

#[test]
fn pause_reopen_preserves_completed_evidence_and_restarts_unfinished_file() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    fs::create_dir(&source).unwrap();
    for (name, size) in [("a", 3), ("b", 3), ("c", 20000), ("d", 20000)] {
        fs::write(source.join(name), vec![b'x'; size]).unwrap();
    }
    let db = temp.path().join("job.sqlite");
    scan(&db, &source);
    let mut job = Job::compare(&db).unwrap();
    for _ in 0..1000 {
        job.compare_step(1024).unwrap();
        if job
            .comparisons()
            .unwrap()
            .iter()
            .any(|r| r.stage == ComparisonStage::Hashing)
        {
            break;
        }
    }
    let before = job.comparisons().unwrap();
    assert_eq!(before[0].stage, ComparisonStage::FullHash);
    assert_eq!(before[2].stage, ComparisonStage::Hashing);
    assert!(before[2].bytes_read > 0);
    assert!(before[2].full_hash.is_none());
    job.pause_comparison().unwrap();
    drop(job);
    let observer = Job::open(&db).unwrap();
    assert_eq!(
        observer.comparison_progress().unwrap().state,
        ComparisonState::Paused
    );
    assert_eq!(
        observer.comparisons().unwrap()[0].full_hash,
        before[0].full_hash
    );
    drop(observer);
    let mut job = Job::compare(&db).unwrap();
    assert_eq!(job.comparisons().unwrap()[2].bytes_read, 0);
    assert_eq!(job.comparisons().unwrap()[0].full_hash, before[0].full_hash);
    finish(&mut job);
    assert_eq!(job.comparison_progress().unwrap().full_hashes, 4);
    drop(job);
    let mut job = Job::compare(&db).unwrap();
    let bytes = job.comparison_progress().unwrap().bytes_read;
    finish(&mut job);
    assert_eq!(job.comparison_progress().unwrap().bytes_read, bytes);
}

#[test]
fn changed_missing_link_replaced_and_unreadable_sources_leave_explicit_gaps() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    fs::create_dir(&source).unwrap();
    for name in ["changed", "missing", "linked", "blocked"] {
        fs::write(source.join(name), b"old").unwrap();
    }
    for name in ["good-a", "good-b"] {
        fs::write(source.join(name), b"stable").unwrap();
    }
    let db = temp.path().join("job.sqlite");
    scan(&db, &source);
    fs::write(source.join("changed"), b"new").unwrap();
    fs::remove_file(source.join("missing")).unwrap();
    fs::remove_file(source.join("linked")).unwrap();
    symlink("good-a", source.join("linked")).unwrap();
    fs::set_permissions(source.join("blocked"), fs::Permissions::from_mode(0o000)).unwrap();
    let mut job = Job::compare(&db).unwrap();
    finish(&mut job);
    let records = job.comparisons().unwrap();
    for name in ["changed", "missing", "linked", "blocked"] {
        let r = records
            .iter()
            .find(|r| r.relative_path == Path::new(name))
            .unwrap();
        assert_eq!(r.stage, ComparisonStage::Failed);
        assert!(r.failure.is_some());
        assert!(r.full_hash.is_none());
    }
    assert_eq!(job.comparison_progress().unwrap().full_hashes, 2);
    assert_eq!(
        job.comparison_progress().unwrap().state,
        ComparisonState::FinishedWithErrors
    );
    assert_eq!(job.inventory().unwrap().len(), 7);
    fs::set_permissions(source.join("blocked"), fs::Permissions::from_mode(0o600)).unwrap();
}

#[test]
fn cached_unique_and_full_evidence_is_validated_and_scan_resume_invalidates_generations() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    fs::create_dir(&source).unwrap();
    fs::write(source.join("a"), b"abc").unwrap();
    fs::write(source.join("b"), b"abc").unwrap();
    fs::write(source.join("unique"), b"larger unique").unwrap();
    let db = temp.path().join("job.sqlite");
    scan(&db, &source);
    let mut job = Job::compare(&db).unwrap();
    finish(&mut job);
    drop(job);
    fs::write(source.join("a"), b"xyz").unwrap();
    fs::remove_file(source.join("unique")).unwrap();
    let mut job = Job::compare(&db).unwrap();
    finish(&mut job);
    let records = job.comparisons().unwrap();
    assert_eq!(records[0].stage, ComparisonStage::Failed);
    assert_eq!(records[2].stage, ComparisonStage::Failed);
    assert!(records[0].full_hash.is_none());
    drop(job);
    let mut job = Job::resume(&db).unwrap();
    assert_eq!(
        job.comparison_progress().unwrap().state,
        ComparisonState::Invalidated
    );
    assert!(job.comparisons().unwrap().iter().all(|r| r.stale));
    while job.step(32).unwrap().state == State::Running {}
    drop(job);
    let mut job = Job::compare(&db).unwrap();
    finish(&mut job);
    let records = job.comparisons().unwrap();
    assert!(records.iter().any(|r| r.generation == 1 && r.stale));
    assert!(records.iter().any(|r| r.generation == 2 && !r.stale));
}

#[test]
fn comparison_preserves_names_contents_mtimes_links_and_exclusions() {
    use std::{collections::BTreeMap, os::unix::fs::symlink};
    type Snapshot = BTreeMap<
        std::path::PathBuf,
        (
            std::time::SystemTime,
            Option<Vec<u8>>,
            Option<std::path::PathBuf>,
        ),
    >;
    fn snapshot(source: &Path) -> Snapshot {
        fs::read_dir(source)
            .unwrap()
            .map(|e| {
                let p = e.unwrap().path();
                let m = fs::symlink_metadata(&p).unwrap();
                (
                    p.clone(),
                    (
                        m.modified().unwrap(),
                        m.is_file().then(|| fs::read(&p).unwrap()),
                        m.is_symlink().then(|| fs::read_link(&p).unwrap()),
                    ),
                )
            })
            .collect()
    }
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    fs::create_dir(&source).unwrap();
    for name in [".hidden", "a", "n�"] {
        fs::write(source.join(name), b"abc").unwrap();
    }
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::ffi::OsStringExt;
        fs::write(
            source.join(std::ffi::OsString::from_vec(vec![b'n', 0xff])),
            b"abc",
        )
        .unwrap();
    }
    fs::create_dir(source.join("empty")).unwrap();
    symlink("a", source.join("link")).unwrap();
    let before = snapshot(&source);
    let db = temp.path().join("job.sqlite");
    let mut job = Job::create(
        &db,
        std::slice::from_ref(&source),
        Options {
            empty_folders: folderloom_core::EmptyFolders::Ignore,
        },
    )
    .unwrap();
    while job.step(32).unwrap().state == State::Running {}
    let inventory = job.inventory().unwrap();
    drop(job);
    let mut job = Job::compare(&db).unwrap();
    finish(&mut job);
    assert_eq!(snapshot(&source), before);
    let after = job.inventory().unwrap();
    assert_eq!(after.len(), inventory.len());
    assert!(
        after
            .iter()
            .any(|r| r.relative_path == Path::new("empty") && r.excluded)
    );
    assert!(
        after
            .iter()
            .any(|r| r.entry_type == folderloom_core::EntryType::Symlink)
    );
    assert_eq!(
        job.comparison_progress().unwrap().full_hashes,
        if cfg!(target_os = "linux") { 4 } else { 3 }
    );
}

#[test]
fn legacy_inventory_is_readable_and_migrates_without_inventing_identity_evidence() {
    use std::os::unix::fs::MetadataExt;
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    fs::create_dir(&source).unwrap();
    fs::write(source.join("a"), b"abc").unwrap();
    let source = fs::canonicalize(source).unwrap();
    let db = temp.path().join("job.sqlite");
    // Construct a milestone-1 fixture; all assertions use the public Job boundary.
    let fixture = rusqlite::Connection::open(&db).unwrap();
    fixture.execute_batch("PRAGMA user_version=1; CREATE TABLE job(id INTEGER PRIMARY KEY,state TEXT,generation INTEGER,ignore_empty INTEGER);INSERT INTO job VALUES(1,'complete',1,0);CREATE TABLE roots(id INTEGER PRIMARY KEY,path BLOB,identity TEXT);CREATE TABLE entries(root INTEGER,path BLOB,kind TEXT,size INTEGER,modified TEXT,target BLOB,excluded INTEGER DEFAULT 0,generation INTEGER,PRIMARY KEY(root,path));CREATE TABLE directories(root INTEGER,path BLOB,state TEXT,stamp TEXT,PRIMARY KEY(root,path));CREATE TABLE failures(root INTEGER,path BLOB,message TEXT,generation INTEGER);").unwrap();
    use std::os::unix::ffi::OsStrExt;
    let meta = fs::metadata(&source).unwrap();
    fixture
        .execute(
            "INSERT INTO roots VALUES(1,?,?)",
            rusqlite::params![
                source.as_os_str().as_bytes(),
                format!("{}:{}", meta.dev(), meta.ino())
            ],
        )
        .unwrap();
    fixture
        .execute(
            "INSERT INTO entries VALUES(1,?,'file',3,NULL,NULL,0,1)",
            [b"a".as_slice()],
        )
        .unwrap();
    drop(fixture);
    let before = fs::read(&db).unwrap();
    let job = Job::open(&db).unwrap();
    assert_eq!(
        job.comparison_progress().unwrap().state,
        ComparisonState::NotStarted
    );
    assert!(job.comparisons().unwrap().is_empty());
    assert!(job.comparison_root_failures().unwrap().is_empty());
    assert_eq!(job.comparison_progress().unwrap().root_failures, 0);
    assert_eq!(job.inventory().unwrap().len(), 1);
    drop(job);
    assert_eq!(fs::read(&db).unwrap(), before);
    let mut job = Job::compare(&db).unwrap();
    finish(&mut job);
    let records = job.comparisons().unwrap();
    assert_eq!(records[0].stage, ComparisonStage::Failed);
    assert!(
        records[0]
            .failure
            .as_deref()
            .unwrap()
            .contains("rescan required")
    );
    drop(job);
    let mut job = Job::resume(&db).unwrap();
    while job.step(32).unwrap().state == State::Running {}
    drop(job);
    let mut job = Job::compare(&db).unwrap();
    finish(&mut job);
    assert_eq!(job.comparison_progress().unwrap().unique_sizes, 1);
    assert_eq!(job.comparison_progress().unwrap().failures, 0);
}

#[test]
fn comparison_writer_ownership_and_read_only_status_preserve_source_boundary() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    fs::create_dir(&source).unwrap();
    fs::write(source.join("a"), b"abc").unwrap();
    let db = temp.path().join("job.sqlite");
    scan(&db, &source);
    let mut writer = Job::compare(&db).unwrap();
    assert!(Job::compare(&db).is_err());
    assert!(Job::resume(&db).is_err());
    let mut reader = Job::open(&db).unwrap();
    assert_eq!(
        reader.comparison_progress().unwrap().state,
        ComparisonState::Running
    );
    assert!(reader.compare_step(1).is_err());
    assert!(reader.pause_comparison().is_err());
    assert!(writer.compare_step(0).is_err());
    finish(&mut writer);
    assert_eq!(
        reader.comparison_progress().unwrap().state,
        ComparisonState::Complete
    );
    drop(reader);
    drop(writer);
    let moved = source.join("moved.sqlite");
    fs::rename(&db, &moved).unwrap();
    let before = fs::read(&moved).unwrap();
    assert!(Job::compare(&moved).is_err());
    assert_eq!(fs::read(&moved).unwrap(), before);
    assert!(!source.join("moved.sqlite.lock").exists());
}

#[test]
fn root_and_ancestor_replacement_during_bounded_reads_never_publish_partial_evidence() {
    use std::os::unix::fs::symlink;
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    fs::create_dir_all(source.join("nested")).unwrap();
    for name in ["a", "b"] {
        fs::write(source.join("nested").join(name), vec![b'x'; 20000]).unwrap();
    }
    let db = temp.path().join("job.sqlite");
    scan(&db, &source);
    let mut job = Job::compare(&db).unwrap();
    job.compare_step(32).unwrap();
    fs::rename(source.join("nested"), temp.path().join("original")).unwrap();
    symlink(temp.path().join("original"), source.join("nested")).unwrap();
    finish(&mut job);
    let records = job.comparisons().unwrap();
    assert!(records.iter().all(|r| r.stage == ComparisonStage::Failed
        && r.full_hash.is_none()
        && r.sample_hash.is_none()));
    drop(job);
    fs::remove_file(source.join("nested")).unwrap();
    fs::rename(temp.path().join("original"), source.join("nested")).unwrap();
    let mut job = Job::resume(&db).unwrap();
    while job.step(32).unwrap().state == State::Running {}
    drop(job);
    let mut job = Job::compare(&db).unwrap();
    job.compare_step(32).unwrap();
    fs::rename(&source, temp.path().join("old-root")).unwrap();
    fs::create_dir(&source).unwrap();
    finish(&mut job);
    assert!(
        job.comparisons()
            .unwrap()
            .iter()
            .filter(|r| !r.stale)
            .all(|r| r.stage == ComparisonStage::Failed && r.full_hash.is_none())
    );
}

#[test]
fn empty_files_have_canonical_full_hash_and_nonmatching_samples_remain_filters() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    fs::create_dir(&source).unwrap();
    for (name, data) in [
        ("empty-a", b"".as_slice()),
        ("empty-b", b"".as_slice()),
        ("x", b"abc".as_slice()),
        ("y", b"xyz".as_slice()),
    ] {
        fs::write(source.join(name), data).unwrap();
    }
    let db = temp.path().join("job.sqlite");
    scan(&db, &source);
    let mut job = Job::compare(&db).unwrap();
    finish(&mut job);
    let records = job.comparisons().unwrap();
    assert_eq!(
        records[0].full_hash.as_deref(),
        Some("af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262")
    );
    assert_eq!(records[0].full_hash, records[1].full_hash);
    assert!(
        records[2..]
            .iter()
            .all(|r| r.stage == ComparisonStage::UniqueSample
                && r.full_hash.is_none()
                && r.sample_hash.is_some())
    );
    assert_ne!(records[2].sample_hash, records[3].sample_hash);
}

#[test]
fn missing_empty_root_is_a_saved_comparison_gap_while_good_roots_finish() {
    let temp = tempfile::tempdir().unwrap();
    let empty = temp.path().join("empty");
    let good = temp.path().join("good");
    fs::create_dir(&empty).unwrap();
    fs::create_dir(&good).unwrap();
    for name in ["a", "b"] {
        fs::write(good.join(name), b"abc").unwrap();
    }
    let db = temp.path().join("job.sqlite");
    let mut job = Job::create(&db, &[empty.clone(), good], Options::default()).unwrap();
    while job.step(64).unwrap().state == State::Running {}
    drop(job);
    fs::remove_dir(&empty).unwrap();
    let mut job = Job::compare(&db).unwrap();
    assert_eq!(job.comparison_progress().unwrap().root_failures, 1);
    finish(&mut job);
    drop(job);
    let job = Job::open(&db).unwrap();
    let progress = job.comparison_progress().unwrap();
    assert_eq!(progress.state, ComparisonState::FinishedWithErrors);
    assert_eq!(progress.full_hashes, 2);
    assert_eq!(progress.failures, 1);
    assert_eq!(progress.root_failures, 1);
    let gaps = job.comparison_root_failures().unwrap();
    assert_eq!(gaps.len(), 1);
    assert_eq!(gaps[0].root_id, 1);
    assert_eq!(gaps[0].generation, 1);
    assert!(!gaps[0].stale);
    assert!(!gaps[0].message.is_empty());
    assert_eq!(job.comparisons().unwrap().len(), 2);
    assert_eq!(job.progress().unwrap().state, State::Complete);
    assert!(job.failures().unwrap().is_empty());
}

#[test]
fn root_loss_during_comparison_is_saved_even_without_file_placements() {
    let temp = tempfile::tempdir().unwrap();
    let empty = temp.path().join("empty");
    let good = temp.path().join("good");
    fs::create_dir(&empty).unwrap();
    fs::create_dir(&good).unwrap();
    for name in ["a", "b"] {
        fs::write(good.join(name), b"abc").unwrap();
    }
    let db = temp.path().join("job.sqlite");
    let mut job = Job::create(&db, &[empty.clone(), good], Options::default()).unwrap();
    while job.step(64).unwrap().state == State::Running {}
    drop(job);
    let mut job = Job::compare(&db).unwrap();
    assert_eq!(job.compare_step(1).unwrap().state, ComparisonState::Running);
    assert!(job.comparison_root_failures().unwrap().is_empty());
    fs::remove_dir(&empty).unwrap();
    finish(&mut job);
    drop(job);
    let job = Job::open(&db).unwrap();
    assert_eq!(
        job.comparison_progress().unwrap().state,
        ComparisonState::FinishedWithErrors
    );
    assert_eq!(job.comparison_progress().unwrap().full_hashes, 2);
    assert_eq!(job.comparison_root_failures().unwrap().len(), 1);
    assert!(job.failures().unwrap().is_empty());
}

#[test]
fn inaccessible_empty_root_is_a_comparison_gap() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    fs::create_dir(&source).unwrap();
    let db = temp.path().join("job.sqlite");
    scan(&db, &source);
    fs::set_permissions(&source, fs::Permissions::from_mode(0o000)).unwrap();
    let mut job = Job::compare(&db).unwrap();
    finish(&mut job);
    fs::set_permissions(&source, fs::Permissions::from_mode(0o700)).unwrap();
    drop(job);
    let job = Job::open(&db).unwrap();
    assert_eq!(
        job.comparison_progress().unwrap().state,
        ComparisonState::FinishedWithErrors
    );
    assert_eq!(job.comparison_progress().unwrap().root_failures, 1);
    assert!(job.comparisons().unwrap().is_empty());
}

#[test]
fn changed_root_directory_leaves_a_gap_without_discarding_unchanged_file_evidence() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    fs::create_dir(&source).unwrap();
    for name in ["a", "b"] {
        fs::write(source.join(name), b"abc").unwrap();
    }
    let db = temp.path().join("job.sqlite");
    scan(&db, &source);
    fs::write(source.join("new-entry"), b"not inventoried").unwrap();
    let mut job = Job::compare(&db).unwrap();
    finish(&mut job);
    drop(job);
    let job = Job::open(&db).unwrap();
    let progress = job.comparison_progress().unwrap();
    assert_eq!(progress.state, ComparisonState::FinishedWithErrors);
    assert_eq!(progress.full_hashes, 2);
    assert_eq!(progress.failures, 1);
    assert_eq!(progress.root_failures, 1);
    assert!(
        job.comparison_root_failures().unwrap()[0]
            .message
            .contains("changed since inventory")
    );
    assert_eq!(job.inventory().unwrap().len(), 3);
    assert!(job.failures().unwrap().is_empty());
}

#[test]
fn empty_and_link_only_roots_cannot_silently_disappear_or_be_replaced() {
    use std::os::unix::fs::symlink;
    for link_only in [false, true] {
        for replacement in ["missing", "directory", "symlink"] {
            for during_comparison in [false, true] {
                let temp = tempfile::tempdir().unwrap();
                let source = temp.path().join("source");
                let moved = temp.path().join("original");
                fs::create_dir(&source).unwrap();
                if link_only {
                    symlink("missing-target", source.join("link")).unwrap();
                }
                let db = temp.path().join("job.sqlite");
                scan(&db, &source);
                let mut writer = during_comparison.then(|| Job::compare(&db).unwrap());
                fs::rename(&source, &moved).unwrap();
                match replacement {
                    "directory" => fs::create_dir(&source).unwrap(),
                    "symlink" => symlink(&moved, &source).unwrap(),
                    _ => {}
                }
                let mut job = writer.take().unwrap_or_else(|| Job::compare(&db).unwrap());
                finish(&mut job);
                drop(job);
                let job = Job::open(&db).unwrap();
                let progress = job.comparison_progress().unwrap();
                assert_eq!(
                    progress.state,
                    ComparisonState::FinishedWithErrors,
                    "link_only={link_only} replacement={replacement} during={during_comparison}"
                );
                assert_eq!(progress.failures, 1);
                assert_eq!(progress.root_failures, 1);
                assert_eq!(progress.files, 0);
                assert!(job.comparisons().unwrap().is_empty());
                assert_eq!(job.comparison_root_failures().unwrap().len(), 1);
                assert!(job.failures().unwrap().is_empty());
                assert_eq!(job.progress().unwrap().state, State::Complete);
                if link_only {
                    assert_eq!(
                        fs::read_link(moved.join("link")).unwrap(),
                        Path::new("missing-target")
                    );
                }
                drop(job);
                match replacement {
                    "directory" => fs::remove_dir(&source).unwrap(),
                    "symlink" => fs::remove_file(&source).unwrap(),
                    _ => {}
                }
                fs::rename(&moved, &source).unwrap();
                let mut job = Job::resume(&db).unwrap();
                assert_eq!(
                    job.comparison_progress().unwrap().state,
                    ComparisonState::Invalidated
                );
                assert!(job.comparison_root_failures().unwrap()[0].stale);
                while job.step(64).unwrap().state == State::Running {}
                drop(job);
                let mut job = Job::compare(&db).unwrap();
                finish(&mut job);
                assert_eq!(
                    job.comparison_progress().unwrap().state,
                    ComparisonState::Complete
                );
                assert_eq!(job.comparison_progress().unwrap().failures, 0);
                assert!(job.comparison_root_failures().unwrap()[0].stale);
            }
        }
    }
}
