//! Host-level gateway usage capture tests: real axum host + local upstreams.
//!
//! Locks two contracts: every completed exchange lands exactly once in the
//! spool with the expected tokens, and installing the spool never changes the
//! response wire bytes.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use axum::body::Body;
use axum::extract::State;
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

use crate::bridge::usage_capture::GatewayUsageEvent;
use crate::bridge::{
    BridgeLocalSurface, BridgeRuntimeHost, BridgeStartSpec, BridgeUpstreamConfig,
    BridgeUpstreamProtocol, ResolvedAuth,
};

fn spec(profile_id: &str, port: u16, upstream_port: u16) -> BridgeStartSpec {
    // One local bearer per profile: a host rejects a second edge that claims
    // the same local token (ConflictingStart).
    let local_token = format!("local-capture-token-{profile_id}");
    BridgeStartSpec::new(
        profile_id,
        port,
        local_token,
        BridgeUpstreamConfig {
            base_url: format!("http://127.0.0.1:{upstream_port}"),
            model: Some("kimi-test".to_owned()),
            source_connection_id: Some("connection-test".to_owned()),
            auth: ResolvedAuth::bearer("upstream-test-token"),
            protocol: BridgeUpstreamProtocol::OpenAiChatCompletions,
            local_surface: BridgeLocalSurface::Responses,
        },
    )
}

async fn json_upstream() -> (u16, tokio::task::JoinHandle<()>) {
    async fn chat(Json(_body): Json<Value>) -> Json<Value> {
        Json(json!({
            "id": "chat-test",
            "model": "kimi-test",
            "created": 1,
            "choices": [{ "message": { "role": "assistant", "content": "hello" }, "finish_reason": "stop" }],
            "usage": {
                "prompt_tokens": 11, "completion_tokens": 4, "total_tokens": 15,
                "prompt_tokens_details": { "cached_tokens": 3 }
            }
        }))
    }
    let listener =
        tokio::net::TcpListener::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
            .await
            .expect("bind mock json upstream");
    let port = listener.local_addr().expect("upstream addr").port();
    let task = tokio::spawn(async move {
        axum::serve(listener, Router::new().route("/chat/completions", post(chat)))
            .await
            .expect("serve mock json upstream");
    });
    (port, task)
}

async fn sse_upstream(chunks: Vec<&'static [u8]>) -> (u16, tokio::task::JoinHandle<()>) {
    async fn chat(State(chunks): State<Vec<&'static [u8]>>) -> Response {
        let output = futures_util::stream::iter(
            chunks
                .into_iter()
                .map(|chunk| Ok::<_, std::convert::Infallible>(axum::body::Bytes::from_static(chunk))),
        );
        (
            [(header::CONTENT_TYPE, "text/event-stream")],
            Body::from_stream(output),
        )
            .into_response()
    }
    let listener =
        tokio::net::TcpListener::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
            .await
            .expect("bind mock sse upstream");
    let port = listener.local_addr().expect("upstream addr").port();
    let task = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new()
                .route("/chat/completions", post(chat))
                .with_state(chunks),
        )
        .await
        .expect("serve mock sse upstream");
    });
    (port, task)
}

async fn client() -> reqwest::Client {
    reqwest::Client::builder().build().expect("test client")
}

/// Replace every UUID-shaped substring so two wire bodies that differ only in
/// the per-request id can be compared byte for byte.
fn normalize_request_ids(body: &str) -> String {
    let bytes = body.as_bytes();
    let mut out = String::with_capacity(body.len());
    let mut index = 0;
    while index < bytes.len() {
        let rest = &bytes[index..];
        let is_uuid = rest.len() >= 36
            && rest[8] == b'-'
            && rest[13] == b'-'
            && rest[18] == b'-'
            && rest[23] == b'-'
            && rest[..8]
                .iter()
                .chain(rest[9..13].iter())
                .chain(rest[14..18].iter())
                .chain(rest[19..23].iter())
                .chain(rest[24..36].iter())
                .all(|b| b.is_ascii_hexdigit());
        if is_uuid {
            out.push_str("<request-id>");
            index += 36;
        } else {
            let ch = body[index..].chars().next().expect("utf8 boundary");
            out.push(ch);
            index += ch.len_utf8();
        }
    }
    out
}

fn spool_events(dir: &std::path::Path) -> Vec<GatewayUsageEvent> {
    let mut lines = Vec::new();
    for entry in std::fs::read_dir(dir).expect("spool dir") {
        let path = entry.expect("spool entry").path();
        let name = path.file_name().expect("file name").to_string_lossy().to_string();
        assert!(name.starts_with("gateway-") && name.ends_with(".jsonl"), "{name}");
        let raw = std::fs::read_to_string(&path).expect("spool file");
        for line in raw.lines() {
            lines.push(serde_json::from_str(line).expect("one JSON object per line"));
        }
    }
    lines
}

#[tokio::test]
async fn capture_records_non_stream_and_stream_events_without_changing_frames() {
    let spool_dir = tempfile::tempdir().expect("spool tempdir");

    // Identical upstream pair so both hosts see byte-identical upstreams.
    let (plain_port, plain_task) = json_upstream().await;
    let (spied_port, spied_task) = json_upstream().await;
    let plain_host = BridgeRuntimeHost::new();
    let spied_host = BridgeRuntimeHost::new();
    spied_host.set_usage_spool_dir(spool_dir.path().to_path_buf());

    let plain = plain_host
        .start(spec("capture-plain", 0, plain_port))
        .await
        .expect("start plain host");
    let spied = spied_host
        .start(spec("capture-spied", 0, spied_port))
        .await
        .expect("start spied host");

    let http = client().await;
    let request = |port: u16, profile: &'static str, body: Value| {
        let http = http.clone();
        let url = format!("http://127.0.0.1:{port}/v1/responses");
        async move {
            http.post(url)
                .header(
                    header::AUTHORIZATION,
                    format!("Bearer local-capture-token-{profile}"),
                )
                .json(&body)
                .send()
                .await
                .expect("bridge request")
        }
    };

    // Non-stream: translated Responses JSON.
    let plain_json = request(
        plain.port,
        "capture-plain",
        json!({"model":"test","input":"hello"}),
    )
    .await;
    let spied_json = request(
        spied.port,
        "capture-spied",
        json!({"model":"test","input":"hello"}),
    )
    .await;
    assert_eq!(plain_json.status(), StatusCode::OK);
    assert_eq!(spied_json.status(), StatusCode::OK);
    let plain_body = plain_json.text().await.expect("plain body");
    let spied_body = spied_json.text().await.expect("spied body");
    assert_eq!(normalize_request_ids(&plain_body), normalize_request_ids(&spied_body));
    assert!(spied_body.contains("\"text\":\"hello\""));

    // Stream: converted Responses SSE with a usage-bearing upstream chunk.
    let chunks: Vec<&'static [u8]> = vec![
        b"data: {\"id\":\"chat-stream\",\"model\":\"kimi-test\",\"choices\":[{\"delta\":{\"content\":\"hel\"}}],\"usage\":{\"prompt_tokens\":11,\"completion_tokens\":4,\"total_tokens\":15,\"prompt_tokens_details\":{\"cached_tokens\":3}}}\n\n",
        b"data: {\"choices\":[{\"delta\":{\"content\":\"lo\"},\"finish_reason\":\"stop\"}]}\n\n",
        b"data: [DONE]\n\n",
    ];
    let (plain_sse_port, plain_sse_task) = sse_upstream(chunks.clone()).await;
    let (spied_sse_port, spied_sse_task) = sse_upstream(chunks).await;
    let plain_stream_edge = plain_host
        .start(spec("capture-plain-stream", 0, plain_sse_port))
        .await
        .expect("start plain stream edge");
    let spied_stream_edge = spied_host
        .start(spec("capture-spied-stream", 0, spied_sse_port))
        .await
        .expect("start spied stream edge");

    let plain_sse = request(
        plain_stream_edge.port,
        "capture-plain-stream",
        json!({"model":"test","input":"hello","stream":true}),
    )
    .await;
    let spied_sse = request(
        spied_stream_edge.port,
        "capture-spied-stream",
        json!({"model":"test","input":"hello","stream":true}),
    )
    .await;
    assert_eq!(plain_sse.status(), StatusCode::OK);
    assert_eq!(spied_sse.status(), StatusCode::OK);
    let plain_stream_body = plain_sse.text().await.expect("plain stream body");
    let spied_stream_body = spied_sse.text().await.expect("spied stream body");
    assert_eq!(
        normalize_request_ids(&plain_stream_body),
        normalize_request_ids(&spied_stream_body)
    );
    assert!(spied_stream_body.contains("\"delta\":\"hel\""));
    assert!(spied_stream_body.contains("response.completed"));

    // Exactly one spool line per completed exchange.
    let events = spool_events(spool_dir.path());
    assert_eq!(events.len(), 2, "one non-stream + one stream event");

    let non_stream = &events[0];
    assert_eq!(non_stream.profile_id, "capture-spied");
    assert_eq!(non_stream.surface, "responses");
    assert_eq!(non_stream.upstream_channel.as_deref(), Some("openai_chat"));
    assert_eq!(non_stream.ticket_id.as_deref(), Some("account:connection-test"));
    assert_eq!(non_stream.account_source_kind.as_deref(), Some("account"));
    assert_eq!(non_stream.account_source_id.as_deref(), Some("connection-test"));
    assert_eq!(non_stream.model.as_deref(), Some("test"));
    assert_eq!(non_stream.upstream_model.as_deref(), Some("kimi-test"));
    assert_eq!(non_stream.input_tokens, 11);
    assert_eq!(non_stream.output_tokens, 4);
    assert_eq!(non_stream.cached_input_tokens, Some(3));
    assert_eq!(non_stream.status, "ok");
    assert_eq!(non_stream.status_code, Some(200));
    assert_eq!(non_stream.attempts, Some(1));
    assert!(non_stream.ttft_ms.is_none(), "non-stream has no TTFT");
    assert!(non_stream.latency_ms.is_some());

    let stream = &events[1];
    assert_eq!(stream.input_tokens, 11);
    assert_eq!(stream.output_tokens, 4);
    assert_eq!(stream.cached_input_tokens, Some(3));
    assert_eq!(stream.status, "ok");
    assert!(stream.ttft_ms.is_some(), "stream records time to first frame");

    plain_host.shutdown().await.expect("plain shutdown");
    spied_host.shutdown().await.expect("spied shutdown");
    plain_task.abort();
    spied_task.abort();
    plain_sse_task.abort();
    spied_sse_task.abort();
}

#[tokio::test]
async fn capture_records_failed_streams_without_touching_the_error_frame() {
    let spool_dir = tempfile::tempdir().expect("spool tempdir");
    let (upstream_port, upstream_task) =
        sse_upstream(vec![b"data: private malformed content\n\n"]).await;
    let host = BridgeRuntimeHost::new();
    host.set_usage_spool_dir(spool_dir.path().to_path_buf());
    let status = host
        .start(spec("capture-failed", 0, upstream_port))
        .await
        .expect("start");

    let body = client()
        .await
        .post(format!("http://127.0.0.1:{}/v1/responses", status.port))
        .header(
            header::AUTHORIZATION,
            "Bearer local-capture-token-capture-failed",
        )
        .json(&json!({"model":"test","input":"hello","stream":true}))
        .send()
        .await
        .expect("stream request")
        .text()
        .await
        .expect("stream body");
    assert!(body.contains("The upstream model provider returned an invalid stream."));
    assert!(!body.contains("private malformed content"));

    let events = spool_events(spool_dir.path());
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].status, "failed");
    assert_eq!(events[0].error_class.as_deref(), Some("stream_error"));
    assert_eq!(events[0].profile_id, "capture-failed");
    assert_eq!(events[0].input_tokens, 0, "no usage survived the failure");
    assert!(events[0].latency_ms.is_some());

    host.stop("capture-failed").await.expect("stop");
    upstream_task.abort();
}

#[tokio::test]
async fn runtime_does_not_forward_anthropic_key_across_redirect() {
    async fn redirected_target(
        State(forwarded_key): State<Arc<Mutex<Option<String>>>>,
        headers: axum::http::HeaderMap,
    ) -> StatusCode {
        let key = headers
            .get("x-api-key")
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        *forwarded_key.lock().unwrap() = key;
        StatusCode::OK
    }

    let forwarded_key = Arc::new(Mutex::new(None));
    let target_listener = tokio::net::TcpListener::bind(SocketAddr::new(
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        0,
    ))
    .await
    .expect("bind redirect target");
    let target_port = target_listener.local_addr().expect("target addr").port();
    let target_task = tokio::spawn({
        let forwarded_key = forwarded_key.clone();
        async move {
            axum::serve(
                target_listener,
                Router::new()
                    .route("/messages", get(redirected_target).post(redirected_target))
                    .with_state(forwarded_key),
            )
            .await
            .expect("serve redirect target");
        }
    });

    let redirect_listener = tokio::net::TcpListener::bind(SocketAddr::new(
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        0,
    ))
    .await
    .expect("bind redirect source");
    let redirect_port = redirect_listener
        .local_addr()
        .expect("redirect addr")
        .port();
    let location = format!("http://127.0.0.1:{target_port}/messages");
    let redirect_task = tokio::spawn(async move {
        let app = Router::new().route("/messages", post(move || {
            let location = location.clone();
            async move {
                Response::builder()
                    .status(StatusCode::FOUND)
                    .header(axum::http::header::LOCATION, location)
                    .body(axum::body::Body::empty())
                    .unwrap()
            }
        }));
        axum::serve(redirect_listener, app)
            .await
            .expect("serve redirect source");
    });

    let host = BridgeRuntimeHost::new();
    let runtime = host
        .start(BridgeStartSpec::new(
            "redirect-runtime",
            0,
            "local-redirect-token",
            BridgeUpstreamConfig {
                base_url: format!("http://127.0.0.1:{redirect_port}"),
                model: Some("claude-test".to_owned()),
                source_connection_id: Some("anthropic-source".to_owned()),
                auth: ResolvedAuth::bearer("anthropic-upstream-secret"),
                protocol: BridgeUpstreamProtocol::AnthropicMessages,
                local_surface: BridgeLocalSurface::Messages,
            },
        ))
        .await
        .expect("start redirect runtime");

    let response = client()
        .await
        .post(format!(
            "http://127.0.0.1:{}/v1/messages",
            runtime.port
        ))
        .header(
            header::AUTHORIZATION,
            "Bearer local-redirect-token",
        )
        .json(&json!({
            "model": "claude-test",
            "max_tokens": 16,
            "messages": [{"role": "user", "content": "hello"}]
        }))
        .send()
        .await
        .expect("runtime request");
    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
    assert_eq!(
        forwarded_key.lock().unwrap().as_deref(),
        None,
        "the redirect target must never receive the Anthropic API key"
    );

    host.shutdown().await.expect("shutdown redirect runtime");
    redirect_task.abort();
    target_task.abort();
}
