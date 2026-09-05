//! SQLite persistence for the long-lived chat runtime.
//!
//! Runtime state intentionally lives in its own tables.  The existing chat
//! tables remain the source for the normal conversation history; runtime rows
//! only add the durable owner, replay sequence, and pending server requests.

use chrono::Utc;
use rusqlite::{OptionalExtension, params};

use crate::error::{AppError, Result};
use crate::models::{AgentId, ChatEvent, ChatMessage, ChatMessageStatus, ChatRole};
use crate::storage::Database;

use super::types::{
    RuntimeEvent, RuntimePhase, RuntimeQuestion, RuntimeRequest, RuntimeRequestKind,
    RuntimeSnapshot,
};

#[derive(Clone)]
pub(crate) struct RuntimeStore {
    db: Database,
}

#[derive(Debug, Clone)]
pub(crate) struct RuntimeRecord {
    pub conversation_id: String,
    pub enabled: bool,
    pub run_id: Option<String>,
    pub phase: RuntimePhase,
    pub last_sequence: i64,
    pub thread_id: Option<String>,
    pub turn_id: Option<String>,
    pub chat_turn: Option<i64>,
    pub message_id: Option<String>,
    pub last_client_request_id: Option<String>,
    pub last_steer_client_request_id: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct PersistedRequest {
    pub request: RuntimeRequest,
    pub server_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OperationState {
    New,
    Pending,
    Accepted,
    Failed,
}

impl RuntimeStore {
    pub(crate) fn new(db: Database) -> Self {
        Self { db }
    }

    pub(crate) fn ensure_conversation(&self, conversation_id: &str) -> Result<()> {
        self.db.with_conn(|conn| {
            let exists: bool = conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM conversations WHERE id = ?1)",
                params![conversation_id],
                |row| row.get(0),
            )?;
            if exists {
                Ok(())
            } else {
                Err(AppError::NotFound(format!(
                    "conversation not found: {conversation_id}"
                )))
            }
        })
    }

    /// Enable runtime mode only for a new/empty Codex conversation.  Existing
    /// legacy conversations keep their old send path forever unless a future
    /// explicit upgrade flow is added.
    pub(crate) fn enable_if_new(&self, conversation_id: &str) -> Result<()> {
        self.ensure_conversation(conversation_id)?;
        self.db.with_conn(|conn| {
            let existing: Option<bool> = conn
                .query_row(
                    "SELECT enabled != 0 FROM chat_runtime WHERE conversation_id = ?1",
                    params![conversation_id],
                    |row| row.get(0),
                )
                .optional()?;
            if existing == Some(true) {
                return Ok(());
            }

            let codex =
                self.conversation_agent_conn(conn, conversation_id)? == Some(AgentId::Codex);
            if !codex {
                return Err(AppError::Unsupported("持续聊天目前只支持 Codex".into()));
            }
            let has_messages: bool = conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM chat_messages WHERE conversation_id = ?1)",
                params![conversation_id],
                |row| row.get(0),
            )?;
            if has_messages {
                return Err(AppError::Unsupported(
                    "已有会话继续使用原来的聊天方式，请新建 Codex 会话".into(),
                ));
            }

            let now = Utc::now().to_rfc3339();
            conn.execute(
                r#"
                INSERT INTO chat_runtime (
                    conversation_id, enabled, phase, last_sequence, updated_at
                ) VALUES (?1, 1, 'idle', 0, ?2)
                ON CONFLICT(conversation_id) DO UPDATE SET
                    enabled = 1,
                    updated_at = excluded.updated_at
                "#,
                params![conversation_id, now],
            )?;
            Ok(())
        })
    }

    pub(crate) fn recover_active(&self) -> Result<()> {
        self.db.with_conn(|conn| {
            let mut stmt = conn.prepare(
                r#"
                SELECT conversation_id, run_id, last_sequence, chat_turn, message_id
                FROM chat_runtime
                WHERE enabled != 0
                  AND phase IN ('starting', 'running', 'waiting', 'cancelling')
                "#,
            )?;
            let rows = stmt.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, Option<i64>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                ))
            })?;
            let mut stale = Vec::new();
            for row in rows {
                stale.push(row?);
            }
            drop(stmt);
            if stale.is_empty() {
                return Ok(());
            }
            conn.execute_batch("BEGIN IMMEDIATE")?;
            let now = Utc::now().to_rfc3339();
            let result = (|| {
                for (conversation_id, run_id, last_sequence, chat_turn, message_id) in stale {
                    if let Some(message_id) = message_id {
                        conn.execute(
                            "UPDATE chat_messages SET status = 'cancelled', error = ?2 WHERE id = ?1",
                            params![message_id, "runtime interrupted"],
                        )?;
                    }
                    let error_event = serde_json::to_string(&ChatEvent::Error {
                        message: "Codex runtime interrupted; start a new turn to continue".into(),
                    })?;
                    let finished_event = serde_json::to_string(&ChatEvent::Finished {
                        turn: chat_turn.unwrap_or(0),
                        ok: false,
                        cancelled: true,
                    })?;
                    let first = last_sequence.checked_add(1).ok_or_else(|| {
                        AppError::InvalidArg("runtime event sequence exhausted".into())
                    })?;
                    conn.execute(
                        "INSERT INTO chat_runtime_events (conversation_id, sequence, event_json, created_at) VALUES (?1, ?2, ?3, ?4)",
                        params![conversation_id, first, error_event, now],
                    )?;
                    conn.execute(
                        "INSERT INTO chat_runtime_events (conversation_id, sequence, event_json, created_at) VALUES (?1, ?2, ?3, ?4)",
                        params![conversation_id, first + 1, finished_event, now],
                    )?;
                    conn.execute(
                        r#"
                        UPDATE chat_runtime
                        SET phase = 'interrupted', last_sequence = ?2, updated_at = ?3
                        WHERE conversation_id = ?1
                        "#,
                        params![conversation_id, first + 1, now],
                    )?;
                    conn.execute(
                        "DELETE FROM chat_runtime_requests WHERE conversation_id = ?1",
                        params![conversation_id],
                    )?;
                    let _ = run_id;
                }
                Ok(())
            })();
            finish_transaction(conn, result)
        })
    }

    pub(crate) fn record(&self, conversation_id: &str) -> Result<Option<RuntimeRecord>> {
        self.db
            .with_conn(|conn| self.record_conn(conn, conversation_id))
    }

    pub(crate) fn persisted_enabled(&self, conversation_id: &str) -> Result<bool> {
        Ok(self
            .record(conversation_id)?
            .is_some_and(|record| record.enabled))
    }

    pub(crate) fn snapshot(
        &self,
        conversation_id: &str,
        after_sequence: Option<i64>,
    ) -> Result<RuntimeSnapshot> {
        if after_sequence.is_some_and(|sequence| sequence < 0) {
            return Err(AppError::InvalidArg(
                "afterSequence must be non-negative".into(),
            ));
        }
        let after = after_sequence.unwrap_or(-1);
        self.db.with_conn(|conn| {
            let Some(record) = self.record_conn(conn, conversation_id)? else {
                let exists: bool = conn.query_row(
                    "SELECT EXISTS(SELECT 1 FROM conversations WHERE id = ?1)",
                    params![conversation_id],
                    |row| row.get(0),
                )?;
                if !exists {
                    return Err(AppError::NotFound(format!(
                        "conversation not found: {conversation_id}"
                    )));
                }
                let agent = self.conversation_agent_conn(conn, conversation_id)?;
                let has_messages: bool = conn.query_row(
                    "SELECT EXISTS(SELECT 1 FROM chat_messages WHERE conversation_id = ?1)",
                    params![conversation_id],
                    |row| row.get(0),
                )?;
                if agent == Some(AgentId::Codex) && !has_messages {
                    return Ok(RuntimeSnapshot {
                        conversation_id: conversation_id.to_string(),
                        enabled: true,
                        run_id: None,
                        phase: RuntimePhase::Idle,
                        last_sequence: 0,
                        events: Vec::new(),
                        pending_requests: Vec::new(),
                        gap: false,
                        current_message: None,
                    });
                }
                return Ok(RuntimeSnapshot::disabled(conversation_id));
            };
            let first: Option<i64> = conn
                .query_row(
                    "SELECT MIN(sequence) FROM chat_runtime_events WHERE conversation_id = ?1",
                    params![conversation_id],
                    |row| row.get(0),
                )
                .optional()?
                .flatten();
            let gap = first.is_some_and(|sequence| {
                after >= 0
                    && after
                        .checked_add(1)
                        .is_some_and(|expected| sequence > expected)
            });
            let mut stmt = conn.prepare(
                r#"
                SELECT sequence, event_json
                FROM chat_runtime_events
                WHERE conversation_id = ?1 AND sequence > ?2
                ORDER BY sequence ASC
                "#,
            )?;
            let rows = stmt.query_map(params![conversation_id, after], |row| {
                let sequence: i64 = row.get(0)?;
                let event_json: String = row.get(1)?;
                let event = serde_json::from_str(&event_json).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        1,
                        rusqlite::types::Type::Text,
                        Box::new(error),
                    )
                })?;
                Ok(RuntimeEvent { sequence, event })
            })?;
            let mut events = Vec::new();
            for row in rows {
                events.push(row?);
            }
            let pending_requests = self.pending_requests_conn(conn, conversation_id)?;
            let current_message = self.current_message_conn(conn, record.message_id.as_deref())?;
            Ok(RuntimeSnapshot {
                conversation_id: record.conversation_id,
                enabled: record.enabled,
                run_id: record.run_id,
                phase: record.phase,
                last_sequence: record.last_sequence,
                events,
                pending_requests,
                gap,
                current_message,
            })
        })
    }

    pub(crate) fn commit_event(
        &self,
        conversation_id: &str,
        phase: RuntimePhase,
        run_id: Option<&str>,
        event: &ChatEvent,
    ) -> Result<i64> {
        let event_json = serde_json::to_string(event)?;
        let now = Utc::now().to_rfc3339();
        self.db.with_conn(|conn| {
            conn.execute_batch("BEGIN IMMEDIATE")?;
            let result =
                (|| insert_event_conn(conn, conversation_id, phase, run_id, &event_json, &now))();
            finish_transaction(conn, result)
        })
    }

    /// Update an existing chat message and append its corresponding runtime
    /// event under one SQLite transaction.  A snapshot can therefore never
    /// expose a delta whose durable history update is still pending.
    pub(crate) fn append_message_event(
        &self,
        conversation_id: &str,
        message: &ChatMessage,
        phase: RuntimePhase,
        run_id: Option<&str>,
        event: &ChatEvent,
    ) -> Result<i64> {
        let event_json = serde_json::to_string(event)?;
        let now = Utc::now().to_rfc3339();
        self.db.with_conn(|conn| {
            conn.execute_batch("BEGIN IMMEDIATE")?;
            let result = (|| {
                update_chat_message_conn(conn, message)?;
                insert_event_conn(conn, conversation_id, phase, run_id, &event_json, &now)
            })();
            finish_transaction(conn, result)
        })
    }

    /// Terminalize the current agent message and publish all terminal events
    /// atomically.  Pending approval/question rows are cleared in the same
    /// transaction, so a terminal snapshot cannot retain stale controls.
    pub(crate) fn finish_message_events(
        &self,
        conversation_id: &str,
        message: Option<&ChatMessage>,
        phase: RuntimePhase,
        run_id: Option<&str>,
        events: &[ChatEvent],
    ) -> Result<()> {
        let event_json = events
            .iter()
            .map(serde_json::to_string)
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let now = Utc::now().to_rfc3339();
        self.db.with_conn(|conn| {
            conn.execute_batch("BEGIN IMMEDIATE")?;
            let result = (|| {
                if let Some(message) = message {
                    update_chat_message_conn(conn, message)?;
                }
                for event in &event_json {
                    insert_event_conn(conn, conversation_id, phase, run_id, event, &now)?;
                }
                conn.execute(
                    "DELETE FROM chat_runtime_requests WHERE conversation_id = ?1",
                    params![conversation_id],
                )?;
                Ok(())
            })();
            finish_transaction(conn, result)
        })
    }

    /// Allocate a legacy chat turn and publish its initial events in one
    /// SQLite transaction.  A crash cannot leave a visible `started` event
    /// without the corresponding user/agent rows or runtime owner state.
    pub(crate) fn begin_turn<F>(
        &self,
        conversation_id: &str,
        user: &mut ChatMessage,
        agent: &mut ChatMessage,
        run_id: &str,
        thread_id: Option<&str>,
        event_builder: F,
    ) -> Result<i64>
    where
        F: FnOnce(i64) -> Vec<ChatEvent>,
    {
        let now = Utc::now().to_rfc3339();
        self.db.with_conn(|conn| {
            conn.execute_batch("BEGIN IMMEDIATE")?;
            let result = (|| {
                let max: Option<i64> = conn
                    .query_row(
                        "SELECT MAX(turn) FROM chat_messages WHERE conversation_id = ?1",
                        params![conversation_id],
                        |row| row.get(0),
                    )?;
                let turn = max.unwrap_or(0).checked_add(1).ok_or_else(|| {
                    AppError::InvalidArg("chat turn sequence exhausted".into())
                })?;
                let events = event_builder(turn);
                let event_json = events
                    .iter()
                    .map(serde_json::to_string)
                    .collect::<std::result::Result<Vec<_>, _>>()?;
                user.turn = turn;
                user.conversation_id = conversation_id.to_string();
                agent.turn = turn;
                agent.conversation_id = conversation_id.to_string();
                insert_chat_message_conn(conn, user)?;
                insert_chat_message_conn(conn, agent)?;
                let current: i64 = conn.query_row(
                    "SELECT last_sequence FROM chat_runtime WHERE conversation_id = ?1",
                    params![conversation_id],
                    |row| row.get(0),
                )?;
                let mut sequence = current;
                for json in &event_json {
                    sequence = sequence.checked_add(1).ok_or_else(|| {
                        AppError::InvalidArg("runtime event sequence exhausted".into())
                    })?;
                    conn.execute(
                        "INSERT INTO chat_runtime_events (conversation_id, sequence, event_json, created_at) VALUES (?1, ?2, ?3, ?4)",
                        params![conversation_id, sequence, json, now],
                    )?;
                }
                conn.execute(
                    r#"
                    UPDATE chat_runtime
                    SET phase = 'starting', run_id = ?2, thread_id = ?3,
                        turn_id = NULL, chat_turn = ?4, message_id = ?5,
                        last_sequence = ?6, updated_at = ?7
                    WHERE conversation_id = ?1
                    "#,
                    params![
                        conversation_id,
                        run_id,
                        thread_id,
                        turn,
                        agent.id,
                        sequence,
                        now
                    ],
                )?;
                Ok(turn)
            })();
            finish_transaction(conn, result)
        })
    }

    pub(crate) fn set_state(
        &self,
        conversation_id: &str,
        phase: RuntimePhase,
        run_id: Option<&str>,
        thread_id: Option<&str>,
        turn_id: Option<&str>,
        chat_turn: Option<i64>,
        message_id: Option<&str>,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.db.with_conn(|conn| {
            conn.execute(
                r#"
                UPDATE chat_runtime
                SET phase = ?2, run_id = ?3, thread_id = ?4, turn_id = ?5,
                    chat_turn = ?6, message_id = ?7, updated_at = ?8
                WHERE conversation_id = ?1
                "#,
                params![
                    conversation_id,
                    phase.as_str(),
                    run_id,
                    thread_id,
                    turn_id,
                    chat_turn,
                    message_id,
                    now
                ],
            )?;
            Ok(())
        })
    }

    pub(crate) fn set_last_client_request_id(
        &self,
        conversation_id: &str,
        client_request_id: &str,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.db.with_conn(|conn| {
            conn.execute(
                "UPDATE chat_runtime SET last_client_request_id = ?2, updated_at = ?3 WHERE conversation_id = ?1",
                params![conversation_id, client_request_id, now],
            )?;
            Ok(())
        })
    }

    pub(crate) fn set_last_steer_client_request_id(
        &self,
        conversation_id: &str,
        client_request_id: &str,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.db.with_conn(|conn| {
            conn.execute(
                "UPDATE chat_runtime SET last_steer_client_request_id = ?2, updated_at = ?3 WHERE conversation_id = ?1",
                params![conversation_id, client_request_id, now],
            )?;
            Ok(())
        })
    }

    pub(crate) fn begin_operation(
        &self,
        conversation_id: &str,
        operation_kind: &str,
        client_request_id: &str,
        run_id: Option<&str>,
    ) -> Result<OperationState> {
        let now = Utc::now().to_rfc3339();
        self.db.with_conn(|conn| {
            let inserted = conn.execute(
                r#"
                INSERT INTO chat_runtime_operations
                    (conversation_id, operation_kind, client_request_id, run_id, status, created_at)
                VALUES (?1, ?2, ?3, ?4, 'pending', ?5)
                ON CONFLICT(conversation_id, operation_kind, client_request_id) DO NOTHING
                "#,
                params![
                    conversation_id,
                    operation_kind,
                    client_request_id,
                    run_id,
                    now
                ],
            )?;
            if inserted != 0 {
                return Ok(OperationState::New);
            }
            let status: String = conn.query_row(
                "SELECT status FROM chat_runtime_operations WHERE conversation_id = ?1 AND operation_kind = ?2 AND client_request_id = ?3",
                params![conversation_id, operation_kind, client_request_id],
                |row| row.get(0),
            )?;
            match status.as_str() {
                "pending" => Ok(OperationState::Pending),
                "accepted" => Ok(OperationState::Accepted),
                "failed" => Ok(OperationState::Failed),
                _ => Err(AppError::InvalidArg("invalid runtime operation status".into())),
            }
        })
    }

    pub(crate) fn mark_operation(
        &self,
        conversation_id: &str,
        operation_kind: &str,
        client_request_id: &str,
        status: OperationState,
        run_id: Option<&str>,
    ) -> Result<()> {
        let status = match status {
            OperationState::Accepted => "accepted",
            OperationState::Failed => "failed",
            OperationState::New | OperationState::Pending => "pending",
        };
        self.db.with_conn(|conn| {
            conn.execute(
                r#"
                UPDATE chat_runtime_operations
                SET status = ?4, run_id = COALESCE(?5, run_id)
                WHERE conversation_id = ?1 AND operation_kind = ?2 AND client_request_id = ?3
                "#,
                params![
                    conversation_id,
                    operation_kind,
                    client_request_id,
                    status,
                    run_id
                ],
            )?;
            Ok(())
        })
    }

    pub(crate) fn reply_was_recorded(
        &self,
        conversation_id: &str,
        client_request_id: &str,
    ) -> Result<bool> {
        self.db.with_conn(|conn| {
            conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM chat_runtime_replies WHERE conversation_id = ?1 AND client_request_id = ?2)",
                params![conversation_id, client_request_id],
                |row| row.get(0),
            )
            .map_err(Into::into)
        })
    }

    pub(crate) fn record_reply(
        &self,
        conversation_id: &str,
        run_id: &str,
        client_request_id: &str,
        request_id: &str,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.db.with_conn(|conn| {
            conn.execute_batch("BEGIN IMMEDIATE")?;
            let result = (|| {
                conn.execute(
                    r#"
                    INSERT INTO chat_runtime_replies
                        (conversation_id, run_id, client_request_id, request_id, created_at)
                    VALUES (?1, ?2, ?3, ?4, ?5)
                    ON CONFLICT(conversation_id, client_request_id) DO NOTHING
                    "#,
                    params![conversation_id, run_id, client_request_id, request_id, now],
                )?;
                conn.execute(
                    "DELETE FROM chat_runtime_requests WHERE conversation_id = ?1 AND request_id = ?2",
                    params![conversation_id, request_id],
                )?;
                Ok(())
            })();
            finish_transaction(conn, result)
        })
    }

    pub(crate) fn add_request(
        &self,
        conversation_id: &str,
        request: &RuntimeRequest,
        server_method: &str,
        server_id: &str,
    ) -> Result<()> {
        let questions = serde_json::to_string(&request.questions)?;
        let now = Utc::now().to_rfc3339();
        self.db.with_conn(|conn| {
            conn.execute_batch("BEGIN IMMEDIATE")?;
            let result = (|| {
                conn.execute(
                    r#"
                    INSERT INTO chat_runtime_requests
                        (conversation_id, request_id, run_id, kind, title, detail,
                         questions_json, server_method, server_id, created_at)
                    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                    ON CONFLICT(conversation_id, request_id) DO NOTHING
                    "#,
                    params![
                        conversation_id,
                        request.id,
                        request.run_id,
                        request.kind.as_str(),
                        request.title,
                        request.detail,
                        questions,
                        server_method,
                        server_id,
                        now,
                    ],
                )?;
                conn.execute(
                    "UPDATE chat_runtime SET phase = 'waiting', updated_at = ?2 WHERE conversation_id = ?1",
                    params![conversation_id, now],
                )?;
                Ok(())
            })();
            finish_transaction(conn, result)
        })
    }

    pub(crate) fn request(
        &self,
        conversation_id: &str,
        request_id: &str,
    ) -> Result<Option<PersistedRequest>> {
        self.db.with_conn(|conn| {
            conn.query_row(
                r#"
                SELECT request_id, run_id, kind, title, detail, questions_json,
                       server_method, server_id
                FROM chat_runtime_requests
                WHERE conversation_id = ?1 AND request_id = ?2
                "#,
                params![conversation_id, request_id],
                |row| {
                    let kind_raw: String = row.get(2)?;
                    let kind = RuntimeRequestKind::parse(&kind_raw).ok_or_else(|| {
                        rusqlite::Error::FromSqlConversionFailure(
                            2,
                            rusqlite::types::Type::Text,
                            Box::new(std::io::Error::new(
                                std::io::ErrorKind::InvalidData,
                                format!("invalid runtime request kind: {kind_raw}"),
                            )),
                        )
                    })?;
                    let questions_json: String = row.get(5)?;
                    let questions = serde_json::from_str(&questions_json).map_err(|error| {
                        rusqlite::Error::FromSqlConversionFailure(
                            5,
                            rusqlite::types::Type::Text,
                            Box::new(error),
                        )
                    })?;
                    Ok(PersistedRequest {
                        request: RuntimeRequest {
                            id: row.get(0)?,
                            run_id: row.get(1)?,
                            kind,
                            title: row.get(3)?,
                            detail: row.get(4)?,
                            questions,
                        },
                        server_id: row.get(7)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
        })
    }

    pub(crate) fn pending_wire_requests(
        &self,
        conversation_id: &str,
    ) -> Result<Vec<PersistedRequest>> {
        self.db.with_conn(|conn| {
            let mut stmt = conn.prepare(
                r#"
                SELECT request_id, run_id, kind, title, detail, questions_json,
                       server_method, server_id
                FROM chat_runtime_requests
                WHERE conversation_id = ?1
                ORDER BY created_at ASC, request_id ASC
                "#,
            )?;
            let rows = stmt.query_map(params![conversation_id], |row| {
                let kind_raw: String = row.get(2)?;
                let kind = RuntimeRequestKind::parse(&kind_raw).ok_or_else(|| {
                    rusqlite::Error::FromSqlConversionFailure(
                        2,
                        rusqlite::types::Type::Text,
                        Box::new(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            format!("invalid runtime request kind: {kind_raw}"),
                        )),
                    )
                })?;
                let questions_json: String = row.get(5)?;
                let questions = serde_json::from_str(&questions_json).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        5,
                        rusqlite::types::Type::Text,
                        Box::new(error),
                    )
                })?;
                Ok(PersistedRequest {
                    request: RuntimeRequest {
                        id: row.get(0)?,
                        run_id: row.get(1)?,
                        kind,
                        title: row.get(3)?,
                        detail: row.get(4)?,
                        questions,
                    },
                    server_id: row.get(7)?,
                })
            })?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row?);
            }
            Ok(out)
        })
    }

    #[cfg(test)]
    pub(crate) fn remove_request(&self, conversation_id: &str, request_id: &str) -> Result<bool> {
        self.db.with_conn(|conn| {
            let n = conn.execute(
                "DELETE FROM chat_runtime_requests WHERE conversation_id = ?1 AND request_id = ?2",
                params![conversation_id, request_id],
            )?;
            Ok(n != 0)
        })
    }

    pub(crate) fn clear_requests(&self, conversation_id: &str) -> Result<()> {
        self.db.with_conn(|conn| {
            conn.execute(
                "DELETE FROM chat_runtime_requests WHERE conversation_id = ?1",
                params![conversation_id],
            )?;
            Ok(())
        })
    }

    fn pending_requests_conn(
        &self,
        conn: &rusqlite::Connection,
        conversation_id: &str,
    ) -> Result<Vec<RuntimeRequest>> {
        let mut stmt = conn.prepare(
            r#"
                SELECT request_id, run_id, kind, title, detail, questions_json
                FROM chat_runtime_requests
                WHERE conversation_id = ?1
                ORDER BY created_at ASC, request_id ASC
                "#,
        )?;
        let rows = stmt.query_map(params![conversation_id], |row| {
            let kind_raw: String = row.get(2)?;
            let kind = RuntimeRequestKind::parse(&kind_raw).ok_or_else(|| {
                rusqlite::Error::FromSqlConversionFailure(
                    2,
                    rusqlite::types::Type::Text,
                    Box::new(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("invalid runtime request kind: {kind_raw}"),
                    )),
                )
            })?;
            let questions_json: String = row.get(5)?;
            let questions =
                serde_json::from_str::<Vec<RuntimeQuestion>>(&questions_json).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        5,
                        rusqlite::types::Type::Text,
                        Box::new(error),
                    )
                })?;
            Ok(RuntimeRequest {
                id: row.get(0)?,
                run_id: row.get(1)?,
                kind,
                title: row.get(3)?,
                detail: row.get(4)?,
                questions,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    fn record_conn(
        &self,
        conn: &rusqlite::Connection,
        conversation_id: &str,
    ) -> Result<Option<RuntimeRecord>> {
        conn.query_row(
            r#"
            SELECT conversation_id, enabled, run_id, phase, last_sequence,
                   thread_id, turn_id, chat_turn, message_id,
                   last_client_request_id, last_steer_client_request_id
            FROM chat_runtime WHERE conversation_id = ?1
            "#,
            params![conversation_id],
            |row| {
                let phase_raw: String = row.get(3)?;
                let phase = RuntimePhase::parse(&phase_raw).ok_or_else(|| {
                    rusqlite::Error::FromSqlConversionFailure(
                        3,
                        rusqlite::types::Type::Text,
                        Box::new(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            format!("invalid runtime phase: {phase_raw}"),
                        )),
                    )
                })?;
                Ok(RuntimeRecord {
                    conversation_id: row.get(0)?,
                    enabled: row.get::<_, i64>(1)? != 0,
                    run_id: row.get(2)?,
                    phase,
                    last_sequence: row.get(4)?,
                    thread_id: row.get(5)?,
                    turn_id: row.get(6)?,
                    chat_turn: row.get(7)?,
                    message_id: row.get(8)?,
                    last_client_request_id: row.get(9)?,
                    last_steer_client_request_id: row.get(10)?,
                })
            },
        )
        .optional()
        .map_err(Into::into)
    }

    fn current_message_conn(
        &self,
        conn: &rusqlite::Connection,
        message_id: Option<&str>,
    ) -> Result<Option<ChatMessage>> {
        let Some(message_id) = message_id else {
            return Ok(None);
        };
        conn.query_row(
            r#"
            SELECT id, conversation_id, turn, role, agent_id, content,
                   status, exit_code, duration_ms, error, created_at
            FROM chat_messages WHERE id = ?1
            "#,
            params![message_id],
            |row| {
                let role_raw: String = row.get(3)?;
                let role = ChatRole::parse(&role_raw).ok_or_else(|| {
                    rusqlite::Error::FromSqlConversionFailure(
                        3,
                        rusqlite::types::Type::Text,
                        Box::new(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            format!("invalid chat role: {role_raw}"),
                        )),
                    )
                })?;
                let agent_raw: Option<String> = row.get(4)?;
                let agent_id = agent_raw
                    .as_deref()
                    .map(|raw| {
                        AgentId::parse(raw).ok_or_else(|| {
                            rusqlite::Error::FromSqlConversionFailure(
                                4,
                                rusqlite::types::Type::Text,
                                Box::new(std::io::Error::new(
                                    std::io::ErrorKind::InvalidData,
                                    format!("invalid agent id: {raw}"),
                                )),
                            )
                        })
                    })
                    .transpose()?;
                let status_raw: String = row.get(6)?;
                let status = ChatMessageStatus::parse(&status_raw).ok_or_else(|| {
                    rusqlite::Error::FromSqlConversionFailure(
                        6,
                        rusqlite::types::Type::Text,
                        Box::new(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            format!("invalid chat message status: {status_raw}"),
                        )),
                    )
                })?;
                let duration_raw: i64 = row.get(8)?;
                let duration_ms = u64::try_from(duration_raw).map_err(|_| {
                    rusqlite::Error::FromSqlConversionFailure(
                        8,
                        rusqlite::types::Type::Integer,
                        Box::new(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            "negative chat message duration",
                        )),
                    )
                })?;
                Ok(ChatMessage {
                    id: row.get(0)?,
                    conversation_id: row.get(1)?,
                    turn: row.get(2)?,
                    role,
                    agent_id,
                    content: row.get(5)?,
                    status,
                    exit_code: row.get(7)?,
                    duration_ms,
                    error: row.get(9)?,
                    created_at: row.get(10)?,
                })
            },
        )
        .optional()
        .map_err(Into::into)
    }

    fn conversation_agent_conn(
        &self,
        conn: &rusqlite::Connection,
        conversation_id: &str,
    ) -> Result<Option<AgentId>> {
        let raw: Option<String> = conn
            .query_row(
                "SELECT agent_ids FROM conversations WHERE id = ?1",
                params![conversation_id],
                |row| row.get(0),
            )
            .optional()?;
        let Some(raw) = raw else { return Ok(None) };
        let agents: Vec<AgentId> = serde_json::from_str(&raw)?;
        Ok(agents.into_iter().next())
    }
}

fn insert_chat_message_conn(conn: &rusqlite::Connection, message: &ChatMessage) -> Result<()> {
    let agent_id = message.agent_id.map(|agent| agent.as_str().to_string());
    let duration = i64::try_from(message.duration_ms)
        .map_err(|_| AppError::InvalidArg("chat message duration exceeds i64 range".into()))?;
    conn.execute(
        r#"
        INSERT INTO chat_messages (
            id, conversation_id, turn, role, agent_id, content,
            status, exit_code, duration_ms, error, created_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
        "#,
        params![
            message.id,
            message.conversation_id,
            message.turn,
            message.role.as_str(),
            agent_id,
            message.content,
            message.status.as_str(),
            message.exit_code,
            duration,
            message.error,
            message.created_at,
        ],
    )?;
    Ok(())
}

fn update_chat_message_conn(conn: &rusqlite::Connection, message: &ChatMessage) -> Result<()> {
    let agent_id = message.agent_id.map(|agent| agent.as_str().to_string());
    let duration = i64::try_from(message.duration_ms)
        .map_err(|_| AppError::InvalidArg("chat message duration exceeds i64 range".into()))?;
    let changed = conn.execute(
        r#"
        UPDATE chat_messages
        SET content = ?2,
            status = ?3,
            exit_code = ?4,
            duration_ms = ?5,
            error = ?6,
            agent_id = ?7
        WHERE id = ?1
        "#,
        params![
            message.id,
            message.content,
            message.status.as_str(),
            message.exit_code,
            duration,
            message.error,
            agent_id,
        ],
    )?;
    if changed == 0 {
        return Err(AppError::NotFound(format!(
            "chat message not found: {}",
            message.id
        )));
    }
    Ok(())
}

fn insert_event_conn(
    conn: &rusqlite::Connection,
    conversation_id: &str,
    phase: RuntimePhase,
    run_id: Option<&str>,
    event_json: &str,
    now: &str,
) -> Result<i64> {
    let current: i64 = conn.query_row(
        "SELECT last_sequence FROM chat_runtime WHERE conversation_id = ?1",
        params![conversation_id],
        |row| row.get(0),
    )?;
    let sequence = current
        .checked_add(1)
        .ok_or_else(|| AppError::InvalidArg("runtime event sequence exhausted".into()))?;
    conn.execute(
        r#"
        INSERT INTO chat_runtime_events
            (conversation_id, sequence, event_json, created_at)
        VALUES (?1, ?2, ?3, ?4)
        "#,
        params![conversation_id, sequence, event_json, now],
    )?;
    conn.execute(
        r#"
        UPDATE chat_runtime
        SET phase = ?2, run_id = ?3, last_sequence = ?4, updated_at = ?5
        WHERE conversation_id = ?1
        "#,
        params![conversation_id, phase.as_str(), run_id, sequence, now],
    )?;
    conn.execute(
        r#"
        DELETE FROM chat_runtime_events
        WHERE conversation_id = ?1
          AND sequence <= (
            SELECT COALESCE(MAX(sequence), 0) - 2048
            FROM chat_runtime_events WHERE conversation_id = ?1
          )
        "#,
        params![conversation_id],
    )?;
    Ok(sequence)
}

fn finish_transaction<T>(conn: &rusqlite::Connection, result: Result<T>) -> Result<T> {
    match result {
        Ok(value) => match conn.execute_batch("COMMIT") {
            Ok(()) => Ok(value),
            Err(error) => {
                let _ = conn.execute_batch("ROLLBACK");
                Err(error.into())
            }
        },
        Err(error) => {
            let _ = conn.execute_batch("ROLLBACK");
            Err(error)
        }
    }
}

impl RuntimeRequestKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Command => "command",
            Self::File => "file",
            Self::Question => "question",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "command" => Some(Self::Command),
            "file" => Some(Self::File),
            "question" => Some(Self::Question),
            _ => None,
        }
    }
}
