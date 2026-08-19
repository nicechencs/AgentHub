//! Chat conversations: CRUD + single-agent send with isolated context stitching.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use chrono::Utc;
use uuid::Uuid;

use crate::catalog::limits::DEFAULT_RUN_TIMEOUT;
use crate::error::{AppError, Result};
use crate::logging::{self, targets};
use crate::models::{
    AgentId, AgentRunResult, ChatEvent, ChatMessage, ChatMessageStatus, ChatRole, Conversation,
    OutputStream, RunEvent, RunMode, RunOptions, RunStatus,
};
use crate::services::RunService;
use crate::storage::{ChatRepo, Database};
use crate::utils::process::CancelToken;

// Re-export so existing `chat_service::CONTEXT_CHAR_LIMIT` callers keep working.
pub use crate::catalog::limits::CONTEXT_CHAR_LIMIT;

fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

pub struct ChatService {
    repo: ChatRepo,
    run: Arc<RunService>,
    active: Mutex<HashMap<String, CancelToken>>,
}

impl ChatService {
    pub fn new(db: Database, run: Arc<RunService>) -> Self {
        Self {
            repo: ChatRepo::new(db),
            run,
            active: Mutex::new(HashMap::new()),
        }
    }

    pub fn list_conversations(&self) -> Result<Vec<Conversation>> {
        self.repo.list_conversations()
    }

    pub fn get_conversation(&self, id: &str) -> Result<Conversation> {
        self.repo
            .get_conversation(id)?
            .ok_or_else(|| AppError::NotFound(format!("conversation not found: {id}")))
    }

    pub fn create_conversation(
        &self,
        agent_ids: Vec<AgentId>,
        cwd: Option<String>,
    ) -> Result<Conversation> {
        let agent_ids = require_single_agent(agent_ids)?;
        if let Some(ref c) = cwd {
            validate_cwd(c)?;
        }
        let now = Utc::now().to_rfc3339();
        let conv = Conversation {
            id: format!("conv-{}", Uuid::new_v4()),
            title: String::new(),
            agent_ids,
            cwd,
            allow_dangerous: false,
            created_at: now.clone(),
            updated_at: now,
            native_session_id: None,
        };
        self.repo.create_conversation(&conv)?;
        Ok(conv)
    }

    pub fn update_conversation(
        &self,
        id: &str,
        title: Option<String>,
        agent_ids: Option<Vec<AgentId>>,
        cwd: Option<Option<String>>,
        allow_dangerous: Option<bool>,
    ) -> Result<Conversation> {
        let mut conv = self.get_conversation(id)?;
        if let Some(t) = title {
            conv.title = t;
        }
        if let Some(agents) = agent_ids {
            let next = require_single_agent(agents)?;
            if next != conv.agent_ids {
                conv.native_session_id = None;
            }
            conv.agent_ids = next;
        }
        if let Some(c) = cwd {
            if let Some(ref path) = c {
                validate_cwd(path)?;
            }
            if conv.cwd != c {
                conv.native_session_id = None;
            }
            conv.cwd = c;
        }
        if let Some(d) = allow_dangerous {
            conv.allow_dangerous = d;
        }
        conv.updated_at = Utc::now().to_rfc3339();
        self.repo.update_conversation(&conv)?;
        Ok(conv)
    }

    pub fn delete_conversation(&self, id: &str) -> Result<()> {
        let started = Instant::now();
        // Cancel any in-flight send so subprocesses stop and the active map is
        // cleared by the send path (or remove here if already gone).
        let result = (|| {
            let _ = self.cancel(id);
            if !self.repo.delete_conversation(id)? {
                return Err(AppError::NotFound(format!("conversation not found: {id}")));
            }
            Ok(())
        })();
        match &result {
            Ok(()) => {
                tracing::info!(
                    module = targets::CHAT,
                    op = "delete",
                    conversation_id = id,
                    elapsed_ms = elapsed_ms(started),
                    "delete ok"
                );
            }
            Err(e) => {
                logging::log_app_error(targets::CHAT, "delete", e);
            }
        }
        result
    }

    pub fn list_messages(&self, conversation_id: &str) -> Result<Vec<ChatMessage>> {
        // Ensure conversation exists.
        let _ = self.get_conversation(conversation_id)?;
        self.repo.list_messages(conversation_id)
    }

    /// Cancel an in-flight send for this conversation (best-effort).
    pub fn cancel(&self, conversation_id: &str) -> Result<()> {
        let result = (|| {
            let guard = self
                .active
                .lock()
                .map_err(|_| AppError::message("chat.lock", "active cancel map poisoned"))?;
            if let Some(token) = guard.get(conversation_id) {
                token.cancel();
            }
            Ok(())
        })();
        match &result {
            Ok(()) => {
                tracing::debug!(
                    module = targets::CHAT,
                    op = "cancel",
                    conversation_id = conversation_id,
                    "cancel ok"
                );
            }
            Err(e) => {
                logging::log_app_error(targets::CHAT, "cancel", e);
            }
        }
        result
    }

    /// Send a user message and fan out to the conversation's agents.
    pub fn send(
        &self,
        conversation_id: &str,
        user_input: &str,
        on_event: &(dyn Fn(ChatEvent) + Send + Sync),
    ) -> Result<()> {
        let started = Instant::now();
        let result = self.send_inner(conversation_id, user_input, on_event);
        match &result {
            Ok(()) => {
                tracing::info!(
                    module = targets::CHAT,
                    op = "send",
                    conversation_id = conversation_id,
                    elapsed_ms = elapsed_ms(started),
                    "send ok"
                );
            }
            Err(e) => {
                logging::log_app_error(targets::CHAT, "send", e);
            }
        }
        result
    }

    fn send_inner(
        &self,
        conversation_id: &str,
        user_input: &str,
        on_event: &(dyn Fn(ChatEvent) + Send + Sync),
    ) -> Result<()> {
        let user_input = user_input.trim();
        if user_input.is_empty() {
            return Err(AppError::InvalidArg("prompt must not be empty".into()));
        }

        let mut conv = self.get_conversation(conversation_id)?;
        if let Some(ref c) = conv.cwd {
            validate_cwd(c)?;
        }

        let history = self.repo.list_messages(conversation_id)?;
        // Legacy multi-agent rows: send only the first agent.
        let agents: Vec<AgentId> = conv.agent_ids.first().copied().into_iter().collect();
        if agents.is_empty() {
            return Err(AppError::InvalidArg(
                "conversation must select at least one agent".into(),
            ));
        }
        let agents_joined = agents
            .iter()
            .map(|a| a.as_str())
            .collect::<Vec<_>>()
            .join(",");
        tracing::debug!(
            module = targets::CHAT,
            op = "send",
            conversation_id = conversation_id,
            agents = %agents_joined,
            prompt_len = user_input.chars().count(),
            "send start"
        );
        let now = Utc::now().to_rfc3339();

        let mut user_msg = ChatMessage {
            id: format!("msg-{}", Uuid::new_v4()),
            conversation_id: conversation_id.to_string(),
            turn: 0,
            role: ChatRole::User,
            agent_id: None,
            content: user_input.to_string(),
            status: ChatMessageStatus::Ok,
            exit_code: None,
            duration_ms: 0,
            error: None,
            created_at: now.clone(),
        };

        // Pre-build running placeholders so the UI can bind streaming chunks.
        let mut agent_rows: Vec<ChatMessage> = agents
            .iter()
            .map(|&agent| ChatMessage {
                id: format!("msg-{}", Uuid::new_v4()),
                conversation_id: conversation_id.to_string(),
                turn: 0,
                role: ChatRole::Agent,
                agent_id: Some(agent),
                content: String::new(),
                status: ChatMessageStatus::Running,
                exit_code: None,
                duration_ms: 0,
                error: None,
                created_at: Utc::now().to_rfc3339(),
            })
            .collect();

        // Single-flight: register cancel token before allocating the turn so a
        // concurrent send fails without creating orphan messages.
        let cancel = CancelToken::new();
        {
            let mut guard = self
                .active
                .lock()
                .map_err(|_| AppError::message("chat.lock", "active cancel map poisoned"))?;
            if guard.contains_key(conversation_id) {
                return Err(AppError::InvalidArg(
                    "conversation already has an in-flight send".into(),
                ));
            }
            guard.insert(conversation_id.to_string(), cancel.clone());
        }

        // Guard: always remove active entry; on early failure after placeholders,
        // mark remaining running rows failed.
        let send_result = (|| -> Result<(
            i64,
            bool,
            Vec<AgentRunResult>,
            HashMap<AgentId, ChatMessage>,
        )> {
            if conv.title.trim().is_empty() {
                conv.title = truncate_title(user_input, 30);
            }
            if conv.agent_ids.len() > 1 {
                conv.agent_ids.truncate(1);
            }
            conv.updated_at = now;
            self.repo.update_conversation(&conv)?;

            let turn =
                self.repo
                    .insert_turn_messages(conversation_id, &mut user_msg, &mut agent_rows)?;

            on_event(ChatEvent::Started {
                turn,
                agents: agents.clone(),
            });

            let mut placeholders: HashMap<AgentId, ChatMessage> = HashMap::new();
            for msg in agent_rows {
                if let Some(agent) = msg.agent_id {
                    placeholders.insert(agent, msg);
                }
            }

            let resume_id = conv
                .native_session_id
                .as_deref()
                .and_then(crate::adapters::session_resume::valid_session_id)
                .filter(|_| {
                    agents
                        .first()
                        .is_some_and(|a| crate::adapters::supports_print_resume(*a))
                })
                .map(str::to_string);

            let mut jobs: Vec<(AgentId, String)> = Vec::with_capacity(agents.len());
            for &agent in &agents {
                let prompt = if resume_id.is_some() {
                    user_input.to_string()
                } else {
                    build_agent_prompt(&history, agent, user_input)
                };
                jobs.push((agent, prompt));
            }

            let opts = RunOptions {
                mode: RunMode::Parallel,
                timeout: DEFAULT_RUN_TIMEOUT,
                cwd: conv.cwd.as_ref().map(PathBuf::from),
                dry_run: false,
                skip_missing: true,
                allow_dangerous: conv.allow_dangerous,
                max_output_bytes: 2 * 1024 * 1024,
                // Claude/Codex → stream-json / --json; others remain text.
                process_mode: crate::models::ProcessMode::Auto,
                native_session_id: resume_id,
            };
            let max_out = opts.max_output_bytes;
            tracing::debug!(
                module = targets::CHAT,
                op = "send",
                conversation_id = conversation_id,
                process_mode = opts.process_mode.as_str(),
                agents = %agents_joined,
                "chat run options"
            );

            let placeholders = Mutex::new(placeholders);
            let run_cb = |ev: RunEvent| match ev {
                RunEvent::Started { agent, command } => {
                    tracing::debug!(
                        module = targets::CHAT,
                        op = "agent_started",
                        agent = agent.as_str(),
                        command = %crate::utils::redact::redact_text(&command),
                        "agent process started"
                    );
                    on_event(ChatEvent::AgentStarted {
                        turn,
                        agent,
                        command,
                    });
                }
                RunEvent::Chunk {
                    agent,
                    stream,
                    text,
                } => {
                    if stream == OutputStream::Stdout {
                        if let Ok(mut map) = placeholders.lock() {
                            if let Some(msg) = map.get_mut(&agent) {
                                append_capped(&mut msg.content, &text, max_out);
                            }
                        }
                    }
                    on_event(ChatEvent::AgentChunk {
                        turn,
                        agent,
                        stream,
                        text,
                    });
                }
                RunEvent::Step { agent, step } => {
                    tracing::trace!(
                        module = targets::CHAT,
                        op = "agent_process",
                        agent = agent.as_str(),
                        step = step.kind(),
                        "process step"
                    );
                    on_event(ChatEvent::AgentProcess {
                        turn,
                        agent,
                        step,
                    });
                }
                RunEvent::Finished { agent: _ } => {}
            };

            let report = match self.run.run_each(&jobs, &opts, &cancel, &run_cb) {
                Ok(r) => r,
                Err(e) => {
                    let mut map = placeholders
                        .into_inner()
                        .map_err(|_| AppError::message("chat.lock", "placeholders poisoned"))?;
                    fail_remaining(
                        &self.repo,
                        &mut map,
                        &e.to_string(),
                        ChatMessageStatus::Failed,
                        on_event,
                        turn,
                    )?;
                    on_event(ChatEvent::Error {
                        message: e.to_string(),
                    });
                    return Err(e);
                }
            };

            let map = placeholders
                .into_inner()
                .map_err(|_| AppError::message("chat.lock", "placeholders poisoned"))?;

            Ok((turn, report.ok, report.results, map))
        })();

        {
            let mut guard = self
                .active
                .lock()
                .map_err(|_| AppError::message("chat.lock", "active cancel map poisoned"))?;
            guard.remove(conversation_id);
        }

        let (turn, report_ok, results, mut remaining) = send_result?;

        if let Some(sid) = results.iter().find_map(|r| r.native_session_id.clone()) {
            if let Ok(mut latest) = self.get_conversation(conversation_id) {
                if latest.native_session_id.as_deref() != Some(sid.as_str()) {
                    latest.native_session_id = Some(sid);
                    latest.updated_at = Utc::now().to_rfc3339();
                    if let Err(e) = self.repo.update_conversation(&latest) {
                        tracing::warn!(
                            module = targets::CHAT,
                            op = "persist_native_session",
                            conversation_id = conversation_id,
                            error = %e,
                            "failed to persist native session id"
                        );
                    }
                }
            }
        }

        for result in &results {
            if let Some(msg) = finalize_agent_message(&mut remaining, result) {
                // Best-effort persist; if conversation was deleted mid-send, skip.
                match self.repo.update_message(&msg) {
                    Ok(()) | Err(AppError::NotFound(_)) => {
                        on_event(ChatEvent::AgentFinished {
                            turn,
                            agent: result.agent,
                            message: msg,
                        });
                    }
                    Err(e) => {
                        fail_remaining(
                            &self.repo,
                            &mut remaining,
                            &e.to_string(),
                            ChatMessageStatus::Failed,
                            on_event,
                            turn,
                        )?;
                        on_event(ChatEvent::Error {
                            message: e.to_string(),
                        });
                        return Err(e);
                    }
                }
            }
        }

        // Any leftover placeholders (should be rare) mark failed.
        if !remaining.is_empty() {
            fail_remaining(
                &self.repo,
                &mut remaining,
                "internal: missing agent result",
                ChatMessageStatus::Failed,
                on_event,
                turn,
            )?;
        }

        on_event(ChatEvent::Finished {
            turn,
            ok: report_ok,
        });
        Ok(())
    }
}

fn fail_remaining(
    repo: &ChatRepo,
    map: &mut HashMap<AgentId, ChatMessage>,
    err: &str,
    status: ChatMessageStatus,
    on_event: &(dyn Fn(ChatEvent) + Send + Sync),
    turn: i64,
) -> Result<()> {
    let agents: Vec<AgentId> = map.keys().copied().collect();
    for agent in agents {
        let Some(mut msg) = map.remove(&agent) else {
            continue;
        };
        msg.status = status;
        msg.error = Some(err.to_string());
        if msg.content.is_empty() {
            msg.content = err.to_string();
        }
        match repo.update_message(&msg) {
            Ok(()) | Err(AppError::NotFound(_)) => {}
            Err(e) => return Err(e),
        }
        on_event(ChatEvent::AgentFinished {
            turn,
            agent,
            message: msg,
        });
    }
    Ok(())
}

fn finalize_agent_message(
    map: &mut HashMap<AgentId, ChatMessage>,
    result: &AgentRunResult,
) -> Option<ChatMessage> {
    let mut msg = map.remove(&result.agent)?;
    // When streaming was capped, prefer the runner's capped stdout over partial stream.
    if result.truncated {
        msg.content = result.stdout.clone();
    } else if msg.content.is_empty() && !result.stdout.is_empty() {
        msg.content = result.stdout.clone();
    }
    msg.status = map_run_status(result.status);
    msg.exit_code = result.exit_code;
    msg.duration_ms = result.duration_ms;
    msg.error = result.error.clone();
    if msg.status == ChatMessageStatus::Skipped && msg.content.is_empty() {
        if let Some(err) = &result.error {
            msg.content = err.clone();
        }
    }
    Some(msg)
}

fn map_run_status(status: RunStatus) -> ChatMessageStatus {
    match status {
        RunStatus::Ok | RunStatus::DryRun => ChatMessageStatus::Ok,
        RunStatus::Failed => ChatMessageStatus::Failed,
        RunStatus::Timeout => ChatMessageStatus::Timeout,
        RunStatus::Skipped => ChatMessageStatus::Skipped,
        RunStatus::Cancelled => ChatMessageStatus::Cancelled,
    }
}

fn truncate_title(s: &str, max: usize) -> String {
    let s = s.trim();
    if s.chars().count() <= max {
        return s.to_string();
    }
    let t: String = s.chars().take(max.saturating_sub(1)).collect();
    format!("{t}…")
}

fn dedupe_agents(agents: Vec<AgentId>) -> Vec<AgentId> {
    let mut out = Vec::with_capacity(agents.len());
    for a in agents {
        if !out.contains(&a) {
            out.push(a);
        }
    }
    out
}

fn require_single_agent(agents: Vec<AgentId>) -> Result<Vec<AgentId>> {
    let agents = dedupe_agents(agents);
    if agents.is_empty() {
        return Err(AppError::InvalidArg(
            "conversation must select at least one agent".into(),
        ));
    }
    if agents.len() > 1 {
        return Err(AppError::InvalidArg(
            "conversation can select only one agent".into(),
        ));
    }
    Ok(agents)
}

fn validate_cwd(cwd: &str) -> Result<()> {
    let path = Path::new(cwd);
    if !path.is_dir() {
        return Err(AppError::InvalidArg(format!(
            "cwd is not an existing directory: {cwd}"
        )));
    }
    Ok(())
}

/// Append `chunk` to `dest` without exceeding `max` bytes (UTF-8 safe cut).
fn append_capped(dest: &mut String, chunk: &str, max: usize) {
    if dest.len() >= max {
        return;
    }
    let room = max - dest.len();
    if chunk.len() <= room {
        dest.push_str(chunk);
        return;
    }
    let mut end = room;
    while end > 0 && !chunk.is_char_boundary(end) {
        end -= 1;
    }
    dest.push_str(&chunk[..end]);
}

/// Build a per-agent prompt with isolated history (user + this agent's ok replies only).
pub fn build_agent_prompt(history: &[ChatMessage], agent: AgentId, user_input: &str) -> String {
    let user_input = user_input.trim();
    if history.is_empty() {
        return user_input.to_string();
    }

    // Group by turn: keep user messages + this agent's ok replies.
    let mut turns: Vec<(String, Option<String>)> = Vec::new();
    let mut current_user: Option<String> = None;
    let mut current_agent: Option<String> = None;
    let mut current_turn: Option<i64> = None;

    let flush = |turns: &mut Vec<(String, Option<String>)>,
                 user: &mut Option<String>,
                 agent_reply: &mut Option<String>| {
        if let Some(u) = user.take() {
            turns.push((u, agent_reply.take()));
        }
    };

    for msg in history {
        if current_turn != Some(msg.turn) {
            flush(&mut turns, &mut current_user, &mut current_agent);
            current_turn = Some(msg.turn);
        }
        match msg.role {
            ChatRole::User => {
                current_user = Some(msg.content.clone());
            }
            ChatRole::Agent => {
                if msg.agent_id == Some(agent) && msg.status == ChatMessageStatus::Ok {
                    current_agent = Some(msg.content.clone());
                }
            }
        }
    }
    flush(&mut turns, &mut current_user, &mut current_agent);

    if turns.is_empty() {
        return user_input.to_string();
    }

    // Drop oldest whole turns until under limit.
    let mut omitted = false;
    let render = |turns: &[(String, Option<String>)], omitted: bool| -> String {
        let mut body = String::new();
        if omitted {
            body.push_str("[更早的对话已省略]\n");
        }
        for (u, a) in turns {
            body.push_str("[用户] ");
            body.push_str(u);
            body.push('\n');
            if let Some(reply) = a {
                body.push_str("[助手] ");
                body.push_str(reply);
                body.push('\n');
            }
        }
        body
    };

    while !turns.is_empty() {
        let body = render(&turns, omitted);
        let full = format!(
            "以下是我们此前的对话记录，请在此基础上回答最后的「当前问题」。\n\n## 历史对话\n{body}\n## 当前问题\n{user_input}"
        );
        if full.chars().count() <= CONTEXT_CHAR_LIMIT {
            return full;
        }
        turns.remove(0);
        omitted = true;
    }

    user_input.to_string()
}

#[cfg(test)]
mod tests;
