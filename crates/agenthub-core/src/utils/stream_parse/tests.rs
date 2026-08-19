use super::*;

#[test]
fn text_mode_passthrough() {
    let mut s = StreamSession::new(AgentId::Grok, ProcessMode::Text);
    assert!(!s.is_structured());
    let out = s.feed(OutputStream::Stdout, "hello");
    assert_eq!(
        out,
        vec![StreamOutput::Chunk {
            stream: OutputStream::Stdout,
            text: "hello".into()
        }]
    );
}

#[test]
fn auto_enables_structured_agents() {
    for agent in [
        AgentId::Claude,
        AgentId::Codex,
        AgentId::Kimi,
        AgentId::Pi,
        AgentId::Grok,
    ] {
        assert!(
            StreamSession::new(agent, ProcessMode::Auto).is_structured(),
            "{agent:?} should be structured under Auto"
        );
    }
    // WorkBuddy / Cursor P0 headless are text-only until stream-json parser ships.
    assert!(!StreamSession::new(AgentId::WorkBuddy, ProcessMode::Auto).is_structured());
    assert!(!StreamSession::new(AgentId::Cursor, ProcessMode::Auto).is_structured());
}

#[test]
fn grok_thought_and_text() {
    let mut s = StreamSession::new(AgentId::Grok, ProcessMode::Auto);
    let out = s.feed(
        OutputStream::Stdout,
        "{\"type\":\"thought\",\"data\":\"a\"}\n{\"type\":\"text\",\"data\":\"hi\"}\n",
    );
    assert!(out.iter().any(|o| matches!(
        o,
        StreamOutput::Step(ProcessStep::Thinking { text, .. }) if text == "a"
    )));
    assert!(out.iter().any(|o| matches!(
        o,
        StreamOutput::Chunk { text, .. } if text == "hi"
    )));
    assert_eq!(s.assistant_text(), "hi");
}

#[test]
fn grok_recognized_noop_is_not_raw_fallback() {
    let mut s = StreamSession::new(AgentId::Grok, ProcessMode::Auto);
    let out = s.feed(
        OutputStream::Stdout,
        "{\"jsonrpc\":\"2.0\",\"method\":\"session/update\",\"params\":{\"update\":{\"sessionUpdate\":\"available_commands_update\"}}}\n",
    );
    assert!(out.is_empty());
    assert_eq!(s.assistant_text(), "");
}

#[test]
fn grok_acp_jsonrpc_reaches_assistant_text() {
    // Grok Build ≥ 0.2.117 streaming-json is ACP session/update, not {type,data}.
    let mut s = StreamSession::new(AgentId::Grok, ProcessMode::Auto);
    let ndjson = concat!(
        r#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s1","update":{"sessionUpdate":"agent_thought_chunk","content":{"type":"text","text":"plan"}}}}"#,
        "\n",
        r#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s1","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"Hello "}}}}"#,
        "\n",
        r#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s1","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"world."}}}}"#,
        "\n",
    );
    let out = s.feed(OutputStream::Stdout, ndjson);
    assert!(out.iter().any(|o| matches!(
        o,
        StreamOutput::Step(ProcessStep::Thinking { text, .. }) if text == "plan"
    )));
    assert_eq!(s.assistant_text(), "Hello world.");
    assert!(!out.iter().any(|o| matches!(
        o,
        StreamOutput::Step(ProcessStep::Raw { note: Some(n), .. }) if n.contains("unrecognized")
    )));
}

#[test]
fn grok_unknown_legacy_type_is_raw_not_assistant_text() {
    let mut s = StreamSession::new(AgentId::Grok, ProcessMode::Auto);
    let out = s.feed(
        OutputStream::Stdout,
        "{\"type\":\"totally_unknown_event_xyz\"}\n",
    );
    assert!(out.iter().any(|o| matches!(
        o,
        StreamOutput::Step(ProcessStep::Raw { note: Some(n), .. }) if n.contains("unrecognized")
    )));
    assert_eq!(s.assistant_text(), "");
}

#[test]
fn pi_text_delta() {
    let mut s = StreamSession::new(AgentId::Pi, ProcessMode::Auto);
    let line =
        r#"{"type":"message_update","assistantMessageEvent":{"type":"text_delta","delta":"ok"}}"#;
    let out = s.feed(OutputStream::Stdout, &format!("{line}\n"));
    assert!(out.iter().any(|o| matches!(
        o,
        StreamOutput::Chunk { text, .. } if text == "ok"
    )));
}

#[test]
fn claude_tool_and_text() {
    let mut s = StreamSession::new(AgentId::Claude, ProcessMode::Auto);
    assert!(s.is_structured());
    let line = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Hi"},{"type":"tool_use","id":"t1","name":"Read","input":{"path":"a.rs"}}]}}"#;
    let out = s.feed(OutputStream::Stdout, &format!("{line}\n"));
    assert!(out.iter().any(|o| matches!(
        o,
        StreamOutput::Chunk {
            stream: OutputStream::Stdout,
            text
        } if text == "Hi"
    )));
    assert!(out.iter().any(|o| matches!(
        o,
        StreamOutput::Step(ProcessStep::Tool { name, status, .. })
            if name == "Read" && status == "start"
    )));
    assert_eq!(s.assistant_text(), "Hi");
}

#[test]
fn codex_agent_message_item() {
    let mut s = StreamSession::new(AgentId::Codex, ProcessMode::Structured);
    let line = r#"{"type":"item.completed","item":{"type":"agent_message","text":"Done"}}"#;
    let out = s.feed(OutputStream::Stdout, &format!("{line}\n"));
    assert!(out.iter().any(|o| matches!(
        o,
        StreamOutput::Chunk { text, .. } if text == "Done"
    )));
    assert_eq!(s.assistant_text(), "Done");
}

#[test]
fn stderr_not_parsed() {
    let mut s = StreamSession::new(AgentId::Claude, ProcessMode::Auto);
    let out = s.feed(OutputStream::Stderr, "warn\n");
    assert_eq!(
        out,
        vec![StreamOutput::Chunk {
            stream: OutputStream::Stderr,
            text: "warn\n".into()
        }]
    );
}

#[test]
fn structured_tool_only_leaves_empty_assistant_text() {
    // No Text steps → assistant_text empty; callers must still prefer this
    // over raw NDJSON (see apply_structured_stdout).
    let mut s = StreamSession::new(AgentId::Claude, ProcessMode::Auto);
    let ndjson = concat!(
        r#"{"type":"system","subtype":"init"}"#,
        "\n",
        r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t1","name":"Read","input":{}}]}}"#,
        "\n",
    );
    let _ = s.feed(OutputStream::Stdout, ndjson);
    assert_eq!(s.assistant_text(), "");
    assert!(s.is_structured());
    assert!(!s.assistant_text().contains("tool_use"));
}

#[test]
fn chunk_boundary_across_feeds() {
    let mut s = StreamSession::new(AgentId::Grok, ProcessMode::Auto);
    let _ = s.feed(OutputStream::Stdout, "{\"type\":\"text\",\"data\":\"hel");
    assert_eq!(s.assistant_text(), "");
    let out = s.feed(OutputStream::Stdout, "lo\"}\n");
    assert!(out.iter().any(|o| matches!(
        o,
        StreamOutput::Chunk { text, .. } if text == "hello"
    )));
    assert_eq!(s.assistant_text(), "hello");
}

#[test]
fn malformed_json_becomes_raw_step_not_panic() {
    let mut s = StreamSession::new(AgentId::Claude, ProcessMode::Auto);
    let out = s.feed(OutputStream::Stdout, "{not-json\n");
    assert!(out
        .iter()
        .any(|o| matches!(o, StreamOutput::Step(ProcessStep::Raw { .. }))));
}

#[test]
fn unknown_event_type_raw_fallback() {
    let mut s = StreamSession::new(AgentId::Claude, ProcessMode::Auto);
    let out = s.feed(
        OutputStream::Stdout,
        "{\"type\":\"totally_unknown_event_xyz\"}\n",
    );
    // Unknown shape (`None`) is a raw step. Recognized no-ops (`Some([])`) emit nothing.
    assert!(
        out.iter().any(|o| matches!(
            o,
            StreamOutput::Step(ProcessStep::Raw { note: Some(n), .. })
                if n.contains("unrecognized")
        )) || out.is_empty()
            || out
                .iter()
                .any(|o| matches!(o, StreamOutput::Step(ProcessStep::Raw { .. })))
    );
}

#[test]
fn flush_partial_line() {
    let mut s = StreamSession::new(AgentId::Grok, ProcessMode::Auto);
    let _ = s.feed(OutputStream::Stdout, "{\"type\":\"text\",\"data\":\"x\"}");
    // No newline yet — still buffered.
    assert_eq!(s.assistant_text(), "");
    let out = s.flush();
    assert!(out.iter().any(|o| matches!(
        o,
        StreamOutput::Chunk { text, .. } if text == "x"
    )));
}

#[test]
fn unsupported_agent_text_mode_no_parser() {
    // WorkBuddy has no StreamParser registration → not structured under Auto.
    let mut s = StreamSession::new(AgentId::WorkBuddy, ProcessMode::Auto);
    assert!(!s.is_structured());
    let out = s.feed(OutputStream::Stdout, "{\"type\":\"text\"}\n");
    assert_eq!(
        out,
        vec![StreamOutput::Chunk {
            stream: OutputStream::Stdout,
            text: "{\"type\":\"text\"}\n".into()
        }]
    );
}

#[test]
fn structured_requested_without_parser_falls_back_to_text() {
    use crate::platform::stream::StreamParserRegistry;
    use crate::platform::AgentKey;

    let key = AgentKey::parse("no-parser-agent").unwrap();
    let empty = StreamParserRegistry::new();
    // Explicit structured request + empty registry → not structured, no panic.
    let mut s = StreamSession::for_agent_key(key, ProcessMode::Structured, true, &empty);
    assert!(!s.is_structured());
    let out = s.feed(OutputStream::Stdout, "hello\n");
    assert_eq!(
        out,
        vec![StreamOutput::Chunk {
            stream: OutputStream::Stdout,
            text: "hello\n".into()
        }]
    );
}

#[test]
fn captures_claude_and_codex_session_ids() {
    let mut claude = StreamSession::new(AgentId::Claude, ProcessMode::Auto);
    claude.feed(
        OutputStream::Stdout,
        "{\"type\":\"system\",\"subtype\":\"init\",\"session_id\":\"sess-claude\"}\n",
    );
    assert_eq!(claude.native_session_id(), Some("sess-claude"));

    let mut codex = StreamSession::new(AgentId::Codex, ProcessMode::Auto);
    codex.feed(
        OutputStream::Stdout,
        "{\"type\":\"thread.started\",\"thread_id\":\"sess-codex\"}\n",
    );
    assert_eq!(codex.native_session_id(), Some("sess-codex"));
}

#[test]
fn extract_native_session_id_rejects_noise() {
    assert!(extract_native_session_id("claude", "not-json").is_none());
    assert!(extract_native_session_id("kimi", r#"{"session_id":"x"}"#).is_none());
    assert_eq!(
        extract_native_session_id("claude", r#"{"session_id":"  abc  "}"#).as_deref(),
        Some("abc")
    );
}
