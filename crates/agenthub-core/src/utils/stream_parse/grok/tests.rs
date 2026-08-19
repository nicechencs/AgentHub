use crate::models::ProcessStep;

use super::parse_line;

#[test]
fn parses_thought_and_text_data() {
    let t = parse_line(r#"{"type":"thought","data":"plan"}"#).unwrap();
    assert!(matches!(&t[0], ProcessStep::Thinking { text, .. } if text == "plan"));
    let x = parse_line(r#"{"type":"text","data":"hi"}"#).unwrap();
    assert_eq!(x, vec![ProcessStep::Text { text: "hi".into() }]);
}

#[test]
fn parses_end_status() {
    let s = parse_line(r#"{"type":"end"}"#).unwrap();
    assert!(matches!(
        &s[0],
        ProcessStep::Status { phase, .. } if phase == "result"
    ));
}

#[test]
fn parses_acp_jsonrpc_thought_and_message() {
    let thought = parse_line(
        r#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"agent-sess-1","update":{"sessionUpdate":"agent_thought_chunk","content":{"type":"text","text":"Thinking about the answer…"},"messageId":"msg-thought-1"}}}"#,
    )
    .unwrap();
    assert!(matches!(
        &thought[0],
        ProcessStep::Thinking { text, done } if text == "Thinking about the answer…" && !*done
    ));

    let hello = parse_line(
        r#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"agent-sess-1","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"Hello "},"messageId":"msg-1"}}}"#,
    )
    .unwrap();
    assert_eq!(
        hello,
        vec![ProcessStep::Text {
            text: "Hello ".into()
        }]
    );

    let world = parse_line(
        r#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"agent-sess-1","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"world."},"messageId":"msg-1"}}}"#,
    )
    .unwrap();
    assert_eq!(
        world,
        vec![ProcessStep::Text {
            text: "world.".into()
        }]
    );
}

#[test]
fn parses_acp_tool_call_and_completed_update() {
    let start = parse_line(
        r#"{"jsonrpc":"2.0","method":"session/update","params":{"update":{"sessionUpdate":"tool_call","toolCallId":"t1","title":"Read","kind":"read","status":"pending","rawInput":{"path":"a.rs"}}}}"#,
    )
    .unwrap();
    assert!(matches!(
        &start[0],
        ProcessStep::Tool { id, name, status, result, .. }
            if id.as_deref() == Some("t1") && name == "Read" && status == "start" && result.is_none()
    ));

    let done = parse_line(
        r#"{"jsonrpc":"2.0","method":"session/update","params":{"update":{"sessionUpdate":"tool_call_update","toolCallId":"t1","title":"Read","status":"completed","content":[{"type":"text","text":"fn main() {}"}]}}}"#,
    )
    .unwrap();
    assert!(matches!(
        &done[0],
        ProcessStep::Tool { id, status, result, .. }
            if id.as_deref() == Some("t1")
                && status == "end"
                && result.as_deref() == Some("fn main() {}")
    ));
}

#[test]
fn prompt_complete_is_result_status() {
    let s = parse_line(
        r#"{"jsonrpc":"2.0","method":"_x.ai/session/prompt_complete","params":{"sessionId":"s1","stopReason":"end_turn"}}"#,
    )
    .unwrap();
    assert!(matches!(
        &s[0],
        ProcessStep::Status { phase, detail }
            if phase == "result" && detail.as_deref() == Some("end_turn")
    ));
}

#[test]
fn unknown_acp_kind_is_empty_not_none() {
    let s = parse_line(
        r#"{"jsonrpc":"2.0","method":"session/update","params":{"update":{"sessionUpdate":"available_commands_update","availableCommands":[]}}}"#,
    )
    .unwrap();
    assert!(s.is_empty());

    let usage = parse_line(
        r#"{"jsonrpc":"2.0","method":"session/update","params":{"update":{"sessionUpdate":"turn_completed","usage":{"inputTokens":1}}}}"#,
    )
    .unwrap();
    assert!(usage.is_empty());
}

#[test]
fn retry_state_becomes_status() {
    let s = parse_line(
        r#"{"method":"session/update","params":{"update":{"sessionUpdate":"retry_state","attempt":2,"maxRetries":15,"reason":"timeout"}}}"#,
    )
    .unwrap();
    assert!(matches!(
        &s[0],
        ProcessStep::Status { phase, detail }
            if phase == "running" && detail.as_deref() == Some("retry 2/15: timeout")
    ));
}

#[test]
fn other_jsonrpc_methods_are_swallowed() {
    let s = parse_line(r#"{"jsonrpc":"2.0","method":"session/request_permission","params":{}}"#)
        .unwrap();
    assert!(s.is_empty());
}

#[test]
fn parses_unwrapped_session_update() {
    let s = parse_line(
        r#"{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"Hi"}}"#,
    )
    .unwrap();
    assert_eq!(s, vec![ProcessStep::Text { text: "Hi".into() }]);
}

#[test]
fn parses_xai_session_update_alias() {
    let s = parse_line(
        r#"{"method":"_x.ai/session/update","params":{"update":{"sessionUpdate":"agent_thought_chunk","content":{"type":"text","text":"hmm"}}}}"#,
    )
    .unwrap();
    assert!(matches!(
        &s[0],
        ProcessStep::Thinking { text, .. } if text == "hmm"
    ));
}

#[test]
fn failed_tool_update_maps_to_end() {
    let s = parse_line(
        r#"{"method":"session/update","params":{"update":{"sessionUpdate":"tool_call_update","tool_call_id":"t9","title":"Bash","status":"failed","content":[{"type":"text","text":"exit 1"}]}}}"#,
    )
    .unwrap();
    assert!(matches!(
        &s[0],
        ProcessStep::Tool { id, name, status, result, .. }
            if id.as_deref() == Some("t9")
                && name == "Bash"
                && status == "end"
                && result.as_deref() == Some("exit 1")
    ));
}

#[test]
fn plan_update_is_status() {
    let s = parse_line(
        r#"{"method":"session/update","params":{"update":{"sessionUpdate":"plan","planContent":"1. read\n2. edit"}}}"#,
    )
    .unwrap();
    assert!(matches!(
        &s[0],
        ProcessStep::Status { phase, detail }
            if phase == "running" && detail.as_deref() == Some("1. read\n2. edit")
    ));
}

#[test]
fn malformed_json_is_none() {
    assert!(parse_line("{not-json").is_none());
    assert!(parse_line("").is_none());
}
