//! Small JSON-lines transport for the Codex app-server process.
//!
//! This module intentionally owns only process and protocol plumbing. It does
//! not start a turn, read Codex configuration, or otherwise interpret the
//! application protocol.

use std::collections::VecDeque;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use serde_json::{Map, Value, json};
use thiserror::Error;

use crate::utils::process::{
    ChildPoll, ProcessControl, ReaderJoin, apply_no_window, configure_process_group,
    join_reader_bounded, kill_process_tree, poll_child, reap_child_lossy,
};

const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(8);
const MAX_STDOUT_LINE_BYTES: usize = 1024 * 1024;
const MAX_STDERR_BYTES: usize = 64 * 1024;
const WIRE_CHANNEL_CAPACITY: usize = 128;
const EVENT_QUEUE_CAPACITY: usize = 256;
const CHILD_POLL_INTERVAL: Duration = Duration::from_millis(20);

/// A server-to-client message that does not complete a client request.
#[derive(Debug, Clone, PartialEq)]
pub enum CodexEvent {
    Notification {
        method: String,
        params: Value,
    },
    Request {
        id: Value,
        method: String,
        params: Value,
    },
    /// A response which arrived without matching the request currently being
    /// awaited. Keeping it visible makes request ids and protocol failures
    /// diagnosable without silently discarding wire messages.
    Response {
        id: Value,
        result: Option<Value>,
        error: Option<Value>,
    },
    Exited,
}

#[derive(Debug, Error)]
pub enum CodexTransportError {
    #[error("codex app-server I/O failed: {0}")]
    Io(#[from] io::Error),
    #[error("codex app-server request timed out")]
    Timeout,
    #[error("codex app-server exited")]
    Exited,
    #[error("codex app-server protocol error: {0}")]
    Protocol(String),
    #[error("codex app-server returned an error: {error}")]
    Server { error: Value },
    #[error("codex app-server event queue is full")]
    EventQueueFull,
}

type TransportResult<T> = Result<T, CodexTransportError>;

#[derive(Debug)]
enum WireEvent {
    Message(Value),
    Eof,
    Error(String),
}

#[derive(Debug)]
enum WireMessage {
    Eof,
    Notification {
        method: String,
        params: Value,
    },
    Request {
        id: Value,
        method: String,
        params: Value,
    },
    Response {
        id: Value,
        result: Option<Value>,
        error: Option<Value>,
    },
}

/// A synchronous, single-request-at-a-time Codex app-server connection.
pub struct CodexTransport {
    stdin: Option<ChildStdin>,
    child: Option<Child>,
    process_control: ProcessControl,
    wire_rx: Receiver<WireEvent>,
    wire_thread: Option<JoinHandle<()>>,
    stderr_thread: Option<JoinHandle<()>>,
    stderr: Arc<Mutex<Vec<u8>>>,
    stop: Arc<AtomicBool>,
    events: VecDeque<CodexEvent>,
    next_id: u64,
    exited: bool,
    shutdown: bool,
}

impl CodexTransport {
    /// Spawn `program app-server` in `cwd` and complete the app-server
    /// initialize handshake.
    pub fn spawn(program: &Path, cwd: &Path) -> Result<Self, CodexTransportError> {
        let mut command = std::process::Command::new(program);
        command
            .arg("app-server")
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        apply_no_window(&mut command);
        let process_control = configure_process_group(&mut command)?;
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(error) => return Err(error.into()),
        };
        if let Err(error) = process_control.attach(&child) {
            kill_process_tree(&process_control, &mut child);
            reap_child_lossy(&mut child, &process_control);
            return Err(error.into());
        }

        let stdin = child.stdin.take().ok_or_else(|| {
            io::Error::new(io::ErrorKind::BrokenPipe, "app-server stdin was not piped")
        });
        let stdout = child.stdout.take().ok_or_else(|| {
            io::Error::new(io::ErrorKind::BrokenPipe, "app-server stdout was not piped")
        });
        let stderr = child.stderr.take().ok_or_else(|| {
            io::Error::new(io::ErrorKind::BrokenPipe, "app-server stderr was not piped")
        });
        let (stdin, stdout, stderr) = match (stdin, stdout, stderr) {
            (Ok(stdin), Ok(stdout), Ok(stderr)) => (stdin, stdout, stderr),
            (Err(error), _, _) | (_, Err(error), _) | (_, _, Err(error)) => {
                kill_process_tree(&process_control, &mut child);
                reap_child_lossy(&mut child, &process_control);
                return Err(error.into());
            }
        };

        let (wire_tx, wire_rx) = mpsc::sync_channel(WIRE_CHANNEL_CAPACITY);
        let stop = Arc::new(AtomicBool::new(false));
        let stderr_capture = Arc::new(Mutex::new(Vec::with_capacity(MAX_STDERR_BYTES)));
        let wire_stop = Arc::clone(&stop);
        let wire_thread = match thread::Builder::new()
            .name("agenthub-codex-stdout".into())
            .spawn(move || read_stdout(stdout, wire_tx, wire_stop))
        {
            Ok(handle) => handle,
            Err(error) => {
                kill_process_tree(&process_control, &mut child);
                reap_child_lossy(&mut child, &process_control);
                return Err(error.into());
            }
        };
        let stderr_stop = Arc::clone(&stop);
        let stderr_target = Arc::clone(&stderr_capture);
        let stderr_thread = match thread::Builder::new()
            .name("agenthub-codex-stderr".into())
            .spawn(move || read_stderr(stderr, stderr_target, stderr_stop))
        {
            Ok(handle) => handle,
            Err(error) => {
                stop.store(true, Ordering::SeqCst);
                kill_process_tree(&process_control, &mut child);
                reap_child_lossy(&mut child, &process_control);
                join_reader(wire_thread);
                return Err(error.into());
            }
        };

        let mut transport = Self {
            stdin: Some(stdin),
            child: Some(child),
            process_control,
            wire_rx,
            wire_thread: Some(wire_thread),
            stderr_thread: Some(stderr_thread),
            stderr: stderr_capture,
            stop,
            events: VecDeque::new(),
            next_id: 1,
            exited: false,
            shutdown: false,
        };

        let initialize_params = json!({
            "clientInfo": {
                "name": "agenthub-chat",
                "version": env!("CARGO_PKG_VERSION"),
            }
        });
        transport.request_inner("initialize", initialize_params, HANDSHAKE_TIMEOUT)?;
        transport.send_notification("initialized", None)?;
        Ok(transport)
    }

    /// Send a request and wait for its matching response. Notifications and
    /// server requests received while waiting remain available via
    /// [`Self::recv_timeout`].
    pub fn request(
        &mut self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, CodexTransportError> {
        self.request_inner(method, params, timeout)
    }

    /// Receive one queued server event, waiting up to `timeout` for a new
    /// line. `Ok(None)` means that no event arrived before the deadline.
    pub fn recv_timeout(
        &mut self,
        timeout: Duration,
    ) -> Result<Option<CodexEvent>, CodexTransportError> {
        if let Some(event) = self.events.pop_front() {
            return Ok(Some(event));
        }

        let deadline = Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Ok(None);
            }
            let wait = remaining.min(CHILD_POLL_INTERVAL);
            match self.wire_rx.recv_timeout(wait) {
                Ok(event) => match self.consume_wire_event(event)? {
                    Some(event) => return Ok(Some(event)),
                    None => continue,
                },
                Err(RecvTimeoutError::Timeout) => {
                    if self.poll_exited()? {
                        return Ok(Some(CodexEvent::Exited));
                    }
                }
                Err(RecvTimeoutError::Disconnected) => {
                    self.exited = true;
                    return Ok(Some(CodexEvent::Exited));
                }
            }
        }
    }

    /// Respond to a server request. `Ok(value)` writes a JSON-RPC result;
    /// `Err(error)` writes the JSON-RPC error object supplied by the caller.
    pub fn respond(
        &mut self,
        id: Value,
        response: Result<Value, Value>,
    ) -> Result<(), CodexTransportError> {
        let mut message = Map::new();
        message.insert("id".into(), id);
        match response {
            Ok(result) => {
                message.insert("result".into(), result);
            }
            Err(error) => {
                message.insert("error".into(), error);
            }
        }
        self.send_value(Value::Object(message))
    }

    /// Return the bounded stderr captured so far. The transport never logs it
    /// or includes it in protocol errors.
    pub fn stderr(&self) -> String {
        let bytes = self.stderr.lock().expect("stderr capture lock poisoned");
        String::from_utf8_lossy(&bytes).into_owned()
    }

    /// Terminate the app-server process tree and reap the process.
    pub fn shutdown(&mut self) {
        if self.shutdown {
            return;
        }
        self.shutdown = true;
        self.stop.store(true, Ordering::SeqCst);
        self.stdin.take();
        if let Some(mut child) = self.child.take() {
            kill_process_tree(&self.process_control, &mut child);
            reap_child_lossy(&mut child, &self.process_control);
        }
        if let Some(thread) = self.wire_thread.take() {
            join_reader(thread);
        }
        if let Some(thread) = self.stderr_thread.take() {
            join_reader(thread);
        }
    }

    fn request_inner(
        &mut self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> TransportResult<Value> {
        if self.exited || self.shutdown {
            return Err(CodexTransportError::Exited);
        }
        let id = Value::from(self.next_id);
        self.next_id = self
            .next_id
            .checked_add(1)
            .ok_or_else(|| CodexTransportError::Protocol("request id exhausted".into()))?;
        self.send_value(json!({ "id": id, "method": method, "params": params }))?;

        let deadline = Instant::now() + timeout;
        let mut deferred = VecDeque::new();
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                self.restore_deferred(deferred)?;
                return Err(CodexTransportError::Timeout);
            }
            match self.next_wire_message(remaining)? {
                Some(WireMessage::Response {
                    id: response_id,
                    result,
                    error,
                }) if response_id == id => {
                    self.restore_deferred(deferred)?;
                    if let Some(error) = error {
                        return Err(CodexTransportError::Server { error });
                    }
                    return result.ok_or_else(|| {
                        CodexTransportError::Protocol("response omitted result".into())
                    });
                }
                Some(message @ WireMessage::Response { .. }) => deferred.push_back(message),
                Some(WireMessage::Notification { method, params }) => {
                    self.push_event(CodexEvent::Notification { method, params })?;
                }
                Some(WireMessage::Request { id, method, params }) => {
                    self.push_event(CodexEvent::Request { id, method, params })?;
                }
                Some(WireMessage::Eof) => {
                    self.restore_deferred(deferred)?;
                    self.exited = true;
                    return Err(CodexTransportError::Exited);
                }
                None => {
                    self.restore_deferred(deferred)?;
                    return Err(CodexTransportError::Timeout);
                }
            }
        }
    }

    fn send_notification(&mut self, method: &str, params: Option<Value>) -> TransportResult<()> {
        let mut message = Map::new();
        message.insert("method".into(), Value::String(method.into()));
        if let Some(params) = params {
            message.insert("params".into(), params);
        }
        self.send_value(Value::Object(message))
    }

    fn send_value(&mut self, value: Value) -> TransportResult<()> {
        if self.shutdown || self.exited {
            return Err(CodexTransportError::Exited);
        }
        let stdin = self.stdin.as_mut().ok_or(CodexTransportError::Exited)?;
        serde_json::to_writer(&mut *stdin, &value)
            .map_err(|error| CodexTransportError::Protocol(error.to_string()))?;
        stdin.write_all(b"\n")?;
        stdin.flush()?;
        Ok(())
    }

    fn next_wire_message(&mut self, timeout: Duration) -> TransportResult<Option<WireMessage>> {
        let deadline = Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Ok(None);
            }
            let wait = remaining.min(CHILD_POLL_INTERVAL);
            match self.wire_rx.recv_timeout(wait) {
                Ok(event) => match event {
                    WireEvent::Message(value) => match classify_message(value) {
                        Ok(message) => return Ok(Some(message)),
                        Err(error) => {
                            self.shutdown();
                            return Err(error);
                        }
                    },
                    WireEvent::Eof => return Ok(Some(WireMessage::Eof)),
                    WireEvent::Error(error) => {
                        self.shutdown();
                        return Err(CodexTransportError::Protocol(error));
                    }
                },
                Err(RecvTimeoutError::Timeout) => {
                    if self.poll_exited()? {
                        return Ok(Some(WireMessage::Eof));
                    }
                }
                Err(RecvTimeoutError::Disconnected) => return Ok(Some(WireMessage::Eof)),
            }
        }
    }

    fn consume_wire_event(&mut self, event: WireEvent) -> TransportResult<Option<CodexEvent>> {
        match event {
            WireEvent::Message(value) => match classify_message(value) {
                Err(error) => {
                    self.shutdown();
                    return Err(error);
                }
                Ok(message) => match message {
                    WireMessage::Notification { method, params } => {
                        Ok(Some(CodexEvent::Notification { method, params }))
                    }
                    WireMessage::Request { id, method, params } => {
                        Ok(Some(CodexEvent::Request { id, method, params }))
                    }
                    WireMessage::Response { id, result, error } => {
                        Ok(Some(CodexEvent::Response { id, result, error }))
                    }
                    WireMessage::Eof => unreachable!(),
                },
            },
            WireEvent::Eof => {
                self.exited = true;
                Ok(Some(CodexEvent::Exited))
            }
            WireEvent::Error(error) => {
                self.shutdown();
                Err(CodexTransportError::Protocol(error))
            }
        }
    }

    fn poll_exited(&mut self) -> TransportResult<bool> {
        if self.exited {
            return Ok(true);
        }
        let Some(child) = self.child.as_mut() else {
            self.exited = true;
            return Ok(true);
        };
        match poll_child(child, &self.process_control)? {
            ChildPoll::Running => Ok(false),
            ChildPoll::Exited(_) => {
                self.exited = true;
                Ok(true)
            }
        }
    }

    fn push_event(&mut self, event: CodexEvent) -> TransportResult<()> {
        if self.events.len() >= EVENT_QUEUE_CAPACITY {
            return Err(CodexTransportError::EventQueueFull);
        }
        self.events.push_back(event);
        Ok(())
    }

    fn restore_deferred(&mut self, deferred: VecDeque<WireMessage>) -> TransportResult<()> {
        for message in deferred.into_iter().rev() {
            let event = match message {
                WireMessage::Response { id, result, error } => {
                    CodexEvent::Response { id, result, error }
                }
                WireMessage::Notification { method, params } => {
                    CodexEvent::Notification { method, params }
                }
                WireMessage::Request { id, method, params } => {
                    CodexEvent::Request { id, method, params }
                }
                WireMessage::Eof => CodexEvent::Exited,
            };
            if self.events.len() >= EVENT_QUEUE_CAPACITY {
                return Err(CodexTransportError::EventQueueFull);
            }
            self.events.push_front(event);
        }
        Ok(())
    }
}

impl Drop for CodexTransport {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn classify_message(value: Value) -> TransportResult<WireMessage> {
    let object = value.as_object().ok_or_else(|| {
        CodexTransportError::Protocol("JSON-RPC message must be an object".into())
    })?;
    let has_id = object.contains_key("id");
    let has_method = object.get("method").and_then(Value::as_str).is_some();
    let has_result = object.contains_key("result");
    let has_error = object.contains_key("error");
    if has_id && has_method {
        return Ok(WireMessage::Request {
            id: object.get("id").cloned().expect("id was checked above"),
            method: object
                .get("method")
                .and_then(Value::as_str)
                .expect("method was checked above")
                .into(),
            params: object.get("params").cloned().unwrap_or(Value::Null),
        });
    }
    if has_id && (has_result || has_error) {
        if has_result && has_error {
            return Err(CodexTransportError::Protocol(
                "response cannot contain both result and error".into(),
            ));
        }
        return Ok(WireMessage::Response {
            id: object.get("id").cloned().expect("id was checked above"),
            result: object.get("result").cloned(),
            error: object.get("error").cloned(),
        });
    }
    if has_method && !has_id {
        return Ok(WireMessage::Notification {
            method: object
                .get("method")
                .and_then(Value::as_str)
                .expect("method was checked above")
                .into(),
            params: object.get("params").cloned().unwrap_or(Value::Null),
        });
    }
    Err(CodexTransportError::Protocol(
        "invalid JSON-RPC message shape".into(),
    ))
}

fn read_stdout(stdout: impl Read, tx: SyncSender<WireEvent>, stop: Arc<AtomicBool>) {
    let mut reader = BufReader::new(stdout);
    let mut line = Vec::new();
    loop {
        if stop.load(Ordering::SeqCst) {
            return;
        }
        line.clear();
        match reader.read_until(b'\n', &mut line) {
            Ok(0) => {
                send_wire_event(&tx, &stop, WireEvent::Eof);
                return;
            }
            Ok(_) => {
                if line.len() > MAX_STDOUT_LINE_BYTES + 1 {
                    send_wire_event(
                        &tx,
                        &stop,
                        WireEvent::Error(format!(
                            "stdout JSON line exceeds {MAX_STDOUT_LINE_BYTES} bytes"
                        )),
                    );
                    return;
                }
                if line.last() != Some(&b'\n') {
                    send_wire_event(
                        &tx,
                        &stop,
                        WireEvent::Error("incomplete stdout JSON line".into()),
                    );
                    return;
                }
                if line.last() == Some(&b'\n') {
                    line.pop();
                }
                if line.last() == Some(&b'\r') {
                    line.pop();
                }
                if line.iter().all(u8::is_ascii_whitespace) {
                    continue;
                }
                match serde_json::from_slice::<Value>(&line) {
                    Ok(value) => send_wire_event(&tx, &stop, WireEvent::Message(value)),
                    Err(error) => {
                        send_wire_event(
                            &tx,
                            &stop,
                            WireEvent::Error(format!("invalid stdout JSON: {error}")),
                        );
                        return;
                    }
                }
            }
            Err(error) => {
                send_wire_event(
                    &tx,
                    &stop,
                    WireEvent::Error(format!("stdout read failed: {error}")),
                );
                return;
            }
        }
    }
}

fn read_stderr(mut stderr: impl Read, capture: Arc<Mutex<Vec<u8>>>, stop: Arc<AtomicBool>) {
    let mut chunk = [0_u8; 8192];
    loop {
        match stderr.read(&mut chunk) {
            Ok(0) => return,
            Ok(count) => {
                if let Ok(mut bytes) = capture.lock() {
                    let remaining = MAX_STDERR_BYTES.saturating_sub(bytes.len());
                    bytes.extend_from_slice(&chunk[..count.min(remaining)]);
                }
                if stop.load(Ordering::SeqCst) {
                    // Continue draining until the pipe closes so the child
                    // cannot be held up by stderr during shutdown.
                    continue;
                }
            }
            Err(_) => return,
        }
    }
}

fn send_wire_event(tx: &SyncSender<WireEvent>, stop: &AtomicBool, mut event: WireEvent) {
    loop {
        match tx.try_send(event) {
            Ok(()) | Err(TrySendError::Disconnected(_)) => return,
            Err(TrySendError::Full(returned)) => {
                event = returned;
                if stop.load(Ordering::SeqCst) {
                    return;
                }
                thread::sleep(Duration::from_millis(5));
            }
        }
    }
}

fn join_reader(thread: JoinHandle<()>) {
    match join_reader_bounded(thread, Duration::from_millis(250)) {
        ReaderJoin::Complete(_) => {}
        ReaderJoin::Pending(thread) => {
            let _ = thread::Builder::new()
                .name("agenthub-codex-reap-reader".into())
                .spawn(move || {
                    let _ = thread.join();
                });
        }
    }
}

#[cfg(test)]
mod tests;
