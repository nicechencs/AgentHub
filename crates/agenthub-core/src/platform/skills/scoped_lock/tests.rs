use super::*;
use std::fs;
use std::sync::{Arc, Barrier};
use std::thread;

use crate::utils::test_temp::real_tempdir;

#[test]
fn acquire_and_drop_releases_lock() {
    let dir = real_tempdir();
    {
        let _lock = acquire_skill_lock(dir.path(), "demo").unwrap();
        assert!(acquire_skill_lock(dir.path(), "demo").is_err());
    }
    let _again = acquire_skill_lock(dir.path(), "demo").unwrap();
}

#[test]
fn leftover_or_malformed_metadata_does_not_block_acquire() {
    let dir = real_tempdir();
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
    let dir = real_tempdir();
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
    let dir = real_tempdir();
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
    let dir = real_tempdir();
    let _root = acquire_skill_root_lock(dir.path()).unwrap();
    assert!(acquire_skill_root_lock(dir.path()).is_err());
    let _other = acquire_skill_lock(dir.path(), "demo").unwrap();
}

#[test]
fn distinct_source_roots_do_not_share_root_lock() {
    let a = real_tempdir();
    let b = real_tempdir();
    let _la = acquire_skill_root_lock(a.path()).unwrap();
    let _lb = acquire_skill_root_lock(b.path()).unwrap();
}

#[test]
fn different_skill_ids_do_not_block_each_other() {
    let dir = real_tempdir();
    let _a = acquire_skill_lock(dir.path(), "one").unwrap();
    let _b = acquire_skill_lock(dir.path(), "two").unwrap();
}

#[test]
fn sanitized_keys_do_not_collide() {
    let dir = real_tempdir();
    let _slash = acquire_skill_lock(dir.path(), "a/b").unwrap();
    assert!(acquire_skill_lock(dir.path(), "a_b").is_ok());
}

#[test]
fn sanitize_escapes_instead_of_dropping_characters() {
    assert_eq!(sanitize_lock_key("a/b"), "a%2Fb");
    assert_eq!(sanitize_lock_key("a_b"), "a_b");
    assert_eq!(sanitize_lock_key(""), "%00");
}

#[cfg(unix)]
#[test]
fn symlink_skill_lock_leaves_fail_closed_without_touching_target() {
    use std::os::unix::fs::symlink;

    let dir = real_tempdir();
    let target = dir.path().join("sentinel");
    fs::write(&target, b"must remain unchanged").unwrap();
    fs::create_dir_all(dir.path().join(".locks")).unwrap();

    let lock_path = dir.path().join(".locks/skill-demo.lock");
    symlink(&target, &lock_path).unwrap();
    assert!(acquire_skill_lock(dir.path(), "demo").is_err());
    assert_eq!(fs::read(&target).unwrap(), b"must remain unchanged");
}
