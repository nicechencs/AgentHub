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

#[test]
fn usage_step_maps_codex_and_grok_fields_without_inventing_totals() {
    let codex = ProcessStep::from_usage_object(&serde_json::json!({
        "inputTokens": 100,
        "cachedInputTokens": 20,
        "cacheWriteInputTokens": 0,
        "outputTokens": 10,
        "reasoningOutputTokens": 5,
        "totalTokens": 110
    }))
    .unwrap();
    match &codex {
        ProcessStep::Usage {
            input,
            output,
            cache_read,
            cache_write,
            reasoning,
            total,
            scope,
            context_window,
        } => {
            assert_eq!(*input, Some(100));
            assert_eq!(*output, Some(10));
            assert_eq!(*cache_read, Some(20));
            assert_eq!(*cache_write, Some(0));
            assert_eq!(*reasoning, Some(5));
            assert_eq!(*total, Some(110));
            assert_eq!(scope.as_deref(), None);
            assert_eq!(*context_window, None);
        }
        other => panic!("unexpected: {other:?}"),
    }
    let json = serde_json::to_string(&codex).unwrap();
    assert!(json.contains(r#""type":"usage""#));
    assert!(json.contains(r#""cacheRead":20"#));

    let grok = ProcessStep::from_usage_object(&serde_json::json!({
        "inputTokens": 18444,
        "outputTokens": 130,
        "cachedReadTokens": 11264,
        "reasoningTokens": 73
    }))
    .unwrap();
    match grok {
        ProcessStep::Usage {
            input,
            output,
            cache_read,
            reasoning,
            total,
            ..
        } => {
            assert_eq!(input, Some(18444));
            assert_eq!(output, Some(130));
            assert_eq!(cache_read, Some(11264));
            assert_eq!(reasoning, Some(73));
            assert_eq!(total, None);
        }
        other => panic!("unexpected: {other:?}"),
    }

    assert!(ProcessStep::from_usage_object(&serde_json::json!({
        "inputTokens": 0,
        "outputTokens": 0
    }))
    .is_none());
}

#[test]
fn codex_token_usage_splits_turn_and_session_without_inventing_either() {
    let steps = ProcessStep::from_codex_token_usage(&serde_json::json!({
        "last": {
            "inputTokens": 100,
            "cachedInputTokens": 20,
            "outputTokens": 10,
            "reasoningOutputTokens": 5,
            "totalTokens": 110
        },
        "total": {
            "inputTokens": 400,
            "cachedInputTokens": 80,
            "outputTokens": 40,
            "reasoningOutputTokens": 15,
            "totalTokens": 440
        },
        "modelContextWindow": 258400
    }));
    assert_eq!(steps.len(), 2);
    match &steps[0] {
        ProcessStep::Usage {
            scope,
            input,
            output,
            total,
            context_window,
            ..
        } => {
            assert_eq!(scope.as_deref(), Some("turn"));
            assert_eq!(*input, Some(100));
            assert_eq!(*output, Some(10));
            assert_eq!(*total, Some(110));
            assert_eq!(*context_window, None);
        }
        other => panic!("unexpected: {other:?}"),
    }
    match &steps[1] {
        ProcessStep::Usage {
            scope,
            input,
            output,
            total,
            context_window,
            ..
        } => {
            assert_eq!(scope.as_deref(), Some("session"));
            assert_eq!(*input, Some(400));
            assert_eq!(*output, Some(40));
            assert_eq!(*total, Some(440));
            assert_eq!(*context_window, Some(258400));
        }
        other => panic!("unexpected: {other:?}"),
    }

    let last_only = ProcessStep::from_codex_token_usage(&serde_json::json!({
        "last": { "inputTokens": 8, "outputTokens": 2, "totalTokens": 10 }
    }));
    assert_eq!(last_only.len(), 1);
    match &last_only[0] {
        ProcessStep::Usage { scope, .. } => assert_eq!(scope.as_deref(), Some("turn")),
        other => panic!("unexpected: {other:?}"),
    }
}

#[test]
fn conversation_title_from_prompt_keeps_full_semantic_phrase() {
    let long = "Use your terminal to write exactly what I asked without clipping the title";
    let title = conversation_title_from_prompt(long);
    assert_eq!(title, long);
    assert!(!title.contains('…'));
    assert!(!title.contains("..."));
}

#[test]
fn conversation_title_from_prompt_strips_paths_without_ellipsis() {
    assert_eq!(
        conversation_title_from_prompt("请在 /workspace/src/app.ts 检查问题"),
        "检查问题"
    );
    assert_eq!(
        conversation_title_from_prompt("Only modify /tmp/qa/ping.png"),
        "Only modify"
    );
    let recovered = conversation_title_from_prompt(
        "Please create or edit /workspace/src/pages/chat/ChatSessionRail.tsx to add a hover title",
    );
    assert_eq!(recovered, "Please create or edit to add a hover title");
    assert!(!recovered.contains('…'));
}
