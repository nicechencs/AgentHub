//! ACP `terminal/*` host for one command at a time.
//!
//! Piped stdio (not a conversation TTY). Output is tailed for the Chat card.

use std::collections::HashMap;
use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use serde_json::Value;
use uuid::Uuid;

use crate::error::{AppError, Result};
use crate::utils::process::{
    configure_process_group, poll_child, reap_child, ChildPoll, ProcessControl,
};

use super::ops::AcpTerminalCreate;
use super::types::RuntimeHostTerminal;

pub(crate) const DEFAULT_OUTPUT_LIMIT: usize = 64 * 1024;

pub(crate) struct HostedTerminals {
    conversation_id: String,
    items: HashMap<String, HostedTerminal>,
    views: Arc<Mutex<HashMap<String, Vec<RuntimeHostTerminal>>>>,
}

struct HostedTerminal {
    command: String,
    child: Child,
    control: ProcessControl,
    output: Arc<Mutex<String>>,
    truncated: Arc<AtomicBool>,
    exit_code: Option<i32>,
    wait_id: Option<Value>,
}

impl HostedTerminals {
    pub(crate) fn new(
        conversation_id: String,
        views: Arc<Mutex<HashMap<String, Vec<RuntimeHostTerminal>>>>,
    ) -> Self {
        Self {
            conversation_id,
            items: HashMap::new(),
            views,
        }
    }

    pub(crate) fn create(&mut self, spec: AcpTerminalCreate) -> Result<String> {
        let mut cmd = Command::new(&spec.command);
        cmd.args(&spec.args)
            .current_dir(&spec.cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (key, value) in &spec.env {
            cmd.env(key, value);
        }
        crate::utils::process::apply_no_window(&mut cmd);
        let control = configure_process_group(&mut cmd).map_err(|error| {
            AppError::message("chat.runtime.terminal", format!("没法启动命令：{error}"))
        })?;
        let mut child = cmd.spawn().map_err(|error| {
            AppError::message("chat.runtime.terminal", format!("没法启动命令：{error}"))
        })?;
        if let Err(error) = control.attach(&child) {
            let _ = child.kill();
            control.terminate(&mut child);
            return Err(AppError::message(
                "chat.runtime.terminal",
                format!("没法启动命令：{error}"),
            ));
        }
        let output = Arc::new(Mutex::new(String::new()));
        let truncated = Arc::new(AtomicBool::new(false));
        spawn_pipe_reader(child.stdout.take(), Arc::clone(&output), Arc::clone(&truncated), spec.output_limit);
        spawn_pipe_reader(child.stderr.take(), Arc::clone(&output), Arc::clone(&truncated), spec.output_limit);
        let id = Uuid::new_v4().to_string();
        let label = display_command(&spec.command, &spec.args);
        self.items.insert(
            id.clone(),
            HostedTerminal {
                command: label,
                child,
                control,
                output,
                truncated,
                exit_code: None,
                wait_id: None,
            },
        );
        self.publish();
        Ok(id)
    }

    pub(crate) fn output(&mut self, terminal_id: &str) -> Result<(String, bool, Option<i32>)> {
        self.poll_one(terminal_id)?;
        let item = self.get(terminal_id)?;
        let text = item.output.lock().map(|guard| guard.clone()).unwrap_or_default();
        Ok((text, item.truncated.load(Ordering::SeqCst), item.exit_code))
    }

    pub(crate) fn begin_wait(&mut self, terminal_id: &str, request_id: Value) -> Result<Option<i32>> {
        self.poll_one(terminal_id)?;
        let item = self.get_mut(terminal_id)?;
        if let Some(code) = item.exit_code {
            return Ok(Some(code));
        }
        item.wait_id = Some(request_id);
        Ok(None)
    }

    pub(crate) fn kill(&mut self, terminal_id: &str) -> Result<Option<Value>> {
        let item = self.get_mut(terminal_id)?;
        item.control.terminate(&mut item.child);
        let _ = item.child.kill();
        self.poll_one(terminal_id)?;
        Ok(self.take_wait(terminal_id))
    }

    pub(crate) fn release(&mut self, terminal_id: &str) -> Result<Option<Value>> {
        let wait = self.take_wait(terminal_id);
        if let Some(mut item) = self.items.remove(terminal_id) {
            item.control.terminate(&mut item.child);
            let _ = item.child.kill();
        }
        self.publish();
        Ok(wait)
    }

    pub(crate) fn poll_exits(&mut self) -> Vec<(Value, i32)> {
        let ids: Vec<String> = self.items.keys().cloned().collect();
        let mut completed = Vec::new();
        for id in ids {
            let _ = self.poll_one(&id);
            if let Some(item) = self.items.get(&id) {
                if let (Some(code), Some(wait_id)) = (item.exit_code, item.wait_id.clone()) {
                    completed.push((wait_id, code));
                }
            }
            if let Some(item) = self.items.get_mut(&id) {
                if item.exit_code.is_some() {
                    item.wait_id = None;
                }
            }
        }
        if !completed.is_empty() {
            self.publish();
        }
        completed
    }

    pub(crate) fn kill_all(&mut self) -> Vec<(Value, i32)> {
        let ids: Vec<String> = self.items.keys().cloned().collect();
        let mut completed = Vec::new();
        for id in ids {
            if let Ok(Some(wait_id)) = self.kill(&id) {
                let code = self.items.get(&id).and_then(|item| item.exit_code).unwrap_or(1);
                completed.push((wait_id, code));
            }
        }
        completed
    }

    pub(crate) fn snapshot(&self) -> Vec<RuntimeHostTerminal> {
        self.items
            .iter()
            .map(|(id, item)| item.view(id))
            .collect()
    }

    fn poll_one(&mut self, terminal_id: &str) -> Result<()> {
        let item = self.get_mut(terminal_id)?;
        if item.exit_code.is_some() {
            return Ok(());
        }
        match poll_child(&mut item.child, &item.control) {
            Ok(ChildPoll::Exited(observed_status)) => {
                item.exit_code = Some(exit_code_after_poll(
                    &mut item.child,
                    &item.control,
                    observed_status,
                ));
                self.publish();
            }
            Ok(ChildPoll::Running) => {}
            Err(_) => {
                item.exit_code = Some(1);
                self.publish();
            }
        }
        Ok(())
    }

    fn take_wait(&mut self, terminal_id: &str) -> Option<Value> {
        self.items.get_mut(terminal_id).and_then(|item| item.wait_id.take())
    }

    fn get(&self, terminal_id: &str) -> Result<&HostedTerminal> {
        self.items.get(terminal_id).ok_or_else(|| {
            AppError::NotFound(format!("terminal not found: {terminal_id}"))
        })
    }

    fn get_mut(&mut self, terminal_id: &str) -> Result<&mut HostedTerminal> {
        self.items.get_mut(terminal_id).ok_or_else(|| {
            AppError::NotFound(format!("terminal not found: {terminal_id}"))
        })
    }

    fn publish(&self) {
        if let Ok(mut guard) = self.views.lock() {
            guard.insert(self.conversation_id.clone(), self.snapshot());
        }
    }
}

impl HostedTerminal {
    fn view(&self, id: &str) -> RuntimeHostTerminal {
        RuntimeHostTerminal {
            id: id.to_string(),
            command: self.command.clone(),
            output: self.output.lock().map(|guard| guard.clone()).unwrap_or_default(),
            truncated: self.truncated.load(Ordering::SeqCst),
            exit_code: self.exit_code,
            running: self.exit_code.is_none(),
        }
    }
}

fn exit_code_after_poll(
    child: &mut Child,
    control: &ProcessControl,
    observed_status: Option<std::process::ExitStatus>,
) -> i32 {
    // Unix `poll_child` uses waitid + WNOWAIT, so Exited(None) still needs a reap
    // to recover the real status. Windows already reaped via try_wait.
    control.cleanup_remaining_group(child);
    let status = match observed_status {
        Some(status) => {
            control.disarm();
            status
        }
        None => match reap_child(child, control) {
            Ok(status) => status,
            Err(_) => return 1,
        },
    };
    status.code().unwrap_or(1)
}

fn display_command(command: &str, args: &[String]) -> String {
    let mut parts = vec![command.to_string()];
    parts.extend(args.iter().cloned());
    parts.join(" ")
}

fn spawn_pipe_reader(
    pipe: Option<impl Read + Send + 'static>,
    output: Arc<Mutex<String>>,
    truncated: Arc<AtomicBool>,
    limit: usize,
) {
    let Some(mut pipe) = pipe else {
        return;
    };
    thread::Builder::new()
        .name("agenthub-host-terminal-out".into())
        .spawn(move || {
            let mut buf = [0_u8; 4096];
            loop {
                match pipe.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => append_output(&output, &truncated, limit, &buf[..n]),
                    Err(_) => break,
                }
            }
        })
        .ok();
}

fn append_output(output: &Mutex<String>, truncated: &AtomicBool, limit: usize, chunk: &[u8]) {
    let text = String::from_utf8_lossy(chunk);
    if let Ok(mut guard) = output.lock() {
        guard.push_str(&text);
        if guard.len() > limit {
            let extra = guard.len() - limit;
            guard.drain(..extra);
            truncated.store(true, Ordering::SeqCst);
        }
    }
}

#[cfg(test)]
mod tests;
