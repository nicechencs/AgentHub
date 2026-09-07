use super::*;

#[test]
fn parses_command_execution() {
    let line =
        r#"{"type":"item.started","item":{"type":"command_execution","command":"ls","id":"1"}}"#;
    let steps = parse_line(line).unwrap();
    assert!(matches!(
        &steps[0],
        ProcessStep::Tool { name, status, .. } if name == "command_execution" && status == "start"
    ));
}

#[test]
fn parses_agent_message_completed() {
    let line = r#"{"type":"item.completed","item":{"type":"agent_message","text":"hello world"}}"#;
    let steps = parse_line(line).unwrap();
    assert_eq!(
        steps,
        vec![ProcessStep::Text {
            text: "hello world".into()
        }]
    );
}

#[test]
fn agent_message_updated_does_not_emit_text() {
    let line = r#"{"type":"item.updated","item":{"type":"agent_message","text":"partial"}}"#;
    let steps = parse_line(line).unwrap();
    assert!(steps.is_empty());
}
