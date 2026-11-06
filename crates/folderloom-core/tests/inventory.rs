use folderloom_core::{Job, Options, State};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[test]
fn inventory_survives_reopen_and_preserves_hidden_empty_and_unique_entries() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    fs::create_dir_all(source.join("empty")).unwrap();
    fs::write(source.join(".hidden"), b"secret").unwrap();
    fs::write(source.join("unique"), b"unique bytes").unwrap();
    let db = temp.path().join("job.sqlite");
    let mut job = Job::create(&db, &[source], Options::default()).unwrap();
    while job.step(2).unwrap().state == State::Running {}
    drop(job);
    let job = Job::open(&db).unwrap();
    assert_eq!(job.progress().unwrap().state, State::Complete);
    let mut paths: Vec<_> = job
        .inventory()
        .unwrap()
        .into_iter()
        .map(|e| e.relative_path)
        .collect();
    paths.sort();
    assert_eq!(paths, ["", ".hidden", "empty", "unique"].map(PathBuf::from));
}

#[test]
fn storage_in_source_or_its_alias_is_rejected_before_any_write() {
    use std::os::unix::fs::symlink;
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    fs::create_dir(&source).unwrap();
    let alias = temp.path().join("alias");
    symlink(&source, &alias).unwrap();
    for db in [
        source.join("job.sqlite"),
        alias.join("job.sqlite"),
        source.join("missing/job.sqlite"),
    ] {
        assert!(Job::create(&db, std::slice::from_ref(&source), Options::default()).is_err());
        assert!(!db.exists());
    }
    assert_eq!(fs::read_dir(&source).unwrap().count(), 0);
}

#[test]
fn pause_resume_refreshes_changed_and_removed_observations() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    fs::create_dir(&source).unwrap();
    fs::write(source.join("old"), b"old").unwrap();
    let db = temp.path().join("job.sqlite");
    let mut job = Job::create(&db, std::slice::from_ref(&source), Options::default()).unwrap();
    job.step(1).unwrap();
    job.pause().unwrap();
    assert_eq!(job.progress().unwrap().state, State::Paused);
    drop(job);
    fs::remove_file(source.join("old")).unwrap();
    fs::write(source.join("new"), b"new content").unwrap();
    let mut job = Job::resume(&db).unwrap();
    assert_eq!(job.progress().unwrap().generation, 2);
    assert!(
        job.inventory()
            .unwrap()
            .iter()
            .any(|e| e.relative_path == Path::new("old") && e.stale)
    );
    while job.step(1).unwrap().state == State::Running {}
    assert_eq!(job.progress().unwrap().state, State::FinishedWithErrors);
    assert!(
        job.inventory()
            .unwrap()
            .iter()
            .any(|e| e.relative_path == Path::new("new") && e.size == 11 && !e.stale)
    );
}

#[test]
fn root_loss_after_checkpoint_and_replacement_never_claim_full_coverage() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    fs::create_dir(&source).unwrap();
    fs::write(source.join("one"), b"one").unwrap();
    fs::write(source.join("two"), b"two").unwrap();
    let db = temp.path().join("job.sqlite");
    let mut job = Job::create(&db, std::slice::from_ref(&source), Options::default()).unwrap();
    job.step(1).unwrap();
    fs::rename(&source, temp.path().join("moved")).unwrap();
    while job.step(1).unwrap().state == State::Running {}
    assert_eq!(job.progress().unwrap().state, State::FinishedWithErrors);
    assert!(
        job.inventory()
            .unwrap()
            .iter()
            .all(|e| e.stale && !e.directory_complete)
    );
    assert!(!job.failures().unwrap().is_empty());
    drop(job);
    fs::create_dir(&source).unwrap();
    let mut job = Job::resume(&db).unwrap();
    while job.step(1).unwrap().state == State::Running {}
    assert_eq!(job.progress().unwrap().state, State::FinishedWithErrors);
    assert!(
        job.failures()
            .unwrap()
            .iter()
            .any(|f| f.message.contains("identity"))
    );
}

#[test]
fn a_discovered_directory_replaced_before_enumeration_is_not_silently_verified() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    fs::create_dir_all(source.join("child")).unwrap();
    let db = temp.path().join("job.sqlite");
    let mut job = Job::create(&db, std::slice::from_ref(&source), Options::default()).unwrap();
    job.step(2).unwrap(); // root directory reached EOF; child is still pending
    fs::rename(source.join("child"), temp.path().join("original-child")).unwrap();
    fs::create_dir(source.join("child")).unwrap();
    while job.step(2).unwrap().state == State::Running {}
    assert_eq!(job.progress().unwrap().state, State::FinishedWithErrors);
    assert!(
        job.failures()
            .unwrap()
            .iter()
            .any(|f| f.relative_path == Path::new("child"))
    );
}

#[test]
fn duplicate_selected_roots_do_not_create_a_partial_job() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    fs::create_dir(&source).unwrap();
    let db = temp.path().join("job.sqlite");
    assert!(Job::create(&db, &[source.clone(), source], Options::default()).is_err());
    assert!(!db.exists());
}

#[test]
fn native_names_and_links_remain_distinct_and_never_traverse_targets() {
    use folderloom_core::EntryType;
    use std::{
        ffi::OsString,
        os::unix::{ffi::OsStringExt, fs::symlink},
    };
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    let outside = temp.path().join("outside");
    fs::create_dir(&source).unwrap();
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("must-not-scan"), b"private fixture").unwrap();
    let native = OsString::from_vec(vec![b'n', 0xff]);
    #[cfg(target_os = "linux")]
    fs::write(source.join(&native), b"native").unwrap();
    fs::write(source.join("n�"), b"unicode").unwrap();
    symlink(&outside, source.join("link")).unwrap();
    symlink("missing", source.join("broken")).unwrap();
    symlink(".", source.join("cycle")).unwrap();
    let db = temp.path().join("job.sqlite");
    let mut job = Job::create(&db, &[source], Options::default()).unwrap();
    while job.step(2).unwrap().state == State::Running {}
    drop(job);
    let entries = Job::open(&db).unwrap().inventory().unwrap();
    assert_eq!(entries.len(), if cfg!(target_os = "linux") { 6 } else { 5 });
    if cfg!(target_os = "linux") {
        assert!(
            entries
                .iter()
                .any(|e| e.relative_path == Path::new(&native))
        );
    }
    assert!(entries.iter().any(|e| e.relative_path == Path::new("n�")));
    let link = entries
        .iter()
        .find(|e| e.relative_path == Path::new("link"))
        .unwrap();
    assert_eq!(link.entry_type, EntryType::Symlink);
    assert_eq!(link.link_target, Some(outside));
    assert_eq!(
        entries
            .iter()
            .filter(|e| e.entry_type == EntryType::Symlink)
            .count(),
        3
    );
}

#[test]
fn ignore_empty_policy_is_persisted_and_exclusions_remain_visible() {
    use folderloom_core::EmptyFolders;
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    fs::create_dir_all(source.join("empty")).unwrap();
    let db = temp.path().join("job.sqlite");
    let mut job = Job::create(
        &db,
        &[source],
        Options {
            empty_folders: EmptyFolders::Ignore,
        },
    )
    .unwrap();
    while job.step(2).unwrap().state == State::Running {}
    drop(job);
    let entries = Job::open(&db).unwrap().inventory().unwrap();
    assert_eq!(entries.len(), 2);
    assert!(
        entries
            .iter()
            .find(|e| e.relative_path == Path::new("empty"))
            .unwrap()
            .excluded
    );
    assert!(
        !entries
            .iter()
            .find(|e| e.relative_path.as_os_str().is_empty())
            .unwrap()
            .excluded
    );
}

#[test]
fn unreadable_directory_is_a_durable_failure_and_can_be_retried() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    let blocked = source.join("blocked");
    fs::create_dir_all(&blocked).unwrap();
    fs::write(blocked.join("hidden-child"), b"data").unwrap();
    fs::set_permissions(&blocked, fs::Permissions::from_mode(0o000)).unwrap();
    // Root privileges bypass permission denial; CI runs this as an ordinary user.
    if fs::read_dir(&blocked).is_ok() {
        fs::set_permissions(&blocked, fs::Permissions::from_mode(0o700)).unwrap();
        return;
    }
    let db = temp.path().join("job.sqlite");
    let mut job = Job::create(&db, std::slice::from_ref(&source), Options::default()).unwrap();
    while job.step(2).unwrap().state == State::Running {}
    assert_eq!(job.progress().unwrap().state, State::FinishedWithErrors);
    drop(job);
    let job = Job::open(&db).unwrap();
    assert!(
        job.failures()
            .unwrap()
            .iter()
            .any(|f| f.relative_path == Path::new("blocked"))
    );
    assert!(
        !job.inventory()
            .unwrap()
            .iter()
            .find(|e| e.relative_path == Path::new("blocked"))
            .unwrap()
            .directory_complete
    );
    drop(job);
    fs::set_permissions(&blocked, fs::Permissions::from_mode(0o700)).unwrap();
    let mut job = Job::resume(&db).unwrap();
    while job.step(2).unwrap().state == State::Running {}
    assert_eq!(job.progress().unwrap().state, State::Complete);
    assert_eq!(job.inventory().unwrap().len(), 3);
    assert!(!job.failures().unwrap().is_empty()); // history survives a successful retry
}

#[test]
fn source_names_content_and_modification_times_are_unchanged() {
    use std::collections::BTreeMap;
    fn snapshot(
        root: &std::path::Path,
    ) -> BTreeMap<PathBuf, (Option<Vec<u8>>, std::time::SystemTime)> {
        fn visit(
            root: &std::path::Path,
            p: &std::path::Path,
            result: &mut BTreeMap<PathBuf, (Option<Vec<u8>>, std::time::SystemTime)>,
        ) {
            let meta = fs::symlink_metadata(p).unwrap();
            result.insert(
                p.strip_prefix(root).unwrap().into(),
                (
                    meta.is_file().then(|| fs::read(p).unwrap()),
                    meta.modified().unwrap(),
                ),
            );
            if meta.is_dir() {
                for child in fs::read_dir(p).unwrap() {
                    visit(root, &child.unwrap().path(), result);
                }
            }
        }
        let mut result = BTreeMap::new();
        visit(root, root, &mut result);
        result
    }
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    fs::create_dir_all(source.join("nested/empty")).unwrap();
    fs::write(source.join("nested/file"), b"unchanged").unwrap();
    fs::write(source.join(".hidden"), b"also unchanged").unwrap();
    let before = snapshot(&source);
    let db = temp.path().join("job.sqlite");
    let mut job = Job::create(&db, std::slice::from_ref(&source), Options::default()).unwrap();
    while job.step(1).unwrap().state == State::Running {}
    drop(job);
    let mut job = Job::resume(&db).unwrap();
    while job.step(2).unwrap().state == State::Running {}
    assert_eq!(snapshot(&source), before);
}

#[test]
fn relocated_database_and_sidecar_aliases_are_rejected_without_source_writes() {
    use std::os::unix::fs::symlink;
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    fs::create_dir(&source).unwrap();
    let db = temp.path().join("job.sqlite");
    drop(Job::create(&db, std::slice::from_ref(&source), Options::default()).unwrap());
    let source_file = source.join("untouched");
    fs::write(&source_file, b"source bytes").unwrap();
    let before = fs::metadata(&source_file).unwrap().modified().unwrap();
    let journal = temp.path().join("job.sqlite-journal");
    symlink(&source_file, &journal).unwrap();
    assert!(Job::resume(&db).is_err());
    assert_eq!(fs::read(&source_file).unwrap(), b"source bytes");
    assert_eq!(
        fs::metadata(&source_file).unwrap().modified().unwrap(),
        before
    );
    fs::remove_file(journal).unwrap();
    let moved = source.join("moved.sqlite");
    fs::rename(&db, &moved).unwrap();
    let before = fs::metadata(&moved).unwrap().modified().unwrap();
    assert!(Job::open(&moved).is_err());
    assert!(Job::resume(&moved).is_err());
    assert_eq!(fs::metadata(&moved).unwrap().modified().unwrap(), before);
    assert_eq!(fs::read_dir(&source).unwrap().count(), 2);
}

#[test]
fn only_one_writer_can_resume_and_status_does_not_change_the_saved_state() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    fs::create_dir(&source).unwrap();
    let db = temp.path().join("job.sqlite");
    let mut writer = Job::create(&db, &[source], Options::default()).unwrap();
    assert!(Job::resume(&db).is_err());
    let observer = Job::open(&db).unwrap();
    assert_eq!(observer.progress().unwrap().generation, 1);
    assert_eq!(observer.progress().unwrap().state, State::Running);
    while writer.step(1).unwrap().state == State::Running {}
    assert_eq!(observer.progress().unwrap().state, State::Complete);
    drop(observer);
    drop(writer);
    let mut resumed = Job::resume(&db).unwrap();
    assert_eq!(resumed.progress().unwrap().generation, 2);
    assert_eq!(resumed.progress().unwrap().state, State::Running);
    while resumed.step(1).unwrap().state == State::Running {}
    assert_eq!(resumed.progress().unwrap().state, State::Complete);
}
