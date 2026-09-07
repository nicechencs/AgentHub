use super::*;

#[test]
fn parses_text_delta() {
    let line =
        r#"{"type":"message_update","assistantMessageEvent":{"type":"text_delta","delta":"ok"}}"#;
    let steps = parse_line(line).unwrap();
    assert_eq!(steps, vec![ProcessStep::Text { text: "ok".into() }]);
}

#[test]
fn parses_thinking_delta() {
    let line = r#"{"type":"message_update","assistantMessageEvent":{"type":"thinking_delta","delta":"plan"}}"#;
    let steps = parse_line(line).unwrap();
    assert!(matches!(
        &steps[0],
        ProcessStep::Thinking { text, done: false } if text == "plan"
    ));
}

#[test]
fn parses_agent_start() {
    let steps = parse_line(r#"{"type":"agent_start"}"#).unwrap();
    assert!(matches!(
        &steps[0],
        ProcessStep::Status { phase, .. } if phase == "starting"
    ));
}
