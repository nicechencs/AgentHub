use super::*;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Duration;
use tempfile::tempdir;

#[test]
fn acquire_and_drop_releases_os_lock() {
    let dir = tempdir().unwrap();
    {
        let _lock = AgentWriteLock::acquire(dir.path(), AgentId::Claude).unwrap();
        let err = AgentWriteLock::acquire(dir.path(), AgentId::Claude).unwrap_err();
        assert_eq!(err.code(), "agent.lock");
    }
    let _again = AgentWriteLock::acquire(dir.path(), AgentId::Claude).unwrap();
}

#[cfg(unix)]
#[test]
fn unix_platform_exposes_no_follow_flag() {
    assert_ne!(libc::O_NOFOLLOW, 0);
}

#[test]
fn different_agents_do_not_block_each_other() {
    let dir = tempdir().unwrap();
    let _a = AgentWriteLock::acquire(dir.path(), AgentId::Claude).unwrap();
    let _b = AgentWriteLock::acquire(dir.path(), AgentId::Codex).unwrap();
}

#[test]
fn malformed_metadata_does_not_override_os_lock() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("provider-grok.lock");
    write_metadata_file(&path, "not a valid owner record").unwrap();

    // A malformed/legacy visible leaf is fail-closed during migration, even
    // though the sidecar itself is available.
    assert!(AgentWriteLock::acquire(dir.path(), AgentId::Grok).is_err());
    fs::remove_file(&path).unwrap();
    let _again = AgentWriteLock::acquire(dir.path(), AgentId::Grok).unwrap();
    assert!(LockOwner::parse(&fs::read_to_string(path).unwrap()).is_some());
}

#[test]
fn empty_or_partial_metadata_never_allows_a_second_holder() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("provider-claude.lock");

    for metadata in ["", "pid=123\ncreated_unix_ms="] {
        {
            let raw_lock = AdvisoryFileLock::try_acquire(&advisory_path(&path))
                .unwrap()
                .unwrap();
            write_metadata_file(&path, metadata).unwrap();
            assert!(AgentWriteLock::acquire(dir.path(), AgentId::Claude).is_err());
            drop(raw_lock);
        }

        assert!(AgentWriteLock::acquire(dir.path(), AgentId::Claude).is_err());
        fs::remove_file(&path).unwrap();
        let recovered = AgentWriteLock::acquire(dir.path(), AgentId::Claude).unwrap();
        drop(recovered);
    }
}

#[test]
fn three_way_competition_has_at_most_one_holder() {
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
fn pid_reuse_metadata_cannot_override_a_live_os_lock() {
    let dir = tempdir().unwrap();
    let held = AgentWriteLock::acquire(dir.path(), AgentId::Claude).unwrap();
    write_metadata_file(
        &dir.path().join("provider-claude.lock"),
        "protocol=2\npid=1\ncreated_unix_ms=1\ntoken=reused-pid\n",
    )
    .unwrap();

    assert!(AgentWriteLock::acquire(dir.path(), AgentId::Claude).is_err());
    drop(held);
    let _after_exit = AgentWriteLock::acquire(dir.path(), AgentId::Claude).unwrap();
}

#[test]
fn protocol_v2_metadata_is_reusable_after_sidecar_release() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("provider-claude.lock");
    let held = AgentWriteLock::acquire(dir.path(), AgentId::Claude).unwrap();
    let metadata = read_metadata_file(&path).unwrap();
    drop(held);

    // Simulate a process crash after metadata was durably written: the
    // sidecar is free, and protocol-v2 identifies this as new-protocol
    // residue rather than an old visible-only holder.
    write_metadata_file(&path, &metadata).unwrap();
    let _recovered = AgentWriteLock::acquire(dir.path(), AgentId::Claude).unwrap();
}

#[test]
fn legacy_visible_holder_is_not_overwritten_by_new_protocol() {
    if std::env::var_os("AGENTHUB_LEGACY_LOCK_DIR").is_some() {
        return;
    }

    let dir = tempdir().unwrap();
    let ready = dir.path().join("legacy-ready");
    let mut child = Command::new(std::env::current_exe().unwrap())
        .arg("agent_lock_child_holds_legacy_lock")
        .env("AGENTHUB_LEGACY_LOCK_DIR", dir.path())
        .env("AGENTHUB_LEGACY_LOCK_READY", &ready)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    let mut ready_seen = false;
    for _ in 0..500 {
        if ready.exists() {
            ready_seen = true;
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    if !ready_seen {
        let _ = child.kill();
        let _ = child.wait();
        panic!("legacy child did not create visible lock");
    }

    assert!(AgentWriteLock::acquire(dir.path(), AgentId::Claude).is_err());
    child.kill().unwrap();
    child.wait().unwrap();

    // The old process may have crashed before its Drop cleanup; explicit
    // cleanup is required because legacy metadata is intentionally fail-safe.
    fs::remove_file(dir.path().join("provider-claude.lock")).unwrap();
    let _after_cleanup = AgentWriteLock::acquire(dir.path(), AgentId::Claude).unwrap();
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

    let sidecar = advisory_path(&dir.path().join("provider-codex.lock"));
    symlink(&target, &sidecar).unwrap();
    assert!(AgentWriteLock::acquire(dir.path(), AgentId::Codex).is_err());
    assert_eq!(fs::read(&target).unwrap(), b"must remain unchanged");
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
        if error.kind() == std::io::ErrorKind::PermissionDenied {
            return;
        }
        panic!("could not create reparse-point fixture: {error}");
    }
    assert!(AgentWriteLock::acquire(dir.path(), AgentId::Claude).is_err());
    assert_eq!(fs::read(&target).unwrap(), b"must remain unchanged");

    let sidecar = advisory_path(&dir.path().join("provider-codex.lock"));
    symlink_file(&target, &sidecar).unwrap();
    assert!(AgentWriteLock::acquire(dir.path(), AgentId::Codex).is_err());
    assert_eq!(fs::read(&target).unwrap(), b"must remain unchanged");
}

#[test]
fn inspect_locks_reports_os_held_and_malformed_metadata() {
    let dir = tempdir().unwrap();
    let _held = AgentWriteLock::acquire(dir.path(), AgentId::Claude).unwrap();
    fs::write(dir.path().join("provider-grok.lock"), b"not-a-lock").unwrap();
    fs::write(dir.path().join("readme.txt"), b"ignore").unwrap();

    let rows = inspect_locks(dir.path());
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].agent, "claude");
    assert_eq!(rows[0].status, "held");
    assert_eq!(rows[0].pid, Some(std::process::id()));
    assert_eq!(rows[1].agent, "grok");
    assert_eq!(rows[1].status, "malformed");
}

#[test]
fn advisory_lock_is_released_when_holder_process_exits() {
    if std::env::var_os("AGENTHUB_LOCK_CHILD_DIR").is_some() {
        return;
    }

    let dir = tempdir().unwrap();
    let ready = dir.path().join("ready");
    let mut child = Command::new(std::env::current_exe().unwrap())
        .arg("agent_lock_child_holds_advisory_lock")
        .env("AGENTHUB_LOCK_CHILD_DIR", dir.path())
        .env("AGENTHUB_LOCK_CHILD_READY", &ready)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    let mut ready_seen = false;
    for _ in 0..500 {
        if ready.exists() {
            ready_seen = true;
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    if !ready_seen {
        let _ = child.kill();
        let _ = child.wait();
        panic!("child did not acquire advisory lock");
    }

    child.kill().unwrap();
    child.wait().unwrap();
    let _after_crash = AgentWriteLock::acquire(dir.path(), AgentId::Claude).unwrap();
}

#[test]
fn agent_lock_child_holds_advisory_lock() {
    let Ok(raw_dir) = std::env::var("AGENTHUB_LOCK_CHILD_DIR") else {
        return;
    };
    let ready = PathBuf::from(std::env::var("AGENTHUB_LOCK_CHILD_READY").unwrap());
    let dir = PathBuf::from(raw_dir);
    let _lock = AgentWriteLock::acquire(&dir, AgentId::Claude).unwrap();
    fs::write(ready, b"ready").unwrap();
    thread::sleep(Duration::from_secs(30));
}

#[test]
fn agent_lock_child_holds_legacy_lock() {
    let Ok(raw_dir) = std::env::var("AGENTHUB_LEGACY_LOCK_DIR") else {
        return;
    };
    let ready = PathBuf::from(std::env::var("AGENTHUB_LEGACY_LOCK_READY").unwrap());
    let path = PathBuf::from(raw_dir).join("provider-claude.lock");
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .unwrap();
    writeln!(file, "pid={}", std::process::id()).unwrap();
    writeln!(file, "created_unix_ms=1").unwrap();
    writeln!(file, "token=legacy-holder").unwrap();
    file.sync_all().unwrap();
    fs::write(ready, b"ready").unwrap();
    thread::sleep(Duration::from_secs(30));
}
