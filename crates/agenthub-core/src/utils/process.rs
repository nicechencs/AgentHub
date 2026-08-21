//! Process helpers: detect capture + timed multi-agent runs.
//! Windows: CREATE_NO_WINDOW to avoid flashing consoles.

use std::ffi::OsStr;
use std::io::{self, Read};
#[cfg(windows)]
use std::mem::size_of;
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
    let process_control = configure_process_group(&mut cmd)?;
    let mut child = cmd.spawn()?;
    if let Err(error) = process_control.attach(&child) {
        let _ = child.kill();
        process_control.terminate(&mut child);
        let _ = child.wait();
        return Err(error);
    }
    match wait_with_timeout(
        &mut child,
        timeout,
        DETECT_CAPTURE_MAX_BYTES,
        &process_control,
    ) {
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

/// Owns the lifetime and cleanup mechanism for one spawned process tree.
///
/// Unix uses a dedicated process group. Windows uses a Job Object configured
/// to kill all assigned descendants when the owner is dropped.
pub(crate) struct ProcessControl {
    #[cfg(unix)]
    pid: std::cell::Cell<i32>,
    #[cfg(windows)]
    job: WindowsJob,
}

pub(crate) enum ChildPoll {
    Running,
    Exited(Option<std::process::ExitStatus>),
}

pub(crate) fn poll_child(
    child: &mut Child,
    process_control: &ProcessControl,
) -> io::Result<ChildPoll> {
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
    {
        let _ = child;
        return if observe_unix_child_exit(process_control.pid.get())? {
            Ok(ChildPoll::Exited(None))
        } else {
            Ok(ChildPoll::Running)
        };
    }
    #[cfg(all(
        unix,
        not(any(
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
        ))
    ))]
    {
        // Some less common Unix targets do not expose waitid through libc.
        // try_wait reaps the leader, so disable group cleanup before returning
        // and never signal a possibly reused process-group id afterwards.
        match child.try_wait()? {
            Some(status) => {
                process_control.pid.set(0);
                Ok(ChildPoll::Exited(Some(status)))
            }
            None => Ok(ChildPoll::Running),
        }
    }
    #[cfg(not(unix))]
    {
        match child.try_wait()? {
            Some(status) => Ok(ChildPoll::Exited(Some(status))),
            None => Ok(ChildPoll::Running),
        }
    }
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
fn observe_unix_child_exit(pid: i32) -> io::Result<bool> {
    let mut info = unsafe { std::mem::zeroed::<libc::siginfo_t>() };
    loop {
        let result = unsafe {
            libc::waitid(
                libc::P_PID,
                pid as libc::id_t,
                &mut info,
                libc::WEXITED | libc::WNOWAIT | libc::WNOHANG,
            )
        };
        if result == 0 {
            // libc exposes the platform-specific siginfo_t layout and accessor;
            // do not interpret the blob at a hard-coded offset.
            return Ok(unsafe { info.si_pid() } == pid as libc::pid_t);
        }

        let error = io::Error::last_os_error();
        if error.raw_os_error() == Some(libc::EINTR) {
            info = unsafe { std::mem::zeroed::<libc::siginfo_t>() };
            continue;
        }
        return Err(error);
    }
}

#[cfg(test)]
thread_local! {
    // The fault injection is consumed by the thread that performs attach.
    // A process-wide flag would let unrelated parallel spawn tests consume it.
    static FORCE_ATTACH_FAILURE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(test)]
pub(crate) fn force_attach_failure_for_test() {
    FORCE_ATTACH_FAILURE.with(|flag| flag.set(true));
}

#[cfg(windows)]
struct WindowsJob {
    handle: *mut std::ffi::c_void,
}

#[cfg(windows)]
unsafe impl Send for WindowsJob {}

#[cfg(windows)]
impl WindowsJob {
    fn new() -> io::Result<Self> {
        let handle = unsafe { create_job_object() };
        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }

        let mut limits = JobObjectExtendedLimitInformation::default();
        limits.basic.limit_flags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let configured = unsafe {
            set_information_job_object(
                handle,
                &limits,
                size_of::<JobObjectExtendedLimitInformation>(),
            )
        };
        if configured == 0 {
            unsafe {
                close_handle(handle);
            }
            return Err(io::Error::last_os_error());
        }
        Ok(Self { handle })
    }

    fn attach_and_resume(&self, child: &Child) -> io::Result<()> {
        use std::os::windows::io::AsRawHandle;

        let assigned = unsafe {
            assign_process_to_job_object(self.handle, child.as_raw_handle() as *mut _)
        };
        if assigned == 0 {
            return Err(io::Error::last_os_error());
        }
        resume_suspended_process(child.id())?;
        Ok(())
    }

    fn terminate(&self) {
        unsafe {
            let _ = terminate_job_object(self.handle, 1);
        }
    }
}

#[cfg(windows)]
impl Drop for WindowsJob {
    fn drop(&mut self) {
        unsafe {
            let _ = close_handle(self.handle);
        }
    }
}

#[cfg(windows)]
const JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE: u32 = 0x0000_2000;

#[cfg(windows)]
#[repr(C)]
#[derive(Default)]
struct JobObjectBasicLimitInformation {
    per_process_user_time_limit: i64,
    per_job_user_time_limit: i64,
    limit_flags: u32,
    minimum_working_set_size: usize,
    maximum_working_set_size: usize,
    active_process_limit: u32,
    affinity: usize,
    priority_class: u32,
    scheduling_class: u32,
}

#[cfg(windows)]
#[repr(C)]
#[derive(Default)]
struct IoCounters {
    read_operation_count: u64,
    write_operation_count: u64,
    other_operation_count: u64,
    read_transfer_count: u64,
    write_transfer_count: u64,
    other_transfer_count: u64,
}

#[cfg(windows)]
#[repr(C)]
#[derive(Default)]
struct JobObjectExtendedLimitInformation {
    basic: JobObjectBasicLimitInformation,
    io: IoCounters,
    process_memory_limit: usize,
    job_memory_limit: usize,
    peak_process_memory_used: usize,
    peak_job_memory_used: usize,
}

#[cfg(windows)]
#[repr(C)]
#[derive(Default)]
struct ThreadEntry32 {
    size: u32,
    usage: u32,
    thread_id: u32,
    owner_process_id: u32,
    base_priority: i32,
    delta_priority: i32,
    flags: u32,
}

#[cfg(windows)]
#[link(name = "kernel32")]
unsafe extern "system" {
    fn CreateJobObjectW(
        attributes: *const std::ffi::c_void,
        name: *const u16,
    ) -> *mut std::ffi::c_void;
    fn SetInformationJobObject(
        job: *mut std::ffi::c_void,
        information_class: u32,
        information: *const std::ffi::c_void,
        information_length: u32,
    ) -> i32;
    fn AssignProcessToJobObject(
        job: *mut std::ffi::c_void,
        process: *mut std::ffi::c_void,
    ) -> i32;
    fn CreateToolhelp32Snapshot(flags: u32, process_id: u32) -> *mut std::ffi::c_void;
    fn Thread32First(snapshot: *mut std::ffi::c_void, entry: *mut ThreadEntry32) -> i32;
    fn Thread32Next(snapshot: *mut std::ffi::c_void, entry: *mut ThreadEntry32) -> i32;
    fn OpenThread(
        desired_access: u32,
        inherit_handle: i32,
        thread_id: u32,
    ) -> *mut std::ffi::c_void;
    fn ResumeThread(thread: *mut std::ffi::c_void) -> u32;
    fn TerminateJobObject(job: *mut std::ffi::c_void, exit_code: u32) -> i32;
    fn CloseHandle(handle: *mut std::ffi::c_void) -> i32;
}

#[cfg(windows)]
unsafe fn create_job_object() -> *mut std::ffi::c_void {
    CreateJobObjectW(std::ptr::null(), std::ptr::null())
}

#[cfg(windows)]
unsafe fn set_information_job_object(
    job: *mut std::ffi::c_void,
    information: &JobObjectExtendedLimitInformation,
    information_length: usize,
) -> i32 {
    SetInformationJobObject(
        job,
        9,
        information as *const _ as *const std::ffi::c_void,
        information_length as u32,
    )
}

#[cfg(windows)]
unsafe fn assign_process_to_job_object(
    job: *mut std::ffi::c_void,
    process: *mut std::ffi::c_void,
) -> i32 {
    AssignProcessToJobObject(job, process)
}

#[cfg(windows)]
fn resume_suspended_process(process_id: u32) -> io::Result<()> {
    const TH32CS_SNAPTHREAD: u32 = 0x0000_0004;
    const THREAD_SUSPEND_RESUME: u32 = 0x0002;
    const INVALID_HANDLE_VALUE: *mut std::ffi::c_void = -1isize as *mut std::ffi::c_void;

    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) };
    if snapshot.is_null() || snapshot == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }

    let mut entry = ThreadEntry32 {
        size: size_of::<ThreadEntry32>() as u32,
        ..ThreadEntry32::default()
    };
    let mut result = Err(io::Error::last_os_error());
    let mut has_entry = unsafe { Thread32First(snapshot, &mut entry) } != 0;
    while has_entry {
        if entry.owner_process_id == process_id {
            let thread = unsafe { OpenThread(THREAD_SUSPEND_RESUME, 0, entry.thread_id) };
            if !thread.is_null() {
                let resumed = unsafe { ResumeThread(thread) };
                unsafe {
                    CloseHandle(thread);
                }
                if resumed != u32::MAX {
                    result = Ok(());
                    break;
                }
                result = Err(io::Error::last_os_error());
            } else {
                result = Err(io::Error::last_os_error());
            }
        }
        has_entry = unsafe { Thread32Next(snapshot, &mut entry) } != 0;
    }
    unsafe {
        CloseHandle(snapshot);
    }
    result
}

#[cfg(windows)]
unsafe fn terminate_job_object(job: *mut std::ffi::c_void, exit_code: u32) -> i32 {
    TerminateJobObject(job, exit_code)
}

#[cfg(windows)]
unsafe fn close_handle(handle: *mut std::ffi::c_void) -> i32 {
    CloseHandle(handle)
}

/// Configure a child before spawn. Windows starts suspended so the root and
/// all descendants are assigned to the Job Object before any user code runs.
pub(crate) fn configure_process_group(cmd: &mut Command) -> io::Result<ProcessControl> {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
        return Ok(ProcessControl {
            pid: std::cell::Cell::new(0),
        });
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        const CREATE_SUSPENDED: u32 = 0x0000_0004;
        let job = WindowsJob::new()?;
        cmd.creation_flags(CREATE_NO_WINDOW | CREATE_SUSPENDED);
        return Ok(ProcessControl { job });
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = cmd;
        Ok(ProcessControl {})
    }
}

impl ProcessControl {
    pub(crate) fn attach(&self, child: &Child) -> io::Result<()> {
        #[cfg(test)]
        if FORCE_ATTACH_FAILURE.with(|flag| flag.replace(false)) {
            let _ = child;
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "injected process attach failure",
            ));
        }
        #[cfg(unix)]
        {
            self.pid.set(child.id() as i32);
            return Ok(());
        }
        #[cfg(windows)]
        {
            return self.job.attach_and_resume(child);
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = child;
            Ok(())
        }
    }

    pub(crate) fn terminate(&self, child: &mut Child) {
        #[cfg(unix)]
        {
            const SIGKILL: i32 = 9;
            let pid = self.pid.get();
            if pid > 0 {
                unsafe {
                    let _ = libc_kill(-pid, SIGKILL);
                }
            }
        }
        #[cfg(windows)]
        {
            let _ = child;
            self.job.terminate();
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = child.kill();
        }
    }
}

#[cfg(unix)]
#[link(name = "c")]
unsafe extern "C" {
    fn kill(pid: i32, signal: i32) -> i32;
}

#[cfg(unix)]
unsafe fn libc_kill(pid: i32, signal: i32) -> i32 {
    kill(pid, signal)
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
    let process_control = match configure_process_group(&mut cmd) {
        Ok(control) => control,
        Err(e) => {
            return AgentRunResult {
                agent: spec.agent,
                status: RunStatus::Failed,
                exit_code: None,
                duration_ms: started.elapsed().as_millis() as u64,
                stdout: String::new(),
                stderr: String::new(),
                command,
                error: Some(format!("process-group setup failed: {e}")),
                truncated: false,
                native_session_id: None,
            };
        }
    };

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
    if let Err(e) = process_control.attach(&child) {
        let _ = child.kill();
        process_control.terminate(&mut child);
        let _ = child.wait();
        return AgentRunResult {
            agent: spec.agent,
            status: RunStatus::Failed,
            exit_code: None,
            duration_ms: started.elapsed().as_millis() as u64,
            stdout: String::new(),
            stderr: String::new(),
            command,
            error: Some(format!("process-group attach failed: {e}")),
            truncated: false,
            native_session_id: None,
        };
    }

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let max = max_output_bytes;

    let stdout_acc = Arc::new(Mutex::new(Vec::<u8>::new()));
    let stderr_acc = Arc::new(Mutex::new(Vec::<u8>::new()));

    // Bounded fan-in keeps a newline storm from turning live output into an
    // unbounded allocation queue. The accumulator remains the authoritative
    // capped result; this channel is only for best-effort live delivery.
    let (tx, rx) = std::sync::mpsc::sync_channel::<(OutputStream, String)>(32);
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

    const MAX_LIVE_CHUNKS_PER_TICK: usize = 16;
    let poll = Duration::from_millis(50);
    let outcome = loop {
        if cancel.is_cancelled() {
            break StreamPoll::Cancelled;
        }
        for _ in 0..MAX_LIVE_CHUNKS_PER_TICK {
            let Ok((stream, text)) = rx.try_recv() else {
                break;
            };
            on_chunk(stream, &text);
        }
        if cancel.is_cancelled() {
            break StreamPoll::Cancelled;
        }
        match poll_child(&mut child, &process_control) {
            Ok(ChildPoll::Exited(status)) => break StreamPoll::Exited(status),
            Ok(ChildPoll::Running) => {
                if started.elapsed() >= timeout {
                    break StreamPoll::TimedOut;
                }
                thread::sleep(poll);
            }
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                let mut terminated = true;
                let _ = finish_reader_draining(
                    join_reader_bounded_draining(stdout_handle, Duration::from_secs(2), || {
                        drain_streaming_chunks(&rx, on_chunk)
                    }),
                    Duration::from_secs(2),
                    &process_control,
                    &mut child,
                    &mut terminated,
                    || drain_streaming_chunks(&rx, on_chunk),
                );
                let _ = finish_reader_draining(
                    join_reader_bounded_draining(stderr_handle, Duration::from_secs(2), || {
                        drain_streaming_chunks(&rx, on_chunk)
                    }),
                    Duration::from_secs(2),
                    &process_control,
                    &mut child,
                    &mut terminated,
                    || drain_streaming_chunks(&rx, on_chunk),
                );
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
        StreamPoll::Exited(observed_status) => {
            let mut terminated = false;
            let stdout_incomplete = finish_reader_draining(
                join_reader_bounded_draining(stdout_handle, Duration::from_millis(250), || {
                    drain_streaming_chunks(&rx, on_chunk)
                }),
                Duration::from_secs(2),
                &process_control,
                &mut child,
                &mut terminated,
                || drain_streaming_chunks(&rx, on_chunk),
            );
            let stderr_incomplete = finish_reader_draining(
                join_reader_bounded_draining(stderr_handle, Duration::from_millis(250), || {
                    drain_streaming_chunks(&rx, on_chunk)
                }),
                Duration::from_secs(2),
                &process_control,
                &mut child,
                &mut terminated,
                || drain_streaming_chunks(&rx, on_chunk),
            );
            while let Ok((stream, text)) = rx.try_recv() {
                on_chunk(stream, &text);
            }
            let st = stdout_trunc.load(Ordering::SeqCst);
            let se = stderr_trunc.load(Ordering::SeqCst);
            let stdout = string_from_acc(&stdout_acc, st, stdout_incomplete);
            let stderr = string_from_acc(&stderr_acc, se, stderr_incomplete);
            let status = match observed_status {
                Some(status) => status,
                None => match child.wait() {
                    Ok(status) => status,
                    Err(e) => {
                        return AgentRunResult {
                            agent: spec.agent,
                            status: RunStatus::Failed,
                            exit_code: None,
                            duration_ms: started.elapsed().as_millis() as u64,
                            stdout,
                            stderr,
                            command,
                            error: Some(format!("wait failed: {e}")),
                            truncated: true,
                            native_session_id: None,
                        };
                    }
                },
            };
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
                truncated: st || se || stdout_incomplete || stderr_incomplete,
                native_session_id: None,
            }
        }
        StreamPoll::TimedOut => {
            kill_process_tree(&process_control, &mut child);
            let _ = child.wait();
            let mut terminated = true;
            let stdout_incomplete = finish_reader_draining(
                join_reader_bounded_draining(stdout_handle, Duration::from_secs(2), || {
                    drain_streaming_chunks(&rx, on_chunk)
                }),
                Duration::from_secs(2),
                &process_control,
                &mut child,
                &mut terminated,
                || drain_streaming_chunks(&rx, on_chunk),
            );
            let stderr_incomplete = finish_reader_draining(
                join_reader_bounded_draining(stderr_handle, Duration::from_secs(2), || {
                    drain_streaming_chunks(&rx, on_chunk)
                }),
                Duration::from_secs(2),
                &process_control,
                &mut child,
                &mut terminated,
                || drain_streaming_chunks(&rx, on_chunk),
            );
            while let Ok((stream, text)) = rx.try_recv() {
                on_chunk(stream, &text);
            }
            let st = stdout_trunc.load(Ordering::SeqCst);
            let se = stderr_trunc.load(Ordering::SeqCst);
            let stdout = string_from_acc(&stdout_acc, st, stdout_incomplete);
            let stderr = string_from_acc(&stderr_acc, se, stderr_incomplete);
            AgentRunResult {
                agent: spec.agent,
                status: RunStatus::Timeout,
                exit_code: None,
                duration_ms: started.elapsed().as_millis() as u64,
                stdout,
                stderr,
                command,
                error: Some(format!("timed out after {}s", timeout.as_secs())),
                truncated: st || se || stdout_incomplete || stderr_incomplete,
                native_session_id: None,
            }
        }
        StreamPoll::Cancelled => {
            kill_process_tree(&process_control, &mut child);
            let _ = child.wait();
            let mut terminated = true;
            let stdout_incomplete = finish_reader_draining(
                join_reader_bounded_draining(stdout_handle, Duration::from_secs(2), || {
                    drain_streaming_chunks(&rx, on_chunk)
                }),
                Duration::from_secs(2),
                &process_control,
                &mut child,
                &mut terminated,
                || drain_streaming_chunks(&rx, on_chunk),
            );
            let stderr_incomplete = finish_reader_draining(
                join_reader_bounded_draining(stderr_handle, Duration::from_secs(2), || {
                    drain_streaming_chunks(&rx, on_chunk)
                }),
                Duration::from_secs(2),
                &process_control,
                &mut child,
                &mut terminated,
                || drain_streaming_chunks(&rx, on_chunk),
            );
            while let Ok((stream, text)) = rx.try_recv() {
                on_chunk(stream, &text);
            }
            let st = stdout_trunc.load(Ordering::SeqCst);
            let se = stderr_trunc.load(Ordering::SeqCst);
            let stdout = string_from_acc(&stdout_acc, st, stdout_incomplete);
            let stderr = string_from_acc(&stderr_acc, se, stderr_incomplete);
            AgentRunResult {
                agent: spec.agent,
                status: RunStatus::Cancelled,
                exit_code: None,
                duration_ms: started.elapsed().as_millis() as u64,
                stdout,
                stderr,
                command,
                error: Some("cancelled".into()),
                truncated: st || se || stdout_incomplete || stderr_incomplete,
                native_session_id: None,
            }
        }
    }
}

enum StreamPoll {
    Exited(Option<std::process::ExitStatus>),
    TimedOut,
    Cancelled,
}

/// Converts bounded raw output blocks without splitting a valid UTF-8 code
/// point across live callback messages. Invalid bytes are still rendered
/// lossily, while an incomplete suffix is carried into the next block.
pub(crate) struct Utf8ChunkDecoder {
    pending: Vec<u8>,
}

impl Utf8ChunkDecoder {
    pub(crate) fn new() -> Self {
        Self {
            pending: Vec::new(),
        }
    }

    pub(crate) fn push(&mut self, bytes: &[u8]) -> String {
        if bytes.is_empty() && self.pending.is_empty() {
            return String::new();
        }
        let mut combined = Vec::with_capacity(self.pending.len() + bytes.len());
        combined.extend_from_slice(&self.pending);
        combined.extend_from_slice(bytes);
        self.pending.clear();

        let complete = match std::str::from_utf8(&combined) {
            Ok(_) => combined.len(),
            Err(error) if error.error_len().is_none() => error.valid_up_to(),
            Err(_) => combined.len(),
        };
        self.pending.extend_from_slice(&combined[complete..]);
        String::from_utf8_lossy(&combined[..complete]).into_owned()
    }

    pub(crate) fn finish(&mut self) -> String {
        let pending = std::mem::take(&mut self.pending);
        String::from_utf8_lossy(&pending).into_owned()
    }
}

fn drain_streaming_chunks(
    rx: &std::sync::mpsc::Receiver<(OutputStream, String)>,
    on_chunk: &(dyn Fn(OutputStream, &str) + Send + Sync),
) {
    while let Ok((stream, text)) = rx.try_recv() {
        on_chunk(stream, &text);
    }
}

fn read_lines_capped<R: Read>(
    stream: Option<R>,
    max: usize,
    which: OutputStream,
    tx: &std::sync::mpsc::SyncSender<(OutputStream, String)>,
    acc: &Mutex<Vec<u8>>,
    trunc: &AtomicBool,
) {
    let Some(mut r) = stream else {
        return;
    };
    const READ_CHUNK_BYTES: usize = 8192;
    const EMIT_CHUNK_BYTES: usize = 8192;
    let mut chunk = [0u8; READ_CHUNK_BYTES];
    let mut output = Vec::with_capacity(EMIT_CHUNK_BYTES);
    let mut decoder = Utf8ChunkDecoder::new();
    loop {
        match r.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                let accepted = {
                    let mut guard = acc.lock().unwrap_or_else(|e| e.into_inner());
                    let room = max.saturating_sub(guard.len());
                    let take = n.min(room);
                    guard.extend_from_slice(&chunk[..take]);
                    take
                };
                if accepted < n {
                    mark_truncated(trunc);
                    // Keep draining the pipe, but do not enqueue anything after
                    // the cap has been reached.
                }
                for byte in &chunk[..accepted] {
                    output.push(*byte);
                    if output.len() == EMIT_CHUNK_BYTES {
                        let text = decoder.push(&output);
                        if !text.is_empty() {
                            // Process streaming is also consumed by the
                            // structured output parser, so every accepted
                            // block must reach the callback. The bounded queue
                            // is made safe by draining it while readers are
                            // joined below.
                            let _ = tx.send((which, text));
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
            let _ = tx.send((which, text));
        }
    }
    let text = decoder.finish();
    if !text.is_empty() {
        let _ = tx.send((which, text));
    }
}

pub(crate) fn mark_truncated(trunc: &AtomicBool) {
    let _ = trunc.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst);
}

fn string_from_acc(acc: &Mutex<Vec<u8>>, truncated: bool, incomplete: bool) -> String {
    let bytes = acc.lock().map(|g| g.clone()).unwrap_or_default();
    let mut s = String::from_utf8_lossy(&bytes).into_owned();
    if incomplete {
        s.push_str("\n…[incomplete]");
    } else if truncated {
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
    let process_control = match configure_process_group(&mut cmd) {
        Ok(control) => control,
        Err(e) => {
            return AgentRunResult {
                agent: spec.agent,
                status: RunStatus::Failed,
                exit_code: None,
                duration_ms: started.elapsed().as_millis() as u64,
                stdout: String::new(),
                stderr: String::new(),
                command,
                error: Some(format!("process-group setup failed: {e}")),
                truncated: false,
                native_session_id: None,
            };
        }
    };

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
    if let Err(e) = process_control.attach(&child) {
        let _ = child.kill();
        process_control.terminate(&mut child);
        let _ = child.wait();
        return AgentRunResult {
            agent: spec.agent,
            status: RunStatus::Failed,
            exit_code: None,
            duration_ms: started.elapsed().as_millis() as u64,
            stdout: String::new(),
            stderr: String::new(),
            command,
            error: Some(format!("process-group attach failed: {e}")),
            truncated: false,
            native_session_id: None,
        };
    }

    match wait_with_timeout(&mut child, timeout, max_output_bytes, &process_control) {
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
fn wait_with_timeout(
    child: &mut Child,
    timeout: Duration,
    max_output_bytes: usize,
    process_control: &ProcessControl,
) -> WaitOutcome {
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    let max = max_output_bytes;
    let stdout_acc = Arc::new(Mutex::new(Vec::new()));
    let stderr_acc = Arc::new(Mutex::new(Vec::new()));
    let stdout_trunc = Arc::new(AtomicBool::new(false));
    let stderr_trunc = Arc::new(AtomicBool::new(false));
    let stdout_acc_r = Arc::clone(&stdout_acc);
    let stderr_acc_r = Arc::clone(&stderr_acc);
    let stdout_trunc_r = Arc::clone(&stdout_trunc);
    let stderr_trunc_r = Arc::clone(&stderr_trunc);
    let stdout_handle = thread::spawn(move || {
        read_capped_into(stdout, max, &stdout_acc_r, &stdout_trunc_r);
    });
    let stderr_handle = thread::spawn(move || {
        read_capped_into(stderr, max, &stderr_acc_r, &stderr_trunc_r);
    });

    let started = Instant::now();
    let poll = Duration::from_millis(50);
    let outcome = loop {
        match poll_child(child, process_control) {
            Ok(ChildPoll::Exited(status)) => break WaitPoll::Exited(status),
            Ok(ChildPoll::Running) => {
                if started.elapsed() >= timeout {
                    break WaitPoll::TimedOut;
                }
                thread::sleep(poll);
            }
            Err(e) => {
                // Do not signal a Unix process group when observation failed:
                // its leader state is unknown, so only target the owned Child.
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
    };

    match outcome {
        WaitPoll::Exited(observed_status) => {
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
            let status = match observed_status {
                Some(status) => status,
                None => match child.wait() {
                    Ok(status) => status,
                    Err(error) => return WaitOutcome::IoError(error),
                },
            };
            WaitOutcome::Finished {
                status,
                stdout: string_from_acc(&stdout_acc, stdout_trunc.load(Ordering::SeqCst), stdout_incomplete),
                stderr: string_from_acc(&stderr_acc, stderr_trunc.load(Ordering::SeqCst), stderr_incomplete),
                truncated: stdout_trunc.load(Ordering::SeqCst)
                    || stderr_trunc.load(Ordering::SeqCst)
                    || stdout_incomplete
                    || stderr_incomplete,
            }
        }
        WaitPoll::TimedOut => {
            // Kill first so readers see EOF, then join with a short deadline.
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
            WaitOutcome::Timeout {
                stdout: string_from_acc(&stdout_acc, stdout_trunc.load(Ordering::SeqCst), stdout_incomplete),
                stderr: string_from_acc(&stderr_acc, stderr_trunc.load(Ordering::SeqCst), stderr_incomplete),
                truncated: stdout_trunc.load(Ordering::SeqCst)
                    || stderr_trunc.load(Ordering::SeqCst)
                    || stdout_incomplete
                    || stderr_incomplete,
            }
        }
    }
}

enum WaitPoll {
    Exited(Option<std::process::ExitStatus>),
    TimedOut,
}

/// Read at most `max` bytes from an optional pipe; mark truncated if more data remains.
fn read_capped<R: Read>(stream: Option<R>, max: usize) -> (Vec<u8>, bool) {
    let acc = Mutex::new(Vec::new());
    let trunc = AtomicBool::new(false);
    read_capped_into(stream, max, &acc, &trunc);
    let buf = acc.lock().map(|g| g.clone()).unwrap_or_default();
    (buf, trunc.load(Ordering::SeqCst))
}

fn read_capped_into<R: Read>(
    stream: Option<R>,
    max: usize,
    acc: &Mutex<Vec<u8>>,
    trunc: &AtomicBool,
) {
    let Some(mut r) = stream else {
        return;
    };
    let mut chunk = [0u8; 8192];
    loop {
        match r.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                let mut guard = acc.lock().unwrap_or_else(|e| e.into_inner());
                let room = max.saturating_sub(guard.len());
                let take = n.min(room);
                guard.extend_from_slice(&chunk[..take]);
                drop(guard);
                if take < n {
                    mark_truncated(trunc);
                }
            }
            Err(_) => break,
        }
    }
}

pub(crate) enum ReaderJoin<T> {
    Complete(thread::Result<T>),
    Pending(thread::JoinHandle<T>),
}

pub(crate) fn join_reader_bounded<T: Send + 'static>(
    handle: thread::JoinHandle<T>,
    timeout: Duration,
) -> ReaderJoin<T> {
    let deadline = Instant::now() + timeout;
    while !handle.is_finished() {
        if Instant::now() >= deadline {
            return ReaderJoin::Pending(handle);
        }
        thread::sleep(Duration::from_millis(5));
    }
    ReaderJoin::Complete(handle.join())
}

/// Join a process-stream reader while continuing to drain its bounded live
/// output queue. Process streaming callbacks are lossless (the callback also
/// feeds structured-output parsing), so a reader may be waiting on `send` even
/// after the child has exited and its pipe is held by a descendant.
pub(crate) fn join_reader_bounded_draining<T: Send + 'static, F: FnMut()>(
    handle: thread::JoinHandle<T>,
    timeout: Duration,
    mut drain: F,
) -> ReaderJoin<T> {
    let deadline = Instant::now() + timeout;
    while !handle.is_finished() {
        drain();
        if Instant::now() >= deadline {
            return ReaderJoin::Pending(handle);
        }
        thread::sleep(Duration::from_millis(5));
    }
    drain();
    ReaderJoin::Complete(handle.join())
}

pub(crate) fn finish_reader<T: Send + 'static>(
    first: ReaderJoin<T>,
    second_timeout: Duration,
    process_control: &ProcessControl,
    child: &mut Child,
    terminated: &mut bool,
) -> bool {
    let pending = match first {
        ReaderJoin::Complete(Ok(_)) => return false,
        ReaderJoin::Complete(Err(_)) => {
            if !*terminated {
                kill_process_tree(process_control, child);
                *terminated = true;
            }
            return true;
        }
        ReaderJoin::Pending(handle) => handle,
    };
    if !*terminated {
        kill_process_tree(process_control, child);
        *terminated = true;
    }
    match join_reader_bounded(pending, second_timeout) {
        ReaderJoin::Complete(Ok(_)) => false,
        ReaderJoin::Complete(Err(_)) => true,
        ReaderJoin::Pending(handle) => {
            drop(handle);
            true
        }
    }
}

pub(crate) fn finish_reader_draining<T: Send + 'static, F: FnMut()>(
    first: ReaderJoin<T>,
    second_timeout: Duration,
    process_control: &ProcessControl,
    child: &mut Child,
    terminated: &mut bool,
    mut drain: F,
) -> bool {
    let pending = match first {
        ReaderJoin::Complete(Ok(_)) => return false,
        ReaderJoin::Complete(Err(_)) => {
            if !*terminated {
                kill_process_tree(process_control, child);
                *terminated = true;
            }
            return true;
        }
        ReaderJoin::Pending(handle) => handle,
    };
    if !*terminated {
        kill_process_tree(process_control, child);
        *terminated = true;
    }
    match join_reader_bounded_draining(pending, second_timeout, &mut drain) {
        ReaderJoin::Complete(Ok(_)) => false,
        ReaderJoin::Complete(Err(_)) => true,
        ReaderJoin::Pending(handle) => {
            drop(handle);
            true
        }
    }
}

pub(crate) fn kill_process_tree(process_control: &ProcessControl, child: &mut Child) {
    process_control.terminate(child);
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
mod tests;
