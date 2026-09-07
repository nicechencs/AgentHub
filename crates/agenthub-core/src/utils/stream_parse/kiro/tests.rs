use crate::models::ProcessStep;

use super::parse_line;

#[test]
fn parses_verified_v2_envelope() {
    let start = parse_line(
        r#"{"type":"runStarted","data":{"payloadSchema":"acp","acpProtocolVersion":1,"engine":"v2"}}"#,
    )
    .unwrap();
    assert!(matches!(
        &start[0],
        ProcessStep::Status { phase, detail }
            if phase == "starting" && detail.as_deref() == Some("v2")
    ));

    assert_eq!(
        parse_line(
            r#"{"type":"metadata","data":{"sessionId":"43829d57-18ca-483f-b0df-054a5e1c395e","contextUsagePercentage":5.3}}"#
        ),
        Some(vec![])
    );

    let text = parse_line(
        r#"{"type":"sessionUpdate","data":{"sessionId":"43829d57-18ca-483f-b0df-054a5e1c395e","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"hi"}}}}"#,
    )
    .unwrap();
    assert_eq!(text, vec![ProcessStep::Text { text: "hi".into() }]);

    let done = parse_line(
        r#"{"type":"runFinished","data":{"sessionId":"43829d57-18ca-483f-b0df-054a5e1c395e","status":"success","stopReason":"end_turn","finalText":"hi"}}"#,
    )
    .unwrap();
    assert!(matches!(
        &done[0],
        ProcessStep::Status { phase, detail }
            if phase == "result" && detail.as_deref() == Some("end_turn")
    ));
    assert!(!done.iter().any(|s| matches!(s, ProcessStep::Text { .. })));
}

#[test]
fn run_error_from_v1_engine_is_error_step() {
    let err = parse_line(
        r#"{"type":"runError","data":{"sessionId":null,"stage":"engine","message":"--output-format stream-json is not supported on the v1 engine. Pass --agent-engine v2 (or v3)."}}"#,
    )
    .unwrap();
    assert!(matches!(
        &err[0],
        ProcessStep::Error { message } if message.contains("v1 engine")
    ));
}

#[test]
fn tool_call_and_unknown_acp_kind_are_not_raw() {
    let start = parse_line(
        r#"{"type":"sessionUpdate","data":{"sessionId":"s1","update":{"sessionUpdate":"tool_call","toolCallId":"t1","title":"Read","status":"in_progress"}}}"#,
    )
    .unwrap();
    assert!(matches!(
        &start[0],
        ProcessStep::Tool { id, name, status, .. }
            if id.as_deref() == Some("t1") && name == "Read" && status == "start"
    ));

    assert_eq!(
        parse_line(
            r#"{"type":"sessionUpdate","data":{"sessionId":"s1","update":{"sessionUpdate":"config_option_update","configOptions":[]}}}"#
        ),
        Some(vec![])
    );
}

#[test]
fn unknown_top_level_type_is_none() {
    assert!(parse_line(r#"{"type":"totally_unknown_kiro_event"}"#).is_none());
}
