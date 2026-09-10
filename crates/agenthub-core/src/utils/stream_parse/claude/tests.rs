use super::*;
use crate::models::{AgentId, OutputStream, ProcessMode, ProcessStep};
use crate::utils::stream_parse::{StreamOutput, StreamSession};

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

#[test]
fn assistant_then_result_does_not_double_text_in_session() {
    let mut s = StreamSession::new(AgentId::Claude, ProcessMode::Auto);
    let ndjson = concat!(
        r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"PONGC"}]}}"#,
        "\n",
        r#"{"type":"result","subtype":"success","result":"PONGC"}"#,
        "\n",
    );
    let out = s.feed(OutputStream::Stdout, ndjson);
    assert_eq!(s.assistant_text(), "PONGC");
    assert!(out.iter().any(|o| matches!(
        o,
        StreamOutput::Step(ProcessStep::Status { phase, .. }) if phase == "result"
    )));
    let text_chunks: Vec<_> = out
        .iter()
        .filter_map(|o| match o {
            StreamOutput::Chunk { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(text_chunks, vec!["PONGC"]);
}

#[test]
fn result_only_still_fills_assistant_text() {
    let mut s = StreamSession::new(AgentId::Claude, ProcessMode::Auto);
    let line = r#"{"type":"result","subtype":"success","result":"only-result"}"#;
    let _ = s.feed(OutputStream::Stdout, &format!("{line}\n"));
    assert_eq!(s.assistant_text(), "only-result");
}

#[test]
fn deltas_then_result_does_not_double_assistant_text() {
    let mut s = StreamSession::new(AgentId::Claude, ProcessMode::Auto);
    let ndjson = concat!(
        r#"{"type":"content_block_delta","delta":{"type":"text_delta","text":"PONG"}}"#,
        "\n",
        r#"{"type":"content_block_delta","delta":{"type":"text_delta","text":"C"}}"#,
        "\n",
        r#"{"type":"result","subtype":"success","result":"PONGC"}"#,
        "\n",
    );
    let _ = s.feed(OutputStream::Stdout, ndjson);
    assert_eq!(s.assistant_text(), "PONGC");
}
