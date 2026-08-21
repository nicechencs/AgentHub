#[cfg(unix)]
use std::time::{Duration, Instant};

#[cfg(unix)]
use super::command_exec::{CommandExecutor, ExecRequest, SystemCommandExecutor};
use crate::services::{with_install_log_hook, InstallLogHook};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

#[cfg(unix)]
#[test]
fn timeout_kills_descendants_that_hold_output_pipes() {
    let started = Instant::now();
    let result = SystemCommandExecutor.run(&ExecRequest {
        program: "sh".into(),
        args: vec!["-c".into(), "(sleep 30)& wait".into()],
        timeout: Duration::from_millis(200),
        max_output_bytes: 1024,
    });

    assert!(result.timed_out);
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "reader cleanup took {:?}",
        started.elapsed()
    );
}

#[cfg(unix)]
#[test]
fn exited_leader_with_descendant_pipe_is_cleaned_before_reap() {
    let started = Instant::now();
    let result = SystemCommandExecutor.run(&ExecRequest {
        program: "sh".into(),
        args: vec!["-c".into(), "(sleep 30)& exit 0".into()],
        timeout: Duration::from_secs(2),
        max_output_bytes: 1024,
    });

    assert!(result.success(), "unexpected command result: {result:?}");
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "reader cleanup took {:?}",
        started.elapsed()
    );
}

#[cfg(unix)]
#[test]
fn fast_newline_output_completes_without_waiting_on_live_queue() {
    let calls = Arc::new(AtomicUsize::new(0));
    let seen = Arc::clone(&calls);
    let hook: InstallLogHook = Arc::new(move |_| {
        seen.fetch_add(1, Ordering::SeqCst);
    });
    let result = with_install_log_hook(hook, || {
        SystemCommandExecutor.run(&ExecRequest {
            program: "sh".into(),
            args: vec!["-c".into(), "yes | head -c 2097152".into()],
            timeout: Duration::from_secs(5),
            max_output_bytes: 2 * 1024 * 1024,
        })
    });

    assert!(result.success(), "unexpected command result: {result:?}");
    assert!(
        calls.load(Ordering::SeqCst) <= 300,
        "newline storm produced too many live callbacks"
    );
}
