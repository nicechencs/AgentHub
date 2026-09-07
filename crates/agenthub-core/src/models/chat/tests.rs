use super::*;

#[test]
fn chat_role_parse() {
    assert_eq!(ChatRole::parse("user"), Some(ChatRole::User));
    assert_eq!(ChatRole::parse("AGENT"), Some(ChatRole::Agent));
    assert_eq!(ChatRole::parse("system"), None);
}

#[test]
fn chat_status_parse() {
    assert_eq!(
        ChatMessageStatus::parse("cancelled"),
        Some(ChatMessageStatus::Cancelled)
    );
    assert_eq!(
        ChatMessageStatus::parse("canceled"),
        Some(ChatMessageStatus::Cancelled)
    );
    assert_eq!(
        ChatMessageStatus::parse("running"),
        Some(ChatMessageStatus::Running)
    );
}

#[test]
fn chat_event_serde_tag() {
    let ev = ChatEvent::Error {
        message: "boom".into(),
    };
    let json = serde_json::to_string(&ev).unwrap();
    assert!(json.contains(r#""type":"error""#));
    assert!(json.contains(r#""message":"boom""#));
}

#[test]
fn chat_event_stream_variants_include_turn_camel_case() {
    let chunk = ChatEvent::AgentChunk {
        turn: 3,
        agent: AgentId::Claude,
        stream: OutputStream::Stdout,
        text: "hi".into(),
    };
    let json = serde_json::to_string(&chunk).unwrap();
    assert!(json.contains(r#""type":"agentChunk""#));
    assert!(json.contains(r#""turn":3"#));
    assert!(json.contains(r#""stream":"stdout""#));
    let back: ChatEvent = serde_json::from_str(&json).unwrap();
    match back {
        ChatEvent::AgentChunk { turn, text, .. } => {
            assert_eq!(turn, 3);
            assert_eq!(text, "hi");
        }
        other => panic!("unexpected: {other:?}"),
    }

    let started = ChatEvent::AgentStarted {
        turn: 1,
        agent: AgentId::Grok,
        command: "grok -p".into(),
    };
    let s = serde_json::to_string(&started).unwrap();
    assert!(s.contains(r#""type":"agentStarted""#));
    assert!(s.contains(r#""turn":1"#));

    let step = ChatEvent::AgentProcess {
        turn: 2,
        agent: AgentId::Codex,
        step: ProcessStep::Tool {
            id: Some("t1".into()),
            name: "shell".into(),
            input: Some(serde_json::json!({"cmd":"ls"})),
            status: "start".into(),
            result: None,
        },
    };
    let js = serde_json::to_string(&step).unwrap();
    assert!(js.contains(r#""type":"agentProcess""#));
    assert!(js.contains(r#""name":"shell""#));
    let back: ChatEvent = serde_json::from_str(&js).unwrap();
    match back {
        ChatEvent::AgentProcess { turn, step, .. } => {
            assert_eq!(turn, 2);
            assert_eq!(step.kind(), "tool");
        }
        other => panic!("unexpected: {other:?}"),
    }
}

#[test]
fn chat_message_serde_camel_case() {
    let msg = ChatMessage {
        id: "m1".into(),
        conversation_id: "c1".into(),
        turn: 2,
        role: ChatRole::Agent,
        agent_id: Some(AgentId::Codex),
        content: "body".into(),
        status: ChatMessageStatus::Ok,
        exit_code: Some(0),
        duration_ms: 10,
        error: None,
        created_at: "t".into(),
    };
    let json = serde_json::to_string(&msg).unwrap();
    assert!(json.contains(r#""conversationId":"c1""#));
    assert!(json.contains(r#""agentId":"codex""#));
    assert!(json.contains(r#""durationMs":10"#));
    let back: ChatMessage = serde_json::from_str(&json).unwrap();
    assert_eq!(back.turn, 2);
    assert_eq!(back.agent_id, Some(AgentId::Codex));
}
