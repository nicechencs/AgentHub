//! Timed command execution for install scripts (mockable in tests).
//!
//! Output is streamed line-by-line to [`crate::services::emit_install_log`] so the
//! GUI can show live progress during long downloads (npm / native installers).

use std::io::{self, Read};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::services::emit_install_log;
use super::process::{
    configure_process_group, finish_reader, join_reader_bounded, kill_process_tree, mark_truncated,
    poll_child, ChildPoll, ProcessControl, Utf8ChunkDecoder,
};

/// Request to run an external program with timeout.
#[derive(Debug, Clone)]
pub struct ExecRequest {
    pub program: String,
    pub args: Vec<String>,
    pub timeout: Duration,
    pub max_output_bytes: usize,
}

/// Captured result of a timed command.
#[derive(Debug, Clone)]
pub struct ExecResult {
    pub command: String,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub timed_out: bool,
    pub spawn_error: Option<String>,
}

impl ExecResult {
    pub fn success(&self) -> bool {
        self.spawn_error.is_none() && !self.timed_out && self.exit_code == Some(0)
    }

    pub fn display_lines(&self) -> Vec<String> {
        let mut lines = vec![format!("$ {}", self.command)];
        if let Some(err) = &self.spawn_error {
            lines.push(format!("spawn failed: {err}"));
            return lines;
        }
        for line in self.stdout.lines() {
            lines.push(line.to_string());
        }
        for line in self.stderr.lines() {
            lines.push(line.to_string());
        }
        if self.timed_out {
            lines.push(format!(
                "✗ timed out after {}s",
                // timeout not stored; caller may add context
                0
            ));
        } else if let Some(code) = self.exit_code {
            if code == 0 {
                lines.push("✓ exit 0".into());
            } else {
                lines.push(format!("✗ exit {code}"));
            }
        }
        lines
    }
}

/// Pluggable executor (production = system; tests = mock).
pub trait CommandExecutor: Send + Sync {
    fn run(&self, req: &ExecRequest) -> ExecResult;
}

/// Default system executor with CREATE_NO_WINDOW on Windows.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemCommandExecutor;

impl CommandExecutor for SystemCommandExecutor {
    fn run(&self, req: &ExecRequest) -> ExecResult {
        run_timed(&req.program, &req.args, req.timeout, req.max_output_bytes)
    }
}

fn display_command(program: &str, args: &[String]) -> String {
    let mut parts = vec![quote_if_needed(program)];
    for a in args {
        parts.push(quote_if_needed(a));
    }
    parts.join(" ")
}

fn quote_if_needed(s: &str) -> String {
    if s.contains(' ') || s.contains('\t') {
        format!("\"{s}\"")
    } else {
        s.to_string()
    }
}

fn run_timed(
    program: &str,
    args: &[String],
    timeout: Duration,
    max_output_bytes: usize,
) -> ExecResult {
    let command = display_command(program, args);
    // Stage marker before long-running download/install so GUI is not silent.
    emit_install_log(&format!("$ {command}"));
    emit_install_log("# 子进程已启动，正在拉取输出…");
    let mut cmd = Command::new(program);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // Reduce buffering for Python-based installers (legacy kimi-cli, uv, pip).
    cmd.env("PYTHONUNBUFFERED", "1");
    cmd.env("NPM_CONFIG_PROGRESS", "true");
    cmd.env("NPM_CONFIG_LOGLEVEL", "info");
    let process_control = match configure_process_group(&mut cmd) {
        Ok(control) => control,
        Err(e) => {
            return ExecResult {
                command,
                exit_code: None,
                stdout: String::new(),
                stderr: String::new(),
                timed_out: false,
                spawn_error: Some(format!("process-group setup failed: {e}")),
            };
        }
    };

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            return ExecResult {
                command,
                exit_code: None,
                stdout: String::new(),
                stderr: String::new(),
                timed_out: false,
                spawn_error: Some(e.to_string()),
            };
        }
    };
    if let Err(e) = process_control.attach(&child) {
        let _ = child.kill();
        process_control.terminate(&mut child);
        let _ = child.wait();
        return ExecResult {
            command,
            exit_code: None,
            stdout: String::new(),
            stderr: String::new(),
            timed_out: false,
            spawn_error: Some(format!("process-group attach failed: {e}")),
        };
    }

    match wait_with_timeout_streaming(&mut child, timeout, max_output_bytes, &process_control) {
        WaitOutcome::Finished {
            status,
            stdout,
            stderr,
        } => ExecResult {
            command,
            exit_code: status.code(),
            stdout,
            stderr,
            timed_out: false,
            spawn_error: None,
        },
        WaitOutcome::Timeout { stdout, stderr } => ExecResult {
            command,
            exit_code: None,
            stdout,
            stderr,
            timed_out: true,
            spawn_error: None,
        },
        WaitOutcome::IoError(e) => ExecResult {
            command,
            exit_code: None,
            stdout: String::new(),
            stderr: String::new(),
            timed_out: false,
            spawn_error: Some(e.to_string()),
        },
    }
}

enum WaitOutcome {
    Finished {
        status: std::process::ExitStatus,
        stdout: String,
        stderr: String,
    },
    Timeout {
        stdout: String,
        stderr: String,
    },
    IoError(io::Error),
}

fn wait_with_timeout_streaming(
    child: &mut Child,
    timeout: Duration,
    max_output_bytes: usize,
    process_control: &ProcessControl,
) -> WaitOutcome {
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let max = max_output_bytes;

    let stdout_acc = Arc::new(Mutex::new(Vec::<u8>::new()));
    let stderr_acc = Arc::new(Mutex::new(Vec::<u8>::new()));
    let stdout_trunc = Arc::new(AtomicBool::new(false));
    let stderr_trunc = Arc::new(AtomicBool::new(false));

    // Bounded fan-in prevents a newline storm from flooding the UI queue.
    // The accumulator remains authoritative for the final capped output.
    let (tx, rx) = mpsc::sync_channel::<String>(32);

    let stdout_acc_r = Arc::clone(&stdout_acc);
    let stdout_trunc_r = Arc::clone(&stdout_trunc);
    let tx_out = tx.clone();
    let stdout_handle = thread::spawn(move || {
        read_stream_lines(stdout, max, &tx_out, &stdout_acc_r, &stdout_trunc_r);
    });

    let stderr_acc_r = Arc::clone(&stderr_acc);
    let stderr_trunc_r = Arc::clone(&stderr_trunc);
    let tx_err = tx.clone();
    let stderr_handle = thread::spawn(move || {
        read_stream_lines(stderr, max, &tx_err, &stderr_acc_r, &stderr_trunc_r);
    });
    drop(tx);

    let started = Instant::now();
    const MAX_LIVE_CHUNKS_PER_TICK: usize = 16;
    let poll = Duration::from_millis(50);
    loop {
        // Drain a bounded number of chunks, and skip live delivery once the
        // deadline is reached so timeout handling cannot be starved by output.
        if started.elapsed() < timeout {
            for _ in 0..MAX_LIVE_CHUNKS_PER_TICK {
                let Ok(line) = rx.try_recv() else {
                    break;
                };
                emit_install_log(&line);
            }
        }

        match poll_child(child, process_control) {
            Ok(ChildPoll::Exited(observed_status)) => {
                // Readers finish; flush remaining lines.
                let mut terminated = false;
                let stdout_incomplete = finish_reader(
                    join_reader_bounded(stdout_handle, Duration::from_millis(250)),
                    Duration::from_secs(2),
                    process_control,
                    child,
                    &mut terminated,
                );
                let stderr_incomplete = finish_reader(
                    join_reader_bounded(stderr_handle, Duration::from_millis(250)),
                    Duration::from_secs(2),
                    process_control,
                    child,
                    &mut terminated,
                );
                while let Ok(line) = rx.try_recv() {
                    emit_install_log(&line);
                }
                let stdout = acc_to_string(
                    &stdout_acc,
                    stdout_trunc.load(Ordering::Relaxed),
                    stdout_incomplete,
                );
                let stderr = acc_to_string(
                    &stderr_acc,
                    stderr_trunc.load(Ordering::Relaxed),
                    stderr_incomplete,
                );
                let status = match observed_status {
                    Some(status) => status,
                    None => match child.wait() {
                        Ok(status) => status,
                        Err(error) => return WaitOutcome::IoError(error),
                    },
                };
                return WaitOutcome::Finished {
                    status,
                    stdout,
                    stderr,
                };
            }
            Ok(ChildPoll::Running) => {
                if started.elapsed() >= timeout {
                    // Kill first so reader threads unblock on pipe EOF, then join.
                    kill_process_tree(process_control, child);
                    let _ = child.wait();
                    let mut terminated = true;
                    let stdout_incomplete = finish_reader(
                        join_reader_bounded(stdout_handle, Duration::from_secs(2)),
                        Duration::from_secs(2),
                        process_control,
                        child,
                        &mut terminated,
                    );
                    let stderr_incomplete = finish_reader(
                        join_reader_bounded(stderr_handle, Duration::from_secs(2)),
                        Duration::from_secs(2),
                        process_control,
                        child,
                        &mut terminated,
                    );
                    while let Ok(line) = rx.try_recv() {
                        emit_install_log(&line);
                    }
                    let stdout = acc_to_string(
                        &stdout_acc,
                        stdout_trunc.load(Ordering::Relaxed),
                        stdout_incomplete,
                    );
                    let stderr = acc_to_string(
                        &stderr_acc,
                        stderr_trunc.load(Ordering::Relaxed),
                        stderr_incomplete,
                    );
                    return WaitOutcome::Timeout { stdout, stderr };
                }
                thread::sleep(poll);
            }
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                let mut terminated = true;
                let _ = finish_reader(
                    join_reader_bounded(stdout_handle, Duration::from_secs(2)),
                    Duration::from_secs(2),
                    process_control,
                    child,
                    &mut terminated,
                );
                let _ = finish_reader(
                    join_reader_bounded(stderr_handle, Duration::from_secs(2)),
                    Duration::from_secs(2),
                    process_control,
                    child,
                    &mut terminated,
                );
                return WaitOutcome::IoError(e);
            }
        }
    }
}

fn acc_to_string(acc: &Mutex<Vec<u8>>, truncated: bool, incomplete: bool) -> String {
    let bytes = acc.lock().map(|g| g.clone()).unwrap_or_default();
    let mut s = String::from_utf8_lossy(&bytes).into_owned();
    if incomplete {
        s.push_str("\n…(output incomplete)");
    } else if truncated {
        s.push_str("\n…(output truncated)");
    }
    s
}

/// Read pipe bytes, emit bounded chunks, and accumulate a capped body.
fn read_stream_lines<R: Read>(
    reader: Option<R>,
    max: usize,
    tx: &mpsc::SyncSender<String>,
    acc: &Mutex<Vec<u8>>,
    truncated: &AtomicBool,
) {
    let Some(mut r) = reader else {
        return;
    };
    const READ_CHUNK_BYTES: usize = 8192;
    const EMIT_CHUNK_BYTES: usize = 8192;
    let mut output = Vec::with_capacity(EMIT_CHUNK_BYTES);
    let mut chunk = [0u8; READ_CHUNK_BYTES];
    let mut decoder = Utf8ChunkDecoder::new();
    loop {
        match r.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                let accepted = {
                    let mut buf = acc.lock().unwrap_or_else(|e| e.into_inner());
                    let room = max.saturating_sub(buf.len());
                    let take = n.min(room);
                    buf.extend_from_slice(&chunk[..take]);
                    take
                };
                if accepted < n {
                    mark_truncated(truncated);
                    // Continue draining the pipe, but never enqueue after cap.
                }
                for byte in &chunk[..accepted] {
                    output.push(*byte);
                    if output.len() == EMIT_CHUNK_BYTES {
                        let text = decoder.push(&output);
                        if !text.is_empty() {
                            let _ = tx.try_send(text);
                        }
                        output.clear();
                    }
                }
            }
            Err(_) => break,
        }
    }
    if !output.is_empty() {
        let text = decoder.push(&output);
        if !text.is_empty() {
            let _ = tx.try_send(text);
        }
    }
    let text = decoder.finish();
    if !text.is_empty() {
        let _ = tx.try_send(text);
    }
}
