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
