//! Durable Codex app-server chat runtime.
//!
//! A runtime owns one Codex app-server process per conversation.  All commands
//! for that process are serialized through its worker queue, so a late answer
//! cannot race a stop or be delivered to a newer turn.  The worker commits
//! normalized events to SQLite before a snapshot can expose them.

mod codex_transport;
mod file_change;
mod ops;
mod store;
mod types;

pub(crate) use store::{
    is_acp_runtime_agent, is_claude_stream_runtime_agent, is_runtime_chat_agent,
};

pub use types::{
    RuntimeChannel, RuntimeDecision, RuntimeEvent, RuntimeExtensionItem, RuntimeExtensionKind,
    RuntimeFileChange, RuntimeLocalImage, RuntimeModelOption, RuntimeNativeCommand, RuntimeOptions,
    RuntimePermissionOption, RuntimePhase, RuntimePlanEntry, RuntimeQuestion, RuntimeQuestionOption,
    RuntimeReply,
    RuntimeRequest, RuntimeRequestKind, RuntimeSkillRef, RuntimeSnapshot, RuntimeStartExtras,
    RuntimeTurnSettings,
};

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use chrono::Utc;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::error::{AppError, Result};
use crate::logging;
use crate::models::{
    AgentId, ChatEvent, ChatMessage, ChatMessageStatus, ChatRole, OutputStream, ProcessStep,
};
use crate::services::RunService;
use crate::storage::{ChatRepo, Database};
use crate::utils::redact::redact_text;

use self::codex_transport::{CodexEvent, CodexTransport};
use self::store::{OperationState, RuntimeStore};

const CODEX_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const CODEX_POLL_INTERVAL: Duration = Duration::from_millis(100);
const ACP_CANCEL_DEADLINE: Duration = Duration::from_secs(10);

enum RuntimeCommand {
    Start {
        prompt: String,
        client_request_id: String,
        extras: RuntimeStartExtras,
        result: SyncSender<Result<RuntimeSnapshot>>,
    },
    Reply {
        reply: RuntimeReply,
        result: SyncSender<Result<()>>,
    },
    Steer {
        prompt: String,
        run_id: String,
        client_request_id: String,
        result: SyncSender<Result<()>>,
    },
    Cancel {
        run_id: String,
        result: SyncSender<Result<()>>,
    },
    Shutdown {
        done: SyncSender<()>,
    },
}

#[derive(Clone)]
struct ActorHandle {
    tx: SyncSender<RuntimeCommand>,
    abort: Arc<AtomicBool>,
}

/// One serialized owner per conversation.  The map itself is only a routing
/// table; process state is never mutated from command callers.
#[derive(Default, Clone)]
struct CatalogCache {
    models: Vec<RuntimeModelOption>,
    extensions: Vec<RuntimeExtensionItem>,
    from_codex: bool,
    native_commands: Vec<RuntimeNativeCommand>,
    image_input: Option<bool>,
    catalog_epoch: i64,
    plan: Vec<RuntimePlanEntry>,
}

fn merge_catalog_cache(previous: Option<&CatalogCache>, mut fetched: CatalogCache) -> CatalogCache {
    if let Some(previous) = previous {
        if fetched.native_commands.is_empty() {
            fetched.native_commands = previous.native_commands.clone();
        }
        if fetched.image_input.is_none() {
            fetched.image_input = previous.image_input;
        }
        fetched.catalog_epoch = previous.catalog_epoch.max(fetched.catalog_epoch);
        if fetched.plan.is_empty() {
            fetched.plan = previous.plan.clone();
        }
    }
    fetched
}

fn runtime_channel(agent: Option<AgentId>) -> RuntimeChannel {
    match agent {
        Some(AgentId::Grok | AgentId::Kiro) => RuntimeChannel::Acp,
        Some(AgentId::Claude) => RuntimeChannel::StreamJson,
        Some(AgentId::Codex) => RuntimeChannel::AppServer,
        _ => RuntimeChannel::Legacy,
    }
}

pub struct ChatRuntime {
    store: RuntimeStore,
    repo: ChatRepo,
    run: Arc<RunService>,
    actors: Mutex<HashMap<String, ActorHandle>>,
    catalogs: Arc<Mutex<HashMap<String, CatalogCache>>>,
    codex_program_override: Arc<Mutex<Option<PathBuf>>>,
}

impl ChatRuntime {
    pub fn new(db: Database, run: Arc<RunService>) -> Self {
        let store = RuntimeStore::new(db.clone());
        if let Err(error) = store.recover_active() {
            tracing::warn!(error = %error, "failed to recover stale chat runtime rows");
        }
        Self {
            store,
            repo: ChatRepo::new(db),
            run,
            actors: Mutex::new(HashMap::new()),
            catalogs: Arc::new(Mutex::new(HashMap::new())),
            codex_program_override: Arc::new(Mutex::new(None)),
        }
    }

    pub fn snapshot(
        &self,
        conversation_id: &str,
        after_sequence: Option<i64>,
    ) -> Result<RuntimeSnapshot> {
        let mut snapshot = self.store.snapshot(conversation_id, after_sequence)?;
        if let Some(cache) = self.peek_catalog(conversation_id) {
            snapshot.catalog_epoch = cache.catalog_epoch;
            snapshot.plan = cache.plan;
        }
        Ok(snapshot)
    }

    pub(crate) fn is_enabled(&self, conversation_id: &str) -> Result<bool> {
        self.store.persisted_enabled(conversation_id)
    }

    /// Empty Codex chats advertise `enabled` so the first send uses this path.
    /// Agent / cwd stay editable until a continuous session actually starts.
    pub(crate) fn session_locked(
        &self,
        conversation_id: &str,
        native_session_id: Option<&str>,
    ) -> Result<bool> {
        let Some(record) = self.store.record(conversation_id)? else {
            return Ok(false);
        };
        if !record.enabled {
            return Ok(false);
        }
        if native_session_id.is_some_and(|id| !id.trim().is_empty()) {
            return Ok(true);
        }
        Ok(record.phase != RuntimePhase::Idle || record.run_id.is_some())
    }

    /// Drop an idle runtime row so switching away from Codex can use the
    /// normal send path. Started sessions must be rejected by the caller.
    pub(crate) fn abandon_unstarted(&self, conversation_id: &str) -> Result<()> {
        self.shutdown(conversation_id);
        if let Ok(mut catalogs) = self.catalogs.lock() {
            catalogs.remove(conversation_id);
        }
        self.store.delete_unstarted(conversation_id)
    }

    /// Forget warmed model/skills lists so the next idle `options()` refetch
    /// sees the login that is live now (Codex ChatGPT vs API, Grok slots).
    pub fn invalidate_catalogs(&self) {
        if let Ok(mut catalogs) = self.catalogs.lock() {
            catalogs.clear();
        }
    }

    #[cfg(test)]
    pub(crate) fn has_catalog_cache_for_test(&self, conversation_id: &str) -> bool {
        self.catalogs
            .lock()
            .map(|guard| guard.contains_key(conversation_id))
            .unwrap_or(false)
    }

    /// Switch an existing Grok print session onto continuous chat.
    /// Requires a stored native session id; does not invent a new session.
    /// Kiro history stays on its original send path.
    pub fn continue_legacy(&self, conversation_id: &str) -> Result<RuntimeSnapshot> {
        let conversation = self
            .repo
            .get_conversation(conversation_id)?
            .ok_or_else(|| {
                AppError::NotFound(format!("conversation not found: {conversation_id}"))
            })?;
        if conversation.agent_ids.first().copied() == Some(AgentId::Kiro) {
            return Err(AppError::Unsupported(
                "这条 Kiro 对话不能切换聊天方式，请新建对话".into(),
            ));
        }
        if conversation.agent_ids.first().copied() != Some(AgentId::Grok) {
            return Err(AppError::Unsupported("只有 Grok 可以用新方式继续".into()));
        }
        let session_id = conversation
            .native_session_id
            .as_deref()
            .and_then(crate::adapters::session_resume::valid_session_id)
            .ok_or_else(|| AppError::InvalidArg("这条对话没有可接上的会话，请新建对话".into()))?;
        self.store
            .enable_legacy_with_session(conversation_id, session_id)?;
        self.snapshot(conversation_id, None)
    }

    pub fn options(&self, conversation_id: &str) -> Result<RuntimeOptions> {
        self.options_with(conversation_id, false)
    }

    /// Drop the warmed catalog for this conversation and fetch again.
    /// Used after a live login change on an idle Codex / Grok chat.
    pub fn refresh_options(&self, conversation_id: &str) -> Result<RuntimeOptions> {
        self.options_with(conversation_id, true)
    }

    fn options_with(&self, conversation_id: &str, refresh: bool) -> Result<RuntimeOptions> {
        self.store.ensure_conversation(conversation_id)?;
        let agent = self.store.conversation_agent(conversation_id)?;
        if !is_runtime_chat_agent(agent) {
            return Ok(RuntimeOptions::inactive(conversation_id));
        }
        self.store.enable_if_new(conversation_id)?;
        let record = self.store.record(conversation_id)?;
        let frozen = record
            .as_ref()
            .is_some_and(|record| ops::phase_freezes_settings(record.phase))
            || (agent == Some(AgentId::Kiro)
                && record
                    .as_ref()
                    .is_some_and(|record| record.thread_id.is_some()));
        let cache = self.load_catalog(conversation_id, refresh);
        let models = self.effective_models(&cache.models);
        let mut settings = self.store.turn_settings(conversation_id)?;
        // Idle only: quietly repair a stale unsupported effort so the UI menu
        // never keeps offering an incompatible value after a model switch.
        // Frozen turns keep the effective pair that started the turn.
        if !frozen {
            if ops::settings_need_catalog_default(&settings, &models) {
                if let Some(defaults) = ops::default_turn_settings(&models) {
                    settings = self.store.set_turn_settings(conversation_id, &defaults)?;
                }
            }
            if let Some(repaired) = ops::reconcile_turn_settings(&settings, &models) {
                settings = self.store.set_turn_settings(conversation_id, &repaired)?;
            }
        }
        let persistent = is_acp_runtime_agent(agent) || is_claude_stream_runtime_agent(agent);
        let session_ready = record
            .as_ref()
            .and_then(|row| row.thread_id.as_deref())
            .is_some_and(|id| !id.trim().is_empty());
        Ok(RuntimeOptions {
            conversation_id: conversation_id.to_string(),
            settings,
            settings_frozen: frozen,
            models,
            extensions: cache.extensions,
            models_from_codex: cache.from_codex,
            image_input: cache.image_input.unwrap_or(true),
            steer: !persistent,
            transport: runtime_channel(agent),
            native_commands: cache.native_commands,
            session_ready,
        })
    }

    pub fn set_settings(
        &self,
        conversation_id: &str,
        requested: RuntimeTurnSettings,
    ) -> Result<RuntimeTurnSettings> {
        self.store.enable_if_new(conversation_id)?;
        // Reject active turns before spawning a catalog process.
        let agent = self.store.conversation_agent(conversation_id)?;
        if let Some(record) = self.store.record(conversation_id)? {
            if ops::phase_freezes_settings(record.phase) {
                return Err(AppError::InvalidArg(
                    "当前轮次进行中，不能修改模型或思考强度".into(),
                ));
            }
        }
        let prior = self.store.turn_settings(conversation_id)?;
        if agent == Some(AgentId::Kiro)
            && self
                .store
                .record(conversation_id)?
                .is_some_and(|record| record.thread_id.is_some())
        {
            if requested != prior {
                return Err(AppError::InvalidArg(
                    "Kiro 会话创建后不能修改模型或思考强度，请新建对话".into(),
                ));
            }
            return Ok(prior);
        }
        let cache = self.load_catalog(conversation_id, false);
        let models = self.effective_models(&cache.models);
        let effective = ops::validate_turn_settings(&requested, &models, &prior)?;
        self.store.set_turn_settings(conversation_id, &effective)
    }

    pub fn note_thinking_failure(
        &self,
        conversation_id: &str,
        settings: RuntimeTurnSettings,
        error: &str,
    ) -> Result<()> {
        self.store.ensure_conversation(conversation_id)?;
        let _ = self
            .store
            .learn_denied_effort_from_error(&settings, error)?;
        Ok(())
    }

    pub fn start(
        self: &Arc<Self>,
        conversation_id: &str,
        prompt: &str,
        client_request_id: &str,
        extras: RuntimeStartExtras,
    ) -> Result<RuntimeSnapshot> {
        let prompt = prompt.trim();
        if prompt.is_empty() {
            return Err(log_and_return_send_fail(
                conversation_id,
                AppError::InvalidArg("prompt must not be empty".into()),
            ));
        }
        if client_request_id.trim().is_empty() {
            return Err(log_and_return_send_fail(
                conversation_id,
                AppError::InvalidArg("clientRequestId must not be empty".into()),
            ));
        }
        if let Err(error) = ops::validate_local_images(&extras.images) {
            return Err(log_and_return_send_fail(conversation_id, error));
        }
        if let Err(error) = self.store.enable_if_new(conversation_id) {
            return Err(log_and_return_send_fail(conversation_id, error));
        }
        let actor = match self.actor(conversation_id) {
            Ok(actor) => actor,
            Err(error) => return Err(log_and_return_send_fail(conversation_id, error)),
        };
        actor.abort.store(false, Ordering::SeqCst);
        // Cached lists can be checked on the caller thread. A cold catalog
        // fetch is owned by the actor so Cancel/Shutdown can preempt it.
        if let Some(cache) = self.peek_catalog(conversation_id) {
            let models = self.effective_models(&cache.models);
            let mut settings = match self.store.turn_settings(conversation_id) {
                Ok(settings) => settings,
                Err(error) => return Err(log_and_return_send_fail(conversation_id, error)),
            };
            if ops::settings_need_catalog_default(&settings, &models) {
                if let Some(defaults) = ops::default_turn_settings(&models) {
                    settings = match self.store.set_turn_settings(conversation_id, &defaults) {
                        Ok(settings) => settings,
                        Err(error) => return Err(log_and_return_send_fail(conversation_id, error)),
                    };
                }
            }
            if let Err(error) = ops::assert_settings_supported(&settings, &models) {
                return Err(log_and_return_send_fail(conversation_id, error));
            }
            if !extras.skills.is_empty() {
                if let Err(error) = ops::validate_skill_refs(&extras.skills, &cache.extensions) {
                    return Err(log_and_return_send_fail(conversation_id, error));
                }
            }
        }
        let operation =
            match self
                .store
                .begin_operation(conversation_id, "start", client_request_id, None)
            {
                Ok(state) => state,
                Err(error) => return Err(log_and_return_send_fail(conversation_id, error)),
            };
        match operation {
            OperationState::Accepted => return self.snapshot(conversation_id, None),
            OperationState::Pending => {
                return Err(log_and_return_send_fail(
                    conversation_id,
                    operation_replay_error("start", "pending"),
                ));
            }
            OperationState::Failed => {
                return Err(log_and_return_send_fail(
                    conversation_id,
                    operation_replay_error("start", "failed"),
                ));
            }
            OperationState::New => {}
        }
        if actor.abort.load(Ordering::SeqCst) {
            self.store.mark_operation(
                conversation_id,
                "start",
                client_request_id,
                OperationState::Failed,
                None,
            )?;
            return Err(log_and_return_send_fail(conversation_id, cancelled_error()));
        }
        let (tx, rx) = mpsc::sync_channel(1);
        let outcome = actor
            .tx
            .send(RuntimeCommand::Start {
                prompt: prompt.to_string(),
                client_request_id: client_request_id.to_string(),
                extras,
                result: tx,
            })
            .map_err(|_| {
                log_and_return_send_fail(
                    conversation_id,
                    AppError::message("chat.runtime", "runtime worker stopped"),
                )
            })
            .and_then(|_| recv_result(rx));
        match &outcome {
            Ok(snapshot) => self.store.mark_operation(
                conversation_id,
                "start",
                client_request_id,
                OperationState::Accepted,
                snapshot.run_id.as_deref(),
            )?,
            Err(_) => self.store.mark_operation(
                conversation_id,
                "start",
                client_request_id,
                OperationState::Failed,
                None,
            )?,
        }
        outcome
    }

    pub fn reply(&self, reply: RuntimeReply) -> Result<()> {
        if reply.client_request_id.trim().is_empty() {
            return Err(AppError::InvalidArg(
                "clientRequestId must not be empty".into(),
            ));
        }
        match self.store.begin_operation(
            &reply.conversation_id,
            "reply",
            &reply.client_request_id,
            Some(&reply.run_id),
        )? {
            OperationState::Accepted => return Ok(()),
            OperationState::Pending => {
                return Err(operation_replay_error("reply", "pending"));
            }
            OperationState::Failed => {
                return Err(operation_replay_error("reply", "failed"));
            }
            OperationState::New => {}
        }
        let conversation_id = reply.conversation_id.clone();
        let run_id = reply.run_id.clone();
        let client_request_id = reply.client_request_id.clone();
        let actor = match self.actor_for_existing(&reply.conversation_id) {
            Ok(actor) => actor,
            Err(error) => {
                self.store.mark_operation(
                    &conversation_id,
                    "reply",
                    &client_request_id,
                    OperationState::Failed,
                    Some(&run_id),
                )?;
                return Err(error);
            }
        };
        let (tx, rx) = mpsc::sync_channel(1);
        let outcome = actor
            .tx
            .send(RuntimeCommand::Reply { reply, result: tx })
            .map_err(|_| AppError::message("chat.runtime", "runtime worker stopped"))
            .and_then(|_| recv_result(rx));
        self.store.mark_operation(
            &conversation_id,
            "reply",
            &client_request_id,
            if outcome.is_ok() {
                OperationState::Accepted
            } else {
                OperationState::Failed
            },
            Some(&run_id),
        )?;
        outcome
    }

    pub fn steer(
        &self,
        conversation_id: &str,
        run_id: &str,
        prompt: &str,
        client_request_id: &str,
    ) -> Result<()> {
        let prompt = prompt.trim();
        if prompt.is_empty() {
            return Err(AppError::InvalidArg("prompt must not be empty".into()));
        }
        if client_request_id.trim().is_empty() {
            return Err(AppError::InvalidArg(
                "clientRequestId must not be empty".into(),
            ));
        }
        match self.store.begin_operation(
            conversation_id,
            "steer",
            client_request_id,
            Some(run_id),
        )? {
            OperationState::Accepted => return Ok(()),
            OperationState::Pending => {
                return Err(operation_replay_error("steer", "pending"));
            }
            OperationState::Failed => {
                return Err(operation_replay_error("steer", "failed"));
            }
            OperationState::New => {}
        }
        let actor = match self.actor_for_existing(conversation_id) {
            Ok(actor) => actor,
            Err(error) => {
                self.store.mark_operation(
                    conversation_id,
                    "steer",
                    client_request_id,
                    OperationState::Failed,
                    Some(run_id),
                )?;
                return Err(error);
            }
        };
        let (tx, rx) = mpsc::sync_channel(1);
        let outcome = actor
            .tx
            .send(RuntimeCommand::Steer {
                prompt: prompt.to_string(),
                run_id: run_id.to_string(),
                client_request_id: client_request_id.to_string(),
                result: tx,
            })
            .map_err(|_| AppError::message("chat.runtime", "runtime worker stopped"))
            .and_then(|_| recv_result(rx));
        self.store.mark_operation(
            conversation_id,
            "steer",
            client_request_id,
            if outcome.is_ok() {
                OperationState::Accepted
            } else {
                OperationState::Failed
            },
            Some(run_id),
        )?;
        outcome
    }

    pub fn cancel(&self, conversation_id: &str, run_id: &str) -> Result<()> {
        let actor = match self.actor_for_existing(conversation_id) {
            Ok(actor) => actor,
            Err(error) => return Err(log_and_return_stop_fail(conversation_id, error)),
        };
        actor.abort.store(true, Ordering::SeqCst);
        let (tx, rx) = mpsc::sync_channel(1);
        if actor
            .tx
            .send(RuntimeCommand::Cancel {
                run_id: run_id.to_string(),
                result: tx,
            })
            .is_err()
        {
            return Err(log_and_return_stop_fail(
                conversation_id,
                AppError::message("chat.runtime", "runtime worker stopped"),
            ));
        }
        recv_result(rx)
    }

    /// Stop and forget the in-process owner before a conversation is deleted.
    /// The worker owns process teardown and will not receive any new events
    /// after the shutdown command has been acknowledged by the worker.
    pub(crate) fn shutdown(&self, conversation_id: &str) {
        if let Ok(mut actors) = self.actors.lock() {
            if let Some(actor) = actors.remove(conversation_id) {
                actor.abort.store(true, Ordering::SeqCst);
                let (done_tx, done_rx) = mpsc::sync_channel(1);
                if actor
                    .tx
                    .send(RuntimeCommand::Shutdown { done: done_tx })
                    .is_ok()
                {
                    // Wait until the worker confirms process teardown. A timeout
                    // here used to continue deleting the conversation while the
                    // child was still running.
                    let _ = done_rx.recv();
                }
            }
        }
    }

    fn peek_catalog(&self, conversation_id: &str) -> Option<CatalogCache> {
        self.catalogs
            .lock()
            .ok()
            .and_then(|guard| guard.get(conversation_id).cloned())
    }

    #[cfg(test)]
    pub(crate) fn set_codex_program_for_test(&self, path: PathBuf) {
        if let Ok(mut guard) = self.codex_program_override.lock() {
            *guard = Some(path);
        }
    }

    fn effective_models(&self, models: &[RuntimeModelOption]) -> Vec<RuntimeModelOption> {
        let denied = self.store.list_denied_efforts().unwrap_or_default();
        ops::apply_denied_efforts(models, &denied)
    }

    fn load_catalog(&self, conversation_id: &str, refresh: bool) -> CatalogCache {
        let phase = self
            .store
            .record(conversation_id)
            .ok()
            .flatten()
            .map(|record| record.phase);
        let frozen = !ops::may_fetch_catalog(phase);
        // Idle refresh after a login change must not reuse the previous account's list.
        // Frozen turns still serve the warmed cache and never spawn.
        if !refresh || frozen {
            if let Ok(guard) = self.catalogs.lock() {
                if let Some(cache) = guard.get(conversation_id) {
                    return cache.clone();
                }
            }
            if frozen {
                return CatalogCache::default();
            }
        }
        let fetched = self.fetch_catalog(conversation_id);
        if let Ok(mut guard) = self.catalogs.lock() {
            let fetched = merge_catalog_cache(guard.get(conversation_id), fetched);
            guard.insert(conversation_id.to_string(), fetched.clone());
            fetched
        } else {
            fetched
        }
    }

    #[cfg(test)]
    pub(crate) fn seed_catalog_cache_for_test(
        &self,
        conversation_id: &str,
        models: Vec<RuntimeModelOption>,
        extensions: Vec<RuntimeExtensionItem>,
    ) {
        if let Ok(mut guard) = self.catalogs.lock() {
            guard.insert(
                conversation_id.to_string(),
                CatalogCache {
                    models,
                    extensions,
                    from_codex: true,
                    ..CatalogCache::default()
                },
            );
        }
    }

    #[cfg(test)]
    pub(crate) fn seed_native_commands_for_test(
        &self,
        conversation_id: &str,
        commands: Vec<RuntimeNativeCommand>,
    ) {
        if let Ok(mut guard) = self.catalogs.lock() {
            let entry = guard.entry(conversation_id.to_string()).or_default();
            if entry.native_commands != commands {
                entry.catalog_epoch = entry.catalog_epoch.saturating_add(1);
            }
            entry.native_commands = commands;
        }
    }

    #[cfg(test)]
    pub(crate) fn seed_image_input_for_test(&self, conversation_id: &str, image_input: bool) {
        if let Ok(mut guard) = self.catalogs.lock() {
            let entry = guard.entry(conversation_id.to_string()).or_default();
            if entry.image_input != Some(image_input) {
                entry.catalog_epoch = entry.catalog_epoch.saturating_add(1);
            }
            entry.image_input = Some(image_input);
        }
    }

    fn fetch_catalog(&self, conversation_id: &str) -> CatalogCache {
        let Ok(Some(conversation)) = self.repo.get_conversation(conversation_id) else {
            return CatalogCache::default();
        };
        let Ok(cwd) = crate::services::chat_cwd::resolve_runtime_cwd(conversation.cwd.as_deref())
        else {
            return CatalogCache::default();
        };
        match conversation.agent_ids.first().copied() {
            Some(AgentId::Grok) => return self.fetch_grok_catalog(&cwd),
            Some(AgentId::Kiro) => return fetch_kiro_catalog(),
            Some(AgentId::Claude) => return claude_fallback_catalog(),
            _ => {}
        }
        let Ok(program) = resolve_codex_program(&self.run, &self.codex_program_override) else {
            return CatalogCache::default();
        };
        let mut transport = match CodexTransport::spawn(&program, &cwd) {
            Ok(t) => t,
            Err(_) => return CatalogCache::default(),
        };
        let models = transport
            .request("model/list", json!({}), CODEX_REQUEST_TIMEOUT)
            .ok()
            .map(|value| ops::parse_model_list(&value))
            .unwrap_or_default();
        let mut extensions = transport
            .request("skills/list", json!({}), CODEX_REQUEST_TIMEOUT)
            .ok()
            .map(|value| ops::parse_skills_list(&value))
            .unwrap_or_default();
        if let Ok(plugins) = transport.request("plugin/installed", json!({}), CODEX_REQUEST_TIMEOUT)
        {
            extensions.extend(ops::parse_plugins_installed(&plugins));
        }
        transport.shutdown();
        CatalogCache {
            models,
            extensions,
            from_codex: true,
            ..CatalogCache::default()
        }
    }

    fn fetch_grok_catalog(&self, cwd: &PathBuf) -> CatalogCache {
        let Ok(program) = self.run.detect_grok_installation() else {
            return grok_fallback_catalog();
        };
        let mut transport = match CodexTransport::spawn_grok(&program, cwd, None, None) {
            Ok(t) => t,
            Err(_) => return grok_fallback_catalog(),
        };
        let models = transport
            .request("_x.ai/models/list", json!({}), CODEX_REQUEST_TIMEOUT)
            .ok()
            .map(|value| ops::parse_grok_model_list(&value))
            .unwrap_or_default();
        transport.shutdown();
        let models = ops::ensure_grok_catalog_efforts(models);
        if models.is_empty() {
            return grok_fallback_catalog();
        }
        CatalogCache {
            models,
            extensions: Vec::new(),
            from_codex: true,
            ..CatalogCache::default()
        }
    }

    fn actor(&self, conversation_id: &str) -> Result<ActorHandle> {
        let mut actors = self
            .actors
            .lock()
            .map_err(|_| AppError::message("chat.runtime.lock", "runtime actor map poisoned"))?;
        if let Some(actor) = actors.get(conversation_id) {
            return Ok(actor.clone());
        }
        let (tx, rx) = mpsc::sync_channel(32);
        let store = self.store.clone();
        let repo = self.repo.clone();
        let run = Arc::clone(&self.run);
        let catalogs = Arc::clone(&self.catalogs);
        let codex_program_override = Arc::clone(&self.codex_program_override);
        let abort = Arc::new(AtomicBool::new(false));
        let worker_abort = Arc::clone(&abort);
        let id = conversation_id.to_string();
        thread::Builder::new()
            .name(format!("agenthub-chat-runtime-{conversation_id}"))
            .spawn(move || {
                actor_loop(
                    id,
                    rx,
                    store,
                    repo,
                    run,
                    catalogs,
                    codex_program_override,
                    worker_abort,
                )
            })
            .map_err(AppError::from)?;
        let actor = ActorHandle { tx, abort };
        actors.insert(conversation_id.to_string(), actor.clone());
        Ok(actor)
    }

    fn actor_for_existing(&self, conversation_id: &str) -> Result<ActorHandle> {
        let actors = self
            .actors
            .lock()
            .map_err(|_| AppError::message("chat.runtime.lock", "runtime actor map poisoned"))?;
        actors.get(conversation_id).cloned().ok_or_else(|| {
            AppError::message(
                "chat.runtime.interrupted",
                "runtime is no longer connected; start a new turn to continue",
            )
        })
    }
}

impl Drop for ChatRuntime {
    fn drop(&mut self) {
        if let Ok(actors) = self.actors.lock() {
            for actor in actors.values() {
                actor.abort.store(true, Ordering::SeqCst);
                let (done_tx, _done_rx) = mpsc::sync_channel(1);
                let _ = actor
                    .tx
                    .try_send(RuntimeCommand::Shutdown { done: done_tx });
            }
        }
    }
}

fn recv_result<T>(rx: Receiver<Result<T>>) -> Result<T> {
    rx.recv()
        .map_err(|_| AppError::message("chat.runtime", "runtime worker stopped"))?
}

fn operation_replay_error(kind: &str, status: &str) -> AppError {
    AppError::message(
        "chat.runtime.idempotency",
        format!(
            "{kind} request was already recorded with status {status}; use a new clientRequestId"
        ),
    )
}

fn actor_loop(
    conversation_id: String,
    rx: Receiver<RuntimeCommand>,
    store: RuntimeStore,
    repo: ChatRepo,
    run: Arc<RunService>,
    catalogs: Arc<Mutex<HashMap<String, CatalogCache>>>,
    codex_program_override: Arc<Mutex<Option<PathBuf>>>,
    abort: Arc<AtomicBool>,
) {
    let agent = store
        .conversation_agent(&conversation_id)
        .ok()
        .flatten()
        .unwrap_or(AgentId::Codex);
    let mut worker = ActorWorker {
        conversation_id,
        rx,
        store,
        repo,
        run,
        catalogs,
        codex_program_override,
        abort,
        agent,
        transport: None,
        thread_id: None,
        turn_id: None,
        chat_turn: None,
        message_id: None,
        run_id: None,
        last_start_request: None,
        pending_prompt_id: None,
        permission_options: HashMap::new(),
        file_change_items: HashMap::new(),
        cancel_deadline: None,
        session_model: None,
        session_effort: None,
        session_trust_all: None,
        session_allow_always: false,
        pending_fs_writes: HashMap::new(),
        thinking_open: false,
    };
    worker.run();
}

struct ActorWorker {
    conversation_id: String,
    rx: Receiver<RuntimeCommand>,
    store: RuntimeStore,
    repo: ChatRepo,
    run: Arc<RunService>,
    catalogs: Arc<Mutex<HashMap<String, CatalogCache>>>,
    codex_program_override: Arc<Mutex<Option<PathBuf>>>,
    abort: Arc<AtomicBool>,
    agent: AgentId,
    transport: Option<CodexTransport>,
    thread_id: Option<String>,
    turn_id: Option<String>,
    chat_turn: Option<i64>,
    message_id: Option<String>,
    run_id: Option<String>,
    last_start_request: Option<String>,
    pending_prompt_id: Option<Value>,
    permission_options: HashMap<String, Vec<RuntimePermissionOption>>,
    /// File-change rows from `item/started` / `patchUpdated`, keyed by item id.
    /// `item/fileChange/requestApproval` often omits the file list and snippet.
    file_change_items: HashMap<String, Vec<RuntimeFileChange>>,
    cancel_deadline: Option<Instant>,
    session_model: Option<String>,
    session_effort: Option<String>,
    session_trust_all: Option<bool>,
    /// After the user picks session remember, later command/file approvals in
    /// this conversation are accepted without another card. Not written to
    /// SQLite; a new conversation asks again. Codex synthesizes the button
    /// and is told `acceptForSession`. Grok / Kiro `session/request_permission`
    /// only show it when the ACP request includes an `allow_always` kind
    /// (including `allow_always_tool`). Host-owned Grok `fs/write_text_file`
    /// cards synthesize the same three buttons as Codex. Codex starts a fresh
    /// process each turn, so the flag must outlive `terminalize`. ACP usually
    /// keeps one process across turns.
    session_allow_always: bool,
    /// Full write body for in-flight `fs/write_text_file` cards, keyed by the
    /// JSON-RPC request id. Content is not persisted; a dead process cannot
    /// complete the write anyway.
    pending_fs_writes: HashMap<String, (PathBuf, String)>,
    thinking_open: bool,
}

impl ActorWorker {
    fn run(&mut self) {
        self.restore_record();
        loop {
            match self.rx.recv_timeout(CODEX_POLL_INTERVAL) {
                Ok(RuntimeCommand::Start {
                    prompt,
                    client_request_id,
                    extras,
                    result,
                }) => {
                    let outcome = self.start_turn(&prompt, &client_request_id, &extras);
                    let _ = result.send(outcome);
                }
                Ok(RuntimeCommand::Reply { reply, result }) => {
                    let outcome = self.reply(reply);
                    let _ = result.send(outcome);
                }
                Ok(RuntimeCommand::Steer {
                    prompt,
                    run_id,
                    client_request_id,
                    result,
                }) => {
                    let outcome = self.steer(&prompt, &run_id, &client_request_id);
                    let _ = result.send(outcome);
                }
                Ok(RuntimeCommand::Cancel { run_id, result }) => {
                    let outcome = self.cancel(&run_id);
                    let _ = result.send(outcome);
                }
                Ok(RuntimeCommand::Shutdown { done }) => {
                    if let Some(transport) = self.transport.as_mut() {
                        transport.shutdown();
                    }
                    let _ = done.send(());
                    break;
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
            if let Err(error) = self.check_cancel_deadline() {
                self.fail_runtime(error);
            }
            if !self.aborted() {
                if let Err(error) = self.poll_events() {
                    self.fail_runtime(error);
                }
            }
        }
        if let Some(transport) = self.transport.as_mut() {
            transport.shutdown();
        }
    }

    fn restore_record(&mut self) {
        let Ok(Some(record)) = self.store.record(&self.conversation_id) else {
            return;
        };
        self.thread_id = record.thread_id;
        self.turn_id = record.turn_id;
        self.chat_turn = record.chat_turn;
        self.message_id = record.message_id;
        self.run_id = record.run_id;
        self.last_start_request = record.last_client_request_id;
        if let Ok(pending) = self.store.pending_wire_requests(&self.conversation_id) {
            self.permission_options = pending
                .into_iter()
                .filter(|item| !item.request.permission_options.is_empty())
                .map(|item| (item.request.id.clone(), item.request.permission_options))
                .collect();
        }
    }

    fn aborted(&self) -> bool {
        self.abort.load(Ordering::SeqCst)
    }

    fn codex_program(&self) -> Result<PathBuf> {
        resolve_codex_program(&self.run, &self.codex_program_override)
    }

    fn spawn_codex(&self, program: &Path, cwd: &Path) -> Result<CodexTransport> {
        CodexTransport::spawn_interruptible(program, cwd, Arc::clone(&self.abort))
            .map_err(|error| map_transport(self.agent, error))
    }

    fn ensure_start_catalog(&mut self) -> Result<CatalogCache> {
        if let Ok(guard) = self.catalogs.lock() {
            if let Some(cache) = guard.get(&self.conversation_id) {
                return Ok(cache.clone());
            }
        }
        if self.aborted() {
            return Err(cancelled_error());
        }
        let fetched = self.fetch_start_catalog();
        if self.aborted() {
            return Err(cancelled_error());
        }
        if let Ok(mut guard) = self.catalogs.lock() {
            let fetched = merge_catalog_cache(guard.get(&self.conversation_id), fetched);
            guard.insert(self.conversation_id.clone(), fetched.clone());
            return Ok(fetched);
        }
        Ok(fetched)
    }

    fn fetch_start_catalog(&mut self) -> CatalogCache {
        let Ok(Some(conversation)) = self.repo.get_conversation(&self.conversation_id) else {
            return CatalogCache::default();
        };
        let Ok(cwd) = crate::services::chat_cwd::resolve_runtime_cwd(conversation.cwd.as_deref())
        else {
            return CatalogCache::default();
        };
        match conversation.agent_ids.first().copied() {
            Some(AgentId::Grok) => return self.fetch_grok_start_catalog(&cwd),
            Some(AgentId::Kiro) => return fetch_kiro_catalog(),
            Some(AgentId::Claude) => return claude_fallback_catalog(),
            _ => {}
        }
        let Ok(program) = self.codex_program() else {
            return CatalogCache::default();
        };
        if self.aborted() {
            return CatalogCache::default();
        }
        let mut transport =
            match CodexTransport::spawn_interruptible(&program, &cwd, Arc::clone(&self.abort)) {
                Ok(t) => t,
                Err(_) => return CatalogCache::default(),
            };
        let models = transport
            .request("model/list", json!({}), CODEX_REQUEST_TIMEOUT)
            .ok()
            .map(|value| ops::parse_model_list(&value))
            .unwrap_or_default();
        let mut extensions = transport
            .request("skills/list", json!({}), CODEX_REQUEST_TIMEOUT)
            .ok()
            .map(|value| ops::parse_skills_list(&value))
            .unwrap_or_default();
        if let Ok(plugins) = transport.request("plugin/installed", json!({}), CODEX_REQUEST_TIMEOUT)
        {
            extensions.extend(ops::parse_plugins_installed(&plugins));
        }
        transport.shutdown();
        CatalogCache {
            models,
            extensions,
            from_codex: true,
            ..CatalogCache::default()
        }
    }

    fn fetch_grok_start_catalog(&mut self, cwd: &PathBuf) -> CatalogCache {
        let Ok(program) = self.run.detect_grok_installation() else {
            return grok_fallback_catalog();
        };
        if self.aborted() {
            return CatalogCache::default();
        }
        let mut transport = match CodexTransport::spawn_grok_interruptible(
            &program,
            cwd,
            None,
            None,
            false,
            Arc::clone(&self.abort),
        ) {
            Ok(t) => t,
            Err(_) => return grok_fallback_catalog(),
        };
        let models = transport
            .request("_x.ai/models/list", json!({}), CODEX_REQUEST_TIMEOUT)
            .ok()
            .map(|value| ops::parse_grok_model_list(&value))
            .unwrap_or_default();
        transport.shutdown();
        let models = ops::ensure_grok_catalog_efforts(models);
        if models.is_empty() {
            return grok_fallback_catalog();
        }
        CatalogCache {
            models,
            extensions: Vec::new(),
            from_codex: true,
            ..CatalogCache::default()
        }
    }

    fn check_cancel_deadline(&mut self) -> Result<()> {
        let Some(deadline) = self.cancel_deadline else {
            return Ok(());
        };
        if Instant::now() < deadline {
            return Ok(());
        }
        self.cancel_deadline = None;
        if let Some(transport) = self.transport.as_mut() {
            transport.shutdown();
        }
        self.transport = None;
        let message = if self.agent == AgentId::Kiro {
            "取消请求超时，当前对话已中断，请新建对话"
        } else {
            "取消请求超时，已中断当前生成"
        };
        logging::log_chat_error(
            "stop_fail",
            &self.conversation_id,
            Some(self.agent.as_str()),
            None,
            message,
        );
        self.terminalize(
            ChatMessageStatus::Cancelled,
            Some(message),
            RuntimePhase::Interrupted,
            false,
            true,
        )
    }

    fn start_turn(
        &mut self,
        prompt: &str,
        client_request_id: &str,
        extras: &RuntimeStartExtras,
    ) -> Result<RuntimeSnapshot> {
        let previous_chat_turn = self.chat_turn;
        match self.start_turn_inner(prompt, client_request_id, extras) {
            Ok(snapshot) => Ok(snapshot),
            Err(error) if is_cancelled_error(&error) => Err(error),
            Err(error) => {
                // A duplicate/active-run validation error must leave the
                // existing owner untouched. Once begin_turn succeeds, this
                // invocation owns a new chat row and must close it on error.
                if self.chat_turn != previous_chat_turn {
                    let message = error.to_string();
                    self.fail_runtime(AppError::message("chat.runtime", message));
                } else {
                    self.log_send_fail(&error);
                }
                Err(error)
            }
        }
    }

    fn start_turn_inner(
        &mut self,
        prompt: &str,
        client_request_id: &str,
        extras: &RuntimeStartExtras,
    ) -> Result<RuntimeSnapshot> {
        let record = self
            .store
            .record(&self.conversation_id)?
            .ok_or_else(|| AppError::NotFound("runtime conversation not found".into()))?;
        if !record.enabled {
            return Err(AppError::Unsupported("持续聊天未启用".into()));
        }
        if self.thread_id.is_none() {
            self.thread_id = record.thread_id.clone();
        }
        if self.last_start_request.as_deref() == Some(client_request_id) {
            return Ok(self.with_catalog_epoch(self.store.snapshot(&self.conversation_id, None)?));
        }
        if matches!(
            record.phase,
            RuntimePhase::Starting
                | RuntimePhase::Running
                | RuntimePhase::Waiting
                | RuntimePhase::Cancelling
        ) {
            return Err(AppError::InvalidArg(
                "conversation already has an active runtime turn".into(),
            ));
        }
        ops::validate_local_images(&extras.images)?;
        if (is_acp_runtime_agent(Some(self.agent))
            || is_claude_stream_runtime_agent(Some(self.agent)))
            && !extras.skills.is_empty()
        {
            return Err(AppError::Unsupported("目前不能在本轮指定 Skill".into()));
        }
        let acp_prompt_blocks = if is_acp_runtime_agent(Some(self.agent)) {
            Some(ops::grok_prompt_blocks(prompt, &extras.images)?)
        } else {
            None
        };
        let claude_prompt = if is_claude_stream_runtime_agent(Some(self.agent)) {
            Some(ops::claude_user_message(prompt, &extras.images)?)
        } else {
            None
        };
        if self.agent == AgentId::Kiro
            && self.thread_id.is_some()
            && self.transport.as_ref().is_some_and(CodexTransport::is_open)
        {
            let settings = self.store.turn_settings(&self.conversation_id)?;
            let model = settings
                .model
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty());
            let effort = settings
                .effort
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty());
            let trust_all = self
                .repo
                .get_conversation(&self.conversation_id)?
                .is_some_and(|conversation| conversation.allow_dangerous);
            if self.session_model.as_deref() != model
                || self.session_effort.as_deref() != effort
                || self.session_trust_all != Some(trust_all)
            {
                return Err(AppError::message(
                    "chat.runtime.settings",
                    "Kiro 会话设置已固定，请新建对话后修改模型、思考强度或权限",
                ));
            }
        }
        let cache = self.ensure_start_catalog()?;
        let models = {
            let denied = self.store.list_denied_efforts().unwrap_or_default();
            ops::apply_denied_efforts(&cache.models, &denied)
        };
        let mut settings = self.store.turn_settings(&self.conversation_id)?;
        if ops::settings_need_catalog_default(&settings, &models) {
            if let Some(defaults) = ops::default_turn_settings(&models) {
                settings = self
                    .store
                    .set_turn_settings(&self.conversation_id, &defaults)?;
            }
        }
        ops::assert_settings_supported(&settings, &models)?;
        if !extras.skills.is_empty() {
            ops::validate_skill_refs(&extras.skills, &cache.extensions)?;
        }
        if self.aborted() {
            return Err(cancelled_error());
        }
        self.last_start_request = Some(client_request_id.to_string());
        self.store
            .set_last_client_request_id(&self.conversation_id, client_request_id)?;
        if !is_acp_runtime_agent(Some(self.agent))
            && !is_claude_stream_runtime_agent(Some(self.agent))
        {
            if let Some(mut transport) = self.transport.take() {
                transport.shutdown();
            }
        }

        let now = Utc::now().to_rfc3339();
        let message_id = format!("msg-{}", Uuid::new_v4());
        let mut user = ChatMessage {
            id: format!("msg-{}", Uuid::new_v4()),
            conversation_id: self.conversation_id.clone(),
            turn: 0,
            role: ChatRole::User,
            agent_id: None,
            content: prompt.to_string(),
            status: ChatMessageStatus::Ok,
            exit_code: None,
            duration_ms: 0,
            error: None,
            created_at: now.clone(),
        };
        let mut agent = ChatMessage {
            id: message_id.clone(),
            conversation_id: self.conversation_id.clone(),
            turn: 0,
            role: ChatRole::Agent,
            agent_id: Some(self.agent),
            content: String::new(),
            status: ChatMessageStatus::Running,
            exit_code: None,
            duration_ms: 0,
            error: None,
            created_at: now,
        };
        let run_id = format!("run-{}", Uuid::new_v4());
        let thread_id = self.thread_id.clone();
        let chat_turn = self.store.begin_turn(
            &self.conversation_id,
            &mut user,
            &mut agent,
            &run_id,
            thread_id.as_deref(),
            |turn| {
                vec![
                    ChatEvent::Started {
                        turn,
                        agents: vec![self.agent],
                    },
                    ChatEvent::AgentStarted {
                        turn,
                        agent: self.agent,
                        command: match self.agent {
                            AgentId::Grok => "grok agent stdio".into(),
                            AgentId::Kiro => "kiro-cli acp".into(),
                            AgentId::Claude => "claude stream-json".into(),
                            _ => "codex app-server".into(),
                        },
                    },
                ]
            },
        )?;
        self.chat_turn = Some(chat_turn);
        self.message_id = Some(message_id);
        self.turn_id = None;
        self.run_id = Some(run_id);
        self.clear_turn_plan();
        logging::log_chat_info(
            "send",
            &self.conversation_id,
            Some(self.agent.as_str()),
            "send start",
        );

        let start_result = if is_acp_runtime_agent(Some(self.agent)) {
            self.acp_connect_and_prompt(acp_prompt_blocks.unwrap_or_default())
        } else if is_claude_stream_runtime_agent(Some(self.agent)) {
            self.claude_connect_and_prompt(claude_prompt.expect("claude prompt prepared"))
        } else {
            (|| {
                let cwd = self.conversation_cwd()?;
                let program = self.codex_program()?;
                let mut transport = if let Some(thread_id) = self.thread_id.as_deref() {
                    // The transport itself is always a fresh process; thread/resume
                    // reattaches it to Codex's durable native thread.
                    let mut t = self.spawn_codex(&program, &cwd)?;
                    let result = t
                        .request(
                            "thread/resume",
                            json!({
                                "threadId": thread_id,
                                "cwd": cwd.to_string_lossy(),
                                "approvalPolicy": "on-request",
                                "sandbox": "workspace-write",
                                "sandboxPolicy": ops::codex_workspace_write_sandbox_policy(&cwd)
                            }),
                            CODEX_REQUEST_TIMEOUT,
                        )
                        .map_err(|error| map_transport(self.agent, error))?;
                    self.thread_id = Some(thread_id.to_string());
                    let _ = result;
                    t
                } else {
                    let mut t = self.spawn_codex(&program, &cwd)?;
                    let result = t
                        .request(
                            "thread/start",
                            json!({
                                "cwd": cwd.to_string_lossy(),
                                "approvalPolicy": "on-request",
                                "sandbox": "workspace-write",
                                "sandboxPolicy": ops::codex_workspace_write_sandbox_policy(&cwd),
                                "ephemeral": false
                            }),
                            CODEX_REQUEST_TIMEOUT,
                        )
                        .map_err(|error| map_transport(self.agent, error))?;
                    self.thread_id =
                        extract_id(&result, "thread").or_else(|| extract_id(&result, "id"));
                    t
                };
                let thread_id = self.thread_id.clone().ok_or_else(|| {
                    AppError::message("chat.runtime.protocol", "Codex omitted thread id")
                })?;
                let settings = self.store.turn_settings(&self.conversation_id)?;
                let input = ops::build_turn_input(prompt, &extras.images, &extras.skills)?;
                let mut params = json!({
                    "threadId": thread_id,
                    "input": input,
                    "clientUserMessageId": client_request_id,
                    "cwd": cwd.to_string_lossy(),
                    "approvalPolicy": "on-request",
                    "sandboxPolicy": ops::codex_workspace_write_sandbox_policy(&cwd)
                });
                if let Some(model) = settings
                    .model
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                {
                    params["model"] = json!(model);
                }
                if let Some(effort) = settings
                    .effort
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                {
                    params["effort"] = json!(effort);
                }
                let result = transport
                    .request("turn/start", params, CODEX_REQUEST_TIMEOUT)
                    .map_err(|error| map_transport(self.agent, error))?;
                self.turn_id = extract_id(&result, "turn").or_else(|| extract_id(&result, "id"));
                let actual_run = self.turn_id.clone().unwrap_or_else(|| {
                    self.run_id
                        .clone()
                        .unwrap_or_else(|| format!("run-{}", Uuid::new_v4()))
                });
                self.run_id = Some(actual_run.clone());
                self.store.set_state(
                    &self.conversation_id,
                    RuntimePhase::Running,
                    Some(&actual_run),
                    self.thread_id.as_deref(),
                    self.turn_id.as_deref(),
                    self.chat_turn,
                    self.message_id.as_deref(),
                )?;
                self.transport = Some(transport);
                Ok(self.with_catalog_epoch(self.store.snapshot(&self.conversation_id, None)?))
            })()
        };
        if let Err(error) = start_result {
            if is_cancelled_error(&error) {
                let _ = self.terminalize(
                    ChatMessageStatus::Cancelled,
                    Some("已取消"),
                    RuntimePhase::Cancelled,
                    false,
                    true,
                );
            } else {
                let message = redact_text(&error.to_string());
                let _ = self.terminalize(
                    ChatMessageStatus::Failed,
                    Some(&message),
                    RuntimePhase::Failed,
                    false,
                    false,
                );
            }
            if let Some(transport) = self.transport.as_mut() {
                transport.shutdown();
            }
            self.transport = None;
            return Err(error);
        }
        start_result
    }

    fn acp_connect_and_prompt(&mut self, prompt_blocks: Vec<Value>) -> Result<RuntimeSnapshot> {
        let cwd = self.conversation_cwd()?;
        let settings = self.store.turn_settings(&self.conversation_id)?;
        let model = settings
            .model
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty());
        let effort = settings
            .effort
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty());
        let trust_all = self
            .repo
            .get_conversation(&self.conversation_id)
            .ok()
            .flatten()
            .is_some_and(|conversation| conversation.allow_dangerous);

        let mut retried_after_exit = false;
        loop {
            let live = self.transport.as_ref().is_some_and(CodexTransport::is_open);
            let plan = ops::acp_session_plan(self.agent, live, self.thread_id.is_some());
            if matches!(plan, ops::AcpSessionPlan::Unavailable) {
                return Err(AppError::message(
                    "chat.runtime.interrupted",
                    "Kiro 会话所在进程已停止，请新建对话后继续",
                ));
            }
            if self.agent == AgentId::Kiro
                && matches!(plan, ops::AcpSessionPlan::PromptExisting)
                && (self.session_model.as_deref() != model
                    || self.session_effort.as_deref() != effort
                    || self.session_trust_all != Some(trust_all))
            {
                return Err(AppError::message(
                    "chat.runtime.settings",
                    "Kiro 会话设置已固定，请新建对话后修改模型、思考强度或权限",
                ));
            }
            let mut transport = if matches!(plan, ops::AcpSessionPlan::PromptExisting) {
                self.transport.take().ok_or_else(|| {
                    AppError::message("chat.runtime.transport", "ACP process stopped")
                })?
            } else {
                if let Some(mut previous) = self.transport.take() {
                    previous.shutdown();
                }
                match self.agent {
                    AgentId::Kiro => {
                        let program = self.run.detect_kiro_installation()?;
                        CodexTransport::spawn_kiro_interruptible(
                            &program,
                            &cwd,
                            model,
                            effort,
                            trust_all,
                            Arc::clone(&self.abort),
                        )
                    }
                    _ => {
                        let program = self.run.detect_grok_installation()?;
                        CodexTransport::spawn_grok_interruptible(
                            &program,
                            &cwd,
                            model,
                            effort,
                            trust_all,
                            Arc::clone(&self.abort),
                        )
                    }
                }
                .map_err(|error| map_transport(self.agent, error))?
            };
            self.apply_initialize_capabilities(transport.initialize_result());

            match plan {
                ops::AcpSessionPlan::PromptExisting => {}
                ops::AcpSessionPlan::New => {
                    let created = transport
                        .request(
                            "session/new",
                            if self.agent == AgentId::Grok {
                                ops::grok_session_new_params(&cwd, trust_all)
                            } else {
                                ops::acp_session_new_params(&cwd)
                            },
                            CODEX_REQUEST_TIMEOUT,
                        )
                        .map_err(|error| map_transport(self.agent, error))?;
                    self.thread_id =
                        grok_session_id(&created).or_else(|| extract_id(&created, "session"));
                    self.apply_session_model_catalog(&created);
                    if self.agent == AgentId::Kiro {
                        self.session_model = model.map(str::to_owned);
                        self.session_effort = effort.map(str::to_owned);
                        self.session_trust_all = Some(trust_all);
                    }
                }
                ops::AcpSessionPlan::LoadThenPrompt => {
                    let session_id = self.thread_id.clone().ok_or_else(|| {
                        AppError::message("chat.runtime.protocol", "session id omitted")
                    })?;
                    let load_params = json!({
                        "sessionId": session_id,
                        "cwd": cwd.to_string_lossy(),
                        "mcpServers": []
                    });
                    let capabilities = transport.initialize_result().cloned().unwrap_or_default();
                    let agent_capabilities = capabilities
                        .get("agentCapabilities")
                        .or_else(|| capabilities.get("capabilities"))
                        .unwrap_or(&capabilities);
                    let supports_resume = agent_capabilities
                        .get("sessionCapabilities")
                        .and_then(|value| value.get("resume"))
                        .is_some_and(|value| acp_capability_present(Some(value)))
                        || acp_capability_present(agent_capabilities.get("resume"));
                    let supports_load = agent_capabilities
                        .get("loadSession")
                        .is_some_and(|value| acp_capability_present(Some(value)));
                    let method = if supports_resume {
                        "session/resume"
                    } else if supports_load {
                        "session/load"
                    } else {
                        return Err(AppError::message(
                            "chat.runtime.protocol",
                            "Grok 不支持恢复已有会话，请新建对话",
                        ));
                    };
                    let value = transport
                        .request_discarding_history(method, load_params, CODEX_REQUEST_TIMEOUT)
                        .map_err(|error| map_transport(self.agent, error))?;
                    if let Some(id) = grok_session_id(&value) {
                        self.thread_id = Some(id);
                    }
                }
                ops::AcpSessionPlan::Unavailable => unreachable!("handled above"),
            }

            let session_id = self
                .thread_id
                .clone()
                .ok_or_else(|| AppError::message("chat.runtime.protocol", "session id omitted"))?;
            let prompt_params = ops::acp_session_prompt_params(&session_id, prompt_blocks.clone());
            match transport.begin_request("session/prompt", prompt_params) {
                Ok(prompt_id) => {
                    self.pending_prompt_id = Some(prompt_id);
                    let run_id = self
                        .run_id
                        .clone()
                        .unwrap_or_else(|| format!("run-{}", Uuid::new_v4()));
                    self.turn_id = Some(run_id.clone());
                    self.run_id = Some(run_id.clone());
                    self.store.set_state(
                        &self.conversation_id,
                        RuntimePhase::Running,
                        Some(&run_id),
                        self.thread_id.as_deref(),
                        self.turn_id.as_deref(),
                        self.chat_turn,
                        self.message_id.as_deref(),
                    )?;
                    self.transport = Some(transport);
                    return Ok(self.with_catalog_epoch(self.store.snapshot(&self.conversation_id, None)?));
                }
                Err(codex_transport::CodexTransportError::Exited)
                    if matches!(plan, ops::AcpSessionPlan::PromptExisting)
                        && self.agent != AgentId::Kiro
                        && !retried_after_exit =>
                {
                    retried_after_exit = true;
                    self.transport = None;
                    continue;
                }
                Err(error) => return Err(map_transport(self.agent, error)),
            }
        }
    }

    fn claude_connect_and_prompt(&mut self, prompt: Value) -> Result<RuntimeSnapshot> {
        let cwd = self.conversation_cwd()?;
        let settings = self.store.turn_settings(&self.conversation_id)?;
        let model = settings
            .model
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty());
        let effort = settings
            .effort
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty());
        let trust_all = self
            .repo
            .get_conversation(&self.conversation_id)
            .ok()
            .flatten()
            .is_some_and(|conversation| conversation.allow_dangerous);
        let permission_mode = if trust_all {
            "bypassPermissions"
        } else {
            "dontAsk"
        };

        let live = self.transport.as_ref().is_some_and(CodexTransport::is_open);
        let mut transport = if live {
            self.transport.take().ok_or_else(|| {
                AppError::message("chat.runtime.transport", "Claude process stopped")
            })?
        } else {
            if let Some(mut previous) = self.transport.take() {
                previous.shutdown();
            }
            let program = self.run.detect_claude_installation()?;
            let resume = self.thread_id.as_deref();
            CodexTransport::spawn_claude_stream_interruptible(
                &program,
                &cwd,
                model,
                effort,
                permission_mode,
                resume,
                Arc::clone(&self.abort),
            )
            .map_err(|error| map_transport(self.agent, error))?
        };

        transport
            .send_raw_value(&prompt)
            .map_err(|error| map_transport(self.agent, error))?;

        let run_id = self
            .run_id
            .clone()
            .unwrap_or_else(|| format!("run-{}", Uuid::new_v4()));
        self.turn_id = Some(run_id.clone());
        self.run_id = Some(run_id.clone());
        self.store.set_state(
            &self.conversation_id,
            RuntimePhase::Running,
            Some(&run_id),
            self.thread_id.as_deref(),
            self.turn_id.as_deref(),
            self.chat_turn,
            self.message_id.as_deref(),
        )?;
        self.transport = Some(transport);
        Ok(self.with_catalog_epoch(self.store.snapshot(&self.conversation_id, None)?))
    }

    fn claude_stream_event(&mut self, params: &Value) -> Result<()> {
        if let Some(session_id) = params
            .get("session_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            if self.thread_id.as_deref() != Some(session_id) {
                self.thread_id = Some(session_id.to_string());
                self.store.set_state(
                    &self.conversation_id,
                    self.live_phase(RuntimePhase::Running),
                    self.run_id.as_deref(),
                    self.thread_id.as_deref(),
                    self.turn_id.as_deref(),
                    self.chat_turn,
                    self.message_id.as_deref(),
                )?;
            }
        }

        let ty = params.get("type").and_then(Value::as_str).unwrap_or("");
        if ty == "result" {
            let is_err = params
                .get("is_error")
                .and_then(Value::as_bool)
                .unwrap_or(false)
                || params
                    .get("subtype")
                    .and_then(Value::as_str)
                    .is_some_and(|s| s.contains("error"));
            if let Some(steps) = crate::utils::stream_parse::claude::parse_line(&params.to_string())
            {
                for step in steps {
                    match step {
                        ProcessStep::Text { text } => {
                            if !text.is_empty() {
                                // Prefer assistant text already streamed; result text is fallback.
                                let Some(message) = self.current_message()? else {
                                    continue;
                                };
                                if message.content.is_empty() {
                                    self.append_message(
                                        &text,
                                        self.live_phase(RuntimePhase::Running),
                                    )?;
                                }
                            }
                        }
                        other => {
                            self.emit(
                                ChatEvent::AgentProcess {
                                    turn: self.chat_turn.unwrap_or(0),
                                    agent: self.agent,
                                    step: other,
                                },
                                self.live_phase(RuntimePhase::Running),
                            )?;
                        }
                    }
                }
            }
            if is_err {
                let message = params
                    .get("error")
                    .and_then(Value::as_str)
                    .or_else(|| params.get("result").and_then(Value::as_str))
                    .unwrap_or("Claude 运行失败");
                self.terminalize(
                    ChatMessageStatus::Failed,
                    Some(&redact_text(message)),
                    RuntimePhase::Failed,
                    false,
                    false,
                )?;
            } else {
                let cancel_requested = self
                    .store
                    .record(&self.conversation_id)?
                    .is_some_and(|record| record.phase == RuntimePhase::Cancelling)
                    || self.cancel_deadline.is_some();
                if cancel_requested {
                    self.terminalize(
                        ChatMessageStatus::Cancelled,
                        None,
                        RuntimePhase::Cancelled,
                        false,
                        true,
                    )?;
                } else {
                    self.terminalize(
                        ChatMessageStatus::Ok,
                        None,
                        RuntimePhase::Completed,
                        true,
                        false,
                    )?;
                }
            }
            // Keep the process alive for the next user turn.
            return Ok(());
        }

        let Some(steps) = crate::utils::stream_parse::claude::parse_line(&params.to_string())
        else {
            return Ok(());
        };
        for step in steps {
            match step {
                ProcessStep::Text { text } => {
                    if !text.is_empty() {
                        self.append_message(&text, self.live_phase(RuntimePhase::Running))?;
                    }
                }
                other => {
                    self.emit(
                        ChatEvent::AgentProcess {
                            turn: self.chat_turn.unwrap_or(0),
                            agent: self.agent,
                            step: other,
                        },
                        self.live_phase(RuntimePhase::Running),
                    )?;
                }
            }
        }
        Ok(())
    }

    fn reply(&mut self, reply: RuntimeReply) -> Result<()> {
        self.check_run(&reply.run_id)?;
        if self
            .store
            .reply_was_recorded(&reply.conversation_id, &reply.client_request_id)?
        {
            return Ok(());
        }
        let persisted = self
            .store
            .request(&reply.conversation_id, &reply.request_id)?
            .ok_or_else(|| {
                AppError::NotFound("runtime request not found or already resolved".into())
            })?;
        if persisted.request.run_id != reply.run_id {
            return Err(AppError::InvalidArg(
                "runtime request belongs to another run".into(),
            ));
        }
        if reply.client_request_id.trim().is_empty() {
            return Err(AppError::InvalidArg(
                "clientRequestId must not be empty".into(),
            ));
        }
        if matches!(
            persisted.request.kind,
            RuntimeRequestKind::Command | RuntimeRequestKind::File
        ) && ops::is_acp_fs_write_method(&persisted.server_method)
        {
            let decision = match reply.decision {
                Some(RuntimeDecision::Allow) => "accept",
                Some(RuntimeDecision::AllowAlways) => "accept_always",
                Some(RuntimeDecision::Deny) => "decline",
                None => {
                    return Err(AppError::InvalidArg("approval decision is required".into()));
                }
            };
            if reply
                .answers
                .as_ref()
                .is_some_and(|values| !values.is_empty())
            {
                return Err(AppError::InvalidArg(
                    "approval cannot include answers".into(),
                ));
            }
            return self.reply_acp_fs_write(&reply, &persisted, decision);
        }
        let answers = reply.answers.filter(|values| !values.is_empty());
        let value = match persisted.request.kind {
            RuntimeRequestKind::Command | RuntimeRequestKind::File => {
                let decision = match reply.decision {
                    Some(RuntimeDecision::Allow) => "accept",
                    Some(RuntimeDecision::AllowAlways) => "accept_always",
                    Some(RuntimeDecision::Deny) => "decline",
                    None => {
                        return Err(AppError::InvalidArg("approval decision is required".into()));
                    }
                };
                if answers.is_some() {
                    return Err(AppError::InvalidArg(
                        "approval cannot include answers".into(),
                    ));
                }
                if is_acp_runtime_agent(Some(self.agent)) {
                    let options = if persisted.request.permission_options.is_empty() {
                        self.permission_options
                            .get(&persisted.request.id)
                            .cloned()
                            .unwrap_or_default()
                    } else {
                        persisted.request.permission_options.clone()
                    };
                    let value = acp_permission_reply(&options, decision)?;
                    if matches!(reply.decision, Some(RuntimeDecision::AllowAlways)) {
                        self.session_allow_always = true;
                    }
                    value
                } else {
                    if matches!(reply.decision, Some(RuntimeDecision::AllowAlways)) {
                        self.session_allow_always = true;
                    }
                    json!({"decision": codex_approval_decision(decision)})
                }
            }
            RuntimeRequestKind::Question => {
                if reply.decision.is_some() {
                    return Err(AppError::InvalidArg(
                        "question reply cannot include decision".into(),
                    ));
                }
                let answers = answers
                    .ok_or_else(|| AppError::InvalidArg("question answers are required".into()))?;
                validate_answers(&persisted.request.questions, &answers)?;
                let answers = answers
                    .into_iter()
                    .map(|(id, values)| (id, json!({"answers": values})))
                    .collect::<serde_json::Map<_, _>>();
                json!({"answers": answers})
            }
        };
        let transport = self.transport.as_mut().ok_or_else(|| {
            AppError::message(
                "chat.runtime.interrupted",
                format!("{} 已退出", runtime_process_label(self.agent)),
            )
        })?;
        let server_id = parse_wire_id(&persisted.server_id)?;
        // Resolve the durable control before writing the JSON-RPC response.
        // If the process dies after this point, a retry with the same client
        // id cannot answer the server request twice.  A transport failure is
        // terminalized below so the unresolved wire request cannot leave the
        // runtime waiting forever.
        self.store.record_reply(
            &reply.conversation_id,
            &reply.run_id,
            &reply.client_request_id,
            &reply.request_id,
        )?;
        let response_result = transport
            .respond(server_id, Ok(value))
            .map_err(|error| transport_error(self.agent, error));
        if let Err(error) = response_result {
            self.fail_runtime(AppError::message(
                "chat.runtime.transport",
                error.to_string(),
            ));
            return Err(error);
        }
        self.permission_options.remove(&persisted.request.id);
        let phase = if self
            .store
            .snapshot(&self.conversation_id, None)?
            .pending_requests
            .is_empty()
        {
            RuntimePhase::Running
        } else {
            RuntimePhase::Waiting
        };
        self.store.set_state(
            &self.conversation_id,
            phase,
            self.run_id.as_deref(),
            self.thread_id.as_deref(),
            self.turn_id.as_deref(),
            self.chat_turn,
            self.message_id.as_deref(),
        )?;
        Ok(())
    }

    fn steer(&mut self, prompt: &str, run_id: &str, client_request_id: &str) -> Result<()> {
        if is_acp_runtime_agent(Some(self.agent))
            || is_claude_stream_runtime_agent(Some(self.agent))
        {
            return Err(AppError::Unsupported(
                "不能在生成过程中补充要求，请等本轮结束后再发送".into(),
            ));
        }
        self.check_run(run_id)?;
        if client_request_id.trim().is_empty() {
            return Err(AppError::InvalidArg(
                "clientRequestId must not be empty".into(),
            ));
        }
        if self
            .store
            .record(&self.conversation_id)?
            .and_then(|record| record.last_steer_client_request_id)
            .as_deref()
            == Some(client_request_id)
        {
            return Ok(());
        }
        let thread_id = self.thread_id.clone().ok_or_else(|| {
            AppError::message("chat.runtime.interrupted", "Codex thread is unavailable")
        })?;
        let turn_id = self
            .turn_id
            .clone()
            .ok_or_else(|| AppError::message("chat.runtime", "Codex turn is unavailable"))?;
        let transport = self.transport.as_mut().ok_or_else(|| {
            AppError::message(
                "chat.runtime.interrupted",
                format!("{} 已退出", runtime_process_label(self.agent)),
            )
        })?;
        transport
            .request(
                "turn/steer",
                json!({
                    "threadId": thread_id,
                    "expectedTurnId": turn_id,
                    "input": [{"type": "text", "text": prompt}],
                    "clientUserMessageId": client_request_id
                }),
                CODEX_REQUEST_TIMEOUT,
            )
            .map_err(|error| transport_error(self.agent, error))?;
        self.store
            .set_last_steer_client_request_id(&self.conversation_id, client_request_id)?;
        let turn = self.repo.next_turn(&self.conversation_id)?;
        let message = ChatMessage {
            id: format!("msg-{}", Uuid::new_v4()),
            conversation_id: self.conversation_id.clone(),
            turn,
            role: ChatRole::User,
            agent_id: None,
            content: prompt.to_string(),
            status: ChatMessageStatus::Ok,
            exit_code: None,
            duration_ms: 0,
            error: None,
            created_at: Utc::now().to_rfc3339(),
        };
        self.repo.insert_message(&message)?;
        Ok(())
    }

    fn cancel(&mut self, run_id: &str) -> Result<()> {
        let record = self
            .store
            .record(&self.conversation_id)?
            .ok_or_else(|| AppError::NotFound("runtime conversation not found".into()))?;
        if !matches!(
            record.phase,
            RuntimePhase::Starting
                | RuntimePhase::Running
                | RuntimePhase::Waiting
                | RuntimePhase::Cancelling
        ) {
            return Ok(());
        }
        if run_id.trim().is_empty() {
            if let Some(current) = record.run_id.clone().or_else(|| self.run_id.clone()) {
                return self.cancel(&current);
            }
            if self.chat_turn.is_some() {
                self.terminalize(
                    ChatMessageStatus::Cancelled,
                    Some("已取消"),
                    RuntimePhase::Cancelled,
                    false,
                    true,
                )?;
                if let Some(transport) = self.transport.as_mut() {
                    transport.shutdown();
                }
                self.transport = None;
                self.log_stop_ok();
            }
            return Ok(());
        }
        self.check_run(run_id)?;
        self.store.set_state(
            &self.conversation_id,
            RuntimePhase::Cancelling,
            Some(run_id),
            self.thread_id.as_deref().or(record.thread_id.as_deref()),
            self.turn_id.as_deref().or(record.turn_id.as_deref()),
            self.chat_turn.or(record.chat_turn),
            self.message_id.as_deref().or(record.message_id.as_deref()),
        )?;
        // Keep the wire ids long enough to reject the server-side requests
        // explicitly.  Remove the durable controls before returning to the
        // UI so a late reply cannot race the cancellation.
        let pending = self.store.pending_wire_requests(&self.conversation_id)?;
        self.store.clear_requests(&self.conversation_id)?;
        let thread_id = self.thread_id.clone().or(record.thread_id);
        let turn_id = self.turn_id.clone().or(record.turn_id);
        if self.transport.is_none() {
            self.terminalize(
                ChatMessageStatus::Cancelled,
                Some("runtime interrupted before interrupt was sent"),
                RuntimePhase::Interrupted,
                false,
                true,
            )?;
            self.log_stop_ok();
            return Ok(());
        }
        if is_claude_stream_runtime_agent(Some(self.agent)) {
            if let Some(transport) = self.transport.as_mut() {
                transport.shutdown();
            }
            self.transport = None;
            return self.finish_user_stop();
        }
        let (Some(thread_id), Some(turn_id)) = (thread_id, turn_id) else {
            return self.complete_cancel(AppError::message(
                "chat.runtime",
                "Codex turn is unavailable",
            ));
        };
        for request in pending {
            let response = parse_wire_id(&request.server_id).and_then(|server_id| {
                let response = if request.server_method == "session/request_permission" {
                    Ok(json!({
                        "outcome": { "outcome": "cancelled" }
                    }))
                } else {
                    Err(json!({ "code": -32800, "message": "cancelled" }))
                };
                self.transport
                    .as_mut()
                    .ok_or_else(|| {
                        AppError::message(
                            "chat.runtime.interrupted",
                            format!("{} 已退出", runtime_process_label(self.agent)),
                        )
                    })?
                    .respond(server_id, response)
                    .map_err(|error| map_transport(self.agent, error))
            });
            if let Err(error) = response {
                return self.complete_cancel(error);
            }
        }
        let interrupt_result = self
            .transport
            .as_mut()
            .ok_or_else(|| {
                AppError::message(
                    "chat.runtime.interrupted",
                    format!("{} 已退出", runtime_process_label(self.agent)),
                )
            })
            .and_then(|transport| {
                if is_acp_runtime_agent(Some(self.agent)) {
                    transport
                        .notify("session/cancel", Some(json!({"sessionId": thread_id})))
                        .map_err(|error| map_transport(self.agent, error))
                } else {
                    transport
                        .request(
                            "turn/interrupt",
                            json!({"threadId": thread_id, "turnId": turn_id}),
                            CODEX_REQUEST_TIMEOUT,
                        )
                        .map(|_| ())
                        .map_err(|error| map_transport(self.agent, error))
                }
            });
        match interrupt_result {
            Ok(_) if is_acp_runtime_agent(Some(self.agent)) && !self.aborted() => {
                self.cancel_deadline = Some(Instant::now() + ACP_CANCEL_DEADLINE);
                self.log_stop_ok();
                Ok(())
            }
            Ok(_) if self.aborted() => self.finish_user_stop(),
            Ok(_) => {
                self.log_stop_ok();
                Ok(())
            }
            Err(error) => self.complete_cancel(error),
        }
    }

    fn poll_events(&mut self) -> Result<()> {
        for index in 0..64 {
            if self.aborted() {
                return Ok(());
            }
            let Some(transport) = self.transport.as_mut() else {
                return Ok(());
            };
            let event = if index == 0 {
                transport.recv_timeout(CODEX_POLL_INTERVAL)
            } else {
                transport.try_recv()
            };
            let event = match event {
                Ok(event) => event,
                Err(codex_transport::CodexTransportError::Interrupted) => return Ok(()),
                Err(error) => return Err(transport_error(self.agent, error)),
            };
            match event {
                Some(CodexEvent::Request { id, method, params }) => {
                    self.server_request(id, &method, &params)?;
                }
                Some(CodexEvent::Notification { method, params }) => {
                    self.notification(&method, &params)?;
                }
                Some(CodexEvent::Exited) => {
                    self.transport = None;
                    let phase = self.store.record(&self.conversation_id)?.map(|r| r.phase);
                    if matches!(
                        phase,
                        Some(
                            RuntimePhase::Starting
                                | RuntimePhase::Running
                                | RuntimePhase::Waiting
                                | RuntimePhase::Cancelling
                        )
                    ) {
                        self.terminalize(
                            ChatMessageStatus::Cancelled,
                            Some(&format!("{} 已退出", runtime_process_label(self.agent))),
                            RuntimePhase::Interrupted,
                            false,
                            true,
                        )?;
                    }
                    return Ok(());
                }
                Some(CodexEvent::Response { id, result, error }) => {
                    if self.pending_prompt_id.as_ref() == Some(&id) {
                        self.pending_prompt_id = None;
                        if let Some(error) = error {
                            let message = redact_json_text(Some(&error));
                            self.terminalize(
                                ChatMessageStatus::Failed,
                                Some(&message),
                                RuntimePhase::Failed,
                                false,
                                false,
                            )?;
                            if let Some(transport) = self.transport.as_mut() {
                                transport.shutdown();
                            }
                            self.transport = None;
                        } else {
                            self.turn_completed(
                                &result.unwrap_or(json!({ "status": "completed" })),
                            )?;
                        }
                    }
                }
                None => return Ok(()),
            }
        }
        Ok(())
    }

    fn server_request(&mut self, id: Value, method: &str, params: &Value) -> Result<()> {
        let id_string = wire_id_string(&id);
        let phase = self
            .store
            .record(&self.conversation_id)?
            .map(|record| record.phase);
        let session_mismatch = params
            .get("sessionId")
            .and_then(Value::as_str)
            .zip(self.thread_id.as_deref())
            .is_some_and(|(incoming, current)| incoming != current);
        if session_mismatch {
            if let Some(transport) = self.transport.as_mut() {
                transport
                    .respond(
                        id,
                        Err(json!({ "code": -32001, "message": "stale session" })),
                    )
                    .map_err(|error| transport_error(self.agent, error))?;
            }
            return Ok(());
        }
        if phase == Some(RuntimePhase::Cancelling) {
            if let Some(transport) = self.transport.as_mut() {
                let response = if method == "session/request_permission" {
                    Ok(json!({
                        "outcome": { "outcome": "cancelled" }
                    }))
                } else {
                    Err(json!({ "code": -32800, "message": "cancelled" }))
                };
                transport
                    .respond(id, response)
                    .map_err(|error| transport_error(self.agent, error))?;
            }
            return Ok(());
        }
        if is_acp_runtime_agent(Some(self.agent))
            && !matches!(
                phase,
                Some(RuntimePhase::Starting | RuntimePhase::Running | RuntimePhase::Waiting)
            )
        {
            if let Some(transport) = self.transport.as_mut() {
                let response = if method == "session/request_permission" {
                    Ok(json!({
                        "outcome": { "outcome": "cancelled" }
                    }))
                } else {
                    Err(json!({ "code": -32800, "message": "cancelled" }))
                };
                transport
                    .respond(id, response)
                    .map_err(|error| transport_error(self.agent, error))?;
            }
            return Ok(());
        }
        if ops::is_acp_fs_read_method(method) {
            return self.handle_acp_fs_read(id, params);
        }
        if ops::is_acp_fs_write_method(method) {
            return self.handle_acp_fs_write(id, params);
        }
        let run_id = params
            .get("turnId")
            .and_then(Value::as_str)
            .or(self.run_id.as_deref())
            .ok_or_else(|| {
                AppError::message("chat.runtime.protocol", "Codex request omitted run id")
            })?
            .to_string();
        if self
            .run_id
            .as_deref()
            .is_some_and(|current| current != run_id)
        {
            if let Some(transport) = self.transport.as_mut() {
                transport
                    .respond(id, Err(json!({"code": -32001, "message": "stale turn"})))
                    .map_err(|error| transport_error(self.agent, error))?;
            }
            return Ok(());
        }
        let (kind, title, detail, questions, acp_options, file_changes) = match method {
            "session/request_permission" => {
                let file_changes = file_change::extract_file_changes(params);
                let is_file = !file_changes.is_empty() || acp_tool_is_file_change(params);
                let title = if is_file {
                    "修改文件".to_string()
                } else {
                    params
                        .pointer("/toolCall/title")
                        .or_else(|| params.pointer("/toolCall/kind"))
                        .and_then(Value::as_str)
                        .unwrap_or("需要确认")
                        .to_string()
                };
                let detail = if is_file {
                    self.file_change_request_detail(params)
                } else {
                    redact_json_text(
                        params
                            .get("toolCall")
                            .and_then(|call| call.get("rawInput").or_else(|| call.get("title"))),
                    )
                };
                (
                    if is_file {
                        RuntimeRequestKind::File
                    } else {
                        RuntimeRequestKind::Command
                    },
                    title,
                    detail,
                    Vec::new(),
                    acp_permission_options(params),
                    file_changes,
                )
            }
            "item/commandExecution/requestApproval" | "execCommandApproval" => (
                RuntimeRequestKind::Command,
                "执行命令".to_string(),
                redact_json_text(params.get("command").or_else(|| params.get("reason"))),
                Vec::new(),
                acp_permission_options(params),
                Vec::new(),
            ),
            "item/fileChange/requestApproval" | "fileChangeApproval" | "applyPatchApproval" => (
                RuntimeRequestKind::File,
                "修改文件".to_string(),
                self.file_change_request_detail(params),
                Vec::new(),
                acp_permission_options(params),
                self.file_change_request_changes(params),
            ),
            "item/tool/requestUserInput" => (
                RuntimeRequestKind::Question,
                "需要你的回答".to_string(),
                String::new(),
                parse_questions(params.get("questions")),
                None,
                Vec::new(),
            ),
            _ => {
                // Unknown server requests must never be auto-approved. Reply
                // with a protocol error and surface a safe runtime error.
                if let Some(transport) = self.transport.as_mut() {
                    transport
                        .respond(
                            id,
                            Err(json!({"code": -32601, "message": "unsupported request"})),
                        )
                        .map_err(|error| transport_error(self.agent, error))?;
                }
                self.emit_error(
                    &format!(
                        "{} 请求暂不支持：{method}",
                        runtime_process_label(self.agent)
                    ),
                    self.live_phase(RuntimePhase::Running),
                )?;
                return Ok(());
            }
        };
        if self.session_allow_always
            && matches!(kind, RuntimeRequestKind::Command | RuntimeRequestKind::File)
        {
            if is_acp_runtime_agent(Some(self.agent)) {
                let options = acp_options.clone().unwrap_or_default();
                if let Some(response) = acp_auto_allow_response(&options) {
                    if let Some(transport) = self.transport.as_mut() {
                        transport
                            .respond(id, Ok(response))
                            .map_err(|error| transport_error(self.agent, error))?;
                    }
                    return Ok(());
                }
            } else if let Some(transport) = self.transport.as_mut() {
                transport
                    .respond(id, Ok(json!({"decision": "acceptForSession"})))
                    .map_err(|error| transport_error(self.agent, error))?;
                return Ok(());
            }
        }
        let permission_options = match acp_options {
            Some(options) => options,
            None if !is_acp_runtime_agent(Some(self.agent))
                && matches!(kind, RuntimeRequestKind::Command | RuntimeRequestKind::File) =>
            {
                codex_session_permission_options()
            }
            None => Vec::new(),
        };
        let request = RuntimeRequest {
            id: id_string.clone(),
            run_id,
            kind,
            title,
            detail,
            questions,
            permission_options: permission_options.clone(),
            file_changes,
        };
        self.store
            .add_request(&self.conversation_id, &request, method, &id_string)?;
        if !permission_options.is_empty() {
            self.permission_options
                .insert(id_string, permission_options);
        }
        Ok(())
    }

    fn notification(&mut self, method: &str, params: &Value) -> Result<()> {
        match method {
            "claude/stream" => {
                self.claude_stream_event(params)?;
            }
            "session/update" | "session_update" | "_x.ai/session/update" => {
                self.grok_session_update(params)?;
            }
            "_x.ai/session/prompt_complete" => {
                self.turn_completed(params)?;
            }
            "turn/started" => {
                if let Some(id) = extract_id(params, "turn").or_else(|| {
                    params
                        .get("turnId")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                }) {
                    self.turn_id = Some(id.clone());
                    self.run_id = Some(id.clone());
                    self.store.set_state(
                        &self.conversation_id,
                        RuntimePhase::Running,
                        Some(&id),
                        self.thread_id.as_deref(),
                        self.turn_id.as_deref(),
                        self.chat_turn,
                        self.message_id.as_deref(),
                    )?;
                }
            }
            "item/agentMessage/delta" => {
                let text = params
                    .get("delta")
                    .and_then(Value::as_str)
                    .or_else(|| params.get("text").and_then(Value::as_str))
                    .unwrap_or_default();
                if !text.is_empty() {
                    self.append_message(text, self.live_phase(RuntimePhase::Running))?;
                }
            }
            "item/reasoning/summaryTextDelta" | "item/reasoning/textDelta" => {
                let text = params
                    .get("delta")
                    .and_then(Value::as_str)
                    .or_else(|| params.get("text").and_then(Value::as_str))
                    .unwrap_or_default();
                if !text.is_empty() {
                    self.emit(
                        ChatEvent::AgentProcess {
                            turn: self.chat_turn.unwrap_or(0),
                            agent: AgentId::Codex,
                            step: ProcessStep::Thinking {
                                text: redact_text(text),
                                done: false,
                            },
                        },
                        self.live_phase(RuntimePhase::Running),
                    )?;
                }
            }
            "item/commandExecution/outputDelta" | "item/commandExecution/terminalOutputDelta" => {
                self.emit_command_output_delta(params)?;
            }
            "item/started" | "item/completed" | "item/updated" => {
                self.emit_codex_item(method, params.get("item"))?;
            }
            "item/fileChange/patchUpdated" => {
                self.remember_file_change_patch(params);
            }
            "thread/tokenUsage/updated" | "thread/token_usage/updated" => {
                self.emit_usage_steps(codex_usage_steps(params))?;
            }
            "turn/completed" => {
                self.emit_usage_steps(codex_usage_steps(params))?;
                self.turn_completed(params)?;
            }
            "error" => {
                let message =
                    redact_json_text(params.get("message").or_else(|| params.get("error")));
                let will_retry = params
                    .get("willRetry")
                    .or_else(|| params.get("error").and_then(|error| error.get("willRetry")))
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                if will_retry {
                    self.emit_error(&message, self.live_phase(RuntimePhase::Running))?;
                } else {
                    self.terminalize(
                        ChatMessageStatus::Failed,
                        Some(&message),
                        RuntimePhase::Failed,
                        false,
                        false,
                    )?;
                    if let Some(transport) = self.transport.as_mut() {
                        transport.shutdown();
                    }
                    self.transport = None;
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn turn_completed(&mut self, params: &Value) -> Result<()> {
        let status = params
            .get("status")
            .and_then(Value::as_str)
            .or_else(|| {
                params
                    .get("turn")
                    .and_then(|v| v.get("status"))
                    .and_then(Value::as_str)
            })
            .unwrap_or_default();
        let reason = params
            .get("stopReason")
            .or_else(|| params.get("stop_reason"))
            .or_else(|| params.get("turn").and_then(|v| v.get("stopReason")))
            .or_else(|| params.get("turn").and_then(|v| v.get("stop_reason")))
            .and_then(Value::as_str);
        let cancel_requested = self
            .store
            .record(&self.conversation_id)?
            .is_some_and(|record| record.phase == RuntimePhase::Cancelling)
            || self.cancel_deadline.is_some();
        if cancel_requested {
            self.terminalize(
                ChatMessageStatus::Cancelled,
                None,
                RuntimePhase::Cancelled,
                false,
                true,
            )?;
            return Ok(());
        }
        let completion = reason.or_else(|| (!status.is_empty()).then_some(status));
        let (message_status, phase, ok, cancelled, error) = match completion {
            Some("end_turn") | Some("completed") | Some("complete") => (
                ChatMessageStatus::Ok,
                RuntimePhase::Completed,
                true,
                false,
                None,
            ),
            Some("cancelled") | Some("canceled") | Some("interrupted") => (
                ChatMessageStatus::Cancelled,
                RuntimePhase::Cancelled,
                false,
                true,
                None,
            ),
            Some("failed") | Some("error") => (
                ChatMessageStatus::Failed,
                RuntimePhase::Failed,
                false,
                false,
                Some(redact_json_text(
                    params.get("error").or_else(|| params.get("message")),
                )),
            ),
            Some("max_tokens") | Some("max_turn_requests") | Some("refusal") => (
                ChatMessageStatus::Failed,
                RuntimePhase::Failed,
                false,
                false,
                Some(format!(
                    "ACP 生成未完成：{}",
                    reason.unwrap_or(completion.unwrap_or("unknown"))
                )),
            ),
            Some(other) => (
                ChatMessageStatus::Failed,
                RuntimePhase::Failed,
                false,
                false,
                Some(format!("ACP 返回未知结束原因：{other}")),
            ),
            None if !is_acp_runtime_agent(Some(self.agent)) => (
                ChatMessageStatus::Ok,
                RuntimePhase::Completed,
                true,
                false,
                None,
            ),
            None => (
                ChatMessageStatus::Failed,
                RuntimePhase::Failed,
                false,
                false,
                Some("ACP 响应缺少 stopReason".into()),
            ),
        };
        self.terminalize(message_status, error.as_deref(), phase, ok, cancelled)?;
        if !is_acp_runtime_agent(Some(self.agent)) {
            if let Some(transport) = self.transport.as_mut() {
                transport.shutdown();
            }
            self.transport = None;
        }
        Ok(())
    }

    fn emit(&self, event: ChatEvent, phase: RuntimePhase) -> Result<()> {
        self.store
            .commit_event(&self.conversation_id, phase, self.run_id.as_deref(), &event)?;
        Ok(())
    }

    fn emit_usage_steps(&self, steps: Vec<ProcessStep>) -> Result<()> {
        for step in steps {
            self.emit(
                ChatEvent::AgentProcess {
                    turn: self.chat_turn.unwrap_or(0),
                    agent: self.agent,
                    step,
                },
                self.live_phase(RuntimePhase::Running),
            )?;
        }
        Ok(())
    }

    fn emit_error(&self, message: &str, phase: RuntimePhase) -> Result<()> {
        self.emit(
            ChatEvent::Error {
                message: redact_text(message),
            },
            phase,
        )
    }

    fn file_change_request_changes(&self, params: &Value) -> Vec<RuntimeFileChange> {
        let from_params = file_change::extract_file_changes(params);
        if !from_params.is_empty() {
            return from_params;
        }
        if let Some(item_id) = params.get("itemId").and_then(Value::as_str) {
            if let Some(cached) = self.file_change_items.get(item_id) {
                if !cached.is_empty() {
                    return cached.clone();
                }
            }
        }
        Vec::new()
    }

    fn file_change_request_detail(&self, params: &Value) -> String {
        let paths: Vec<String> = self
            .file_change_request_changes(params)
            .into_iter()
            .map(|change| change.path)
            .collect();
        if !paths.is_empty() {
            return file_change::join_file_change_paths(&paths);
        }
        optional_json_text(params.get("reason"))
            .or_else(|| optional_json_text(params.get("grantRoot")))
            .unwrap_or_default()
    }

    fn emit_codex_item(&mut self, method: &str, item: Option<&Value>) -> Result<()> {
        let Some(item) = item else {
            return Ok(());
        };
        let ty = item.get("type").and_then(Value::as_str).unwrap_or_default();
        match ty {
            "fileChange" | "file_change" => self.remember_file_change_item(method, Some(item)),
            "commandExecution" | "command_execution" => {
                self.emit_command_execution_item(method, item)
            }
            "mcpToolCall" | "mcp_tool_call" => self.emit_mcp_tool_item(method, item),
            "reasoning" => self.emit_reasoning_item(method, item),
            _ => Ok(()),
        }
    }

    fn emit_command_execution_item(&self, method: &str, item: &Value) -> Result<()> {
        let id = item.get("id").and_then(Value::as_str).unwrap_or_default();
        let command =
            optional_json_text(item.get("command")).unwrap_or_else(|| "command".to_string());
        let result = optional_json_text(item.get("aggregatedOutput"))
            .or_else(|| optional_json_text(item.get("aggregated_output")));
        self.emit_codex_tool(
            (!id.is_empty()).then(|| id.to_string()),
            "command_execution",
            Some(json!({ "command": command })),
            item_status(method, item),
            result,
        )
    }

    fn emit_command_output_delta(&self, params: &Value) -> Result<()> {
        let text = params
            .get("delta")
            .and_then(Value::as_str)
            .or_else(|| params.get("output").and_then(Value::as_str))
            .unwrap_or_default();
        if text.is_empty() {
            return Ok(());
        }
        let id = params
            .get("itemId")
            .or_else(|| params.get("item_id"))
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        self.emit_codex_tool(
            id,
            "command_execution",
            None,
            "inProgress".to_string(),
            Some(redact_text(text)),
        )
    }

    fn emit_mcp_tool_item(&self, method: &str, item: &Value) -> Result<()> {
        let id = item.get("id").and_then(Value::as_str).unwrap_or_default();
        let name = item
            .get("tool")
            .and_then(Value::as_str)
            .or_else(|| item.get("name").and_then(Value::as_str))
            .unwrap_or("mcp")
            .to_string();
        let input = item
            .get("arguments")
            .cloned()
            .or_else(|| item.get("input").cloned());
        let result = optional_json_text(item.get("result"))
            .or_else(|| optional_json_text(item.get("error")));
        self.emit_codex_tool(
            (!id.is_empty()).then(|| id.to_string()),
            name,
            input,
            item_status(method, item),
            result,
        )
    }

    fn emit_reasoning_item(&self, method: &str, item: &Value) -> Result<()> {
        let text = optional_json_text(item.get("text"))
            .or_else(|| optional_json_text(item.get("summary")))
            .unwrap_or_default();
        if text.is_empty() && method != "item/completed" {
            return Ok(());
        }
        self.emit(
            ChatEvent::AgentProcess {
                turn: self.chat_turn.unwrap_or(0),
                agent: AgentId::Codex,
                step: ProcessStep::Thinking {
                    text: redact_text(&text),
                    done: method == "item/completed",
                },
            },
            self.live_phase(RuntimePhase::Running),
        )
    }

    fn emit_codex_tool(
        &self,
        id: Option<String>,
        name: impl Into<String>,
        input: Option<Value>,
        status: String,
        result: Option<String>,
    ) -> Result<()> {
        self.emit(
            ChatEvent::AgentProcess {
                turn: self.chat_turn.unwrap_or(0),
                agent: AgentId::Codex,
                step: ProcessStep::Tool {
                    id,
                    name: name.into(),
                    input,
                    status,
                    result,
                },
            },
            self.live_phase(RuntimePhase::Running),
        )
    }

    fn remember_file_change_item(&mut self, method: &str, item: Option<&Value>) -> Result<()> {
        let Some(item) = item else {
            return Ok(());
        };
        let ty = item.get("type").and_then(Value::as_str).unwrap_or_default();
        if ty != "fileChange" && ty != "file_change" {
            return Ok(());
        }
        let id = item.get("id").and_then(Value::as_str).unwrap_or_default();
        let changes = file_change::extract_file_changes(item);
        let paths: Vec<String> = changes.iter().map(|change| change.path.clone()).collect();
        if !id.is_empty() && !changes.is_empty() {
            self.file_change_items.insert(id.to_string(), changes);
        }
        let status =
            item.get("status")
                .and_then(Value::as_str)
                .unwrap_or(if method == "item/completed" {
                    "completed"
                } else {
                    "inProgress"
                });
        let path = paths.first().map(String::as_str).unwrap_or("file");
        self.emit(
            ChatEvent::AgentProcess {
                turn: self.chat_turn.unwrap_or(0),
                agent: AgentId::Codex,
                step: ProcessStep::Tool {
                    id: (!id.is_empty()).then(|| id.to_string()),
                    name: "fileChange".into(),
                    input: Some(json!({ "path": path })),
                    status: status.into(),
                    result: None,
                },
            },
            self.live_phase(RuntimePhase::Running),
        )
    }

    fn remember_file_change_patch(&mut self, params: &Value) {
        let Some(id) = params.get("itemId").and_then(Value::as_str) else {
            return;
        };
        let changes = file_change::extract_file_changes(params);
        if !changes.is_empty() {
            self.file_change_items.insert(id.to_string(), changes);
        }
    }

    fn grok_session_update(&mut self, params: &Value) -> Result<()> {
        if let Some(commands) = crate::utils::stream_parse::acp::extract_available_commands(params)
        {
            self.patch_catalog(|cache| {
                cache.native_commands = commands
                    .into_iter()
                    .map(|command| RuntimeNativeCommand {
                        name: command.name,
                        description: command.description,
                        hint: command.hint,
                    })
                    .collect();
            });
        }
        if let Some(catalog) = crate::utils::stream_parse::acp::extract_config_catalog(params) {
            self.apply_acp_config_catalog(catalog);
        }
        if let Some(entries) = crate::utils::stream_parse::acp::extract_plan(params) {
            self.apply_acp_plan(entries);
        }
        let envelope = json!({
            "jsonrpc": "2.0",
            "method": "session/update",
            "params": params,
        });
        let Some(steps) = crate::utils::stream_parse::grok::parse_line(&envelope.to_string())
        else {
            return Ok(());
        };
        for step in steps {
            match step {
                ProcessStep::Text { text } => {
                    if !text.is_empty() {
                        self.finish_open_thinking()?;
                        self.append_message(&text, self.live_phase(RuntimePhase::Running))?;
                    }
                }
                ProcessStep::Thinking { .. } => {
                    self.thinking_open = true;
                    self.emit(
                        ChatEvent::AgentProcess {
                            turn: self.chat_turn.unwrap_or(0),
                            agent: self.agent,
                            step,
                        },
                        self.live_phase(RuntimePhase::Running),
                    )?;
                }
                other => {
                    self.emit(
                        ChatEvent::AgentProcess {
                            turn: self.chat_turn.unwrap_or(0),
                            agent: self.agent,
                            step: other,
                        },
                        self.live_phase(RuntimePhase::Running),
                    )?;
                }
            }
        }
        Ok(())
    }

    fn patch_catalog(&self, patch: impl FnOnce(&mut CatalogCache)) {
        if let Ok(mut guard) = self.catalogs.lock() {
            let entry = guard.entry(self.conversation_id.clone()).or_default();
            let before_commands = entry.native_commands.clone();
            let before_image = entry.image_input;
            let before_models = entry.models.clone();
            patch(entry);
            if entry.native_commands != before_commands
                || entry.image_input != before_image
                || entry.models != before_models
            {
                entry.catalog_epoch = entry.catalog_epoch.saturating_add(1);
            }
        }
    }

    fn with_catalog_epoch(&self, mut snapshot: RuntimeSnapshot) -> RuntimeSnapshot {
        if let Some(cache) = self
            .catalogs
            .lock()
            .ok()
            .and_then(|guard| guard.get(&self.conversation_id).cloned())
        {
            snapshot.catalog_epoch = cache.catalog_epoch;
            snapshot.plan = cache.plan;
        }
        snapshot
    }

    fn apply_acp_plan(&self, entries: Vec<crate::utils::stream_parse::acp::AcpPlanEntry>) {
        if entries.is_empty() {
            return;
        }
        if let Ok(mut guard) = self.catalogs.lock() {
            guard.entry(self.conversation_id.clone()).or_default().plan = entries
                .into_iter()
                .map(|entry| RuntimePlanEntry {
                    content: entry.content,
                    status: entry.status,
                    priority: entry.priority,
                })
                .collect();
        }
    }

    fn clear_turn_plan(&self) {
        if let Ok(mut guard) = self.catalogs.lock() {
            if let Some(entry) = guard.get_mut(&self.conversation_id) {
                entry.plan.clear();
            }
        }
    }

    fn apply_acp_config_catalog(&self, catalog: crate::utils::stream_parse::acp::AcpConfigCatalog) {
        if catalog.models.is_empty() && catalog.efforts.is_empty() {
            return;
        }
        self.patch_catalog(|cache| {
            let efforts = if catalog.efforts.is_empty() {
                cache
                    .models
                    .first()
                    .map(|model| model.efforts.clone())
                    .unwrap_or_default()
            } else {
                catalog.efforts.clone()
            };
            let default_effort = catalog
                .current_effort
                .clone()
                .filter(|effort| efforts.iter().any(|item| item == effort))
                .or_else(|| {
                    cache
                        .models
                        .iter()
                        .find_map(|model| model.default_effort.clone())
                        .filter(|effort| efforts.iter().any(|item| item == effort))
                })
                .or_else(|| efforts.first().cloned());
            if !catalog.models.is_empty() {
                cache.models = catalog
                    .models
                    .into_iter()
                    .map(|id| RuntimeModelOption {
                        id,
                        efforts: efforts.clone(),
                        default_effort: default_effort.clone(),
                    })
                    .collect();
            } else if !efforts.is_empty() {
                for model in &mut cache.models {
                    model.efforts = efforts.clone();
                    if model
                        .default_effort
                        .as_ref()
                        .is_none_or(|effort| !efforts.contains(effort))
                    {
                        model.default_effort = default_effort.clone();
                    }
                }
            }
        });
    }

    fn apply_session_model_catalog(&self, created: &Value) {
        if let Some(catalog) = crate::utils::stream_parse::acp::extract_config_catalog(created) {
            self.apply_acp_config_catalog(catalog);
        }
        let grok_models = ops::parse_grok_model_list(created);
        if grok_models.is_empty() {
            return;
        }
        self.patch_catalog(|cache| {
            if cache.models.is_empty() {
                cache.models = grok_models;
            }
        });
    }

    fn apply_initialize_capabilities(&self, initialize: Option<&Value>) {
        let Some(initialize) = initialize else {
            return;
        };
        let capabilities = initialize
            .get("agentCapabilities")
            .or_else(|| initialize.get("capabilities"))
            .unwrap_or(initialize);
        let prompt = capabilities
            .get("promptCapabilities")
            .or_else(|| capabilities.get("prompt_capabilities"));
        let image = prompt
            .and_then(|value| value.get("image"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        self.patch_catalog(|cache| cache.image_input = Some(image));
    }

    fn finish_open_thinking(&mut self) -> Result<()> {
        if !self.thinking_open {
            return Ok(());
        }
        self.thinking_open = false;
        self.emit(
            ChatEvent::AgentProcess {
                turn: self.chat_turn.unwrap_or(0),
                agent: self.agent,
                step: ProcessStep::Thinking {
                    text: String::new(),
                    done: true,
                },
            },
            self.live_phase(RuntimePhase::Running),
        )
    }

    fn append_message(&self, text: &str, phase: RuntimePhase) -> Result<()> {
        let Some(message_id) = self.message_id.as_deref() else {
            return Ok(());
        };
        let Some(mut message) = self.current_message()? else {
            return Ok(());
        };
        message.content.push_str(text);
        message.id = message_id.to_string();
        let event = ChatEvent::AgentChunk {
            turn: self.chat_turn.unwrap_or(0),
            agent: self.agent,
            stream: OutputStream::Stdout,
            text: text.to_string(),
        };
        self.store.append_message_event(
            &self.conversation_id,
            &message,
            phase,
            self.run_id.as_deref(),
            &event,
        )?;
        Ok(())
    }

    fn current_message(&self) -> Result<Option<ChatMessage>> {
        let Some(id) = self.message_id.as_deref() else {
            return Ok(None);
        };
        Ok(self
            .repo
            .list_messages(&self.conversation_id)?
            .into_iter()
            .find(|message| message.id == id))
    }

    fn fail_runtime(&mut self, error: AppError) {
        let message = redact_text(&error.to_string());
        let _ = self.terminalize(
            ChatMessageStatus::Failed,
            Some(&message),
            RuntimePhase::Failed,
            false,
            false,
        );
        self.transport = None;
    }

    fn cancel_failed(&mut self, error: AppError) -> Result<()> {
        logging::log_chat_error(
            "stop_fail",
            &self.conversation_id,
            Some(self.agent.as_str()),
            Some(error.code()),
            &error.to_string(),
        );
        let message = redact_text(&error.to_string());
        self.terminalize(
            ChatMessageStatus::Cancelled,
            Some(&message),
            RuntimePhase::Interrupted,
            false,
            true,
        )?;
        if let Some(transport) = self.transport.as_mut() {
            transport.shutdown();
        }
        self.transport = None;
        Err(error)
    }

    /// User Stop while generating or waiting for approval. The abort flag is
    /// set before the cancel command, so `turn/interrupt` often returns
    /// `Interrupted` / `chat.runtime.transport`. That is a successful stop.
    fn complete_cancel(&mut self, error: AppError) -> Result<()> {
        if self.aborted() || is_user_stop_error(&error) {
            self.finish_user_stop()
        } else {
            self.cancel_failed(error)
        }
    }

    fn finish_user_stop(&mut self) -> Result<()> {
        self.terminalize(
            ChatMessageStatus::Cancelled,
            None,
            RuntimePhase::Cancelled,
            false,
            true,
        )?;
        if let Some(transport) = self.transport.as_mut() {
            transport.shutdown();
        }
        self.transport = None;
        self.log_stop_ok();
        Ok(())
    }

    fn terminalize(
        &mut self,
        status: ChatMessageStatus,
        error: Option<&str>,
        phase: RuntimePhase,
        ok: bool,
        cancelled: bool,
    ) -> Result<()> {
        let already_terminal = self
            .store
            .record(&self.conversation_id)
            .ok()
            .flatten()
            .is_some_and(|record| {
                matches!(
                    record.phase,
                    RuntimePhase::Completed
                        | RuntimePhase::Failed
                        | RuntimePhase::Cancelled
                        | RuntimePhase::Interrupted
                )
            });
        let _ = self.finish_open_thinking();
        self.cancel_deadline = None;
        self.pending_prompt_id = None;
        self.permission_options.clear();
        self.file_change_items.clear();
        self.pending_fs_writes.clear();
        if let Some(message) = error {
            if let Err(learn_err) = self
                .store
                .learn_thinking_unsupported(&self.conversation_id, message)
            {
                tracing::warn!(
                    error = %learn_err,
                    "failed to persist denied reasoning effort"
                );
            }
        }
        let mut agent_message = self.current_message()?;
        if let Some(message) = agent_message.as_mut() {
            message.status = status;
            message.error = error.map(redact_text);
            message.exit_code = if matches!(status, ChatMessageStatus::Ok) {
                Some(0)
            } else {
                None
            };
        }
        let mut events = Vec::with_capacity(3);
        if let Some(message) = agent_message.as_ref() {
            events.push(ChatEvent::AgentFinished {
                turn: self.chat_turn.unwrap_or(0),
                agent: self.agent,
                message: message.clone(),
            });
        }
        if let Some(error) = error {
            events.push(ChatEvent::Error {
                message: redact_text(error),
            });
        }
        events.push(ChatEvent::Finished {
            turn: self.chat_turn.unwrap_or(0),
            ok,
            cancelled,
        });
        self.store.finish_message_events(
            &self.conversation_id,
            agent_message.as_ref(),
            phase,
            self.run_id.as_deref(),
            &events,
        )?;
        if !already_terminal {
            self.log_terminal_outcome(status, cancelled, error);
        }
        Ok(())
    }

    fn log_terminal_outcome(
        &self,
        status: ChatMessageStatus,
        cancelled: bool,
        error: Option<&str>,
    ) {
        if cancelled {
            return;
        }
        match status {
            ChatMessageStatus::Ok => logging::log_chat_info(
                "send",
                &self.conversation_id,
                Some(self.agent.as_str()),
                "send ok",
            ),
            ChatMessageStatus::Failed | ChatMessageStatus::Timeout => logging::log_chat_error(
                "send_fail",
                &self.conversation_id,
                Some(self.agent.as_str()),
                None,
                error.unwrap_or("send failed"),
            ),
            ChatMessageStatus::Running
            | ChatMessageStatus::Cancelled
            | ChatMessageStatus::Skipped => {}
        }
    }

    fn log_stop_ok(&self) {
        logging::log_chat_info(
            "stop",
            &self.conversation_id,
            Some(self.agent.as_str()),
            "stop ok",
        );
    }

    fn log_send_fail(&self, error: &AppError) {
        logging::log_chat_error(
            "send_fail",
            &self.conversation_id,
            Some(self.agent.as_str()),
            Some(error.code()),
            &error.to_string(),
        );
    }

    fn live_phase(&self, default: RuntimePhase) -> RuntimePhase {
        match self
            .store
            .record(&self.conversation_id)
            .ok()
            .flatten()
            .map(|r| r.phase)
        {
            Some(RuntimePhase::Waiting) => RuntimePhase::Waiting,
            Some(RuntimePhase::Cancelling) => RuntimePhase::Cancelling,
            _ => default,
        }
    }

    fn check_run(&self, run_id: &str) -> Result<()> {
        let record = self
            .store
            .record(&self.conversation_id)?
            .ok_or_else(|| AppError::NotFound("runtime conversation not found".into()))?;
        if record.run_id.as_deref() != Some(run_id) {
            return Err(AppError::InvalidArg(
                "runId does not match the active turn".into(),
            ));
        }
        Ok(())
    }

    fn conversation_cwd(&self) -> Result<PathBuf> {
        let stored = self
            .repo
            .get_conversation(&self.conversation_id)?
            .and_then(|conversation| conversation.cwd);
        crate::services::chat_cwd::resolve_runtime_cwd(stored.as_deref())
    }

    fn respond_jsonrpc(
        &mut self,
        id: Value,
        response: std::result::Result<Value, Value>,
    ) -> Result<()> {
        let transport = self.transport.as_mut().ok_or_else(|| {
            AppError::message(
                "chat.runtime.interrupted",
                format!("{} 已退出", runtime_process_label(self.agent)),
            )
        })?;
        transport
            .respond(id, response)
            .map_err(|error| transport_error(self.agent, error))
    }

    fn handle_acp_fs_read(&mut self, id: Value, params: &Value) -> Result<()> {
        let path = match ops::acp_fs_read_path(params) {
            Ok(path) => path,
            Err(error) => {
                return self.respond_jsonrpc(
                    id,
                    Err(json!({"code": -32602, "message": error.to_string()})),
                );
            }
        };
        match ops::read_text_file_from_disk(&path) {
            Ok(content) => self.respond_jsonrpc(id, Ok(json!({ "content": content }))),
            Err(error) => self.respond_jsonrpc(
                id,
                Err(json!({"code": -32000, "message": error.to_string()})),
            ),
        }
    }

    fn handle_acp_fs_write(&mut self, id: Value, params: &Value) -> Result<()> {
        let id_string = wire_id_string(&id);
        let (path, content) = match ops::acp_fs_write_payload(params) {
            Ok(payload) => payload,
            Err(error) => {
                return self.respond_jsonrpc(
                    id,
                    Err(json!({"code": -32602, "message": error.to_string()})),
                );
            }
        };
        let cwd = self.conversation_cwd()?;
        let auto_write = ops::path_is_inside_cwd(&path, &cwd) || self.session_allow_always;
        if auto_write {
            return match ops::write_text_file_on_disk(&path, &content) {
                Ok(()) => self.respond_jsonrpc(id, Ok(json!({}))),
                Err(error) => self.respond_jsonrpc(
                    id,
                    Err(json!({"code": -32000, "message": error.to_string()})),
                ),
            };
        }
        let run_id = params
            .get("turnId")
            .and_then(Value::as_str)
            .or(self.run_id.as_deref())
            .ok_or_else(|| {
                AppError::message("chat.runtime.protocol", "Codex request omitted run id")
            })?
            .to_string();
        let permission_options = codex_session_permission_options();
        let request = RuntimeRequest {
            id: id_string.clone(),
            run_id,
            kind: RuntimeRequestKind::File,
            title: "修改文件".into(),
            detail: path.to_string_lossy().into_owned(),
            questions: Vec::new(),
            permission_options: permission_options.clone(),
            file_changes: vec![RuntimeFileChange {
                path: path.to_string_lossy().into_owned(),
                kind: Some("write".into()),
                preview: ops::acp_fs_write_preview(&content),
            }],
        };
        self.pending_fs_writes
            .insert(id_string.clone(), (path, content));
        self.store.add_request(
            &self.conversation_id,
            &request,
            "fs/write_text_file",
            &id_string,
        )?;
        self.permission_options
            .insert(id_string, permission_options);
        Ok(())
    }

    fn reply_acp_fs_write(
        &mut self,
        reply: &RuntimeReply,
        persisted: &store::PersistedRequest,
        decision: &str,
    ) -> Result<()> {
        let server_id = parse_wire_id(&persisted.server_id)?;
        if decision == "decline" {
            self.pending_fs_writes.remove(&persisted.request.id);
            self.store.record_reply(
                &reply.conversation_id,
                &reply.run_id,
                &reply.client_request_id,
                &reply.request_id,
            )?;
            self.respond_jsonrpc(
                server_id,
                Err(json!({"code": -32000, "message": "已拒绝写出"})),
            )?;
        } else {
            let Some((path, content)) = self.pending_fs_writes.remove(&persisted.request.id) else {
                return Err(AppError::message("chat.runtime", "写出内容已失效，请重试"));
            };
            let write_result = ops::write_text_file_on_disk(&path, &content);
            if decision == "accept_always" && write_result.is_ok() {
                self.session_allow_always = true;
            }
            self.store.record_reply(
                &reply.conversation_id,
                &reply.run_id,
                &reply.client_request_id,
                &reply.request_id,
            )?;
            match write_result {
                Ok(()) => self.respond_jsonrpc(server_id, Ok(json!({})))?,
                Err(error) => self.respond_jsonrpc(
                    server_id,
                    Err(json!({"code": -32000, "message": error.to_string()})),
                )?,
            }
        }
        self.permission_options.remove(&persisted.request.id);
        let phase = if self
            .store
            .snapshot(&self.conversation_id, None)?
            .pending_requests
            .is_empty()
        {
            RuntimePhase::Running
        } else {
            RuntimePhase::Waiting
        };
        self.store.set_state(
            &self.conversation_id,
            phase,
            self.run_id.as_deref(),
            self.thread_id.as_deref(),
            self.turn_id.as_deref(),
            self.chat_turn,
            self.message_id.as_deref(),
        )?;
        Ok(())
    }
}

fn grok_session_id(value: &Value) -> Option<String> {
    value
        .get("sessionId")
        .or_else(|| value.get("session_id"))
        .or_else(|| value.pointer("/session/sessionId"))
        .or_else(|| value.pointer("/result/sessionId"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn extract_id(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(|nested| {
        nested
            .get("id")
            .and_then(Value::as_str)
            .or_else(|| nested.as_str())
            .map(str::to_string)
    })
}

/// Codex `thread/tokenUsage/updated` (and optional `turn/completed`) last + total.
fn codex_usage_steps(params: &Value) -> Vec<ProcessStep> {
    params
        .get("tokenUsage")
        .or_else(|| params.get("token_usage"))
        .or_else(|| params.pointer("/turn/tokenUsage"))
        .map(ProcessStep::from_codex_token_usage)
        .unwrap_or_default()
}

fn wire_id_string(value: &Value) -> String {
    value
        .as_str()
        .map(str::to_string)
        .unwrap_or_else(|| value.to_string())
}

fn parse_wire_id(value: &str) -> Result<Value> {
    serde_json::from_str(value).or_else(|_| Ok(Value::String(value.to_string())))
}

fn parse_questions(value: Option<&Value>) -> Vec<RuntimeQuestion> {
    let Some(Value::Array(questions)) = value else {
        return Vec::new();
    };
    questions
        .iter()
        .filter_map(|question| {
            Some(RuntimeQuestion {
                id: question.get("id")?.as_str()?.to_string(),
                header: question
                    .get("header")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                question: question
                    .get("question")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                options: question
                    .get("options")
                    .and_then(Value::as_array)
                    .map(|options| {
                        options
                            .iter()
                            .filter_map(|option| {
                                Some(types::RuntimeQuestionOption {
                                    label: option.get("label")?.as_str()?.to_string(),
                                    description: option
                                        .get("description")
                                        .and_then(Value::as_str)
                                        .unwrap_or_default()
                                        .to_string(),
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
                is_other: question
                    .get("isOther")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                is_secret: question
                    .get("isSecret")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            })
        })
        .collect()
}

fn validate_answers(
    questions: &[RuntimeQuestion],
    answers: &std::collections::BTreeMap<String, Vec<String>>,
) -> Result<()> {
    if answers.len() != questions.len() {
        return Err(AppError::InvalidArg(
            "question answers must include exactly the requested questions".into(),
        ));
    }
    for question in questions {
        let Some(values) = answers.get(&question.id) else {
            return Err(AppError::InvalidArg(format!(
                "missing answer for question {}",
                question.id
            )));
        };
        if values.is_empty() || values.len() > 16 {
            return Err(AppError::InvalidArg(
                "question answer count is out of range".into(),
            ));
        }
        for value in values {
            if value.chars().count() > 16_384 {
                return Err(AppError::InvalidArg("question answer is too long".into()));
            }
            if !question.is_other && !question.options.iter().any(|option| option.label == *value) {
                return Err(AppError::InvalidArg(format!(
                    "answer is not one of the options for question {}",
                    question.id
                )));
            }
        }
    }
    Ok(())
}

fn redact_json_text(value: Option<&Value>) -> String {
    let raw = match value {
        Some(Value::String(value)) => value.clone(),
        Some(value) => value.to_string(),
        None => String::new(),
    };
    redact_text(&raw)
}

fn item_status(method: &str, item: &Value) -> String {
    item.get("status")
        .and_then(Value::as_str)
        .unwrap_or(if method == "item/completed" {
            "completed"
        } else {
            "inProgress"
        })
        .to_string()
}

fn optional_json_text(value: Option<&Value>) -> Option<String> {
    match value {
        None | Some(Value::Null) => None,
        Some(Value::String(text)) if text.trim().is_empty() => None,
        other => {
            let text = redact_json_text(other);
            (!text.is_empty()).then_some(text)
        }
    }
}

fn parse_acp_permission_options(params: &Value) -> Vec<RuntimePermissionOption> {
    params
        .get("options")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|option| {
            let id = option
                .get("optionId")
                .or_else(|| option.get("id"))
                .and_then(Value::as_str)
                .map(str::to_owned)?;
            let kind = option
                .get("kind")
                .and_then(Value::as_str)
                .map(str::to_owned)?;
            Some(RuntimePermissionOption { id, kind })
        })
        .collect()
}

fn acp_permission_options(params: &Value) -> Option<Vec<RuntimePermissionOption>> {
    let options = parse_acp_permission_options(params);
    (!options.is_empty()).then_some(options)
}

/// ACP file tools stay file cards even when the payload only named a path.
fn acp_tool_is_file_change(params: &Value) -> bool {
    let kind = params
        .pointer("/toolCall/kind")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    matches!(kind.as_str(), "edit" | "delete" | "move" | "write" | "create")
}

fn codex_session_permission_options() -> Vec<RuntimePermissionOption> {
    vec![
        RuntimePermissionOption {
            id: "accept".into(),
            kind: "allow_once".into(),
        },
        RuntimePermissionOption {
            id: "accept_always".into(),
            kind: "allow_always".into(),
        },
        RuntimePermissionOption {
            id: "decline".into(),
            kind: "reject_once".into(),
        },
    ]
}

fn codex_approval_decision(decision: &str) -> &'static str {
    match decision {
        "decline" => "decline",
        "accept_always" => "acceptForSession",
        _ => "accept",
    }
}

fn acp_capability_present(value: Option<&Value>) -> bool {
    matches!(value, Some(Value::Bool(true)) | Some(Value::Object(_)))
}

fn is_acp_allow_always_kind(kind: &str) -> bool {
    kind == "allow_always" || kind.starts_with("allow_always_")
}

fn acp_kind_matches(option_kind: &str, wanted: &str) -> bool {
    match wanted {
        "allow_once" => option_kind == "allow_once" || option_kind.starts_with("allow_once_"),
        "allow_always" => is_acp_allow_always_kind(option_kind),
        "reject_once" => option_kind == "reject_once" || option_kind.starts_with("reject_once_"),
        _ => option_kind == wanted,
    }
}

fn acp_permission_reply(options: &[RuntimePermissionOption], decision: &str) -> Result<Value> {
    let wanted_kind = match decision {
        "accept" => "allow_once",
        "accept_always" => "allow_always",
        "decline" => "reject_once",
        _ => return Err(AppError::InvalidArg("approval decision is required".into())),
    };
    if let Some(option) = options
        .iter()
        .find(|option| acp_kind_matches(&option.kind, wanted_kind))
    {
        return Ok(json!({
            "outcome": { "outcome": "selected", "optionId": option.id }
        }));
    }
    if decision == "decline" {
        return Ok(json!({
            "outcome": { "outcome": "cancelled" }
        }));
    }
    if decision == "accept_always" {
        return Err(AppError::message(
            "chat.runtime.permission",
            "这次操作不能一直允许",
        ));
    }
    Err(AppError::message(
        "chat.runtime.permission",
        "服务端没有提供一次性允许选项，无法安全批准此请求",
    ))
}

fn acp_auto_allow_response(options: &[RuntimePermissionOption]) -> Option<Value> {
    acp_permission_reply(options, "accept_always")
        .ok()
        .or_else(|| acp_permission_reply(options, "accept").ok())
}

fn runtime_process_label(agent: AgentId) -> &'static str {
    match agent {
        AgentId::Grok => "Grok",
        AgentId::Kiro => "Kiro",
        AgentId::Claude => "Claude",
        _ => "Codex",
    }
}

fn transport_error(agent: AgentId, error: codex_transport::CodexTransportError) -> AppError {
    AppError::message(
        "chat.runtime.transport",
        redact_text(&transport_user_message(agent, &error)),
    )
}

fn map_transport(agent: AgentId, error: codex_transport::CodexTransportError) -> AppError {
    if matches!(error, codex_transport::CodexTransportError::Interrupted) {
        cancelled_error()
    } else {
        AppError::message(
            "chat.runtime.transport",
            redact_text(&transport_user_message(agent, &error)),
        )
    }
}

fn transport_user_message(agent: AgentId, error: &codex_transport::CodexTransportError) -> String {
    let text = error.to_string();
    if text.contains("stdout JSON line exceeds") {
        return "图片太大，请换一张更小的图".into();
    }
    if matches!(error, codex_transport::CodexTransportError::Exited) {
        return format!("{} 已退出", runtime_process_label(agent));
    }
    text.replacen("codex app-server", runtime_process_label(agent), 1)
}

fn cancelled_error() -> AppError {
    AppError::message("chat.runtime.cancelled", "已取消")
}

fn log_and_return_send_fail(conversation_id: &str, error: AppError) -> AppError {
    logging::log_chat_error(
        "send_fail",
        conversation_id,
        None,
        Some(error.code()),
        &error.to_string(),
    );
    error
}

fn log_and_return_stop_fail(conversation_id: &str, error: AppError) -> AppError {
    logging::log_chat_error(
        "stop_fail",
        conversation_id,
        None,
        Some(error.code()),
        &error.to_string(),
    );
    error
}

fn is_cancelled_error(error: &AppError) -> bool {
    error.code() == "chat.runtime.cancelled"
}

fn is_user_stop_error(error: &AppError) -> bool {
    matches!(
        error.code(),
        "chat.runtime.cancelled" | "chat.runtime.interrupted"
    )
}

fn resolve_codex_program(
    run: &RunService,
    override_path: &Mutex<Option<PathBuf>>,
) -> Result<PathBuf> {
    if let Ok(guard) = override_path.lock() {
        if let Some(path) = guard.as_ref() {
            return Ok(path.clone());
        }
    }
    run.detect_codex_installation()
}

fn fetch_kiro_catalog() -> CatalogCache {
    let live = crate::adapters::kiro::kiro_live_chat_model();
    let models: Vec<RuntimeModelOption> = live
        .models
        .into_iter()
        .map(|id| RuntimeModelOption {
            efforts: live.efforts.clone(),
            default_effort: live.effort.clone(),
            id,
        })
        .collect();
    CatalogCache {
        models,
        extensions: Vec::new(),
        from_codex: true,
        ..CatalogCache::default()
    }
}

fn grok_fallback_catalog() -> CatalogCache {
    let live = crate::adapters::grok::grok_live_chat_model();
    let models = ops::ensure_grok_catalog_efforts(
        live.models
            .into_iter()
            .map(|id| RuntimeModelOption {
                efforts: live.efforts.clone(),
                default_effort: live.effort.clone(),
                id,
            })
            .collect(),
    );
    CatalogCache {
        models,
        extensions: Vec::new(),
        from_codex: false,
        ..CatalogCache::default()
    }
}

fn claude_fallback_catalog() -> CatalogCache {
    // Aliases accepted by Claude Code CLI `--model` / `--effort`.
    let efforts = ["low", "medium", "high", "xhigh", "max"]
        .into_iter()
        .map(str::to_string)
        .collect::<Vec<_>>();
    let models = ["sonnet", "opus", "haiku"]
        .into_iter()
        .map(|id| RuntimeModelOption {
            id: id.to_string(),
            efforts: efforts.clone(),
            default_effort: Some("high".into()),
        })
        .collect();
    CatalogCache {
        models,
        extensions: Vec::new(),
        from_codex: false,
        ..CatalogCache::default()
    }
}

#[cfg(test)]
mod actor_tests;
#[cfg(test)]
mod tests;
