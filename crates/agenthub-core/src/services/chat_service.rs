//! Chat conversations: CRUD + single-agent send with isolated context stitching.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use chrono::Utc;
use uuid::Uuid;

use crate::catalog::limits::{CHAT_RUN_IDLE_TIMEOUT, CHAT_RUN_MAX_TIMEOUT};
use crate::error::{AppError, Result};
use crate::logging::{self, targets};
use crate::models::{
    AgentId, AgentRunResult, ChatEvent, ChatHistoryTurn, ChatMessage, ChatMessageStatus, ChatRole,
    Conversation, OutputStream, RunEvent, RunMode, RunOptions, RunStatus,
};
use crate::services::chat_cwd::{
    normalize_cwd, resolve_runtime_cwd, stored_cwd_missing, validate_existing_cwd,
};
use crate::services::RunService;
use crate::storage::{ChatRepo, Database};
use crate::utils::process::CancelToken;

use super::chat_runtime::ChatRuntime;

// Re-export so existing `chat_service::CONTEXT_CHAR_LIMIT` callers keep working.
pub use crate::catalog::limits::CONTEXT_CHAR_LIMIT;

fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

fn is_kiro_http_native_session(id: Option<&str>) -> bool {
    id.and_then(crate::adapters::kiro::http::parse_http_native_session_id)
        .is_some()
}

pub struct ChatService {
    repo: ChatRepo,
    run: Arc<RunService>,
    runtime: Arc<ChatRuntime>,
    active: Mutex<HashMap<String, CancelToken>>,
}

impl ChatService {
    pub fn new(db: Database, run: Arc<RunService>) -> Self {
        let runtime = Arc::new(ChatRuntime::new(db.clone(), Arc::clone(&run)));
        Self {
            repo: ChatRepo::new(db),
            run,
            runtime,
            active: Mutex::new(HashMap::new()),
        }
    }

    /// Durable Codex app-server runtime.  Legacy `send` remains owned by this
    /// service and is intentionally independent of the runtime path.
    pub fn runtime(&self) -> &Arc<ChatRuntime> {
        &self.runtime
    }

    /// Drop warmed model/skills catalogs after a live login change.
    /// Codex and Grok both serve `options()` from this cache.
    pub fn invalidate_runtime_catalogs(&self) {
        self.runtime.invalidate_catalogs();
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
        let cwd = normalize_cwd(cwd);
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
            sending: false,
        };
        self.repo.create_conversation(&conv)?;
        Ok(conv)
    }

    /// Ensure the UI's initial blank conversation exists without creating a
    /// duplicate when initialization is replayed or concurrent.  Explicit
    /// `create_conversation` calls intentionally keep their existing
    /// always-insert behavior.
    pub fn ensure_default_conversation(
        &self,
        agent_ids: Vec<AgentId>,
        cwd: Option<String>,
    ) -> Result<Conversation> {
        let agent_ids = require_single_agent(agent_ids)?;
        let cwd = normalize_cwd(cwd);
        let now = Utc::now().to_rfc3339();
        let candidate = Conversation {
            id: format!("conv-{}", Uuid::new_v4()),
            title: String::new(),
            agent_ids,
            cwd,
            allow_dangerous: false,
            created_at: now.clone(),
            updated_at: now,
            native_session_id: None,
            sending: false,
        };
        self.repo.ensure_default_conversation(&candidate)
    }

    /// Open or create an AgentHub conversation keyed by official session id.
    /// History comes from the store (or is imported once). Missing cwd is kept
    /// for display and never fails this call.
    pub fn open_from_session(
        &self,
        agent_id: AgentId,
        session_id: Option<String>,
        cwd: Option<String>,
        title: Option<String>,
        history: Vec<ChatHistoryTurn>,
    ) -> Result<Conversation> {
        let session_id = session_id.and_then(|value| {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        });
        if let Some(ref sid) = session_id {
            if let Some(existing) = self.repo.find_by_native_session_id(sid)? {
                if !self.repo.has_messages(&existing.id)? && !history.is_empty() {
                    self.import_history(&existing.id, agent_id, &history)?;
                }
                return self.get_conversation(&existing.id);
            }
        }

        let mut conv = self.create_conversation(vec![agent_id], cwd)?;
        if let Some(sid) = session_id {
            conv.native_session_id = Some(sid);
        }
        if let Some(title) = title
            .map(|value| value.trim().to_string())
            .filter(|v| !v.is_empty())
        {
            conv.title = title;
        }
        if conv.native_session_id.is_some() || !conv.title.is_empty() {
            conv.updated_at = Utc::now().to_rfc3339();
            self.repo.update_conversation(&conv)?;
        }
        self.import_history(&conv.id, agent_id, &history)?;
        self.get_conversation(&conv.id)
    }

    fn import_history(
        &self,
        conversation_id: &str,
        agent_id: AgentId,
        history: &[ChatHistoryTurn],
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let mut turn = 0_i64;
        let mut open_user_turn = false;
        for item in history {
            let content = item.content.trim();
            if content.is_empty() {
                continue;
            }
            match item.role {
                ChatRole::User => {
                    turn += 1;
                    open_user_turn = true;
                    self.repo.insert_message(&ChatMessage {
                        id: format!("msg-{}", Uuid::new_v4()),
                        conversation_id: conversation_id.to_string(),
                        turn,
                        role: ChatRole::User,
                        agent_id: None,
                        content: content.to_string(),
                        status: ChatMessageStatus::Ok,
                        exit_code: None,
                        duration_ms: 0,
                        error: None,
                        created_at: now.clone(),
                    })?;
                }
                ChatRole::Agent => {
                    if !open_user_turn {
                        turn += 1;
                    }
                    open_user_turn = false;
                    self.repo.insert_message(&ChatMessage {
                        id: format!("msg-{}", Uuid::new_v4()),
                        conversation_id: conversation_id.to_string(),
                        turn,
                        role: ChatRole::Agent,
                        agent_id: Some(agent_id),
                        content: content.to_string(),
                        status: ChatMessageStatus::Ok,
                        exit_code: None,
                        duration_ms: 0,
                        error: None,
                        created_at: now.clone(),
                    })?;
                }
            }
        }
        Ok(())
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
        if agent_ids.is_some() || cwd.is_some() {
            let started = conv
                .native_session_id
                .as_deref()
                .is_some_and(|sid| !sid.trim().is_empty())
                || self.repo.has_messages(id)?;
            let rebound_cwd = cwd
                .as_ref()
                .and_then(|inner| inner.as_deref())
                .map(str::trim)
                .filter(|path| !path.is_empty());
            let rebind_missing_cwd = agent_ids.is_none()
                && rebound_cwd.is_some()
                && stored_cwd_missing(conv.cwd.as_deref());
            if (started
                || self
                    .runtime
                    .session_locked(id, conv.native_session_id.as_deref())?)
                && !rebind_missing_cwd
            {
                return Err(AppError::message(
                    "invalid_arg",
                    "持续聊天会话不能更换 Agent 或工作目录，请新建会话",
                ));
            }
        }
        if let Some(t) = title {
            conv.title = t;
        }
        let mut leaving_runtime = false;
        if let Some(agents) = agent_ids {
            let next = require_single_agent(agents)?;
            leaving_runtime = crate::services::chat_runtime::is_runtime_chat_agent(
                conv.agent_ids.first().copied(),
            ) && next.first() != conv.agent_ids.first();
            if next != conv.agent_ids {
                conv.native_session_id = None;
            }
            conv.agent_ids = next;
        }
        if let Some(c) = cwd {
            let next = normalize_cwd(c);
            if next.is_none() && stored_cwd_missing(conv.cwd.as_deref()) {
                return Err(AppError::message(
                    "invalid_arg",
                    "原工作目录不存在时，请改绑到仍存在的目录",
                ));
            }
            if let Some(ref path) = next {
                validate_existing_cwd(path)?;
            }
            if conv.cwd != next && !stored_cwd_missing(conv.cwd.as_deref()) {
                conv.native_session_id = None;
            }
            conv.cwd = next;
        }
        if let Some(d) = allow_dangerous {
            conv.allow_dangerous = d;
        }
        if leaving_runtime {
            self.runtime.abandon_unstarted(id)?;
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
            self.runtime.shutdown(id);
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
        let result: Result<()> = (|| {
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
                logging::log_chat_info("stop", conversation_id, None, "stop ok");
            }
            Err(e) => {
                logging::log_chat_error(
                    "stop_fail",
                    conversation_id,
                    None,
                    Some(e.code()),
                    &e.to_string(),
                );
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
        match self.send_inner(conversation_id, user_input, on_event) {
            Ok(true) => {
                tracing::info!(
                    target: targets::CHAT,
                    module = targets::CHAT,
                    op = "send",
                    conversation_id = conversation_id,
                    elapsed_ms = elapsed_ms(started),
                    "send ok"
                );
                Ok(())
            }
            Ok(false) => {
                logging::log_chat_error("send_fail", conversation_id, None, None, "send failed");
                Ok(())
            }
            Err(e) => {
                logging::log_chat_error(
                    "send_fail",
                    conversation_id,
                    None,
                    Some(e.code()),
                    &e.to_string(),
                );
                Err(e)
            }
        }
    }

    /// Kiro HTTP conversations must keep `kiro-http:<id>` after a failed turn.
    /// Clearing it would make the next send look like a new chat and fall back to CLI.
    fn keep_native_session_after_resume_failure(
        resume_id: Option<&str>,
        results: &[AgentRunResult],
    ) -> bool {
        is_kiro_http_native_session(resume_id)
            || results
                .iter()
                .any(|result| is_kiro_http_native_session(result.native_session_id.as_deref()))
    }

    /// Best-effort: drop a stale native session so the next send uses full history.
    /// Failures are warned only and never replace the original send error.
    fn clear_native_session_id(&self, conversation_id: &str) {
        match self.get_conversation(conversation_id) {
            Ok(mut latest) => {
                if latest.native_session_id.is_none() {
                    return;
                }
                latest.native_session_id = None;
                latest.updated_at = Utc::now().to_rfc3339();
                if let Err(e) = self.repo.update_conversation(&latest) {
                    tracing::warn!(
                        module = targets::CHAT,
                        op = "clear_native_session",
                        conversation_id = conversation_id,
                        error = %e,
                        "failed to clear native session id after resume failure"
                    );
                }
            }
            Err(e) => {
                tracing::warn!(
                    module = targets::CHAT,
                    op = "clear_native_session",
                    conversation_id = conversation_id,
                    error = %e,
                    "failed to load conversation to clear native session id"
                );
            }
        }
    }

    fn send_inner(
        &self,
        conversation_id: &str,
        user_input: &str,
        on_event: &(dyn Fn(ChatEvent) + Send + Sync),
    ) -> Result<bool> {
        let user_input = user_input.trim();
        if user_input.is_empty() {
            return Err(AppError::InvalidArg("prompt must not be empty".into()));
        }
        if self.runtime.is_enabled(conversation_id)? {
            return Err(AppError::InvalidArg(
                "此会话使用持续聊天，请通过持续聊天入口发送".into(),
            ));
        }

        let mut conv = self.get_conversation(conversation_id)?;
        let runtime_cwd = resolve_runtime_cwd(conv.cwd.as_deref())?;

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
        logging::log_chat_info(
            "send",
            conversation_id,
            agents.first().map(|agent| agent.as_str()),
            "send start",
        );
        let send_agents = agents.clone();
        let send_cwd = conv.cwd.clone();
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

            let mut jobs: Vec<(AgentId, String)> = Vec::with_capacity(agents.len());
            for &agent in &agents {
                let prompt = if resume_id.is_some() {
                    user_input.to_string()
                } else {
                    build_agent_prompt(&history, agent, user_input)
                };
                jobs.push((agent, prompt));
            }

            let send_prefs = match agents.first().copied() {
                Some(AgentId::Grok) => Some(crate::adapters::grok::grok_send_prefs()),
                Some(AgentId::Kiro) => Some(crate::adapters::kiro::kiro_send_prefs()),
                _ => None,
            };
            let opts = RunOptions {
                mode: RunMode::Parallel,
                timeout: CHAT_RUN_MAX_TIMEOUT,
                idle_timeout: Some(CHAT_RUN_IDLE_TIMEOUT),
                cwd: Some(runtime_cwd),
                dry_run: false,
                skip_missing: true,
                allow_dangerous: conv.allow_dangerous,
                max_output_bytes: 2 * 1024 * 1024,
                // Claude/Codex → stream-json / --json; others remain text.
                process_mode: crate::models::ProcessMode::Auto,
                native_session_id: resume_id.clone(),
                model: send_prefs.as_ref().and_then(|(model, _)| model.clone()),
                effort: send_prefs.as_ref().and_then(|(_, effort)| effort.clone()),
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
                    if resume_id.is_some()
                        && !Self::keep_native_session_after_resume_failure(
                            resume_id.as_deref(),
                            &[],
                        )
                    {
                        self.clear_native_session_id(conversation_id);
                    }
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

        let resume_hard_fail =
            resume_id.is_some() && results.iter().any(|r| r.status.is_hard_failure());
        if resume_hard_fail
            && !Self::keep_native_session_after_resume_failure(resume_id.as_deref(), &results)
        {
            self.clear_native_session_id(conversation_id);
        } else if let Some(sid) = results.iter().find_map(|r| r.native_session_id.clone()) {
            if let Ok(mut latest) = self.get_conversation(conversation_id) {
                if latest.agent_ids != send_agents || latest.cwd != send_cwd {
                    tracing::debug!(
                        module = targets::CHAT,
                        op = "persist_native_session",
                        conversation_id = conversation_id,
                        "discard native session id; cwd or agent changed during send"
                    );
                } else if latest.native_session_id.as_deref() != Some(sid.as_str()) {
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

        let mut finalized_failed = false;
        for result in &results {
            if let Some(msg) = finalize_agent_message(&mut remaining, result) {
                if matches!(
                    msg.status,
                    ChatMessageStatus::Failed | ChatMessageStatus::Timeout
                ) {
                    finalized_failed = true;
                }
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

        let ok = report_ok && !finalized_failed;
        on_event(ChatEvent::Finished {
            turn,
            ok,
            cancelled: results.iter().any(|r| r.status == RunStatus::Cancelled),
        });
        Ok(ok)
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

fn looks_like_stream_protocol(text: &str) -> bool {
    let Some(first) = text
        .trim_start()
        .lines()
        .find(|line| !line.trim().is_empty())
    else {
        return false;
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(first) else {
        return false;
    };
    match value.get("type").and_then(|t| t.as_str()) {
        Some(
            "session" | "agent_start" | "turn_start" | "message_start" | "message_update"
            | "message_end" | "agent_end" | "turn_end" | "agent_settled",
        ) => true,
        _ => value.get("jsonrpc").is_some() && value.get("method").is_some(),
    }
}

fn stdout_as_message_content(stdout: &str) -> Option<&str> {
    if stdout.is_empty() || looks_like_stream_protocol(stdout) {
        None
    } else {
        Some(stdout)
    }
}

/// Strip CSI/OSC/cursor sequences from headless CLI text.
///
/// Kiro (and similar TUI CLIs) still emit color/cursor restore when stdout is a
/// pipe if `TERM` is inherited from Terminal.app / a Linux tty. Windows kiro
/// also colors under `CREATE_NO_WINDOW`. Chat stores the reply, not the TUI.
pub(crate) fn sanitize_cli_chat_text(input: &str) -> String {
    let had_esc = input.bytes().any(|b| b == 0x1b) || input.contains('\u{9b}');
    let stripped = strip_terminal_escapes(input);
    let mut out = stripped.replace("\r\n", "\n").replace('\r', "\n");
    if let Some(rest) = out.strip_prefix('\u{feff}') {
        out = rest.to_string();
    }
    if had_esc {
        if let Some(rest) = out.strip_prefix("> ") {
            out = rest.to_string();
        } else if let Some(rest) = out.strip_prefix('>') {
            out = rest.to_string();
        }
    }
    out.trim_matches('\n').to_string()
}

fn strip_terminal_escapes(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\u{1b}' {
            match chars.next() {
                Some('[') => {
                    for c in chars.by_ref() {
                        if ('\u{40}'..='\u{7e}').contains(&c) {
                            break;
                        }
                    }
                }
                Some(']') => {
                    while let Some(c) = chars.next() {
                        if c == '\u{07}' {
                            break;
                        }
                        if c == '\u{1b}' && chars.peek() == Some(&'\\') {
                            chars.next();
                            break;
                        }
                    }
                }
                Some('(' | ')') => {
                    let _ = chars.next();
                }
                Some(_) | None => {}
            }
            continue;
        }
        // UTF-8 C1 CSI (U+009B). Raw 0x9B is invalid UTF-8 and already lossy.
        if ch == '\u{9b}' {
            for c in chars.by_ref() {
                if ('\u{40}'..='\u{7e}').contains(&c) {
                    break;
                }
            }
            continue;
        }
        if (ch as u32) < 0x20 && ch != '\t' && ch != '\n' && ch != '\r' {
            continue;
        }
        out.push(ch);
    }
    out
}

fn finalize_agent_message(
    map: &mut HashMap<AgentId, ChatMessage>,
    result: &AgentRunResult,
) -> Option<ChatMessage> {
    let mut msg = map.remove(&result.agent)?;
    // When streaming was capped, prefer the runner's capped stdout over partial stream.
    // Never backfill Pi/Grok NDJSON — cancel-before-text used to dump the whole protocol.
    if result.truncated {
        if let Some(stdout) = stdout_as_message_content(&result.stdout) {
            msg.content = stdout.to_string();
        }
    } else if msg.content.is_empty() {
        if let Some(stdout) = stdout_as_message_content(&result.stdout) {
            msg.content = stdout.to_string();
        }
    }
    if looks_like_stream_protocol(&msg.content) {
        msg.content.clear();
    } else {
        msg.content = sanitize_cli_chat_text(&msg.content);
    }
    msg.status = map_run_status(result.status);
    msg.exit_code = result.exit_code;
    msg.duration_ms = result.duration_ms;
    msg.error = result.error.clone();
    if let Some(failure) = upstream_failure_message(&msg.content, &result.stderr, &result.stdout) {
        msg.status = ChatMessageStatus::Failed;
        msg.error = Some(failure.clone());
        if looks_like_raw_upstream_error(&msg.content) {
            msg.content = failure;
        }
    }
    if msg.status == ChatMessageStatus::Skipped && msg.content.is_empty() {
        if let Some(err) = &result.error {
            msg.content = err.clone();
        }
    }
    Some(msg)
}

fn looks_like_raw_upstream_error(text: &str) -> bool {
    let hay = text.to_ascii_lowercase();
    hay.contains("missing environment variable")
        || hay.contains("is not supported by any configured account")
        || hay.contains("oauth refresh failed")
        || hay.contains("invalid_grant")
        || hay.contains("invalid or unknown refresh token")
        || hay.contains("token refresh failed")
        || hay.contains("stealth/ox")
        || (hay.contains("\"code\":404") || hay.contains("\"code\": 404"))
        || looks_like_upstream_api_error(&hay)
}

fn looks_like_auth_refresh_failure(hay: &str) -> bool {
    hay.contains("oauth refresh failed")
        || hay.contains("invalid_grant")
        || hay.contains("invalid or unknown refresh token")
        || hay.contains("token refresh failed")
}

fn looks_like_upstream_api_error(hay: &str) -> bool {
    hay.contains("openai api error")
        || hay.contains("does not support parameter")
        || hay.contains("reasoningeffort")
        || hay.contains("reasoning_effort")
        || ((hay.contains("(400)") || hay.contains(" 400:") || hay.contains("http 400"))
            && (hay.contains("api error")
                || hay.contains("parameter")
                || hay.contains("model ")
                || hay.contains("unsupported")))
}

fn upstream_failure_message(content: &str, stderr: &str, stdout: &str) -> Option<String> {
    let hay = format!("{content}\n{stderr}\n{stdout}").to_ascii_lowercase();
    if hay.contains("missing environment variable") {
        return Some("这份登录还在用另一份 API Key 配置，没法发。请点重试。".into());
    }
    if hay.contains("is not supported by any configured account")
        || hay.contains("model_unavailable")
    {
        return Some("这个模型当前登录用不了。请换一个模型后重试。".into());
    }
    if looks_like_auth_refresh_failure(&hay) {
        return Some("这份登录已失效，请重新登录后重试。".into());
    }
    if hay.contains("stealth/ox")
        || hay.contains("stealth ox")
        || ((hay.contains("\"code\":404")
            || hay.contains("\"code\": 404")
            || hay.contains(" 404:"))
            && (hay.contains("model")
                || hay.contains("retired")
                || hay.contains("glm-5.3")
                || hay.contains("stealth")))
    {
        return Some("这个模型已经下架或当前登录用不了。请换一个模型后重试。".into());
    }
    if looks_like_upstream_api_error(&hay) {
        if hay.contains("reasoningeffort")
            || hay.contains("reasoning_effort")
            || hay.contains("does not support parameter")
        {
            return Some("这个模型不支持当前思考设置。请点重试。".into());
        }
        return Some("这次发送没成功。请点重试。".into());
    }
    None
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
