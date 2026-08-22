use super::*;
use crate::models::AgentId;

#[test]
fn truncate_bytes_under_limit() {
    let (s, t) = truncate_bytes(b"hello", 10);
    assert_eq!(s, "hello");
    assert!(!t);
}

#[test]
fn truncate_bytes_over_limit() {
    let (s, t) = truncate_bytes(b"abcdefghij", 4);
    assert!(t);
    assert!(s.starts_with("abcd"));
    assert!(s.contains("truncated"));
}

#[test]
fn read_capped_marks_truncated() {
    use std::io::Cursor;
    let data = vec![b'x'; 100];
    let (buf, trunc) = read_capped(Some(Cursor::new(data)), 10);
    assert_eq!(buf.len(), 10);
    assert!(trunc);
}

#[cfg(windows)]
#[test]
fn run_capture_timeout_returns_timed_out() {
    let err = run_capture_timeout(
        "ping",
        &["-n", "30", "127.0.0.1"],
        Duration::from_millis(400),
    )
    .unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::TimedOut);
}

#[cfg(windows)]
#[test]
fn attach_failure_kills_suspended_root_before_waiting() {
    force_attach_failure_for_test();
    let started = Instant::now();
    let error = run_capture_timeout(
        "cmd.exe",
        &["/C", "exit", "0"],
        Duration::from_secs(1),
    )
    .expect_err("injected attach failure should be returned");

    assert_eq!(error.kind(), io::ErrorKind::Other);
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "suspended root cleanup took {:?}",
        started.elapsed()
    );
}

#[cfg(unix)]
#[test]
fn attach_failure_reaps_child() {
    force_attach_failure_for_test();
    let started = Instant::now();
    let error = run_capture_timeout("true", &[], Duration::from_secs(1))
        .expect_err("injected attach failure should be returned");
    assert_eq!(error.kind(), io::ErrorKind::Other);
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "attach-failure cleanup took {:?}",
        started.elapsed()
    );
}

#[cfg(not(windows))]
#[test]
fn run_capture_timeout_returns_timed_out() {
    let err = run_capture_timeout("sleep", &["30"], Duration::from_millis(400)).unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::TimedOut);
}

#[cfg(windows)]
#[test]
fn system_runner_timeout_kills_long_process() {
    let spec = RunSpec {
        agent: AgentId::Claude,
        program: PathBuf::from("ping"),
        args: vec!["-n".into(), "30".into(), "127.0.0.1".into()],
        cwd: None,
        env: vec![],
    };
    let r = SystemProcessRunner.run(&spec, Duration::from_secs(1), 64 * 1024);
    assert_eq!(r.status, RunStatus::Timeout);
}

#[cfg(not(windows))]
#[test]
fn system_runner_timeout_kills_long_process() {
    let spec = RunSpec {
        agent: AgentId::Claude,
        program: PathBuf::from("sleep"),
        args: vec!["30".into()],
        cwd: None,
        env: vec![],
    };
    let r = SystemProcessRunner.run(&spec, Duration::from_secs(1), 64 * 1024);
    assert_eq!(r.status, RunStatus::Timeout);
}

#[cfg(windows)]
#[test]
fn streaming_cancel_kills_long_process() {
    let spec = RunSpec {
        agent: AgentId::Claude,
        program: PathBuf::from("ping"),
        args: vec!["-n".into(), "30".into(), "127.0.0.1".into()],
        cwd: None,
        env: vec![],
    };
    let cancel = CancelToken::new();
    let cancel2 = cancel.clone();
    let handle = thread::spawn(move || {
        thread::sleep(Duration::from_millis(200));
        cancel2.cancel();
    });
    let r = SystemProcessRunner.run_streaming(
        &spec,
        Duration::from_secs(30),
        64 * 1024,
        &cancel,
        &|_, _| {},
    );
    let _ = handle.join();
    assert_eq!(r.status, RunStatus::Cancelled);
}

#[cfg(not(windows))]
#[test]
fn streaming_cancel_kills_long_process() {
    let spec = RunSpec {
        agent: AgentId::Claude,
        program: PathBuf::from("sleep"),
        args: vec!["30".into()],
        cwd: None,
        env: vec![],
    };
    let cancel = CancelToken::new();
    let cancel2 = cancel.clone();
    let handle = thread::spawn(move || {
        thread::sleep(Duration::from_millis(200));
        cancel2.cancel();
    });
    let r = SystemProcessRunner.run_streaming(
        &spec,
        Duration::from_secs(30),
        64 * 1024,
        &cancel,
        &|_, _| {},
    );
    let _ = handle.join();
    assert_eq!(r.status, RunStatus::Cancelled);
}

#[test]
fn read_lines_capped_stops_emitting_after_max() {
    use std::io::Cursor;
    use std::sync::mpsc;
    let data = "aaaa\nbbbb\ncccc\n";
    let (tx, rx) = mpsc::sync_channel(32);
    let acc = Mutex::new(Vec::new());
    let trunc = AtomicBool::new(false);
    let incomplete = AtomicBool::new(false);
    read_lines_capped(
        Some(Cursor::new(data.as_bytes())),
        6,
        OutputStream::Stdout,
        &tx,
        &acc,
        &trunc,
        &incomplete,
    );
    drop(tx);
    let chunks: Vec<_> = rx.iter().collect();
    let joined: String = chunks.into_iter().map(|(_, t)| t).collect();
    assert_eq!(joined.len(), 6);
    assert!(trunc.load(Ordering::SeqCst));
    assert!(!incomplete.load(Ordering::SeqCst));
    assert_eq!(acc.lock().unwrap().len(), 6);
}

#[test]
fn read_lines_capped_preserves_consecutive_empty_lines() {
    use std::io::Cursor;
    use std::sync::mpsc;
    let (tx, rx) = mpsc::sync_channel(32);
    let acc = Mutex::new(Vec::new());
    let trunc = AtomicBool::new(false);
    let incomplete = AtomicBool::new(false);
    read_lines_capped(
        Some(Cursor::new(b"first\n\nthird\n")),
        7,
        OutputStream::Stdout,
        &tx,
        &acc,
        &trunc,
        &incomplete,
    );
    drop(tx);
    let chunks: Vec<String> = rx.iter().map(|(_, text)| text).collect();
    let joined: String = chunks.concat();
    assert_eq!(joined, "first\n\n");
    assert!(joined.contains("\n\n"));
    assert_eq!(acc.lock().unwrap().as_slice(), b"first\n\n");
    assert_eq!(acc.lock().unwrap().len(), 7);
    assert!(trunc.load(Ordering::SeqCst));
    assert!(!incomplete.load(Ordering::SeqCst));
}

#[test]
fn read_lines_capped_drains_long_unterminated_line_without_growing() {
    use std::io::Cursor;
    use std::sync::mpsc;

    let data = vec![b'x'; 4 * 1024 * 1024];
    let (tx, rx) = mpsc::sync_channel(32);
    let acc = Mutex::new(Vec::new());
    let trunc = AtomicBool::new(false);
    let incomplete = AtomicBool::new(false);
    read_lines_capped(
        Some(Cursor::new(data)),
        1024,
        OutputStream::Stdout,
        &tx,
        &acc,
        &trunc,
        &incomplete,
    );
    drop(tx);

    let emitted: String = rx.iter().map(|(_, text)| text).collect();
    assert_eq!(acc.lock().unwrap().len(), 1024);
    assert!(trunc.load(Ordering::SeqCst));
    assert!(!incomplete.load(Ordering::SeqCst));
    assert!(emitted.len() <= 1024);
}

#[test]
fn read_lines_capped_drops_full_live_queue_without_blocking() {
    use std::io::Cursor;
    use std::sync::mpsc;

    let data = vec![b'\n'; 256 * 8192];
    let (tx, rx) = mpsc::sync_channel(32);
    let acc = std::sync::Arc::new(Mutex::new(Vec::new()));
    let trunc = std::sync::Arc::new(AtomicBool::new(false));
    let incomplete = std::sync::Arc::new(AtomicBool::new(false));
    let acc_reader = std::sync::Arc::clone(&acc);
    let trunc_reader = std::sync::Arc::clone(&trunc);
    let incomplete_reader = std::sync::Arc::clone(&incomplete);
    let data_reader = data.clone();
    let started = Instant::now();
    let handle = thread::spawn(move || {
        read_lines_capped(
            Some(Cursor::new(data_reader.clone())),
            data_reader.len(),
            OutputStream::Stdout,
            &tx,
            &acc_reader,
            &trunc_reader,
            &incomplete_reader,
        );
    });
    let mut emitted = Vec::new();
    while !handle.is_finished() {
        while let Ok((_, text)) = rx.try_recv() {
            emitted.push(text);
        }
        thread::yield_now();
    }
    handle.join().expect("reader thread");
    while let Ok((_, text)) = rx.try_recv() {
        emitted.push(text);
    }

    assert!(
        started.elapsed() < Duration::from_secs(2),
        "full live queue made the reader block: {:?}",
        started.elapsed()
    );
    assert_eq!(emitted.concat().len(), data.len());
    assert_eq!(acc.lock().unwrap().len(), data.len());
    assert!(!incomplete.load(Ordering::SeqCst));
}

#[test]
fn utf8_decoder_preserves_codepoints_split_across_live_blocks() {
    let mut decoder = Utf8ChunkDecoder::new();
    let cjk = "中".as_bytes();
    let emoji = "🦀".as_bytes();

    assert_eq!(decoder.push(&cjk[..1]), "");
    assert_eq!(decoder.push(&cjk[1..]), "中");
    assert_eq!(decoder.push(&emoji[..2]), "");
    assert_eq!(decoder.push(&emoji[2..]), "🦀");
    assert_eq!(decoder.finish(), "");
    assert!(!decoder.saw_lossy());
}

#[test]
fn utf8_decoder_lossy_only_on_illegal_sequences() {
    let mut decoder = Utf8ChunkDecoder::new();
    assert_eq!(decoder.push(&[0xff, b'A']), "\u{FFFD}A");
    assert!(decoder.saw_lossy());
    assert_eq!(decoder.finish(), "");
}

struct InterruptThenData {
    interrupted: bool,
    inner: std::io::Cursor<Vec<u8>>,
}

impl std::io::Read for InterruptThenData {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if !self.interrupted {
            self.interrupted = true;
            return Err(std::io::Error::from(std::io::ErrorKind::Interrupted));
        }
        self.inner.read(buf)
    }
}

struct FailRead;

impl std::io::Read for FailRead {
    fn read(&mut self, _buf: &mut [u8]) -> std::io::Result<usize> {
        Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "injected read failure",
        ))
    }
}

#[test]
fn interrupted_read_is_retried_and_does_not_mark_incomplete() {
    use std::sync::mpsc;
    let (tx, rx) = mpsc::sync_channel(8);
    let acc = Mutex::new(Vec::new());
    let trunc = AtomicBool::new(false);
    let incomplete = AtomicBool::new(false);
    read_lines_capped(
        Some(InterruptThenData {
            interrupted: false,
            inner: std::io::Cursor::new(b"ok\n".to_vec()),
        }),
        64,
        OutputStream::Stdout,
        &tx,
        &acc,
        &trunc,
        &incomplete,
    );
    drop(tx);
    let joined: String = rx.iter().map(|(_, t)| t).collect();
    assert_eq!(joined, "ok\n");
    assert_eq!(acc.lock().unwrap().as_slice(), b"ok\n");
    assert!(!trunc.load(Ordering::SeqCst));
    assert!(!incomplete.load(Ordering::SeqCst));
}

#[test]
fn ordinary_read_error_marks_incomplete() {
    use std::sync::mpsc;
    let (tx, rx) = mpsc::sync_channel(8);
    let acc = Mutex::new(Vec::new());
    let trunc = AtomicBool::new(false);
    let incomplete = AtomicBool::new(false);
    read_lines_capped(
        Some(FailRead),
        64,
        OutputStream::Stdout,
        &tx,
        &acc,
        &trunc,
        &incomplete,
    );
    drop(tx);
    assert!(rx.iter().next().is_none());
    assert!(acc.lock().unwrap().is_empty());
    assert!(!trunc.load(Ordering::SeqCst));
    assert!(incomplete.load(Ordering::SeqCst));
}

#[test]
fn read_pipe_preserves_cjk_and_emoji_split_across_8k_reads() {
    use std::io::Cursor;
    let mut data = vec![b'x'; 8191];
    data.extend_from_slice("中".as_bytes());
    data.extend_from_slice("🦀".as_bytes());
    let acc = Mutex::new(Vec::new());
    let trunc = AtomicBool::new(false);
    let incomplete = AtomicBool::new(false);
    let mut live = String::new();
    read_pipe_capped(
        Some(Cursor::new(data.clone())),
        data.len(),
        &acc,
        &trunc,
        &incomplete,
        |text| {
            live.push_str(&text);
            true
        },
    );
    let expected = format!("{}中🦀", "x".repeat(8191));
    assert_eq!(live, expected);
    assert_eq!(acc.lock().unwrap().as_slice(), data.as_slice());
    assert!(!trunc.load(Ordering::SeqCst));
    assert!(!incomplete.load(Ordering::SeqCst));
}

#[cfg(all(
    unix,
    any(
        target_os = "aix",
        target_os = "android",
        target_os = "dragonfly",
        target_os = "freebsd",
        target_os = "haiku",
        target_os = "hurd",
        target_os = "illumos",
        target_os = "ios",
        target_os = "linux",
        target_os = "macos",
        target_os = "netbsd",
        target_os = "nto",
        target_os = "openbsd",
        target_os = "solaris"
    )
))]
#[test]
fn waitid_observes_exit_without_reaping_child() {
    let mut cmd = Command::new("sh");
    cmd.args(["-c", "exit 7"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let process_control = configure_process_group(&mut cmd).expect("configure process group");
    let mut child = cmd.spawn().expect("spawn short-lived child");
    process_control.attach(&child).expect("attach child");

    loop {
        match poll_child(&mut child, &process_control).expect("observe child") {
            ChildPoll::Running => thread::sleep(Duration::from_millis(5)),
            ChildPoll::Exited(None) => break,
            ChildPoll::Exited(Some(_)) => panic!("waitid observation unexpectedly reaped child"),
        }
    }

    let status = child.wait().expect("reap after waitid observation");
    assert_eq!(status.code(), Some(7));
}

#[cfg(unix)]
#[test]
fn streaming_newline_storm_has_bounded_live_callbacks() {
    let spec = RunSpec {
        agent: AgentId::Claude,
        program: PathBuf::from("sh"),
        args: vec!["-c".into(), "yes | head -c 2097152".into()],
        cwd: None,
        env: vec![],
    };
    let cancel = CancelToken::new();
    let callbacks = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let seen = std::sync::Arc::clone(&callbacks);
    let _result = SystemProcessRunner.run_streaming(
        &spec,
        Duration::from_secs(5),
        2 * 1024 * 1024,
        &cancel,
        &move |_, _| {
            seen.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        },
    );

    assert!(
        callbacks.load(std::sync::atomic::Ordering::SeqCst) <= 300,
        "newline storm produced too many callbacks"
    );
}

#[cfg(unix)]
#[test]
fn exited_child_at_poll_boundary_beats_timeout() {
    let output = run_capture_timeout("sh", &["-c", "exit 0"], Duration::from_millis(40))
        .expect("an already-exited child should win the poll race");
    assert_eq!(output.status.code(), Some(0));
}

#[cfg(unix)]
#[test]
fn timeout_kills_descendants_that_hold_output_pipes() {
    let started = Instant::now();
    let err = run_capture_timeout(
        "sh",
        &["-c", "(sleep 30)& wait"],
        Duration::from_millis(200),
    )
    .unwrap_err();

    assert_eq!(err.kind(), io::ErrorKind::TimedOut);
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "reader cleanup took {:?}",
        started.elapsed()
    );
}

#[cfg(unix)]
#[test]
fn exited_leader_with_descendant_pipe_is_cleaned_before_reap() {
    let started = Instant::now();
    let output = run_capture_timeout(
        "sh",
        &["-c", "(sleep 30)& exit 0"],
        Duration::from_secs(2),
    )
    .expect("the exited leader should still produce a successful result");

    assert_eq!(output.status.code(), Some(0));
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "reader cleanup took {:?}",
        started.elapsed()
    );
}

#[cfg(unix)]
#[test]
fn detached_descendant_pipe_is_bounded_without_post_reap_group_signal() {
    let started = Instant::now();
    let output = run_capture_timeout(
        "sh",
        &["-c", "(setsid sleep 3)& exit 0"],
        Duration::from_secs(2),
    )
    .expect("a detached descendant must not turn cleanup into a hang");

    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stdout.contains("[incomplete]") || stderr.contains("[incomplete]"),
        "detached pipe should be reported incomplete: stdout={stdout:?}, stderr={stderr:?}"
    );
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "detached reader cleanup took {:?}",
        started.elapsed()
    );
}

#[cfg(windows)]
#[test]
fn windows_job_kills_descendant_after_parent_exits() {
    let started = Instant::now();
    let err = run_capture_timeout(
        "cmd.exe",
        &["/C", "ping -n 30 127.0.0.1"],
        Duration::from_millis(400),
    )
    .unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::TimedOut);
    // Hang detector only — not a latency SLA. `run_capture_timeout` does not
    // return a pid, so we cannot cheaply wait for the spawned tree to vanish.
    // Without the job object, `ping -n 30` occupies ~30s and the waiter can
    // hang on descendant pipes; returning well before that is the reaping check.
    assert!(
        started.elapsed() < Duration::from_secs(20),
        "job cleanup hung (ping -n 30 would occupy ~30s without a job kill): {:?}",
        started.elapsed()
    );
}

fn strip_acc_suffix(s: &str) -> &str {
    s.strip_suffix("\n…[incomplete]")
        .or_else(|| s.strip_suffix("\n…[truncated]"))
        .unwrap_or(s)
}

fn collect_streaming(spec: RunSpec, timeout: Duration, max: usize) -> (AgentRunResult, String) {
    let live = std::sync::Mutex::new(String::new());
    let result = SystemProcessRunner.run_streaming(
        &spec,
        timeout,
        max,
        &CancelToken::new(),
        &|stream, text| {
            if stream == OutputStream::Stdout {
                live.lock().unwrap_or_else(|e| e.into_inner()).push_str(text);
            }
        },
    );
    let live = live.into_inner().unwrap_or_else(|e| e.into_inner());
    (result, live)
}

#[cfg(unix)]
fn printf_spec(script: &str) -> RunSpec {
    RunSpec {
        agent: AgentId::Claude,
        program: PathBuf::from("sh"),
        args: vec!["-c".into(), script.into()],
        cwd: None,
        env: vec![],
    }
}

#[cfg(windows)]
fn printf_spec(script: &str) -> RunSpec {
    RunSpec {
        agent: AgentId::Claude,
        program: PathBuf::from("powershell.exe"),
        args: vec!["-NoProfile".into(), "-Command".into(), script.into()],
        cwd: None,
        env: vec![],
    }
}

#[test]
fn streaming_preserves_consecutive_empty_lines_end_to_end() {
    #[cfg(unix)]
    let spec = printf_spec("printf 'first\\n\\nthird\\n'");
    #[cfg(windows)]
    let spec = printf_spec("[Console]::Out.Write(\"first`n`nthird`n\")");
    let (result, live) = collect_streaming(spec, Duration::from_secs(8), 64 * 1024);
    assert_eq!(result.status, RunStatus::Ok, "stderr={}", result.stderr);
    assert!(!result.truncated);
    let body = strip_acc_suffix(&result.stdout);
    assert!(
        body.contains("first\n\nthird"),
        "stdout lost empty line: {body:?}"
    );
    assert_eq!(live, body);
}

#[test]
fn streaming_fast_exit_beyond_channel_capacity_is_lossless() {
    const N: usize = 400_000;
    #[cfg(unix)]
    let spec = printf_spec(&format!("head -c {N} /dev/zero | tr '\\0' 'A'"));
    #[cfg(windows)]
    let spec = printf_spec(&format!("[Console]::Out.Write(('A'*{N}))"));
    let started = Instant::now();
    let (result, live) = collect_streaming(spec, Duration::from_secs(15), 1024 * 1024);
    assert!(
        started.elapsed() < Duration::from_secs(12),
        "over-capacity stream deadlocked: {:?}",
        started.elapsed()
    );
    assert_eq!(result.status, RunStatus::Ok, "stderr={}", result.stderr);
    assert!(!result.truncated);
    let body = strip_acc_suffix(&result.stdout);
    assert_eq!(live, body);
    assert_eq!(live.len(), N);
    assert!(live.bytes().all(|b| b == b'A'));
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
        thread::sleep(Duration::from_millis(10));
    }
    panic!("pid {pid} still exists after {budget:?}");
}

#[cfg(unix)]
#[test]
fn exited_leader_kills_stdio_closed_descendant() {
    let spec = printf_spec("sleep 30 >/dev/null 2>&1 & echo $!; exit 0");
    let started = Instant::now();
    let result = SystemProcessRunner.run(&spec, Duration::from_secs(3), 64 * 1024);
    assert_eq!(result.status, RunStatus::Ok, "stderr={}", result.stderr);
    assert_eq!(result.exit_code, Some(0));
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "cleanup took {:?}",
        started.elapsed()
    );
    let pid: i32 = strip_acc_suffix(&result.stdout)
        .lines()
        .find(|line| !line.trim().is_empty())
        .expect("descendant pid")
        .trim()
        .parse()
        .expect("numeric pid");
    wait_unix_pid_gone(pid, Duration::from_millis(500));
}

#[cfg(unix)]
#[test]
fn descendant_holding_output_pipe_marks_truncated() {
    let spec = printf_spec("sleep 30 & exit 0");
    let started = Instant::now();
    let result = SystemProcessRunner.run(&spec, Duration::from_secs(3), 64 * 1024);
    assert!(
        result.truncated,
        "stdout={:?} stderr={:?}",
        result.stdout, result.stderr
    );
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "pipe-holder cleanup took {:?}",
        started.elapsed()
    );
}

#[test]
fn first_reader_join_timeout_stays_incomplete_after_second_join() {
    #[cfg(unix)]
    let mut cmd = Command::new("true");
    #[cfg(windows)]
    let mut cmd = Command::new("cmd.exe");
    #[cfg(windows)]
    cmd.args(["/C", "exit", "0"]);
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let process_control = configure_process_group(&mut cmd).expect("configure");
    let mut child = cmd.spawn().expect("spawn");
    process_control.attach(&child).expect("attach");
    let handle = thread::spawn(|| {
        thread::sleep(Duration::from_millis(80));
    });
    let first = join_reader_bounded(handle, Duration::from_millis(5));
    assert!(matches!(first, ReaderJoin::Pending(_)));
    let mut terminated = true;
    let incomplete = finish_reader(
        first,
        Duration::from_secs(1),
        &process_control,
        &mut child,
        &mut terminated,
    );
    assert!(incomplete, "second join success must not clear the first timeout");
    let _ = reap_child(&mut child, &process_control);
}
