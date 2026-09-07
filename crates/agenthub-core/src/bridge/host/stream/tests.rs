use std::collections::VecDeque;
use std::convert::Infallible;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use async_stream::stream;
use axum::body::Body;
use axum::extract::State;
use axum::http::header;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::Router;
use serde_json::json;

use super::sse_buffer_is_legal_trailer;
use crate::bridge::{
    BridgeLocalSurface, BridgeRuntimeHost, BridgeStartSpec, BridgeUpstreamConfig,
    BridgeUpstreamProtocol, ResolvedAuth,
};

const CREATED: &[u8] = br#"data: {"type":"response.created","response":{"id":"resp_stream","model":"gpt-5","status":"in_progress"}}

"#;
const DELTA: &[u8] = br#"data: {"type":"response.output_text.delta","delta":"hello"}

"#;
const COMPLETED: &[u8] = br#"data: {"type":"response.completed","response":{"id":"resp_stream","model":"gpt-5","status":"completed","output":[]}}

"#;
const TRAILER: &[u8] = b": keep-alive\n\n\n\n";

fn deque(bytes: &[u8]) -> VecDeque<u8> {
    bytes.iter().copied().collect()
}

fn concat(parts: &[&[u8]]) -> Vec<u8> {
    parts.iter().copied().flatten().copied().collect()
}

fn spec(profile_id: &str, port: u16, upstream_port: u16) -> BridgeStartSpec {
    BridgeStartSpec::new(
        profile_id,
        port,
        "local-test-token",
        BridgeUpstreamConfig {
            base_url: format!("http://127.0.0.1:{upstream_port}/v1/"),
            model: Some("grok-4.5".to_owned()),
            source_id: Some("connection-test".to_owned()),
            auth: ResolvedAuth::bearer("upstream-test-token"),
            protocol: BridgeUpstreamProtocol::XaiResponsesOauth,
            local_surface: BridgeLocalSurface::Messages,
        },
    )
}

fn messages_spec(profile_id: &str, port: u16, upstream_port: u16) -> BridgeStartSpec {
    spec(profile_id, port, upstream_port)
}

fn chat_spec(profile_id: &str, port: u16, upstream_port: u16) -> BridgeStartSpec {
    let mut configured = spec(profile_id, port, upstream_port);
    configured.upstream.local_surface = BridgeLocalSurface::ChatCompletions;
    configured
}

async fn grok_responses_sse_upstream(chunks: Vec<Vec<u8>>) -> (u16, tokio::task::JoinHandle<()>) {
    async fn responses(State(chunks): State<Vec<Vec<u8>>>) -> Response {
        let output = stream! {
            for chunk in chunks {
                yield Ok::<_, Infallible>(axum::body::Bytes::from(chunk));
            }
        };
        (
            [(header::CONTENT_TYPE, "text/event-stream")],
            Body::from_stream(output),
        )
            .into_response()
    }
    let listener =
        tokio::net::TcpListener::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
            .await
            .expect("bind Grok Responses SSE");
    let port = listener.local_addr().expect("addr").port();
    let task = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new()
                .route("/v1/responses", post(responses))
                .with_state(chunks),
        )
        .await
        .expect("serve Grok Responses SSE");
    });
    (port, task)
}

async fn client() -> reqwest::Client {
    reqwest::Client::builder().build().expect("test client")
}

async fn post_messages_stream(port: u16) -> String {
    client()
        .await
        .post(format!("http://127.0.0.1:{port}/v1/messages"))
        .header("x-api-key", "local-test-token")
        .json(&json!({
            "model": "claude-test",
            "max_tokens": 32,
            "stream": true,
            "messages": [{ "role": "user", "content": "hello" }]
        }))
        .send()
        .await
        .expect("messages stream request")
        .text()
        .await
        .expect("messages stream body")
}

async fn post_chat_stream(port: u16) -> String {
    client()
        .await
        .post(format!("http://127.0.0.1:{port}/v1/chat/completions"))
        .header(header::AUTHORIZATION, "Bearer local-test-token")
        .json(&json!({
            "model": "test",
            "stream": true,
            "messages": [{ "role": "user", "content": "hello" }]
        }))
        .send()
        .await
        .expect("chat stream request")
        .text()
        .await
        .expect("chat stream body")
}

fn assert_messages_completed(body: &str) {
    assert!(body.contains("event: message_stop"), "{body}");
    assert!(body.contains("\"text\":\"hello\""), "{body}");
    assert!(
        !body.contains("The upstream model provider returned an invalid stream."),
        "{body}"
    );
}

fn assert_chat_completed(body: &str) {
    assert!(body.contains("\"content\":\"hello\""), "{body}");
    assert!(body.contains("data: [DONE]"), "{body}");
    assert!(
        !body.contains("The upstream model provider returned an invalid stream."),
        "{body}"
    );
}

fn assert_stream_error(body: &str) {
    assert!(
        body.contains("The upstream model provider returned an invalid stream."),
        "{body}"
    );
}

#[test]
fn legal_trailer_accepts_comments_empty_frames_whitespace_and_done() {
    assert!(sse_buffer_is_legal_trailer(&deque(b"")));
    assert!(sse_buffer_is_legal_trailer(&deque(b"\n")));
    assert!(sse_buffer_is_legal_trailer(&deque(b": keep-alive\n\n\n\n")));
    assert!(sse_buffer_is_legal_trailer(&deque(
        b": comment without delimiter"
    )));
    assert!(sse_buffer_is_legal_trailer(&deque(b"data: \n\n")));
    assert!(sse_buffer_is_legal_trailer(&deque(b"data: [DONE]\n\n")));
}

#[test]
fn legal_trailer_rejects_data_and_corrupt_frames() {
    assert!(!sse_buffer_is_legal_trailer(&deque(
        br#"data: {"type":"response.in_progress"}

"#
    )));
    assert!(!sse_buffer_is_legal_trailer(&deque(b"data: not-json\n\n")));
    assert!(!sse_buffer_is_legal_trailer(&deque(b"data: {\"partial\"")));
}

#[tokio::test]
async fn messages_stream_accepts_same_chunk_trailer_after_completed() {
    let (upstream_port, upstream_task) =
        grok_responses_sse_upstream(vec![concat(&[CREATED, DELTA, COMPLETED, TRAILER])]).await;
    let host = BridgeRuntimeHost::new();
    let status = host
        .start(messages_spec(
            "messages-sse-trailer-same-chunk",
            0,
            upstream_port,
        ))
        .await
        .expect("start");
    let body = post_messages_stream(status.port).await;
    assert_messages_completed(&body);
    host.stop("messages-sse-trailer-same-chunk")
        .await
        .expect("stop");
    upstream_task.abort();
}

#[tokio::test]
async fn messages_stream_accepts_cross_chunk_trailer_after_completed() {
    let (upstream_port, upstream_task) =
        grok_responses_sse_upstream(vec![concat(&[CREATED, DELTA, COMPLETED]), TRAILER.to_vec()])
            .await;
    let host = BridgeRuntimeHost::new();
    let status = host
        .start(messages_spec(
            "messages-sse-trailer-cross-chunk",
            0,
            upstream_port,
        ))
        .await
        .expect("start");
    let body = post_messages_stream(status.port).await;
    assert_messages_completed(&body);
    host.stop("messages-sse-trailer-cross-chunk")
        .await
        .expect("stop");
    upstream_task.abort();
}

#[tokio::test]
async fn messages_stream_without_terminal_still_fails() {
    let (upstream_port, upstream_task) =
        grok_responses_sse_upstream(vec![concat(&[CREATED, DELTA])]).await;
    let host = BridgeRuntimeHost::new();
    let status = host
        .start(messages_spec(
            "messages-sse-trailer-no-terminal",
            0,
            upstream_port,
        ))
        .await
        .expect("start");
    let body = post_messages_stream(status.port).await;
    assert_stream_error(&body);
    host.stop("messages-sse-trailer-no-terminal")
        .await
        .expect("stop");
    upstream_task.abort();
}

#[tokio::test]
async fn chat_stream_accepts_same_chunk_trailer_after_completed() {
    let (upstream_port, upstream_task) =
        grok_responses_sse_upstream(vec![concat(&[CREATED, DELTA, COMPLETED, TRAILER])]).await;
    let host = BridgeRuntimeHost::new();
    let status = host
        .start(chat_spec("chat-sse-trailer-same-chunk", 0, upstream_port))
        .await
        .expect("start");
    let body = post_chat_stream(status.port).await;
    assert_chat_completed(&body);
    host.stop("chat-sse-trailer-same-chunk")
        .await
        .expect("stop");
    upstream_task.abort();
}

#[tokio::test]
async fn chat_stream_accepts_cross_chunk_trailer_after_completed() {
    let (upstream_port, upstream_task) =
        grok_responses_sse_upstream(vec![concat(&[CREATED, DELTA, COMPLETED]), TRAILER.to_vec()])
            .await;
    let host = BridgeRuntimeHost::new();
    let status = host
        .start(chat_spec("chat-sse-trailer-cross-chunk", 0, upstream_port))
        .await
        .expect("start");
    let body = post_chat_stream(status.port).await;
    assert_chat_completed(&body);
    host.stop("chat-sse-trailer-cross-chunk")
        .await
        .expect("stop");
    upstream_task.abort();
}

#[tokio::test]
async fn chat_stream_without_terminal_still_fails() {
    let (upstream_port, upstream_task) =
        grok_responses_sse_upstream(vec![concat(&[CREATED, DELTA])]).await;
    let host = BridgeRuntimeHost::new();
    let status = host
        .start(chat_spec("chat-sse-trailer-no-terminal", 0, upstream_port))
        .await
        .expect("start");
    let body = post_chat_stream(status.port).await;
    assert_stream_error(&body);
    host.stop("chat-sse-trailer-no-terminal")
        .await
        .expect("stop");
    upstream_task.abort();
}

#[tokio::test]
async fn messages_stream_still_fails_on_corrupt_frame_after_completed() {
    let (upstream_port, upstream_task) = grok_responses_sse_upstream(vec![concat(&[
        CREATED,
        DELTA,
        COMPLETED,
        b"data: not-json\n\n",
    ])])
    .await;
    let host = BridgeRuntimeHost::new();
    let status = host
        .start(messages_spec(
            "messages-sse-trailer-corrupt",
            0,
            upstream_port,
        ))
        .await
        .expect("start");
    let body = post_messages_stream(status.port).await;
    assert_stream_error(&body);
    host.stop("messages-sse-trailer-corrupt")
        .await
        .expect("stop");
    upstream_task.abort();
}
