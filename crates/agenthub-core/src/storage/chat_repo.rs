//! Conversations + chat_messages repository — storage boundary only.

use std::collections::HashSet;

use rusqlite::{params, OptionalExtension, Row};

use crate::error::{AppError, Result};
use crate::models::{AgentId, ChatMessage, ChatMessageStatus, ChatRole, Conversation};
use crate::storage::Database;

/// SQLite access for chat tables.
#[derive(Clone)]
pub struct ChatRepo {
    db: Database,
}

impl ChatRepo {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    pub fn create_conversation(&self, record: &Conversation) -> Result<()> {
        let agent_ids = serde_json::to_string(&record.agent_ids)?;
        self.db.with_conn(|conn| {
            insert_conversation_conn(conn, record, &agent_ids)?;
            Ok(())
        })
    }

    /// Return the existing blank conversation or create exactly one atomically.
    ///
    /// A conversation is eligible only while it has no title (apart from
    /// whitespace) and no messages.  This intentionally leaves titled or
    /// already-used conversations untouched, even when they are otherwise
    /// configured like the requested default.  The immediate transaction
    /// serializes the check-and-insert across callers and processes.
    pub fn ensure_default_conversation(&self, record: &Conversation) -> Result<Conversation> {
        let agent_ids = serde_json::to_string(&record.agent_ids)?;
        self.db.with_conn(|conn| {
            conn.execute_batch("BEGIN IMMEDIATE")?;
            let result = (|| {
                let existing = conn
                    .query_row(
                        r#"
                        SELECT c.id, c.title, c.agent_ids, c.cwd, c.allow_dangerous,
                               c.created_at, c.updated_at, c.native_session_id
                        FROM conversations AS c
                        WHERE TRIM(c.title) = ''
                          AND NOT EXISTS (
                              SELECT 1
                              FROM chat_messages AS m
                              WHERE m.conversation_id = c.id
                          )
                        ORDER BY c.updated_at DESC, c.id DESC
                        LIMIT 1
                        "#,
                        [],
                        map_conversation_row,
                    )
                    .optional()?;

                if let Some(mut conversation) = existing {
                    conversation.sending = false;
                    return Ok(conversation);
                }

                insert_conversation_conn(conn, record, &agent_ids)?;
                Ok(record.clone())
            })();

            match result {
                Ok(conversation) => match conn.execute_batch("COMMIT") {
                    Ok(()) => Ok(conversation),
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
        })
    }

    /// Newest-first by `updated_at`, then `id`.
    pub fn list_conversations(&self) -> Result<Vec<Conversation>> {
        self.db.with_conn(|conn| {
            let sending_ids = load_sending_ids(conn)?;
            let mut stmt = conn.prepare(
                r#"
                SELECT id, title, agent_ids, cwd, allow_dangerous, created_at, updated_at,
                       native_session_id
                FROM conversations
                ORDER BY updated_at DESC, id DESC
                "#,
            )?;
            let rows = stmt.query_map([], map_conversation_row)?;
            let mut out = Vec::new();
            for row in rows {
                let mut conv = row?;
                conv.sending = sending_ids.contains(&conv.id);
                out.push(conv);
            }
            Ok(out)
        })
    }

    pub fn get_conversation(&self, id: &str) -> Result<Option<Conversation>> {
        self.db.with_conn(|conn| {
            let mut conv = conn
                .query_row(
                    r#"
                    SELECT id, title, agent_ids, cwd, allow_dangerous, created_at, updated_at,
                           native_session_id
                    FROM conversations
                    WHERE id = ?1
                    "#,
                    params![id],
                    map_conversation_row,
                )
                .optional()
                .map_err(AppError::from)?;
            if let Some(ref mut c) = conv {
                c.sending = conversation_is_sending(conn, &c.id)?;
            }
            Ok(conv)
        })
    }

    pub fn update_conversation(&self, record: &Conversation) -> Result<()> {
        let agent_ids = serde_json::to_string(&record.agent_ids)?;
        let allow = if record.allow_dangerous { 1 } else { 0 };
        self.db.with_conn(|conn| {
            let n = conn.execute(
                r#"
                UPDATE conversations
                SET title = ?2,
                    agent_ids = ?3,
                    cwd = ?4,
                    allow_dangerous = ?5,
                    updated_at = ?6,
                    native_session_id = ?7
                WHERE id = ?1
                "#,
                params![
                    record.id,
                    record.title,
                    agent_ids,
                    record.cwd,
                    allow,
                    record.updated_at,
                    record.native_session_id,
                ],
            )?;
            if n == 0 {
                return Err(AppError::NotFound(format!(
                    "conversation not found: {}",
                    record.id
                )));
            }
            Ok(())
        })
    }

    /// Delete conversation; messages cascade via FK.
    pub fn delete_conversation(&self, id: &str) -> Result<bool> {
        self.db.with_conn(|conn| {
            let n = conn.execute("DELETE FROM conversations WHERE id = ?1", params![id])?;
            Ok(n > 0)
        })
    }

    pub fn insert_message(&self, msg: &ChatMessage) -> Result<()> {
        self.db.with_conn(|conn| insert_message_conn(conn, msg))
    }

    pub fn update_message(&self, msg: &ChatMessage) -> Result<()> {
        let agent_id = msg.agent_id.map(|a| a.as_str().to_string());
        let duration = i64::try_from(msg.duration_ms).map_err(|_| {
            AppError::InvalidArg(format!(
                "duration_ms exceeds i64 range: {}",
                msg.duration_ms
            ))
        })?;
        self.db.with_conn(|conn| {
            let n = conn.execute(
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
                    msg.id,
                    msg.content,
                    msg.status.as_str(),
                    msg.exit_code,
                    duration,
                    msg.error,
                    agent_id,
                ],
            )?;
            if n == 0 {
                return Err(AppError::NotFound(format!(
                    "chat message not found: {}",
                    msg.id
                )));
            }
            Ok(())
        })
    }

    /// Messages ordered by turn ASC, then id ASC (stable within a turn).
    pub fn list_messages(&self, conversation_id: &str) -> Result<Vec<ChatMessage>> {
        self.db.with_conn(|conn| {
            let mut stmt = conn.prepare(
                r#"
                SELECT id, conversation_id, turn, role, agent_id, content,
                       status, exit_code, duration_ms, error, created_at
                FROM chat_messages
                WHERE conversation_id = ?1
                ORDER BY turn ASC, id ASC
                "#,
            )?;
            let rows = stmt.query_map(params![conversation_id], map_message_row)?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row?);
            }
            Ok(out)
        })
    }

    /// Next turn number for a conversation (max+1, or 1 if empty).
    pub fn next_turn(&self, conversation_id: &str) -> Result<i64> {
        self.db
            .with_conn(|conn| next_turn_conn(conn, conversation_id))
    }

    /// Crash recovery: fold leftover `running` placeholders into `cancelled`.
    /// Does not clear `native_session_id` or rewrite error fields.
    pub fn interrupt_stale_running(&self) -> Result<u64> {
        self.db.with_conn(|conn| {
            let n = conn.execute(
                "UPDATE chat_messages SET status = ?1 WHERE status = ?2",
                params![
                    ChatMessageStatus::Cancelled.as_str(),
                    ChatMessageStatus::Running.as_str(),
                ],
            )?;
            Ok(n as u64)
        })
    }

    /// Atomically allocate the next turn and insert the user message plus agent
    /// placeholders. Holds the DB lock for the whole sequence so concurrent sends
    /// cannot observe the same `MAX(turn)`. Uses an explicit transaction so a
    /// mid-batch failure rolls back partial inserts.
    ///
    /// Mutates `user.turn` and each agent message's `turn` to the allocated value.
    pub fn insert_turn_messages(
        &self,
        conversation_id: &str,
        user: &mut ChatMessage,
        agents: &mut [ChatMessage],
    ) -> Result<i64> {
        self.db.with_conn(|conn| {
            conn.execute_batch("BEGIN IMMEDIATE")?;
            let result = (|| {
                let turn = next_turn_conn(conn, conversation_id)?;
                user.turn = turn;
                user.conversation_id = conversation_id.to_string();
                insert_message_conn(conn, user)?;
                for msg in agents.iter_mut() {
                    msg.turn = turn;
                    msg.conversation_id = conversation_id.to_string();
                    insert_message_conn(conn, msg)?;
                }
                Ok(turn)
            })();
            match result {
                Ok(turn) => match conn.execute_batch("COMMIT") {
                    Ok(()) => Ok(turn),
                    Err(error) => {
                        let _ = conn.execute_batch("ROLLBACK");
                        Err(error.into())
                    }
                },
                Err(e) => {
                    let _ = conn.execute_batch("ROLLBACK");
                    Err(e)
                }
            }
        })
    }
}

fn load_sending_ids(conn: &rusqlite::Connection) -> Result<HashSet<String>> {
    let mut stmt = conn
        .prepare("SELECT DISTINCT conversation_id FROM chat_messages WHERE status = 'running'")?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    let mut ids = HashSet::new();
    for row in rows {
        ids.insert(row?);
    }
    Ok(ids)
}

fn insert_conversation_conn(
    conn: &rusqlite::Connection,
    record: &Conversation,
    agent_ids: &str,
) -> Result<()> {
    let allow = if record.allow_dangerous { 1 } else { 0 };
    conn.execute(
        r#"
        INSERT INTO conversations (
            id, title, agent_ids, cwd, allow_dangerous, created_at, updated_at,
            native_session_id
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
        "#,
        params![
            record.id,
            record.title,
            agent_ids,
            record.cwd,
            allow,
            record.created_at,
            record.updated_at,
            record.native_session_id,
        ],
    )?;
    Ok(())
}

fn conversation_is_sending(conn: &rusqlite::Connection, id: &str) -> Result<bool> {
    let n: i64 = conn.query_row(
        r#"
        SELECT EXISTS(
            SELECT 1 FROM chat_messages
            WHERE conversation_id = ?1 AND status = 'running'
        )
        "#,
        params![id],
        |row| row.get(0),
    )?;
    Ok(n != 0)
}

fn next_turn_conn(conn: &rusqlite::Connection, conversation_id: &str) -> Result<i64> {
    let max: Option<i64> = conn
        .query_row(
            "SELECT MAX(turn) FROM chat_messages WHERE conversation_id = ?1",
            params![conversation_id],
            |row| row.get(0),
        )
        .optional()?
        .flatten();
    Ok(max.unwrap_or(0) + 1)
}

fn insert_message_conn(conn: &rusqlite::Connection, msg: &ChatMessage) -> Result<()> {
    let agent_id = msg.agent_id.map(|a| a.as_str().to_string());
    let duration = i64::try_from(msg.duration_ms).map_err(|_| {
        AppError::InvalidArg(format!(
            "duration_ms exceeds i64 range: {}",
            msg.duration_ms
        ))
    })?;
    conn.execute(
        r#"
        INSERT INTO chat_messages (
            id, conversation_id, turn, role, agent_id, content,
            status, exit_code, duration_ms, error, created_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
        "#,
        params![
            msg.id,
            msg.conversation_id,
            msg.turn,
            msg.role.as_str(),
            agent_id,
            msg.content,
            msg.status.as_str(),
            msg.exit_code,
            duration,
            msg.error,
            msg.created_at,
        ],
    )?;
    Ok(())
}

fn map_conversation_row(row: &Row<'_>) -> rusqlite::Result<Conversation> {
    let id: String = row.get(0)?;
    let title: String = row.get(1)?;
    let agent_ids_raw: String = row.get(2)?;
    let cwd: Option<String> = row.get(3)?;
    let allow_i: i64 = row.get(4)?;
    let created_at: String = row.get(5)?;
    let updated_at: String = row.get(6)?;
    let native_session_id: Option<String> = row.get(7)?;

    let agent_ids: Vec<AgentId> = serde_json::from_str(&agent_ids_raw).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(2, rusqlite::types::Type::Text, Box::new(e))
    })?;

    Ok(Conversation {
        id,
        title,
        agent_ids,
        cwd,
        allow_dangerous: allow_i != 0,
        created_at,
        updated_at,
        native_session_id,
        sending: false,
    })
}

fn map_message_row(row: &Row<'_>) -> rusqlite::Result<ChatMessage> {
    let id: String = row.get(0)?;
    let conversation_id: String = row.get(1)?;
    let turn: i64 = row.get(2)?;
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
    let agent_id = match agent_raw {
        None => None,
        Some(raw) if raw.is_empty() => None,
        Some(raw) => Some(AgentId::parse(&raw).ok_or_else(|| {
            rusqlite::Error::FromSqlConversionFailure(
                4,
                rusqlite::types::Type::Text,
                Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("invalid agent_id in chat_messages: {raw}"),
                )),
            )
        })?),
    };
    let content: String = row.get(5)?;
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
    let exit_code: Option<i32> = row.get(7)?;
    let duration_i: i64 = row.get(8)?;
    let error: Option<String> = row.get(9)?;
    let created_at: String = row.get(10)?;
    let duration_ms = u64::try_from(duration_i).map_err(|_| {
        rusqlite::Error::FromSqlConversionFailure(
            8,
            rusqlite::types::Type::Integer,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("negative duration_ms: {duration_i}"),
            )),
        )
    })?;

    Ok(ChatMessage {
        id,
        conversation_id,
        turn,
        role,
        agent_id,
        content,
        status,
        exit_code,
        duration_ms,
        error,
        created_at,
    })
}

#[cfg(test)]
mod tests;
