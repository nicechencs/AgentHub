use super::*;

#[test]
fn parses_assistant_text_and_tool() {
    let line = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"hi"},{"type":"tool_use","id":"1","name":"Bash","input":{"cmd":"ls"}}]}}"#;
    let steps = parse_line(line).unwrap();
    assert!(steps
        .iter()
        .any(|s| matches!(s, ProcessStep::Text { text } if text == "hi")));
    assert!(steps
        .iter()
        .any(|s| matches!(s, ProcessStep::Tool { name, .. } if name == "Bash")));
}
