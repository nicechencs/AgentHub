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
    read_lines_capped(
        Some(Cursor::new(data.as_bytes())),
        6,
        OutputStream::Stdout,
        &tx,
        &acc,
        &trunc,
    );
    drop(tx);
    let chunks: Vec<_> = rx.iter().collect();
    let joined: String = chunks.into_iter().map(|(_, t)| t).collect();
    assert_eq!(joined.len(), 6);
    assert!(trunc.load(Ordering::SeqCst));
    assert_eq!(acc.lock().unwrap().len(), 6);
}

#[test]
fn read_lines_capped_preserves_consecutive_empty_lines() {
    use std::io::Cursor;
    use std::sync::mpsc;
    let (tx, rx) = mpsc::sync_channel(32);
    let acc = Mutex::new(Vec::new());
    let trunc = AtomicBool::new(false);
    read_lines_capped(
        Some(Cursor::new(b"first\n\nthird\n")),
        7,
        OutputStream::Stdout,
        &tx,
        &acc,
        &trunc,
    );
    drop(tx);
    let chunks: Vec<String> = rx.iter().map(|(_, text)| text).collect();
    let joined: String = chunks.concat();
    assert!(joined.starts_with("first\n\n"));
    assert!(joined.contains("\n\n"));
    assert_eq!(acc.lock().unwrap().as_slice(), b"first\n\nt");
    assert!(trunc.load(Ordering::SeqCst));
}

#[test]
fn read_lines_capped_drains_long_unterminated_line_without_growing() {
    use std::io::Cursor;
    use std::sync::mpsc;

    let data = vec![b'x'; 4 * 1024 * 1024];
    let (tx, rx) = mpsc::sync_channel(32);
    let acc = Mutex::new(Vec::new());
    let trunc = AtomicBool::new(false);
    read_lines_capped(
        Some(Cursor::new(data)),
        1024,
        OutputStream::Stdout,
        &tx,
        &acc,
        &trunc,
    );
    drop(tx);

    let emitted: String = rx.iter().map(|(_, text)| text).collect();
    assert_eq!(acc.lock().unwrap().len(), 1024);
    assert!(trunc.load(Ordering::SeqCst));
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
    let acc_reader = std::sync::Arc::clone(&acc);
    let trunc_reader = std::sync::Arc::clone(&trunc);
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
    let result = run_capture_timeout(
        "powershell.exe",
        &[
            "-NoProfile",
            "-Command",
            "Start-Process ping -ArgumentList '-t','127.0.0.1' -WindowStyle Hidden; exit 0",
        ],
        Duration::from_secs(2),
    )
    .expect("parent should exit cleanly");
    assert!(
        started.elapsed() < Duration::from_secs(4),
        "job cleanup took {:?}",
        started.elapsed()
    );
    assert!(result.stdout.len() <= 64 * 1024);
}
