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

fn apply_no_window(cmd: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let _ = cmd;
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
    apply_no_window(&mut cmd);

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

    match wait_with_timeout_streaming(&mut child, timeout, max_output_bytes) {
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
) -> WaitOutcome {
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let max = max_output_bytes;

    let stdout_acc = Arc::new(Mutex::new(Vec::<u8>::new()));
    let stderr_acc = Arc::new(Mutex::new(Vec::<u8>::new()));
    let stdout_trunc = Arc::new(AtomicBool::new(false));
    let stderr_trunc = Arc::new(AtomicBool::new(false));

    // Fan-in so emit_install_log is only called from this wait thread (orderly).
    let (tx, rx) = mpsc::channel::<String>();

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
    let poll = Duration::from_millis(50);
    loop {
        // Drain available lines for live GUI progress.
        while let Ok(line) = rx.try_recv() {
            emit_install_log(&line);
        }

        match child.try_wait() {
            Ok(Some(status)) => {
                // Readers finish; flush remaining lines.
                let _ = stdout_handle.join();
                let _ = stderr_handle.join();
                while let Ok(line) = rx.try_recv() {
                    emit_install_log(&line);
                }
                let stdout = acc_to_string(&stdout_acc, stdout_trunc.load(Ordering::Relaxed));
                let stderr = acc_to_string(&stderr_acc, stderr_trunc.load(Ordering::Relaxed));
                return WaitOutcome::Finished {
                    status,
                    stdout,
                    stderr,
                };
            }
            Ok(None) => {
                if started.elapsed() >= timeout {
                    // Kill first so reader threads unblock on pipe EOF, then join.
                    kill_process_tree(child);
                    let _ = child.wait();
                    let _ = stdout_handle.join();
                    let _ = stderr_handle.join();
                    while let Ok(line) = rx.try_recv() {
                        emit_install_log(&line);
                    }
                    let stdout = acc_to_string(&stdout_acc, stdout_trunc.load(Ordering::Relaxed));
                    let stderr = acc_to_string(&stderr_acc, stderr_trunc.load(Ordering::Relaxed));
                    return WaitOutcome::Timeout { stdout, stderr };
                }
                thread::sleep(poll);
            }
            Err(e) => {
                kill_process_tree(child);
                let _ = child.wait();
                return WaitOutcome::IoError(e);
            }
        }
    }
}

fn acc_to_string(acc: &Mutex<Vec<u8>>, truncated: bool) -> String {
    let bytes = acc.lock().map(|g| g.clone()).unwrap_or_default();
    let mut s = String::from_utf8_lossy(&bytes).into_owned();
    if truncated {
        s.push_str("\n…(output truncated)");
    }
    s
}

/// Read pipe bytes, emit complete lines (split on `\n` / `\r`), accumulate capped body.
fn read_stream_lines<R: Read>(
    reader: Option<R>,
    max: usize,
    tx: &mpsc::Sender<String>,
    acc: &Mutex<Vec<u8>>,
    truncated: &AtomicBool,
) {
    let Some(mut r) = reader else {
        return;
    };
    let mut pending = String::new();
    let mut chunk = [0u8; 8192];
    loop {
        match r.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                {
                    let mut buf = acc.lock().unwrap_or_else(|e| e.into_inner());
                    let remain = max.saturating_sub(buf.len());
                    if remain == 0 {
                        truncated.store(true, Ordering::Relaxed);
                    } else {
                        let take = n.min(remain);
                        buf.extend_from_slice(&chunk[..take]);
                        if take < n {
                            truncated.store(true, Ordering::Relaxed);
                        }
                    }
                }
                let text = String::from_utf8_lossy(&chunk[..n]);
                for ch in text.chars() {
                    if ch == '\n' || ch == '\r' {
                        let line = std::mem::take(&mut pending);
                        let trimmed = line.trim_end();
                        if !trimmed.is_empty() {
                            let _ = tx.send(trimmed.to_string());
                        }
                    } else {
                        pending.push(ch);
                        // Progress spinners may stay on one long line; flush occasionally.
                        if pending.len() >= 200 {
                            let line = std::mem::take(&mut pending);
                            let trimmed = line.trim_end();
                            if !trimmed.is_empty() {
                                let _ = tx.send(trimmed.to_string());
                            }
                        }
                    }
                }
            }
            Err(_) => break,
        }
    }
    let line = pending.trim_end();
    if !line.is_empty() {
        let _ = tx.send(line.to_string());
    }
}

fn kill_process_tree(child: &mut Child) {
    #[cfg(windows)]
    {
        let pid = child.id();
        let _ = Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    #[cfg(not(windows))]
    {
        let _ = child.kill();
    }
}
