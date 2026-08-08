//! Conversations + chat_messages repository — storage boundary only.

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
        let allow = if record.allow_dangerous { 1 } else { 0 };
        self.db.with_conn(|conn| {
            conn.execute(
                r#"
                INSERT INTO conversations (
                    id, title, agent_ids, cwd, allow_dangerous, created_at, updated_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                "#,
                params![
                    record.id,
                    record.title,
                    agent_ids,
                    record.cwd,
                    allow,
                    record.created_at,
                    record.updated_at,
                ],
            )?;
            Ok(())
        })
    }

    /// Newest-first by `updated_at`, then `id`.
    pub fn list_conversations(&self) -> Result<Vec<Conversation>> {
        self.db.with_conn(|conn| {
            let mut stmt = conn.prepare(
                r#"
                SELECT id, title, agent_ids, cwd, allow_dangerous, created_at, updated_at
                FROM conversations
                ORDER BY updated_at DESC, id DESC
                "#,
            )?;
            let rows = stmt.query_map([], map_conversation_row)?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row?);
            }
            Ok(out)
        })
    }

    pub fn get_conversation(&self, id: &str) -> Result<Option<Conversation>> {
        self.db.with_conn(|conn| {
            conn.query_row(
                r#"
                SELECT id, title, agent_ids, cwd, allow_dangerous, created_at, updated_at
                FROM conversations
                WHERE id = ?1
                "#,
                params![id],
                map_conversation_row,
            )
            .optional()
            .map_err(AppError::from)
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
                    updated_at = ?6
                WHERE id = ?1
                "#,
                params![
                    record.id,
                    record.title,
                    agent_ids,
                    record.cwd,
                    allow,
                    record.updated_at,
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
        self.db.with_conn(|conn| next_turn_conn(conn, conversation_id))
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
                Ok(turn) => {
                    conn.execute_batch("COMMIT")?;
                    Ok(turn)
                }
                Err(e) => {
                    let _ = conn.execute_batch("ROLLBACK");
                    Err(e)
                }
            }
        })
    }
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
mod tests {
    use super::*;
    use chrono::Utc;
    use tempfile::tempdir;
    use uuid::Uuid;

    fn sample_conv(id: &str, agents: Vec<AgentId>) -> Conversation {
        let now = Utc::now().to_rfc3339();
        Conversation {
            id: id.into(),
            title: "t".into(),
            agent_ids: agents,
            cwd: Some("/tmp".into()),
            allow_dangerous: false,
            created_at: now.clone(),
            updated_at: now,
        }
    }

    fn sample_msg(id: &str, conv: &str, turn: i64, role: ChatRole) -> ChatMessage {
        ChatMessage {
            id: id.into(),
            conversation_id: conv.into(),
            turn,
            role,
            agent_id: if role == ChatRole::Agent {
                Some(AgentId::Claude)
            } else {
                None
            },
            content: "hi".into(),
            status: ChatMessageStatus::Ok,
            exit_code: None,
            duration_ms: 0,
            error: None,
            created_at: Utc::now().to_rfc3339(),
        }
    }

    #[test]
    fn crud_and_cascade_delete() {
        let dir = tempdir().unwrap();
        let db = Database::open(&dir.path().join("t.db")).unwrap();
        let repo = ChatRepo::new(db);

        let c = sample_conv("c1", vec![AgentId::Claude, AgentId::Codex]);
        repo.create_conversation(&c).unwrap();
        assert_eq!(repo.list_conversations().unwrap().len(), 1);
        let got = repo.get_conversation("c1").unwrap().expect("found");
        assert_eq!(got.agent_ids, vec![AgentId::Claude, AgentId::Codex]);
        assert_eq!(got.cwd.as_deref(), Some("/tmp"));

        repo.insert_message(&sample_msg("m1", "c1", 1, ChatRole::User))
            .unwrap();
        repo.insert_message(&sample_msg("m2", "c1", 1, ChatRole::Agent))
            .unwrap();
        assert_eq!(repo.list_messages("c1").unwrap().len(), 2);
        assert_eq!(repo.next_turn("c1").unwrap(), 2);

        assert!(repo.delete_conversation("c1").unwrap());
        assert!(repo.get_conversation("c1").unwrap().is_none());
        assert!(repo.list_messages("c1").unwrap().is_empty());
    }

    #[test]
    fn insert_turn_messages_allocates_monotonic_turn() {
        let dir = tempdir().unwrap();
        let db = Database::open(&dir.path().join("t.db")).unwrap();
        let repo = ChatRepo::new(db);
        let c = sample_conv("c1", vec![AgentId::Claude]);
        repo.create_conversation(&c).unwrap();

        let mut user1 = sample_msg("u1", "c1", 0, ChatRole::User);
        let mut agents1 = [sample_msg("a1", "c1", 0, ChatRole::Agent)];
        agents1[0].status = ChatMessageStatus::Running;
        let t1 = repo
            .insert_turn_messages("c1", &mut user1, &mut agents1)
            .unwrap();
        assert_eq!(t1, 1);
        assert_eq!(user1.turn, 1);
        assert_eq!(agents1[0].turn, 1);

        let mut user2 = sample_msg("u2", "c1", 0, ChatRole::User);
        let mut agents2 = [sample_msg("a2", "c1", 0, ChatRole::Agent)];
        let t2 = repo
            .insert_turn_messages("c1", &mut user2, &mut agents2)
            .unwrap();
        assert_eq!(t2, 2);
        assert_eq!(repo.list_messages("c1").unwrap().len(), 4);
    }

    #[test]
    fn next_turn_empty_is_one_and_update_message_roundtrip() {
        let dir = tempdir().unwrap();
        let db = Database::open(&dir.path().join("t.db")).unwrap();
        let repo = ChatRepo::new(db);
        let c = sample_conv("c1", vec![AgentId::Claude]);
        repo.create_conversation(&c).unwrap();
        assert_eq!(repo.next_turn("c1").unwrap(), 1);

        let mut msg = sample_msg("m1", "c1", 1, ChatRole::Agent);
        msg.status = ChatMessageStatus::Running;
        msg.content = String::new();
        repo.insert_message(&msg).unwrap();

        msg.content = "done".into();
        msg.status = ChatMessageStatus::Ok;
        msg.exit_code = Some(0);
        msg.duration_ms = 42;
        repo.update_message(&msg).unwrap();

        let got = repo.list_messages("c1").unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].content, "done");
        assert_eq!(got[0].status, ChatMessageStatus::Ok);
        assert_eq!(got[0].exit_code, Some(0));
        assert_eq!(got[0].duration_ms, 42);
        assert_eq!(repo.next_turn("c1").unwrap(), 2);
    }

    #[test]
    fn update_missing_message_is_not_found() {
        let dir = tempdir().unwrap();
        let db = Database::open(&dir.path().join("t.db")).unwrap();
        let repo = ChatRepo::new(db);
        let msg = sample_msg("missing", "nope", 1, ChatRole::User);
        let err = repo.update_message(&msg).unwrap_err();
        assert_eq!(err.code(), "not_found");
    }

    #[test]
    fn update_missing_conversation_is_not_found() {
        let dir = tempdir().unwrap();
        let db = Database::open(&dir.path().join("t.db")).unwrap();
        let repo = ChatRepo::new(db);
        let mut c = sample_conv("ghost", vec![AgentId::Claude]);
        c.title = "x".into();
        let err = repo.update_conversation(&c).unwrap_err();
        assert_eq!(err.code(), "not_found");
    }

    #[test]
    fn invalid_role_errors() {
        let dir = tempdir().unwrap();
        let db = Database::open(&dir.path().join("t.db")).unwrap();
        let repo = ChatRepo::new(db.clone());
        let c = sample_conv("c1", vec![AgentId::Grok]);
        repo.create_conversation(&c).unwrap();

        db.with_conn(|conn| {
            conn.execute(
                r#"
                INSERT INTO chat_messages (
                    id, conversation_id, turn, role, content, status
                ) VALUES (?1, ?2, 1, 'bogus', 'x', 'ok')
                "#,
                params![Uuid::new_v4().to_string(), "c1"],
            )?;
            Ok(())
        })
        .unwrap();

        let err = repo.list_messages("c1").unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("invalid chat role") || msg.contains("bogus"),
            "unexpected: {msg}"
        );
    }
}
