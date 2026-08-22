use super::*;
use std::fs;
use std::sync::{Arc, Barrier};
use std::thread;
use tempfile::tempdir;

#[test]
fn acquire_and_drop_releases_lock() {
    let dir = tempdir().unwrap();
    {
        let _lock = acquire_skill_lock(dir.path(), "demo").unwrap();
        assert!(acquire_skill_lock(dir.path(), "demo").is_err());
    }
    let _again = acquire_skill_lock(dir.path(), "demo").unwrap();
}

#[test]
fn leftover_or_malformed_metadata_does_not_block_acquire() {
    let dir = tempdir().unwrap();
    let path = dir.path().join(".locks/skill-demo.lock");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, b"not a valid owner record").unwrap();
    fs::write(
        dir.path().join(".locks/.skill-demo.lock.os-lock"),
        b"stale sidecar",
    )
    .unwrap();

    let _lock = acquire_skill_lock(dir.path(), "demo").unwrap();
    assert!(SkillLockOwner::parse(&fs::read_to_string(path).unwrap()).is_some());
}

#[test]
fn concurrent_acquire_has_at_most_one_holder() {
    let dir = tempdir().unwrap();
    let held = acquire_skill_lock(dir.path(), "demo").unwrap();
    let barrier = Arc::new(Barrier::new(3));
    let mut workers = Vec::new();
    for _ in 0..2 {
        let barrier = Arc::clone(&barrier);
        let root = dir.path().to_path_buf();
        workers.push(thread::spawn(move || {
            barrier.wait();
            acquire_skill_lock(&root, "demo").is_ok()
        }));
    }
    barrier.wait();
    assert!(!workers.pop().unwrap().join().unwrap());
    assert!(!workers.pop().unwrap().join().unwrap());

    drop(held);
    let _winner = acquire_skill_lock(dir.path(), "demo").unwrap();
}

#[test]
fn panic_while_holding_releases_lock() {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let panicked = std::panic::catch_unwind(|| {
        let _lock = acquire_skill_lock(&root, "demo").unwrap();
        panic!("skill lock holder panicked");
    });
    assert!(panicked.is_err());
    let _again = acquire_skill_lock(dir.path(), "demo").unwrap();
}

#[test]
fn root_lock_conflicts_with_itself_not_other_skills() {
    let dir = tempdir().unwrap();
    let _root = acquire_skill_root_lock(dir.path()).unwrap();
    assert!(acquire_skill_root_lock(dir.path()).is_err());
    let _other = acquire_skill_lock(dir.path(), "demo").unwrap();
}

#[test]
fn different_skill_ids_do_not_block_each_other() {
    let dir = tempdir().unwrap();
    let _a = acquire_skill_lock(dir.path(), "one").unwrap();
    let _b = acquire_skill_lock(dir.path(), "two").unwrap();
}
