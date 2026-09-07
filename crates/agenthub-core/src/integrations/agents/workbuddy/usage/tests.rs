use super::*;

#[test]
fn reads_provider_data_usage_and_peels_cached_tokens() {
    let line = r#"{"type":"function_call","timestamp":"2026-08-29T00:00:00.000Z","sessionId":"sess-wb","providerData":{"model":"hy3-x","messageId":"m1","conversationRequestId":"r1","usage":{"inputTokens":100,"outputTokens":20,"inputTokensDetails":[{"cached_tokens":10}]}}}"#;
    let ev = extract_workbuddy(line, None).unwrap().unwrap();
    assert_eq!(ev.agent_id, AgentId::WorkBuddy);
    assert_eq!(ev.model, "hy3-x");
    assert_eq!(ev.input_tokens, 90);
    assert_eq!(ev.cache_read_tokens, 10);
    assert_eq!(ev.output_tokens, 20);
    assert_eq!(ev.session_id.as_deref(), Some("sess-wb"));
    assert_eq!(ev.raw_hash, "workbuddy:m1:r1");
}

#[test]
fn falls_back_to_claude_message_usage() {
    let line = r#"{"timestamp":"2026-01-09T10:00:00.000Z","sessionId":"wb1","message":{"id":"m1","model":"claude-sonnet-4","usage":{"input_tokens":10,"output_tokens":5,"cache_read_input_tokens":1}}}"#;
    let mut parser = WorkBuddyParser;
    match parser.on_line(line, None) {
        UsageLineOutcome::Event(ev) => {
            assert_eq!(ev.input_tokens, 10);
            assert_eq!(ev.output_tokens, 5);
            assert_eq!(ev.cache_read_tokens, 1);
        }
        other => panic!("expected event, got {other:?}"),
    }
}

#[test]
fn skips_lines_without_usage() {
    let line = r#"{"type":"message","timestamp":"2026-08-29T00:00:00.000Z","sessionId":"sess-wb","content":"hi"}"#;
    let mut parser = WorkBuddyParser;
    assert!(matches!(
        parser.on_line(line, None),
        UsageLineOutcome::Skipped
    ));
}
