use super::*;
use chrono::Utc;
use std::sync::{Arc, Barrier};
use std::thread;
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
        native_session_id: None,
        sending: false,
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
    assert!(repo
        .get_conversation("c1")
        .unwrap()
        .unwrap()
        .native_session_id
        .is_none());

    let mut stored = repo.get_conversation("c1").unwrap().unwrap();
    stored.native_session_id = Some("sess-1".into());
    repo.update_conversation(&stored).unwrap();
    assert_eq!(
        repo.get_conversation("c1")
            .unwrap()
            .unwrap()
            .native_session_id
            .as_deref(),
        Some("sess-1")
    );
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

#[test]
fn ensure_default_reuses_only_a_blank_conversation() {
    let dir = tempdir().unwrap();
    let db = Database::open(&dir.path().join("t.db")).unwrap();
    let repo = ChatRepo::new(db);

    let titled = sample_conv("titled", vec![AgentId::Claude]);
    repo.create_conversation(&titled).unwrap();

    let mut blank = sample_conv("blank", vec![AgentId::Claude]);
    blank.title = "  ".into();
    repo.create_conversation(&blank).unwrap();

    let mut candidate = sample_conv("candidate", vec![AgentId::Codex]);
    candidate.title.clear();
    let reused = repo.ensure_default_conversation(&candidate).unwrap();
    assert_eq!(reused.id, "blank");
    assert_eq!(repo.list_conversations().unwrap().len(), 2);

    repo.insert_message(&sample_msg("m1", "blank", 1, ChatRole::User))
        .unwrap();
    let created = repo.ensure_default_conversation(&candidate).unwrap();
    assert_eq!(created.id, "candidate");
    assert_eq!(repo.list_conversations().unwrap().len(), 3);
}

#[test]
fn ensure_default_is_idempotent_under_concurrent_calls() {
    let dir = tempdir().unwrap();
    let db = Database::open(&dir.path().join("t.db")).unwrap();
    let repo = Arc::new(ChatRepo::new(db));
    let barrier = Arc::new(Barrier::new(8));
    let mut handles = Vec::new();

    for i in 0..8 {
        let repo = Arc::clone(&repo);
        let barrier = Arc::clone(&barrier);
        handles.push(thread::spawn(move || {
            let mut candidate = sample_conv(&format!("candidate-{i}"), vec![AgentId::Claude]);
            candidate.title.clear();
            barrier.wait();
            repo.ensure_default_conversation(&candidate).unwrap()
        }));
    }

    let results: Vec<_> = handles
        .into_iter()
        .map(|handle| handle.join().unwrap())
        .collect();
    assert!(results.windows(2).all(|rows| rows[0].id == rows[1].id));
    assert_eq!(repo.list_conversations().unwrap().len(), 1);
}
