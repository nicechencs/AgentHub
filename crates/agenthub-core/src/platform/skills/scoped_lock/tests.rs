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
        let _lock = acquire_skill_lock(dir.path(), "demo").unwrap();
        assert!(acquire_skill_lock(dir.path(), "demo").is_err());
    }
    let _again = acquire_skill_lock(dir.path(), "demo").unwrap();
}

#[test]
fn malformed_metadata_does_not_override_os_lock() {
    let dir = tempdir().unwrap();
    let path = dir.path().join(".locks/skill-demo.lock");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, b"not a valid owner record").unwrap();

    assert!(acquire_skill_lock(dir.path(), "demo").is_err());
    fs::remove_file(&path).unwrap();
    let _again = acquire_skill_lock(dir.path(), "demo").unwrap();
    assert!(SkillLockOwner::parse(&fs::read_to_string(path).unwrap()).is_some());
}

#[test]
fn empty_or_partial_metadata_never_allows_a_second_holder() {
    let dir = tempdir().unwrap();
    let path = dir.path().join(".locks/skill-demo.lock");
    fs::create_dir_all(path.parent().unwrap()).unwrap();

    for metadata in ["", "pid=123\ncreated_unix_ms="] {
        {
            let raw_lock = AdvisoryFileLock::try_acquire(&advisory_path(&path))
                .unwrap()
                .unwrap();
            write_metadata_file(&path, metadata).unwrap();
            assert!(acquire_skill_lock(dir.path(), "demo").is_err());
            drop(raw_lock);
        }

        assert!(acquire_skill_lock(dir.path(), "demo").is_err());
        fs::remove_file(&path).unwrap();
        let recovered = acquire_skill_lock(dir.path(), "demo").unwrap();
        drop(recovered);
    }
}

#[test]
fn three_way_competition_has_at_most_one_holder() {
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
fn pid_reuse_metadata_cannot_override_a_live_os_lock() {
    let dir = tempdir().unwrap();
    let held = acquire_skill_lock(dir.path(), "demo").unwrap();
    write_metadata_file(
        &dir.path().join(".locks/skill-demo.lock"),
        "protocol=2\npid=1\ncreated_unix_ms=1\ntoken=reused-pid\n",
    )
    .unwrap();

    assert!(acquire_skill_lock(dir.path(), "demo").is_err());
    drop(held);
    let _after_exit = acquire_skill_lock(dir.path(), "demo").unwrap();
}

#[test]
fn protocol_v2_metadata_is_reusable_after_sidecar_release() {
    let dir = tempdir().unwrap();
    let path = dir.path().join(".locks/skill-demo.lock");
    let held = acquire_skill_lock(dir.path(), "demo").unwrap();
    let metadata = read_metadata_file(&path).unwrap();
    drop(held);

    write_metadata_file(&path, &metadata).unwrap();
    let _recovered = acquire_skill_lock(dir.path(), "demo").unwrap();
}

#[test]
fn legacy_visible_holder_is_not_overwritten_by_new_protocol() {
    if std::env::var_os("AGENTHUB_LEGACY_SKILL_LOCK_DIR").is_some() {
        return;
    }

    let dir = tempdir().unwrap();
    let ready = dir.path().join("legacy-ready");
    let mut child = Command::new(std::env::current_exe().unwrap())
        .arg("skill_lock_child_holds_legacy_lock")
        .env("AGENTHUB_LEGACY_SKILL_LOCK_DIR", dir.path())
        .env("AGENTHUB_LEGACY_SKILL_LOCK_READY", &ready)
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
        panic!("legacy skill child did not create visible lock");
    }

    assert!(acquire_skill_lock(dir.path(), "demo").is_err());
    child.kill().unwrap();
    child.wait().unwrap();

    fs::remove_file(dir.path().join(".locks/skill-demo.lock")).unwrap();
    let _after_cleanup = acquire_skill_lock(dir.path(), "demo").unwrap();
}

#[cfg(unix)]
#[test]
fn symlink_lock_leaves_fail_closed_without_touching_target() {
    use std::os::unix::fs::symlink;

    let dir = tempdir().unwrap();
    let target = dir.path().join("sentinel");
    fs::write(&target, b"must remain unchanged").unwrap();
    let lock_dir = dir.path().join(".locks");
    fs::create_dir_all(&lock_dir).unwrap();

    let visible = lock_dir.join("skill-demo.lock");
    symlink(&target, &visible).unwrap();
    assert!(acquire_skill_lock(dir.path(), "demo").is_err());
    assert_eq!(fs::read(&target).unwrap(), b"must remain unchanged");

    let sidecar = advisory_path(&lock_dir.join("skill-other.lock"));
    symlink(&target, &sidecar).unwrap();
    assert!(acquire_skill_lock(dir.path(), "other").is_err());
    assert_eq!(fs::read(&target).unwrap(), b"must remain unchanged");
}

#[cfg(windows)]
#[test]
fn reparse_lock_leaves_fail_closed_without_touching_target() {
    use std::os::windows::fs::symlink_file;

    let dir = tempdir().unwrap();
    let target = dir.path().join("sentinel");
    fs::write(&target, b"must remain unchanged").unwrap();
    let lock_dir = dir.path().join(".locks");
    fs::create_dir_all(&lock_dir).unwrap();

    let visible = lock_dir.join("skill-demo.lock");
    if let Err(error) = symlink_file(&target, &visible) {
        if error.kind() == std::io::ErrorKind::PermissionDenied {
            return;
        }
        panic!("could not create reparse-point fixture: {error}");
    }
    assert!(acquire_skill_lock(dir.path(), "demo").is_err());
    assert_eq!(fs::read(&target).unwrap(), b"must remain unchanged");

    let sidecar = advisory_path(&lock_dir.join("skill-other.lock"));
    symlink_file(&target, &sidecar).unwrap();
    assert!(acquire_skill_lock(dir.path(), "other").is_err());
    assert_eq!(fs::read(&target).unwrap(), b"must remain unchanged");
}

#[test]
fn root_lock_uses_the_same_advisory_protocol() {
    let dir = tempdir().unwrap();
    let _root = acquire_skill_root_lock(dir.path()).unwrap();
    assert!(acquire_skill_root_lock(dir.path()).is_err());
}

#[test]
fn skill_lock_child_holds_legacy_lock() {
    let Ok(raw_dir) = std::env::var("AGENTHUB_LEGACY_SKILL_LOCK_DIR") else {
        return;
    };
    let ready = PathBuf::from(std::env::var("AGENTHUB_LEGACY_SKILL_LOCK_READY").unwrap());
    let lock_dir = PathBuf::from(raw_dir).join(".locks");
    fs::create_dir_all(&lock_dir).unwrap();
    let path = lock_dir.join("skill-demo.lock");
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
