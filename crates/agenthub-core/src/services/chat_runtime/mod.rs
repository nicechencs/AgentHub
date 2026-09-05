//! Durable Codex app-server chat runtime.
//!
//! A runtime owns one Codex app-server process per conversation.  All commands
//! for that process are serialized through its worker queue, so a late answer
//! cannot race a stop or be delivered to a newer turn.  The worker commits
//! normalized events to SQLite before a snapshot can expose them.

mod codex_transport;
mod store;
mod types;

pub use types::{
    RuntimeDecision, RuntimeEvent, RuntimePhase, RuntimeQuestion, RuntimeQuestionOption,
    RuntimeReply, RuntimeRequest, RuntimeRequestKind, RuntimeSnapshot,
};

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use chrono::Utc;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::error::{AppError, Result};
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

enum RuntimeCommand {
    Start {
        prompt: String,
        client_request_id: String,
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
}

/// One serialized owner per conversation.  The map itself is only a routing
/// table; process state is never mutated from command callers.
pub struct ChatRuntime {
    store: RuntimeStore,
    repo: ChatRepo,
    run: Arc<RunService>,
    actors: Mutex<HashMap<String, ActorHandle>>,
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
        }
    }

    pub fn snapshot(
        &self,
        conversation_id: &str,
        after_sequence: Option<i64>,
    ) -> Result<RuntimeSnapshot> {
        self.store.snapshot(conversation_id, after_sequence)
    }

    pub(crate) fn is_enabled(&self, conversation_id: &str) -> Result<bool> {
        self.store.persisted_enabled(conversation_id)
    }

    pub fn start(
        self: &Arc<Self>,
        conversation_id: &str,
        prompt: &str,
        client_request_id: &str,
    ) -> Result<RuntimeSnapshot> {
        let prompt = prompt.trim();
        if prompt.is_empty() {
            return Err(AppError::InvalidArg("prompt must not be empty".into()));
        }
        if client_request_id.trim().is_empty() {
            return Err(AppError::InvalidArg(
                "clientRequestId must not be empty".into(),
            ));
        }
        self.store.enable_if_new(conversation_id)?;
        match self
            .store
            .begin_operation(conversation_id, "start", client_request_id, None)?
        {
            OperationState::Accepted => return self.store.snapshot(conversation_id, None),
            OperationState::Pending => {
                return Err(operation_replay_error("start", "pending"));
            }
            OperationState::Failed => {
                return Err(operation_replay_error("start", "failed"));
            }
            OperationState::New => {}
        }
        let actor = match self.actor(conversation_id) {
            Ok(actor) => actor,
            Err(error) => {
                self.store.mark_operation(
                    conversation_id,
                    "start",
                    client_request_id,
                    OperationState::Failed,
                    None,
                )?;
                return Err(error);
            }
        };
        let (tx, rx) = mpsc::sync_channel(1);
        let outcome = actor
            .tx
            .send(RuntimeCommand::Start {
                prompt: prompt.to_string(),
                client_request_id: client_request_id.to_string(),
                result: tx,
            })
            .map_err(|_| AppError::message("chat.runtime", "runtime worker stopped"))
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
        let actor = self.actor_for_existing(conversation_id)?;
        let (tx, rx) = mpsc::sync_channel(1);
        actor
            .tx
            .send(RuntimeCommand::Cancel {
                run_id: run_id.to_string(),
                result: tx,
            })
            .map_err(|_| AppError::message("chat.runtime", "runtime worker stopped"))?;
        recv_result(rx)
    }

    /// Stop and forget the in-process owner before a conversation is deleted.
    /// The worker owns process teardown and will not receive any new events
    /// after the shutdown command has been acknowledged by the worker.
    pub(crate) fn shutdown(&self, conversation_id: &str) {
        if let Ok(mut actors) = self.actors.lock() {
            if let Some(actor) = actors.remove(conversation_id) {
                let (done_tx, done_rx) = mpsc::sync_channel(1);
                if actor
                    .tx
                    .send(RuntimeCommand::Shutdown { done: done_tx })
                    .is_ok()
                {
                    let _ = done_rx.recv_timeout(Duration::from_secs(5));
                }
            }
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
        let id = conversation_id.to_string();
        thread::Builder::new()
            .name(format!("agenthub-chat-runtime-{conversation_id}"))
            .spawn(move || actor_loop(id, rx, store, repo, run))
            .map_err(AppError::from)?;
        let actor = ActorHandle { tx };
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
) {
    let mut worker = ActorWorker {
        conversation_id,
        rx,
        store,
        repo,
        run,
        transport: None,
        thread_id: None,
        turn_id: None,
        chat_turn: None,
        message_id: None,
        run_id: None,
        last_start_request: None,
    };
    worker.run();
}

struct ActorWorker {
    conversation_id: String,
    rx: Receiver<RuntimeCommand>,
    store: RuntimeStore,
    repo: ChatRepo,
    run: Arc<RunService>,
    transport: Option<CodexTransport>,
    thread_id: Option<String>,
    turn_id: Option<String>,
    chat_turn: Option<i64>,
    message_id: Option<String>,
    run_id: Option<String>,
    last_start_request: Option<String>,
}

impl ActorWorker {
    fn run(&mut self) {
        self.restore_record();
        loop {
            match self.rx.recv_timeout(CODEX_POLL_INTERVAL) {
                Ok(RuntimeCommand::Start {
                    prompt,
                    client_request_id,
                    result,
                }) => {
                    let outcome = self.start_turn(&prompt, &client_request_id);
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
            if let Err(error) = self.poll_events() {
                self.fail_runtime(error);
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
    }

    fn start_turn(&mut self, prompt: &str, client_request_id: &str) -> Result<RuntimeSnapshot> {
        let previous_chat_turn = self.chat_turn;
        match self.start_turn_inner(prompt, client_request_id) {
            Ok(snapshot) => Ok(snapshot),
            Err(error) => {
                // A duplicate/active-run validation error must leave the
                // existing owner untouched. Once begin_turn succeeds, this
                // invocation owns a new chat row and must close it on error.
                if self.chat_turn != previous_chat_turn {
                    let message = error.to_string();
                    self.fail_runtime(AppError::message("chat.runtime", message));
                }
                Err(error)
            }
        }
    }

    fn start_turn_inner(
        &mut self,
        prompt: &str,
        client_request_id: &str,
    ) -> Result<RuntimeSnapshot> {
        let record = self
            .store
            .record(&self.conversation_id)?
            .ok_or_else(|| AppError::NotFound("runtime conversation not found".into()))?;
        if !record.enabled {
            return Err(AppError::Unsupported("持续聊天未启用".into()));
        }
        if self.last_start_request.as_deref() == Some(client_request_id) {
            return self.store.snapshot(&self.conversation_id, None);
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
        self.last_start_request = Some(client_request_id.to_string());
        self.store
            .set_last_client_request_id(&self.conversation_id, client_request_id)?;
        if let Some(mut transport) = self.transport.take() {
            transport.shutdown();
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
            agent_id: Some(AgentId::Codex),
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
                        agents: vec![AgentId::Codex],
                    },
                    ChatEvent::AgentStarted {
                        turn,
                        agent: AgentId::Codex,
                        command: "codex app-server".into(),
                    },
                ]
            },
        )?;
        self.chat_turn = Some(chat_turn);
        self.message_id = Some(message_id);
        self.turn_id = None;
        self.run_id = Some(run_id);

        let start_result = (|| {
            let cwd = self.conversation_cwd()?;
            let program = self.run.detect_codex_installation()?;
            let mut transport = if let Some(thread_id) = self.thread_id.as_deref() {
                // The transport itself is always a fresh process; thread/resume
                // reattaches it to Codex's durable native thread.
                let mut t = CodexTransport::spawn(&program, &cwd).map_err(transport_error)?;
                let result = t
                    .request(
                        "thread/resume",
                        json!({
                            "threadId": thread_id,
                            "cwd": cwd.to_string_lossy(),
                            "approvalPolicy": "on-request",
                            "sandbox": "workspace-write"
                        }),
                        CODEX_REQUEST_TIMEOUT,
                    )
                    .map_err(transport_error)?;
                self.thread_id = Some(thread_id.to_string());
                let _ = result;
                t
            } else {
                let mut t = CodexTransport::spawn(&program, &cwd).map_err(transport_error)?;
                let result = t
                    .request(
                        "thread/start",
                        json!({
                            "cwd": cwd.to_string_lossy(),
                            "approvalPolicy": "on-request",
                            "sandbox": "workspace-write",
                            "ephemeral": false
                        }),
                        CODEX_REQUEST_TIMEOUT,
                    )
                    .map_err(transport_error)?;
                self.thread_id =
                    extract_id(&result, "thread").or_else(|| extract_id(&result, "id"));
                t
            };
            let thread_id = self.thread_id.clone().ok_or_else(|| {
                AppError::message("chat.runtime.protocol", "Codex omitted thread id")
            })?;
            let result = transport
                .request(
                    "turn/start",
                    json!({
                        "threadId": thread_id,
                        "input": [{"type": "text", "text": prompt}],
                        "clientUserMessageId": client_request_id,
                        "cwd": cwd.to_string_lossy(),
                        "approvalPolicy": "on-request",
                        "sandboxPolicy": {
                            "type": "workspaceWrite",
                            "writableRoots": [cwd.to_string_lossy()],
                            "networkAccess": false
                        }
                    }),
                    CODEX_REQUEST_TIMEOUT,
                )
                .map_err(transport_error);
            let result = result?;
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
            self.store.snapshot(&self.conversation_id, None)
        })();
        start_result
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
        let value = match persisted.request.kind {
            RuntimeRequestKind::Command | RuntimeRequestKind::File => {
                let decision = match reply.decision {
                    Some(RuntimeDecision::Allow) => "accept",
                    Some(RuntimeDecision::Deny) => "decline",
                    None => {
                        return Err(AppError::InvalidArg("approval decision is required".into()));
                    }
                };
                if reply.answers.is_some() {
                    return Err(AppError::InvalidArg(
                        "approval cannot include answers".into(),
                    ));
                }
                json!({"decision": decision})
            }
            RuntimeRequestKind::Question => {
                if reply.decision.is_some() {
                    return Err(AppError::InvalidArg(
                        "question reply cannot include decision".into(),
                    ));
                }
                let answers = reply
                    .answers
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
            AppError::message("chat.runtime.interrupted", "Codex process stopped")
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
            .map_err(transport_error);
        if let Err(error) = response_result {
            self.fail_runtime(AppError::message(
                "chat.runtime.transport",
                error.to_string(),
            ));
            return Err(error);
        }
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
            AppError::message("chat.runtime.interrupted", "Codex process stopped")
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
            .map_err(transport_error)?;
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
        self.check_run(run_id)?;
        let record = self
            .store
            .record(&self.conversation_id)?
            .ok_or_else(|| AppError::NotFound("runtime conversation not found".into()))?;
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
            return Ok(());
        }
        let (Some(thread_id), Some(turn_id)) = (thread_id, turn_id) else {
            return self.cancel_failed(AppError::message(
                "chat.runtime",
                "Codex turn is unavailable",
            ));
        };
        for request in pending {
            let response = parse_wire_id(&request.server_id).and_then(|server_id| {
                self.transport
                    .as_mut()
                    .ok_or_else(|| {
                        AppError::message("chat.runtime.interrupted", "Codex process stopped")
                    })?
                    .respond(
                        server_id,
                        Err(json!({"code": -32800, "message": "cancelled"})),
                    )
                    .map_err(transport_error)
            });
            if let Err(error) = response {
                return self.cancel_failed(error);
            }
        }
        let interrupt_result = self
            .transport
            .as_mut()
            .ok_or_else(|| AppError::message("chat.runtime.interrupted", "Codex process stopped"))
            .and_then(|transport| {
                transport
                    .request(
                        "turn/interrupt",
                        json!({"threadId": thread_id, "turnId": turn_id}),
                        CODEX_REQUEST_TIMEOUT,
                    )
                    .map_err(transport_error)
            });
        match interrupt_result {
            Ok(_) => Ok(()),
            Err(error) => self.cancel_failed(error),
        }
    }

    fn poll_events(&mut self) -> Result<()> {
        for index in 0..64 {
            let Some(transport) = self.transport.as_mut() else {
                return Ok(());
            };
            let timeout = if index == 0 {
                CODEX_POLL_INTERVAL
            } else {
                Duration::ZERO
            };
            let event = transport.recv_timeout(timeout).map_err(transport_error)?;
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
                            Some("Codex process stopped"),
                            RuntimePhase::Interrupted,
                            false,
                            true,
                        )?;
                    }
                    return Ok(());
                }
                Some(CodexEvent::Response { .. }) => {}
                None => return Ok(()),
            }
        }
        Ok(())
    }

    fn server_request(&mut self, id: Value, method: &str, params: &Value) -> Result<()> {
        let id_string = wire_id_string(&id);
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
                    .map_err(transport_error)?;
            }
            return Ok(());
        }
        let (kind, title, detail, questions) = match method {
            "item/commandExecution/requestApproval" | "execCommandApproval" => (
                RuntimeRequestKind::Command,
                "执行命令".to_string(),
                redact_json_text(params.get("command").or_else(|| params.get("reason"))),
                Vec::new(),
            ),
            "item/fileChange/requestApproval" | "fileChangeApproval" => (
                RuntimeRequestKind::File,
                "修改文件".to_string(),
                redact_json_text(params.get("reason").or_else(|| params.get("grantRoot"))),
                Vec::new(),
            ),
            "item/tool/requestUserInput" => (
                RuntimeRequestKind::Question,
                "需要你的回答".to_string(),
                String::new(),
                parse_questions(params.get("questions")),
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
                        .map_err(transport_error)?;
                }
                self.emit_error(
                    &format!("Codex 请求暂不支持：{method}"),
                    self.live_phase(RuntimePhase::Running),
                )?;
                return Ok(());
            }
        };
        let request = RuntimeRequest {
            id: id_string.clone(),
            run_id,
            kind,
            title,
            detail,
            questions,
        };
        self.store
            .add_request(&self.conversation_id, &request, method, &id_string)
    }

    fn notification(&mut self, method: &str, params: &Value) -> Result<()> {
        match method {
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
                let text = params
                    .get("delta")
                    .and_then(Value::as_str)
                    .or_else(|| params.get("output").and_then(Value::as_str))
                    .unwrap_or_default();
                if !text.is_empty() {
                    self.emit(
                        ChatEvent::AgentProcess {
                            turn: self.chat_turn.unwrap_or(0),
                            agent: AgentId::Codex,
                            step: ProcessStep::Raw {
                                text: redact_text(text),
                                note: Some("command output".into()),
                            },
                        },
                        self.live_phase(RuntimePhase::Running),
                    )?;
                }
            }
            "turn/completed" => self.turn_completed(params)?,
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
            .unwrap_or("completed");
        let cancelled = matches!(status, "interrupted" | "cancelled" | "canceled");
        let (message_status, phase, ok, error) = if cancelled {
            (
                ChatMessageStatus::Cancelled,
                RuntimePhase::Cancelled,
                false,
                None,
            )
        } else if matches!(status, "failed" | "error") {
            (
                ChatMessageStatus::Failed,
                RuntimePhase::Failed,
                false,
                Some(redact_json_text(
                    params.get("error").or_else(|| params.get("message")),
                )),
            )
        } else {
            (ChatMessageStatus::Ok, RuntimePhase::Completed, true, None)
        };
        self.terminalize(message_status, error.as_deref(), phase, ok, cancelled)?;
        if let Some(transport) = self.transport.as_mut() {
            transport.shutdown();
        }
        self.transport = None;
        Ok(())
    }

    fn emit(&self, event: ChatEvent, phase: RuntimePhase) -> Result<()> {
        self.store
            .commit_event(&self.conversation_id, phase, self.run_id.as_deref(), &event)?;
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
            agent: AgentId::Codex,
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

    fn terminalize(
        &self,
        status: ChatMessageStatus,
        error: Option<&str>,
        phase: RuntimePhase,
        ok: bool,
        cancelled: bool,
    ) -> Result<()> {
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
                agent: AgentId::Codex,
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
        )
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
        self.repo
            .get_conversation(&self.conversation_id)?
            .and_then(|conversation| conversation.cwd.map(PathBuf::from))
            .map(Ok)
            .unwrap_or_else(|| std::env::current_dir().map_err(AppError::from))
    }
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

fn transport_error(error: codex_transport::CodexTransportError) -> AppError {
    AppError::message("chat.runtime.transport", redact_text(&error.to_string()))
}

#[cfg(test)]
mod actor_tests;
#[cfg(test)]
mod tests;
