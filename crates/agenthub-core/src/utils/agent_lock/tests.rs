use super::*;
use std::fs;
use std::sync::{Arc, Barrier};
use std::thread;
use tempfile::tempdir;

#[test]
fn acquire_and_drop_releases_lock() {
    let dir = tempdir().unwrap();
    {
        let _lock = AgentWriteLock::acquire(dir.path(), AgentId::Claude).unwrap();
        let err = AgentWriteLock::acquire(dir.path(), AgentId::Claude).unwrap_err();
        assert_eq!(err.code(), "agent.lock");
    }
    let _again = AgentWriteLock::acquire(dir.path(), AgentId::Claude).unwrap();
}

#[test]
fn different_agents_do_not_block_each_other() {
    let dir = tempdir().unwrap();
    let _a = AgentWriteLock::acquire(dir.path(), AgentId::Claude).unwrap();
    let _b = AgentWriteLock::acquire(dir.path(), AgentId::Codex).unwrap();
}

#[test]
fn leftover_or_malformed_metadata_does_not_block_acquire() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("provider-grok.lock"),
        b"not a valid owner record",
    )
    .unwrap();
    fs::write(
        dir.path().join("provider-claude.lock"),
        "protocol=2\npid=1\ncreated_unix_ms=1\ntoken=old\n",
    )
    .unwrap();
    fs::write(
        dir.path().join(".provider-claude.lock.os-lock"),
        b"stale sidecar",
    )
    .unwrap();

    let _grok = AgentWriteLock::acquire(dir.path(), AgentId::Grok).unwrap();
    let _claude = AgentWriteLock::acquire(dir.path(), AgentId::Claude).unwrap();
    assert!(LockOwner::parse(
        &fs::read_to_string(dir.path().join("provider-claude.lock")).unwrap()
    )
    .is_some());
}

#[test]
fn inspect_locks_reports_held_malformed_and_stale() {
    let dir = tempdir().unwrap();
    let _held = AgentWriteLock::acquire(dir.path(), AgentId::Claude).unwrap();
    fs::write(dir.path().join("provider-grok.lock"), b"not-a-lock").unwrap();
    fs::write(
        dir.path().join("provider-codex.lock"),
        "pid=1\ncreated_unix_ms=1\ntoken=leftover\n",
    )
    .unwrap();
    fs::write(dir.path().join("readme.txt"), b"ignore").unwrap();

    let rows = inspect_locks(dir.path());
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0].agent, "claude");
    assert_eq!(rows[0].status, "held");
    assert_eq!(rows[0].pid, Some(std::process::id()));
    assert_eq!(rows[1].agent, "codex");
    assert_eq!(rows[1].status, "stale");
    assert_eq!(rows[2].agent, "grok");
    assert_eq!(rows[2].status, "malformed");
}

#[test]
fn concurrent_acquire_has_at_most_one_holder() {
    let dir = tempdir().unwrap();
    let held = AgentWriteLock::acquire(dir.path(), AgentId::Claude).unwrap();
    let barrier = Arc::new(Barrier::new(3));
    let mut workers = Vec::new();
    for _ in 0..2 {
        let barrier = Arc::clone(&barrier);
        let path = dir.path().to_path_buf();
        workers.push(thread::spawn(move || {
            barrier.wait();
            AgentWriteLock::acquire(&path, AgentId::Claude).is_ok()
        }));
    }
    barrier.wait();
    assert!(!workers.pop().unwrap().join().unwrap());
    assert!(!workers.pop().unwrap().join().unwrap());

    drop(held);
    let _winner = AgentWriteLock::acquire(dir.path(), AgentId::Claude).unwrap();
}

#[test]
fn panic_while_holding_releases_lock() {
    let dir = tempdir().unwrap();
    let path = dir.path().to_path_buf();
    let panicked = std::panic::catch_unwind(|| {
        let _lock = AgentWriteLock::acquire(&path, AgentId::Claude).unwrap();
        panic!("lock holder panicked");
    });
    assert!(panicked.is_err());
    let _again = AgentWriteLock::acquire(dir.path(), AgentId::Claude).unwrap();
}

#[cfg(unix)]
#[test]
fn symlink_lock_leaves_fail_closed_without_touching_target() {
    use std::os::unix::fs::symlink;

    let dir = tempdir().unwrap();
    let target = dir.path().join("sentinel");
    fs::write(&target, b"must remain unchanged").unwrap();

    let visible = dir.path().join("provider-claude.lock");
    symlink(&target, &visible).unwrap();
    assert!(AgentWriteLock::acquire(dir.path(), AgentId::Claude).is_err());
    assert_eq!(fs::read(&target).unwrap(), b"must remain unchanged");

    // The failed acquire must not strand the in-process claim.
    fs::remove_file(&visible).unwrap();
    let _recovered = AgentWriteLock::acquire(dir.path(), AgentId::Claude).unwrap();
}

#[cfg(windows)]
#[test]
fn reparse_lock_leaves_fail_closed_without_touching_target() {
    use std::os::windows::fs::symlink_file;

    let dir = tempdir().unwrap();
    let target = dir.path().join("sentinel");
    fs::write(&target, b"must remain unchanged").unwrap();

    let visible = dir.path().join("provider-claude.lock");
    if let Err(error) = symlink_file(&target, &visible) {
        // Creating a symlink needs the SeCreateSymbolicLink privilege; skip
        // on hosts without it (CI often runs unelevated).
        const ERROR_PRIVILEGE_NOT_HELD: i32 = 1314;
        if error.kind() == std::io::ErrorKind::PermissionDenied
            || error.raw_os_error() == Some(ERROR_PRIVILEGE_NOT_HELD)
        {
            return;
        }
        panic!("could not create reparse-point fixture: {error}");
    }
    assert!(AgentWriteLock::acquire(dir.path(), AgentId::Claude).is_err());
    assert_eq!(fs::read(&target).unwrap(), b"must remain unchanged");

    // The failed acquire must not strand the in-process claim.
    fs::remove_file(&visible).unwrap();
    let _recovered = AgentWriteLock::acquire(dir.path(), AgentId::Claude).unwrap();
}
