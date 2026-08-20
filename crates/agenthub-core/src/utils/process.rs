//! Process helpers: detect capture + timed multi-agent runs.
//! Windows: CREATE_NO_WINDOW to avoid flashing consoles.

use std::ffi::OsStr;
use std::io::{self, BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::catalog::limits::DETECT_CAPTURE_MAX_BYTES;
use crate::models::{AgentRunResult, OutputStream, RunSpec, RunStatus};

/// Default timeout for version / detect probes (cold-start of large CLIs needs headroom).
pub use crate::catalog::limits::DETECT_CAPTURE_TIMEOUT;

/// Cooperative cancel flag for streaming runs.
#[derive(Debug, Clone, Default)]
pub struct CancelToken {
    cancelled: Arc<AtomicBool>,
}

impl CancelToken {
    pub fn new() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
}

/// Run a command capturing stdout/stderr without showing a window on Windows.
///
/// Bounded by [`DETECT_CAPTURE_TIMEOUT`]; on timeout kills the process tree and
/// returns [`io::ErrorKind::TimedOut`] (callers must not map timeout → NotFound).
pub fn run_capture<S: AsRef<OsStr>>(program: S, args: &[&str]) -> io::Result<Output> {
    run_capture_timeout(program, args, DETECT_CAPTURE_TIMEOUT)
}

/// Like [`run_capture`], with extra child env (e.g. PATH prefixed with Node 22).
pub fn run_capture_with_env<S: AsRef<OsStr>>(
    program: S,
    args: &[&str],
    extra_env: &[(String, String)],
) -> io::Result<Output> {
    run_capture_timeout_env(program, args, DETECT_CAPTURE_TIMEOUT, extra_env)
}

/// Like [`run_capture`] with an explicit timeout.
pub fn run_capture_timeout<S: AsRef<OsStr>>(
    program: S,
    args: &[&str],
    timeout: Duration,
) -> io::Result<Output> {
    run_capture_timeout_env(program, args, timeout, &[])
}

fn run_capture_timeout_env<S: AsRef<OsStr>>(
    program: S,
    args: &[&str],
    timeout: Duration,
    extra_env: &[(String, String)],
) -> io::Result<Output> {
    let mut cmd = Command::new(program);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (k, v) in extra_env {
        cmd.env(k, v);
    }
    apply_no_window(&mut cmd);
    let mut child = cmd.spawn()?;
    match wait_with_timeout(&mut child, timeout, DETECT_CAPTURE_MAX_BYTES) {
        WaitOutcome::Finished {
            status,
            stdout,
            stderr,
            ..
        } => Ok(Output {
            status,
            stdout: stdout.into_bytes(),
            stderr: stderr.into_bytes(),
        }),
        WaitOutcome::Timeout { .. } => Err(io::Error::new(
            io::ErrorKind::TimedOut,
            format!("process timed out after {}s", timeout.as_secs()),
        )),
        WaitOutcome::IoError(e) => Err(e),
    }
}

/// First line of stdout, trimmed.
pub fn stdout_first_line(output: &Output) -> Option<String> {
    let s = String::from_utf8_lossy(&output.stdout);
    s.lines()
        .next()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
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

/// Pluggable process runner (production = system; tests = mock).
pub trait ProcessRunner: Send + Sync {
    fn run(&self, spec: &RunSpec, timeout: Duration, max_output_bytes: usize) -> AgentRunResult;
}

/// Streaming runner: line-level stdout/stderr chunks + cooperative cancel.
pub trait StreamingProcessRunner: Send + Sync {
    fn run_streaming(
        &self,
        spec: &RunSpec,
        timeout: Duration,
        max_output_bytes: usize,
        cancel: &CancelToken,
        on_chunk: &(dyn Fn(OutputStream, &str) + Send + Sync),
    ) -> AgentRunResult;
}

/// Default runner: spawn + poll until exit or timeout, then kill process tree.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemProcessRunner;

impl ProcessRunner for SystemProcessRunner {
    fn run(&self, spec: &RunSpec, timeout: Duration, max_output_bytes: usize) -> AgentRunResult {
        run_spec_with_timeout(spec, timeout, max_output_bytes)
    }
}

impl StreamingProcessRunner for SystemProcessRunner {
    fn run_streaming(
        &self,
        spec: &RunSpec,
        timeout: Duration,
        max_output_bytes: usize,
        cancel: &CancelToken,
        on_chunk: &(dyn Fn(OutputStream, &str) + Send + Sync),
    ) -> AgentRunResult {
        run_spec_streaming(spec, timeout, max_output_bytes, cancel, on_chunk)
    }
}

/// Recording runner for tests: stores specs and returns canned OK results.
#[derive(Debug, Clone)]
pub struct RecordingProcessRunner {
    pub calls: Arc<Mutex<Vec<RunSpec>>>,
    /// Force a status for every call (default Ok).
    pub force_status: Arc<Mutex<RunStatus>>,
    pub delay: Duration,
}

impl Default for RecordingProcessRunner {
    fn default() -> Self {
        Self::new()
    }
}

impl RecordingProcessRunner {
    pub fn new() -> Self {
        Self {
            calls: Arc::new(Mutex::new(Vec::new())),
            force_status: Arc::new(Mutex::new(RunStatus::Ok)),
            delay: Duration::ZERO,
        }
    }

    pub fn with_status(status: RunStatus) -> Self {
        let r = Self::new();
        *r.force_status.lock().unwrap() = status;
        r
    }
}

impl ProcessRunner for RecordingProcessRunner {
    fn run(&self, spec: &RunSpec, _timeout: Duration, _max_output_bytes: usize) -> AgentRunResult {
        if self.delay > Duration::ZERO {
            thread::sleep(self.delay);
        }
        if let Ok(mut g) = self.calls.lock() {
            g.push(spec.clone());
        }
        let status = self
            .force_status
            .lock()
            .map(|g| *g)
            .unwrap_or(RunStatus::Ok);
        AgentRunResult {
            agent: spec.agent,
            status,
            exit_code: if status == RunStatus::Ok {
                Some(0)
            } else {
                Some(1)
            },
            duration_ms: self.delay.as_millis() as u64,
            stdout: format!("mock:{}", spec.agent.as_str()),
            stderr: String::new(),
            command: spec.display_command(),
            error: if status.is_hard_failure() {
                Some(format!("mock status {}", status.as_str()))
            } else {
                None
            },
            truncated: false,
            native_session_id: None,
        }
    }
}

impl StreamingProcessRunner for RecordingProcessRunner {
    fn run_streaming(
        &self,
        spec: &RunSpec,
        timeout: Duration,
        max_output_bytes: usize,
        cancel: &CancelToken,
        on_chunk: &(dyn Fn(OutputStream, &str) + Send + Sync),
    ) -> AgentRunResult {
        // Poll cancel during configured delay so tests can exercise mid-run cancel.
        let delay = self.delay;
        let started = Instant::now();
        let step = Duration::from_millis(20);
        while started.elapsed() < delay {
            if cancel.is_cancelled() {
                if let Ok(mut g) = self.calls.lock() {
                    g.push(spec.clone());
                }
                return AgentRunResult {
                    agent: spec.agent,
                    status: RunStatus::Cancelled,
                    exit_code: None,
                    duration_ms: started.elapsed().as_millis() as u64,
                    stdout: String::new(),
                    stderr: String::new(),
                    command: spec.display_command(),
                    error: Some("cancelled".into()),
                    truncated: false,
                    native_session_id: None,
                };
            }
            let remaining = delay.saturating_sub(started.elapsed());
            thread::sleep(step.min(remaining));
        }
        if cancel.is_cancelled() {
            if let Ok(mut g) = self.calls.lock() {
                g.push(spec.clone());
            }
            return AgentRunResult {
                agent: spec.agent,
                status: RunStatus::Cancelled,
                exit_code: None,
                duration_ms: started.elapsed().as_millis() as u64,
                stdout: String::new(),
                stderr: String::new(),
                command: spec.display_command(),
                error: Some("cancelled".into()),
                truncated: false,
                native_session_id: None,
            };
        }
        // Delay already applied above — avoid double-sleep in `run`.
        let saved_delay = self.delay;
        // Temporarily zero delay via unsafe is wrong; clone-less approach: call body inline.
        if let Ok(mut g) = self.calls.lock() {
            g.push(spec.clone());
        }
        let status = self
            .force_status
            .lock()
            .map(|g| *g)
            .unwrap_or(RunStatus::Ok);
        let _ = (timeout, max_output_bytes, saved_delay);
        let result = AgentRunResult {
            agent: spec.agent,
            status,
            exit_code: if status == RunStatus::Ok {
                Some(0)
            } else {
                Some(1)
            },
            duration_ms: started.elapsed().as_millis() as u64,
            stdout: format!("mock:{}", spec.agent.as_str()),
            stderr: String::new(),
            command: spec.display_command(),
            error: if status.is_hard_failure() {
                Some(format!("mock status {}", status.as_str()))
            } else {
                None
            },
            truncated: false,
            native_session_id: None,
        };
        if !result.stdout.is_empty() {
            on_chunk(OutputStream::Stdout, &result.stdout);
        }
        if !result.stderr.is_empty() {
            on_chunk(OutputStream::Stderr, &result.stderr);
        }
        result
    }
}

fn run_spec_streaming(
    spec: &RunSpec,
    timeout: Duration,
    max_output_bytes: usize,
    cancel: &CancelToken,
    on_chunk: &(dyn Fn(OutputStream, &str) + Send + Sync),
) -> AgentRunResult {
    let command = spec.display_command();
    let started = Instant::now();

    let mut cmd = Command::new(&spec.program);
    cmd.args(&spec.args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (k, v) in &spec.env {
        cmd.env(k, v);
    }
    if let Some(cwd) = &spec.cwd {
        cmd.current_dir(cwd);
    }
    apply_no_window(&mut cmd);

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            return AgentRunResult {
                agent: spec.agent,
                status: RunStatus::Failed,
                exit_code: None,
                duration_ms: started.elapsed().as_millis() as u64,
                stdout: String::new(),
                stderr: String::new(),
                command,
                error: Some(format!("spawn failed: {e}")),
                truncated: false,
                native_session_id: None,
            };
        }
    };

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let max = max_output_bytes;

    let stdout_acc = Arc::new(Mutex::new(Vec::<u8>::new()));
    let stderr_acc = Arc::new(Mutex::new(Vec::<u8>::new()));

    // mpsc fan-in so on_chunk is only invoked from this thread.
    let (tx, rx) = std::sync::mpsc::channel::<(OutputStream, String)>();
    let stdout_trunc = Arc::new(AtomicBool::new(false));
    let stderr_trunc = Arc::new(AtomicBool::new(false));

    let stdout_acc_r = Arc::clone(&stdout_acc);
    let stdout_trunc_r = Arc::clone(&stdout_trunc);
    let tx_out = tx.clone();
    let stdout_handle = thread::spawn(move || {
        read_lines_capped(
            stdout,
            max,
            OutputStream::Stdout,
            &tx_out,
            &stdout_acc_r,
            &stdout_trunc_r,
        )
    });
    let stderr_acc_r = Arc::clone(&stderr_acc);
    let stderr_trunc_r = Arc::clone(&stderr_trunc);
    let stderr_handle = thread::spawn(move || {
        read_lines_capped(
            stderr,
            max,
            OutputStream::Stderr,
            &tx,
            &stderr_acc_r,
            &stderr_trunc_r,
        )
    });

    let poll = Duration::from_millis(50);
    let outcome = loop {
        while let Ok((stream, text)) = rx.try_recv() {
            on_chunk(stream, &text);
        }
        if cancel.is_cancelled() {
            kill_process_tree(&mut child);
            let _ = child.wait();
            break StreamPoll::Cancelled;
        }
        match child.try_wait() {
            Ok(Some(status)) => break StreamPoll::Exited(status),
            Ok(None) => {
                if started.elapsed() >= timeout {
                    break StreamPoll::TimedOut;
                }
                thread::sleep(poll);
            }
            Err(e) => {
                kill_process_tree(&mut child);
                let _ = child.wait();
                let _ = stdout_handle.join();
                let _ = stderr_handle.join();
                while let Ok((stream, text)) = rx.try_recv() {
                    on_chunk(stream, &text);
                }
                return AgentRunResult {
                    agent: spec.agent,
                    status: RunStatus::Failed,
                    exit_code: None,
                    duration_ms: started.elapsed().as_millis() as u64,
                    stdout: String::new(),
                    stderr: String::new(),
                    command,
                    error: Some(format!("wait failed: {e}")),
                    truncated: false,
                    native_session_id: None,
                };
            }
        }
    };

    match outcome {
        StreamPoll::Exited(status) => {
            let _ = stdout_handle.join();
            let _ = stderr_handle.join();
            while let Ok((stream, text)) = rx.try_recv() {
                on_chunk(stream, &text);
            }
            let st = stdout_trunc.load(Ordering::SeqCst);
            let se = stderr_trunc.load(Ordering::SeqCst);
            let stdout = string_from_acc(&stdout_acc, st);
            let stderr = string_from_acc(&stderr_acc, se);
            let code = status.code();
            let ok = status.success();
            AgentRunResult {
                agent: spec.agent,
                status: if ok { RunStatus::Ok } else { RunStatus::Failed },
                exit_code: code,
                duration_ms: started.elapsed().as_millis() as u64,
                stdout,
                stderr,
                command,
                error: if ok {
                    None
                } else {
                    Some(format!("exit code {}", code.unwrap_or(-1)))
                },
                truncated: st || se,
                native_session_id: None,
            }
        }
        StreamPoll::TimedOut => {
            kill_process_tree(&mut child);
            let _ = child.wait();
            let _ = stdout_handle.join();
            let _ = stderr_handle.join();
            while let Ok((stream, text)) = rx.try_recv() {
                on_chunk(stream, &text);
            }
            let st = stdout_trunc.load(Ordering::SeqCst);
            let se = stderr_trunc.load(Ordering::SeqCst);
            let stdout = string_from_acc(&stdout_acc, st);
            let stderr = string_from_acc(&stderr_acc, se);
            AgentRunResult {
                agent: spec.agent,
                status: RunStatus::Timeout,
                exit_code: None,
                duration_ms: started.elapsed().as_millis() as u64,
                stdout,
                stderr,
                command,
                error: Some(format!("timed out after {}s", timeout.as_secs())),
                truncated: st || se,
                native_session_id: None,
            }
        }
        StreamPoll::Cancelled => {
            let _ = stdout_handle.join();
            let _ = stderr_handle.join();
            while let Ok((stream, text)) = rx.try_recv() {
                on_chunk(stream, &text);
            }
            let st = stdout_trunc.load(Ordering::SeqCst);
            let se = stderr_trunc.load(Ordering::SeqCst);
            let stdout = string_from_acc(&stdout_acc, st);
            let stderr = string_from_acc(&stderr_acc, se);
            AgentRunResult {
                agent: spec.agent,
                status: RunStatus::Cancelled,
                exit_code: None,
                duration_ms: started.elapsed().as_millis() as u64,
                stdout,
                stderr,
                command,
                error: Some("cancelled".into()),
                truncated: st || se,
                native_session_id: None,
            }
        }
    }
}

enum StreamPoll {
    Exited(std::process::ExitStatus),
    TimedOut,
    Cancelled,
}

fn read_lines_capped<R: Read>(
    stream: Option<R>,
    max: usize,
    which: OutputStream,
    tx: &std::sync::mpsc::Sender<(OutputStream, String)>,
    acc: &Mutex<Vec<u8>>,
    trunc: &AtomicBool,
) {
    let Some(r) = stream else {
        return;
    };
    let mut reader = BufReader::new(r);
    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {
                let bytes = line.as_bytes();
                let mut emit = true;
                if let Ok(mut g) = acc.lock() {
                    if g.len() >= max {
                        trunc.store(true, Ordering::SeqCst);
                        // Stop emitting chunks once capped; still drain the pipe.
                        emit = false;
                    } else {
                        let room = max.saturating_sub(g.len());
                        let take = bytes.len().min(room);
                        g.extend_from_slice(&bytes[..take]);
                        if take < bytes.len() {
                            trunc.store(true, Ordering::SeqCst);
                            // Emit only the accepted prefix (UTF-8 safe).
                            let mut end = take;
                            while end > 0 && !line.is_char_boundary(end) {
                                end -= 1;
                            }
                            if end > 0 {
                                let _ = tx.send((which, line[..end].to_string()));
                            }
                            emit = false;
                        }
                    }
                }
                if emit {
                    let _ = tx.send((which, line.clone()));
                }
            }
            Err(_) => break,
        }
    }
}

fn string_from_acc(acc: &Mutex<Vec<u8>>, truncated: bool) -> String {
    let bytes = acc.lock().map(|g| g.clone()).unwrap_or_default();
    let mut s = String::from_utf8_lossy(&bytes).into_owned();
    if truncated {
        s.push_str("\n…[truncated]");
    }
    s
}

fn run_spec_with_timeout(
    spec: &RunSpec,
    timeout: Duration,
    max_output_bytes: usize,
) -> AgentRunResult {
    let command = spec.display_command();
    let started = Instant::now();

    let mut cmd = Command::new(&spec.program);
    cmd.args(&spec.args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (k, v) in &spec.env {
        cmd.env(k, v);
    }
    if let Some(cwd) = &spec.cwd {
        cmd.current_dir(cwd);
    }
    apply_no_window(&mut cmd);

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            return AgentRunResult {
                agent: spec.agent,
                status: RunStatus::Failed,
                exit_code: None,
                duration_ms: started.elapsed().as_millis() as u64,
                stdout: String::new(),
                stderr: String::new(),
                command,
                error: Some(format!("spawn failed: {e}")),
                truncated: false,
                native_session_id: None,
            };
        }
    };

    match wait_with_timeout(&mut child, timeout, max_output_bytes) {
        WaitOutcome::Finished {
            status,
            stdout,
            stderr,
            truncated,
        } => {
            let code = status.code();
            let ok = status.success();
            AgentRunResult {
                agent: spec.agent,
                status: if ok { RunStatus::Ok } else { RunStatus::Failed },
                exit_code: code,
                duration_ms: started.elapsed().as_millis() as u64,
                stdout,
                stderr,
                command,
                error: if ok {
                    None
                } else {
                    Some(format!("exit code {}", code.unwrap_or(-1)))
                },
                truncated,
                native_session_id: None,
            }
        }
        WaitOutcome::Timeout {
            stdout,
            stderr,
            truncated,
        } => {
            // Kill process tree first, then join readers (pipes close after kill).
            kill_process_tree(&mut child);
            let _ = child.wait();
            AgentRunResult {
                agent: spec.agent,
                status: RunStatus::Timeout,
                exit_code: None,
                duration_ms: started.elapsed().as_millis() as u64,
                stdout,
                stderr,
                command,
                error: Some(format!("timed out after {}s", timeout.as_secs())),
                truncated,
                native_session_id: None,
            }
        }
        WaitOutcome::IoError(e) => {
            kill_process_tree(&mut child);
            let _ = child.wait();
            AgentRunResult {
                agent: spec.agent,
                status: RunStatus::Failed,
                exit_code: None,
                duration_ms: started.elapsed().as_millis() as u64,
                stdout: String::new(),
                stderr: String::new(),
                command,
                error: Some(format!("wait failed: {e}")),
                truncated: false,
                native_session_id: None,
            }
        }
    }
}

enum WaitOutcome {
    Finished {
        status: std::process::ExitStatus,
        stdout: String,
        stderr: String,
        truncated: bool,
    },
    Timeout {
        stdout: String,
        stderr: String,
        truncated: bool,
    },
    IoError(io::Error),
}

/// Wait for child with timeout. Caps pipe reads at max_output_bytes.
fn wait_with_timeout(child: &mut Child, timeout: Duration, max_output_bytes: usize) -> WaitOutcome {
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    let max = max_output_bytes;
    let stdout_handle = thread::spawn(move || read_capped(stdout, max));
    let stderr_handle = thread::spawn(move || read_capped(stderr, max));

    let started = Instant::now();
    let poll = Duration::from_millis(50);
    let outcome = loop {
        match child.try_wait() {
            Ok(Some(status)) => break WaitPoll::Exited(status),
            Ok(None) => {
                if started.elapsed() >= timeout {
                    break WaitPoll::TimedOut;
                }
                thread::sleep(poll);
            }
            Err(e) => {
                // Still try to drain pipes briefly after kill path.
                kill_process_tree(child);
                let _ = child.wait();
                let _ = join_capped(stdout_handle, Duration::from_secs(2));
                let _ = join_capped(stderr_handle, Duration::from_secs(2));
                return WaitOutcome::IoError(e);
            }
        }
    };

    match outcome {
        WaitPoll::Exited(status) => {
            let (stdout, t1) = join_capped(stdout_handle, Duration::from_secs(5));
            let (stderr, t2) = join_capped(stderr_handle, Duration::from_secs(5));
            WaitOutcome::Finished {
                status,
                stdout,
                stderr,
                truncated: t1 || t2,
            }
        }
        WaitPoll::TimedOut => {
            // Kill first so readers see EOF, then join with a short deadline.
            kill_process_tree(child);
            let _ = child.wait();
            let (stdout, t1) = join_capped(stdout_handle, Duration::from_secs(2));
            let (stderr, t2) = join_capped(stderr_handle, Duration::from_secs(2));
            WaitOutcome::Timeout {
                stdout,
                stderr,
                truncated: t1 || t2,
            }
        }
    }
}

enum WaitPoll {
    Exited(std::process::ExitStatus),
    TimedOut,
}

/// Read at most `max` bytes from an optional pipe; mark truncated if more data remains.
fn read_capped<R: Read>(stream: Option<R>, max: usize) -> (Vec<u8>, bool) {
    let Some(mut r) = stream else {
        return (Vec::new(), false);
    };
    let mut buf = Vec::new();
    let mut chunk = [0u8; 8192];
    let mut truncated = false;
    loop {
        match r.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                if buf.len() >= max {
                    truncated = true;
                    // Keep draining so the writer does not block forever.
                    continue;
                }
                let room = max.saturating_sub(buf.len());
                let take = n.min(room);
                buf.extend_from_slice(&chunk[..take]);
                if take < n {
                    truncated = true;
                }
            }
            Err(_) => break,
        }
    }
    (buf, truncated)
}

fn join_capped(
    handle: thread::JoinHandle<(Vec<u8>, bool)>,
    join_timeout: Duration,
) -> (String, bool) {
    // std JoinHandle has no timeout; park the join on a helper thread.
    let (tx, rx) = std::sync::mpsc::channel();
    thread::spawn(move || {
        let _ = tx.send(handle.join());
    });
    match rx.recv_timeout(join_timeout) {
        Ok(Ok((bytes, trunc))) => {
            let mut s = String::from_utf8_lossy(&bytes).into_owned();
            if trunc {
                s.push_str("\n…[truncated]");
            }
            (s, trunc)
        }
        Ok(Err(_)) => (String::new(), false),
        Err(_) => (String::new(), true), // timed out waiting for reader
    }
}

/// Kill the child; on Windows also attempt to kill the process tree via taskkill.
fn kill_process_tree(child: &mut Child) {
    #[cfg(windows)]
    {
        let pid = child.id();
        // /T = tree, /F = force. Best-effort; still call child.kill() as fallback.
        let _ = Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .and_then(|mut c| c.wait());
    }
    let _ = child.kill();
}

/// Truncate raw bytes to max length; returns (lossy utf-8 string, truncated flag).
pub fn truncate_bytes(raw: &[u8], max: usize) -> (String, bool) {
    if raw.len() <= max {
        return (String::from_utf8_lossy(raw).into_owned(), false);
    }
    let slice = &raw[..max];
    let mut s = String::from_utf8_lossy(slice).into_owned();
    s.push_str("\n…[truncated]");
    (s, true)
}

/// Resolve program path: prefer absolute binary_path, else name for PATH.
pub fn program_from_detect(binary_path: Option<&Path>, fallback_name: &str) -> PathBuf {
    binary_path
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from(fallback_name))
}

#[cfg(test)]
mod tests {
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
        let (tx, rx) = mpsc::channel();
        let acc = Mutex::new(Vec::new());
        let trunc = AtomicBool::new(false);
        read_lines_capped(
            Some(Cursor::new(data.as_bytes())),
            6, // enough for "aaaa\n" + 1 of next
            OutputStream::Stdout,
            &tx,
            &acc,
            &trunc,
        );
        drop(tx);
        let chunks: Vec<_> = rx.iter().collect();
        let joined: String = chunks.into_iter().map(|(_, t)| t).collect();
        assert!(trunc.load(Ordering::SeqCst));
        assert!(joined.len() <= 6 + 1); // allow partial last emit
        assert!(acc.lock().unwrap().len() <= 6);
    }
}
