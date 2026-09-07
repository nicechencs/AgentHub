use super::*;

#[test]
fn parses_system_init() {
    let steps = parse_line(r#"{"type":"system","subtype":"init","session_id":"x"}"#).unwrap();
    assert!(matches!(
        &steps[0],
        ProcessStep::Status { phase, .. } if phase == "starting"
    ));
}

#[test]
fn parses_text_delta() {
    let steps =
        parse_line(r#"{"type":"content_block_delta","delta":{"type":"text_delta","text":"ab"}}"#)
            .unwrap();
    assert_eq!(steps, vec![ProcessStep::Text { text: "ab".into() }]);
}

#[test]
fn result_success_includes_final_text_fallback() {
    let steps =
        parse_line(r#"{"type":"result","subtype":"success","result":"final answer"}"#).unwrap();
    assert!(steps.iter().any(|s| matches!(
        s,
        ProcessStep::Text { text } if text == "final answer"
    )));
}
