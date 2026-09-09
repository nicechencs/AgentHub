use super::*;
use axum::body::to_bytes;
use axum::http::header;

fn test_turn() -> KiroTurnView<'static> {
    KiroTurnView {
        text: "pong".into(),
        model_id: "resolved-kiro-model",
    }
}

async fn body_text(response: Response) -> String {
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body");
    String::from_utf8(body.to_vec()).expect("utf8 response")
}

#[test]
fn prompt_from_messages_joins_roles() {
    let body = json!({
        "messages": [
            {"role": "system", "content": "be brief"},
            {"role": "user", "content": "hi"}
        ]
    });
    let prompt = prompt_from_body(DownstreamSurface::Messages, &body);
    assert!(prompt.contains("System: be brief"));
    assert!(prompt.contains("hi"));
}

#[test]
fn prompt_from_responses_string_input() {
    let body = json!({ "input": "hello kiro" });
    assert_eq!(
        prompt_from_body(DownstreamSurface::Responses, &body),
        "hello kiro"
    );
}

#[test]
fn encode_messages_has_text_and_actual_model() {
    let ir = kiro_ir("abc", "claude-sonnet", "pong");
    let value = encode_surface(DownstreamSurface::Messages, &ir, "abc").unwrap();
    assert_eq!(
        value.get("model").and_then(Value::as_str),
        Some("claude-sonnet")
    );
    assert!(value.to_string().contains("pong"));
}

#[test]
fn encode_chat_stream_terminates_with_done() {
    let ir = kiro_ir("abc", "claude-sonnet", "pong");
    let frames = encode_surface_sse(
        DownstreamSurface::ChatCompletions,
        &ir,
        "abc",
        "claude-sonnet",
    )
    .unwrap();
    assert!(frames.iter().all(|frame| frame.ends_with("\n\n")));
    assert!(frames
        .last()
        .is_some_and(|frame| frame == "data: [DONE]\n\n"));
    assert!(frames.iter().any(|frame| frame.contains("claude-sonnet")));
}

#[test]
fn encode_messages_stream_terminates_with_message_stop() {
    let ir = kiro_ir("abc", "claude-sonnet", "pong");
    let frames =
        encode_surface_sse(DownstreamSurface::Messages, &ir, "abc", "claude-sonnet").unwrap();
    assert!(frames.iter().all(|frame| frame.ends_with("\n\n")));
    assert!(frames
        .last()
        .is_some_and(|frame| frame.contains("message_stop")));
}

#[test]
fn encode_responses_stream_terminates_with_completed_event() {
    let ir = kiro_ir("abc", "claude-sonnet", "pong");
    let frames =
        encode_surface_sse(DownstreamSurface::Responses, &ir, "abc", "claude-sonnet").unwrap();
    assert!(frames.iter().all(|frame| frame.ends_with("\n\n")));
    assert!(frames
        .last()
        .is_some_and(|frame| frame.contains("response.completed")));
    assert!(frames.iter().any(|frame| frame.contains("claude-sonnet")));
}

#[tokio::test]
async fn stream_response_is_sse_for_all_surfaces_and_uses_turn_model() {
    for (surface, terminal) in [
        (DownstreamSurface::ChatCompletions, "data: [DONE]"),
        (DownstreamSurface::Messages, "message_stop"),
        (DownstreamSurface::Responses, "response.completed"),
    ] {
        let turn = test_turn();
        let response = encode_kiro_response(surface, true, "req-1", &turn).unwrap();
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE),
            Some(&header::HeaderValue::from_static("text/event-stream"))
        );
        let body = body_text(response).await;
        assert!(body.ends_with("\n\n"));
        assert!(body.contains(terminal), "{body}");
        assert!(body.contains("resolved-kiro-model"), "{body}");
    }
}

#[tokio::test]
async fn sse_stream_emits_first_delta_before_done() {
    use std::time::Duration;

    use futures_util::StreamExt;
    use tokio::sync::mpsc::unbounded_channel;

    let (tx, rx) = unbounded_channel();
    let mut stream = std::pin::pin!(kiro_sse_byte_stream(
        DownstreamSurface::ChatCompletions,
        "req-1".into(),
        "resolved-kiro-model".into(),
        rx,
        "po".into(),
    ));
    let mut seen = String::new();
    while !seen.contains("po") {
        let chunk = tokio::time::timeout(Duration::from_millis(200), stream.next())
            .await
            .expect("first delta must not wait for upstream EOF")
            .expect("stream open")
            .expect("bytes");
        seen.push_str(&String::from_utf8_lossy(&chunk));
    }
    assert!(seen.contains("resolved-kiro-model"), "{seen}");
    assert!(
        !seen.contains("ng"),
        "later delta must not be present yet: {seen}"
    );
    assert!(
        !seen.contains("data: [DONE]"),
        "terminal frame must wait for Done: {seen}"
    );
    tx.send(Ok(KiroStreamItem::Text("ng".into())))
        .expect("send rest");
    tx.send(Ok(KiroStreamItem::Done)).expect("send done");
    while let Some(chunk) = stream.next().await {
        seen.push_str(&String::from_utf8_lossy(&chunk.expect("bytes")));
    }
    assert!(seen.contains("ng"), "{seen}");
    assert!(seen.contains("data: [DONE]"), "{seen}");
}

#[tokio::test]
async fn stream_without_first_delta_is_upstream_error_not_sse() {
    use tokio::sync::mpsc::unbounded_channel;

    let (tx, rx) = unbounded_channel();
    drop(tx);
    let response = response_from_kiro_stream(
        DownstreamSurface::ChatCompletions,
        "req-1".into(),
        Some("resolved-kiro-model".into()),
        rx,
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
    assert_ne!(
        response.headers().get(header::CONTENT_TYPE),
        Some(&header::HeaderValue::from_static("text/event-stream"))
    );
}

#[tokio::test]
async fn non_stream_response_is_json_for_all_surfaces_and_uses_turn_model() {
    for surface in [
        DownstreamSurface::ChatCompletions,
        DownstreamSurface::Messages,
        DownstreamSurface::Responses,
    ] {
        let turn = test_turn();
        let response = encode_kiro_response(surface, false, "req-1", &turn).unwrap();
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE),
            Some(&header::HeaderValue::from_static("application/json"))
        );
        let body = body_text(response).await;
        let json: Value = serde_json::from_str(&body).expect("json response");
        assert_eq!(json["model"], "resolved-kiro-model");
        assert!(body.contains("pong"), "{body}");
    }
}
