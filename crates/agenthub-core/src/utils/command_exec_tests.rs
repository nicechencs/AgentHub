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

#[cfg(unix)]
#[test]
fn consecutive_empty_lines_reach_hook_and_accumulator() {
    // Relies on with_install_log_hook's run-lock so parallel lib tests cannot
    // steal/clear the process-wide hook mid-run (CI flake on PR CI).
    let chunks = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let sink = Arc::clone(&chunks);
    let hook: InstallLogHook = Arc::new(move |chunk| {
        sink.lock().expect("hook mutex").push(chunk.to_owned());
    });
    let result = with_install_log_hook(hook, || {
        SystemCommandExecutor.run(&ExecRequest {
            program: "sh".into(),
            args: vec!["-c".into(), "printf 'first\\n\\nthird\\n'".into()],
            timeout: Duration::from_secs(5),
            max_output_bytes: 1024,
        })
    });

    assert!(result.success(), "unexpected command result: {result:?}");
    assert!(
        result.stdout.contains("first\n\nthird"),
        "accumulator lost empty lines: {:?}",
        result.stdout
    );
    let joined = chunks.lock().expect("hook mutex").concat();
    // Parallel SystemCommandExecutor runs may still append banners into the
    // active hook; require our payload rather than exact equality.
    assert!(
        joined.contains("first\n\nthird"),
        "live hook lost empty lines: {joined:?}"
    );
}

#[cfg(unix)]
fn unix_pid_exists(pid: i32) -> bool {
    if pid <= 1 {
        return false;
    }
    unsafe { libc::kill(pid, 0) == 0 }
}

#[cfg(unix)]
fn wait_unix_pid_gone(pid: i32, budget: Duration) {
    let started = Instant::now();
    while started.elapsed() < budget {
        if !unix_pid_exists(pid) {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("pid {pid} still exists after {budget:?}");
}

#[cfg(unix)]
#[test]
fn exited_leader_kills_stdio_closed_descendant() {
    let started = Instant::now();
    let result = SystemCommandExecutor.run(&ExecRequest {
        program: "sh".into(),
        args: vec![
            "-c".into(),
            "sleep 30 >/dev/null 2>&1 & echo $!; exit 0".into(),
        ],
        timeout: Duration::from_secs(3),
        max_output_bytes: 1024,
    });
    assert!(result.success(), "unexpected command result: {result:?}");
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "cleanup took {:?}",
        started.elapsed()
    );
    let pid: i32 = result
        .stdout
        .lines()
        .find(|line| {
            !line.trim().is_empty() && !line.contains("incomplete") && !line.contains("truncated")
        })
        .expect("descendant pid")
        .trim()
        .parse()
        .expect("numeric pid");
    wait_unix_pid_gone(pid, Duration::from_millis(500));
}

#[cfg(unix)]
#[test]
fn descendant_holding_output_pipe_marks_incomplete() {
    let started = Instant::now();
    let result = SystemCommandExecutor.run(&ExecRequest {
        program: "sh".into(),
        args: vec!["-c".into(), "sleep 30 & exit 0".into()],
        timeout: Duration::from_secs(3),
        max_output_bytes: 1024,
    });
    assert!(
        result.stdout.contains("incomplete") || result.stderr.contains("incomplete"),
        "stdout={:?} stderr={:?}",
        result.stdout,
        result.stderr
    );
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "pipe-holder cleanup took {:?}",
        started.elapsed()
    );
}
