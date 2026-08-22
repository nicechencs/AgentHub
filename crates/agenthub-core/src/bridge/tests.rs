use std::convert::Infallible;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_stream::stream;
use axum::body::Body;
use axum::extract::State;
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use serde_json::{json, Value};
use tokio::sync::Notify;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};

use super::host::{CleanupCompletion, MAX_IN_FLIGHT_REQUESTS_PER_PROFILE};
use super::{
    protocol::responses::is_leftover_bridge_model, BridgeHostError, BridgeLocalSurface,
    BridgeRuntimeHost, BridgeRuntimeState, BridgeStartSpec, BridgeUpstreamConfig,
    BridgeUpstreamProtocol, BridgeUpstreamStatus, ResolvedAuth, UpstreamAuthReload,
};
use crate::models::{list_local_bridge_models, AdapterSourceProduct, AgentId};

fn spec_with_token(
    profile_id: &str,
    port: u16,
    upstream_port: u16,
    local_token: &str,
) -> BridgeStartSpec {
    BridgeStartSpec::new(
        profile_id,
        port,
        local_token,
        BridgeUpstreamConfig {
            base_url: format!("http://127.0.0.1:{upstream_port}"),
            model: None,
            source_connection_id: Some("connection-test".to_owned()),
            auth: ResolvedAuth::bearer("upstream-test-token"),
            protocol: BridgeUpstreamProtocol::OpenAiChatCompletions,
            local_surface: BridgeLocalSurface::Responses,
        },
    )
}

fn spec(profile_id: &str, port: u16, upstream_port: u16) -> BridgeStartSpec {
    spec_with_token(profile_id, port, upstream_port, "local-test-token")
}

fn anthropic_spec(profile_id: &str, port: u16, upstream_port: u16) -> BridgeStartSpec {
    let mut spec = spec(profile_id, port, upstream_port);
    spec.upstream.protocol = BridgeUpstreamProtocol::AnthropicMessages;
    spec.upstream.local_surface = BridgeLocalSurface::Responses;
    spec
}

fn codex_spec(profile_id: &str, port: u16, upstream_port: u16) -> BridgeStartSpec {
    let mut spec = spec(profile_id, port, upstream_port);
    spec.upstream.base_url = format!("http://127.0.0.1:{upstream_port}/v1/");
    spec.upstream.protocol = BridgeUpstreamProtocol::CodexResponsesOauth;
    spec.upstream.local_surface = BridgeLocalSurface::Messages;
    spec.upstream.auth = ResolvedAuth::bearer("oauth-upstream-token");
    spec
}

fn codex_chat_spec(profile_id: &str, port: u16, upstream_port: u16) -> BridgeStartSpec {
    let mut spec = codex_spec(profile_id, port, upstream_port);
    spec.upstream.local_surface = BridgeLocalSurface::ChatCompletions;
    spec
}

fn unsigned_jwt_exp_offset(seconds_from_now: i64) -> String {
    use base64::Engine;
    let header = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(br#"{"alg":"none"}"#);
    let exp = chrono::Utc::now().timestamp() + seconds_from_now;
    let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(format!(r#"{{"exp":{exp}}}"#).as_bytes());
    format!("{header}.{payload}.sig")
}

fn grok_claude_spec(profile_id: &str, port: u16, upstream_port: u16) -> BridgeStartSpec {
    let mut spec = spec(profile_id, port, upstream_port);
    spec.upstream.base_url = format!("http://127.0.0.1:{upstream_port}/v1/");
    spec.upstream.model = Some("grok-4.5".to_owned());
    spec.upstream.protocol = BridgeUpstreamProtocol::XaiResponsesOauth;
    spec.upstream.local_surface = BridgeLocalSurface::Messages;
    spec
}

fn grok_codex_spec(profile_id: &str, port: u16, upstream_port: u16) -> BridgeStartSpec {
    let mut spec = grok_claude_spec(profile_id, port, upstream_port);
    spec.upstream.local_surface = BridgeLocalSurface::Responses;
    spec
}

async fn upstream() -> (u16, tokio::task::JoinHandle<()>) {
    upstream_at("/chat/completions").await
}

async fn capturing_upstream() -> (u16, Arc<Mutex<Vec<Value>>>, tokio::task::JoinHandle<()>) {
    async fn chat(
        State(captured): State<Arc<Mutex<Vec<Value>>>>,
        Json(body): Json<Value>,
    ) -> Json<Value> {
        captured.lock().expect("lock captured bodies").push(body);
        Json(json!({
            "id": "chat-test",
            "model": "kimi-test",
            "created": 1,
            "choices": [{ "message": { "role": "assistant", "content": "hello" }, "finish_reason": "stop" }],
            "usage": { "prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2 }
        }))
    }
    let captured = Arc::new(Mutex::new(Vec::new()));
    let listener =
        tokio::net::TcpListener::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
            .await
            .expect("bind capturing mock upstream");
    let port = listener.local_addr().expect("upstream addr").port();
    let state = captured.clone();
    let task = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new()
                .route("/chat/completions", post(chat))
                .with_state(state),
        )
        .await
        .expect("serve capturing mock upstream");
    });
    (port, captured, task)
}

async fn upstream_at(path: &'static str) -> (u16, tokio::task::JoinHandle<()>) {
    async fn chat(Json(_body): Json<Value>) -> Json<Value> {
        Json(json!({
            "id": "chat-test",
            "model": "kimi-test",
            "created": 1,
            "choices": [{ "message": { "role": "assistant", "content": "hello" }, "finish_reason": "stop" }],
            "usage": { "prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2 }
        }))
    }
    let listener =
        tokio::net::TcpListener::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
            .await
            .expect("bind mock upstream");
    let port = listener.local_addr().expect("upstream addr").port();
    let task = tokio::spawn(async move {
        axum::serve(listener, Router::new().route(path, post(chat)))
            .await
            .expect("serve mock upstream");
    });
    (port, task)
}

async fn sse_upstream(chunks: Vec<&'static [u8]>) -> (u16, tokio::task::JoinHandle<()>) {
    async fn chat(State(chunks): State<Vec<&'static [u8]>>) -> Response {
        let output = stream! {
            for chunk in chunks {
                yield Ok::<_, Infallible>(axum::body::Bytes::from_static(chunk));
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
            .expect("bind mock SSE upstream");
    let port = listener.local_addr().expect("upstream addr").port();
    let task = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new()
                .route("/chat/completions", post(chat))
                .with_state(chunks),
        )
        .await
        .expect("serve mock SSE upstream");
    });
    (port, task)
}

async fn delayed_sse_upstream(
    delay: Duration,
    chunks: Vec<&'static [u8]>,
) -> (u16, tokio::task::JoinHandle<()>) {
    async fn chat(State((delay, chunks)): State<(Duration, Vec<&'static [u8]>)>) -> Response {
        let output = stream! {
            tokio::time::sleep(delay).await;
            for chunk in chunks {
                yield Ok::<_, Infallible>(axum::body::Bytes::from_static(chunk));
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
            .expect("bind delayed mock SSE upstream");
    let port = listener.local_addr().expect("upstream addr").port();
    let task = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new()
                .route("/chat/completions", post(chat))
                .with_state((delay, chunks)),
        )
        .await
        .expect("serve delayed mock SSE upstream");
    });
    (port, task)
}

async fn slow_upstream(
    started: Arc<Notify>,
    release: Arc<Notify>,
) -> (u16, tokio::task::JoinHandle<()>) {
    async fn chat(
        State((started, release)): State<(Arc<Notify>, Arc<Notify>)>,
        Json(_body): Json<Value>,
    ) -> Json<Value> {
        started.notify_waiters();
        release.notified().await;
        Json(json!({
            "id": "chat-test", "model": "kimi-test", "created": 1,
            "choices": [{ "message": { "role": "assistant", "content": "hello" }, "finish_reason": "stop" }]
        }))
    }
    let listener =
        tokio::net::TcpListener::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
            .await
            .expect("bind slow upstream");
    let port = listener.local_addr().expect("upstream addr").port();
    let task = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new()
                .route("/chat/completions", post(chat))
                .with_state((started, release)),
        )
        .await
        .expect("serve slow upstream");
    });
    (port, task)
}

async fn client() -> reqwest::Client {
    reqwest::Client::builder().build().expect("test client")
}

#[tokio::test]
async fn health_requires_the_local_bearer_token() {
    let (upstream_port, upstream_task) = upstream().await;
    let host = BridgeRuntimeHost::new();
    let status = host
        .start(spec("health", 0, upstream_port))
        .await
        .expect("start");
    let url = format!("http://127.0.0.1:{}/health", status.port);
    let response = client()
        .await
        .get(&url)
        .send()
        .await
        .expect("health request");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let response = client()
        .await
        .get(url)
        .header(header::AUTHORIZATION, "Bearer local-test-token")
        .send()
        .await
        .expect("authorized health request");
    assert_eq!(response.status(), StatusCode::OK);
    let health = response.json::<Value>().await.expect("health json");
    assert_eq!(health["ok"], true);
    assert_eq!(health["listener_status"], "running");
    assert_eq!(health["upstream_status"], "unknown");
    let api_key_health = client()
        .await
        .get(format!("http://127.0.0.1:{}/health", status.port))
        .header("x-api-key", "local-test-token")
        .send()
        .await
        .expect("x-api-key health request");
    assert_eq!(api_key_health.status(), StatusCode::OK);
    assert_eq!(
        host.status("health")
            .expect("status")
            .expect("instance")
            .upstream_status,
        BridgeUpstreamStatus::Unknown
    );
    assert!(
        host.status("health")
            .expect("status")
            .expect("instance")
            .running
    );
    host.stop("health").await.expect("stop");
    upstream_task.abort();
}

#[tokio::test]
async fn models_requires_the_local_bearer_token() {
    let (upstream_port, upstream_task) = upstream().await;
    let host = BridgeRuntimeHost::new();
    let status = host
        .start(spec("models-auth", 0, upstream_port).with_listed_models(vec!["gpt-5.4".into()]))
        .await
        .expect("start");
    let url = format!("http://127.0.0.1:{}/v1/models", status.port);
    let response = client()
        .await
        .get(&url)
        .send()
        .await
        .expect("models request");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let body = response.json::<Value>().await.expect("models json");
    assert_eq!(body["error"]["code"], "invalid_api_key");
    host.stop("models-auth").await.expect("stop");
    upstream_task.abort();
}

#[tokio::test]
async fn models_returns_openai_list_shape_on_both_paths() {
    let (upstream_port, upstream_task) = upstream().await;
    let host = BridgeRuntimeHost::new();
    let status = host
        .start(
            spec("models-list", 0, upstream_port)
                .with_listed_models(vec!["gpt-5.4".into(), "gpt-5".into()]),
        )
        .await
        .expect("start");
    let http = client().await;
    for path in ["/v1/models", "/models"] {
        let response = http
            .get(format!("http://127.0.0.1:{}{path}", status.port))
            .header(header::AUTHORIZATION, "Bearer local-test-token")
            .send()
            .await
            .expect("authorized models request");
        assert_eq!(response.status(), StatusCode::OK);
        let body = response.json::<Value>().await.expect("models json");
        assert_eq!(body["object"], "list");
        assert_eq!(body["data"][0]["id"], "gpt-5.4");
        assert_eq!(body["data"][0]["object"], "model");
        assert_eq!(body["data"][1]["id"], "gpt-5");
        assert_eq!(body["data"].as_array().map(Vec::len), Some(2));
    }
    host.stop("models-list").await.expect("stop");
    upstream_task.abort();
}

#[tokio::test]
async fn models_returns_empty_list_when_mapping_and_default_are_missing() {
    let (upstream_port, upstream_task) = upstream().await;
    let host = BridgeRuntimeHost::new();
    let status = host
        .start(spec("models-empty", 0, upstream_port))
        .await
        .expect("start");
    let response = client()
        .await
        .get(format!("http://127.0.0.1:{}/v1/models", status.port))
        .header(header::AUTHORIZATION, "Bearer local-test-token")
        .send()
        .await
        .expect("authorized models request");
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.json::<Value>().await.expect("models json");
    assert_eq!(body["object"], "list");
    assert_eq!(body["data"], json!([]));
    host.stop("models-empty").await.expect("stop");
    upstream_task.abort();
}

#[tokio::test]
async fn models_lists_codex_to_grok_dispatch_accepted_ids() {
    let listed = list_local_bridge_models(
        AdapterSourceProduct::CodexChatGptSubscription,
        AgentId::Grok,
        None,
    );
    let (upstream_port, upstream_task) = upstream().await;
    let host = BridgeRuntimeHost::new();
    let status = host
        .start(spec("models-codex-grok", 0, upstream_port).with_listed_models(listed.clone()))
        .await
        .expect("start");
    let response = client()
        .await
        .get(format!("http://127.0.0.1:{}/v1/models", status.port))
        .header(header::AUTHORIZATION, "Bearer local-test-token")
        .send()
        .await
        .expect("authorized models request");
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.json::<Value>().await.expect("models json");
    let ids: Vec<String> = body["data"]
        .as_array()
        .expect("data array")
        .iter()
        .map(|item| item["id"].as_str().expect("id").to_owned())
        .collect();
    assert_eq!(ids, listed);
    assert!(!ids.is_empty());
    for id in &ids {
        assert!(!is_leftover_bridge_model(id), "leftover listed: {id}");
    }
    host.stop("models-codex-grok").await.expect("stop");
    upstream_task.abort();
}

#[test]
fn bridge_start_spec_debug_redacts_both_bearer_tokens() {
    let debug = format!("{:?}", spec("redacted", 0, 1));
    assert!(debug.contains("local_token: \"REDACTED\""));
    assert!(!debug.contains("local-test-token"));
    assert!(!debug.contains("upstream-test-token"));
}

#[tokio::test]
async fn cleanup_completion_retains_result_before_waiter_registration() {
    let successful = CleanupCompletion::new();
    // Creating an async future does not poll it. Finish therefore happens deterministically before
    // `wait` registers its receiver, covering the retained-completion requirement directly.
    let successful_wait = successful.wait();
    successful.finish(false);
    tokio::time::timeout(Duration::from_millis(100), successful_wait)
        .await
        .expect("successful completion must be retained for a late waiter")
        .expect("successful cleanup result");

    let failed = CleanupCompletion::new();
    let failed_wait = failed.wait();
    failed.finish(true);
    assert!(matches!(
        tokio::time::timeout(Duration::from_millis(100), failed_wait)
            .await
            .expect("failed completion must be retained for a late waiter"),
        Err(BridgeHostError::StatePoisoned)
    ));
}

#[tokio::test]
async fn shutdown_latches_the_host_and_exposes_stopped_state() {
    let (upstream_port, upstream_task) = upstream().await;
    let host = BridgeRuntimeHost::new();
    let status = host
        .start(spec("closing", 0, upstream_port))
        .await
        .expect("start");
    assert_eq!(status.state, BridgeRuntimeState::Running);
    let stopped = host.stop("closing").await.expect("stop");
    assert_eq!(stopped.state, BridgeRuntimeState::Stopped);
    assert!(!stopped.running);
    host.shutdown().await.expect("shutdown");
    assert!(matches!(
        host.start(spec("closing", 0, upstream_port)).await,
        Err(BridgeHostError::HostClosing)
    ));
    upstream_task.abort();
}

#[tokio::test]
async fn start_rejects_unsafe_upstream_urls_and_keeps_loopback_base_paths() {
    let host = BridgeRuntimeHost::new();
    for base_url in [
        "http://example.com",
        "ftp://127.0.0.1",
        "http://0.0.0.0:8080",
        "http://user:pass@127.0.0.1:8080",
        "https://example.com/#fragment",
    ] {
        let mut invalid = spec("invalid", 0, 1);
        invalid.upstream.base_url = base_url.to_owned();
        assert!(
            matches!(
                host.start(invalid).await,
                Err(super::BridgeHostError::InvalidUpstreamUrl)
            ),
            "{base_url}"
        );
    }

    let (upstream_port, upstream_task) = upstream_at("/coding/v1/chat/completions").await;
    let mut scoped = spec("base-path", 0, upstream_port);
    scoped.upstream.base_url = format!("http://127.0.0.1:{upstream_port}/coding/v1");
    let status = host
        .start(scoped)
        .await
        .expect("loopback base path accepted");
    let response = client()
        .await
        .post(format!("http://127.0.0.1:{}/v1/responses", status.port))
        .header(header::AUTHORIZATION, "Bearer local-test-token")
        .json(&json!({"model":"test","input":"hello"}))
        .send()
        .await
        .expect("response request");
    assert_eq!(response.status(), StatusCode::OK);
    host.stop("base-path").await.expect("stop");
    upstream_task.abort();
}

#[tokio::test]
async fn stop_drains_an_inflight_request_before_returning() {
    let started = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let reached_upstream = started.notified();
    let (upstream_port, upstream_task) = slow_upstream(started.clone(), release.clone()).await;
    let host = BridgeRuntimeHost::new();
    let status = host
        .start(spec("drain", 0, upstream_port))
        .await
        .expect("start");
    let request_client = client().await;
    let request = tokio::spawn(async move {
        request_client
            .post(format!("http://127.0.0.1:{}/v1/responses", status.port))
            .header(header::AUTHORIZATION, "Bearer local-test-token")
            .json(&json!({"model":"test","input":"hello"}))
            .send()
            .await
    });
    reached_upstream.await;
    let draining_host = host.clone();
    let mut stop = tokio::spawn(async move { draining_host.stop("drain").await });
    assert!(tokio::time::timeout(Duration::from_millis(75), &mut stop)
        .await
        .is_err());
    assert!(matches!(
        host.start(spec("drain", 0, upstream_port)).await,
        Err(BridgeHostError::Stopping)
    ));
    release.notify_waiters();
    assert!(stop.await.expect("stop task").is_ok());
    assert_eq!(
        request
            .await
            .expect("request task")
            .expect("response")
            .status(),
        StatusCode::OK
    );
    upstream_task.abort();
}

#[test]
fn sse_frame_delimiter_uses_the_earliest_complete_boundary() {
    let buffer = b"data: one\n\ndata: two\r\n\r\n";
    assert_eq!(super::host::sse_frame_end(buffer), Some((9, 2)));
}

#[tokio::test]
async fn stream_parser_accepts_crlf_split_multiline_data_and_stops_at_done() {
    let (upstream_port, upstream_task) = sse_upstream(vec![
        b"data: {\"id\":\"chat-stream\",\"model\":\"kimi-test\",\r\ndata: \"choices\":[{\"delta\":{\"content\":\"hel",
        b"lo\"}}]}\r\n\r\n",
        b"data: {\"choices\":[{\"delta\":{\"content\":\" world\"},\"finish_reason\":\"stop\"}]}\r\n\r\n",
        b"data: [DONE]\r\n\r\n",
        b"data: private malformed content\r\n\r\n",
    ]).await;
    let host = BridgeRuntimeHost::new();
    let status = host
        .start(spec("sse", 0, upstream_port))
        .await
        .expect("start");
    let body = client()
        .await
        .post(format!("http://127.0.0.1:{}/v1/responses", status.port))
        .header(header::AUTHORIZATION, "Bearer local-test-token")
        .json(&json!({"model":"test","input":"hello","stream":true}))
        .send()
        .await
        .expect("stream request")
        .text()
        .await
        .expect("stream body");
    assert!(body.contains("\"delta\":\"hello\""));
    assert!(body.contains("\"delta\":\" world\""));
    assert!(body.contains("\"text\":\"hello world\""));
    assert!(!body.contains("private malformed content"));
    host.stop("sse").await.expect("stop");
    upstream_task.abort();
}

#[tokio::test]
async fn malformed_stream_data_returns_generic_error_without_leaking_payload() {
    let (upstream_port, upstream_task) =
        sse_upstream(vec![b"data: private malformed content\n\n"]).await;
    let host = BridgeRuntimeHost::new();
    let status = host
        .start(spec("bad-sse", 0, upstream_port))
        .await
        .expect("start");
    let body = client()
        .await
        .post(format!("http://127.0.0.1:{}/v1/responses", status.port))
        .header(header::AUTHORIZATION, "Bearer local-test-token")
        .json(&json!({"model":"test","input":"hello","stream":true}))
        .send()
        .await
        .expect("stream request")
        .text()
        .await
        .expect("stream body");
    assert!(body.contains("The upstream model provider returned an invalid stream."));
    assert!(!body.contains("private malformed content"));
    host.stop("bad-sse").await.expect("stop");
    upstream_task.abort();
}

#[tokio::test]
async fn clean_sse_eof_without_done_is_an_error_not_a_completed_response() {
    let (upstream_port, upstream_task) = sse_upstream(vec![
        b"data: {\"id\":\"chat-stream\",\"model\":\"kimi-test\",\"choices\":[{\"delta\":{\"content\":\"truncated\"}}]}\n\n",
    ])
    .await;
    let host = BridgeRuntimeHost::new();
    let status = host
        .start(spec("sse-eof", 0, upstream_port))
        .await
        .expect("start");
    let body = client()
        .await
        .post(format!("http://127.0.0.1:{}/v1/responses", status.port))
        .header(header::AUTHORIZATION, "Bearer local-test-token")
        .json(&json!({"model":"test","input":"hello","stream":true}))
        .send()
        .await
        .expect("stream request")
        .text()
        .await
        .expect("stream body");
    assert!(body.contains("The upstream model provider returned an invalid stream."));
    assert!(!body.contains("response.completed"));
    host.stop("sse-eof").await.expect("stop");
    upstream_task.abort();
}

#[tokio::test]
async fn idle_sse_chunk_returns_a_generic_error_and_does_not_hold_the_profile_permit() {
    let (upstream_port, upstream_task) =
        delayed_sse_upstream(Duration::from_millis(200), vec![b"data: [DONE]\n\n"]).await;
    let host = BridgeRuntimeHost::new();
    let status = host
        .start(spec("sse-idle", 0, upstream_port))
        .await
        .expect("start");
    let body = client()
        .await
        .post(format!("http://127.0.0.1:{}/v1/responses", status.port))
        .header(header::AUTHORIZATION, "Bearer local-test-token")
        .json(&json!({"model":"test","input":"hello","stream":true}))
        .send()
        .await
        .expect("stream request")
        .text()
        .await
        .expect("stream body");
    assert!(body.contains("The upstream model provider returned an invalid stream."));
    host.stop("sse-idle").await.expect("stop");
    upstream_task.abort();
}

#[tokio::test]
async fn responses_rejects_missing_or_invalid_local_token() {
    let (upstream_port, upstream_task) = upstream().await;
    let host = BridgeRuntimeHost::new();
    let status = host
        .start(spec("auth", 0, upstream_port))
        .await
        .expect("start");
    let url = format!("http://127.0.0.1:{}/v1/responses", status.port);
    let response = client()
        .await
        .post(url)
        .json(&json!({"model":"test","input":"hello"}))
        .send()
        .await
        .expect("request");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    host.stop("auth").await.expect("stop");
    upstream_task.abort();
}

#[tokio::test]
async fn slow_unauthorized_body_is_rejected_before_json_extraction() {
    let (upstream_port, upstream_task) = upstream().await;
    let host = BridgeRuntimeHost::new();
    let status = host
        .start(spec("slow-auth", 0, upstream_port))
        .await
        .expect("start");
    let mut socket = TcpStream::connect((Ipv4Addr::LOCALHOST, status.port))
        .await
        .expect("connect bridge");
    socket
        .write_all(
            b"POST /v1/responses HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer wrong\r\nContent-Type: application/json\r\nContent-Length: 1048576\r\n\r\n",
        )
        .await
        .expect("write request headers");
    let mut response = [0u8; 256];
    let received = tokio::time::timeout(Duration::from_millis(250), socket.read(&mut response))
        .await
        .expect("unauthorized request must not wait for body")
        .expect("read response");
    assert!(std::str::from_utf8(&response[..received])
        .expect("http response")
        .starts_with("HTTP/1.1 401"));
    host.stop("slow-auth").await.expect("stop");
    upstream_task.abort();
}

#[tokio::test]
async fn duplicate_start_is_idempotent_and_stop_releases_port() {
    let (upstream_port, upstream_task) = upstream().await;
    let host = BridgeRuntimeHost::new();
    let first = host
        .start(spec("same", 0, upstream_port))
        .await
        .expect("first start");
    let second = host
        .start(spec("same", 0, upstream_port))
        .await
        .expect("idempotent start");
    assert_eq!(first.port, second.port);
    host.stop("same").await.expect("stop");
    let socket =
        std::net::TcpListener::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), first.port));
    assert!(socket.is_ok(), "stopped listener must release its port");
    upstream_task.abort();
}

#[tokio::test]
async fn profile_admission_rejects_overload_without_affecting_a_second_profile() {
    let started = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let reached_upstream = started.notified();
    let (upstream_port, upstream_task) = slow_upstream(started.clone(), release.clone()).await;
    let (other_upstream_port, other_upstream_task) = upstream().await;
    let host = BridgeRuntimeHost::new();
    let first = host
        .start(spec_with_token(
            "admission-a",
            0,
            upstream_port,
            "local-token-a-aaaaaaaaaaaa",
        ))
        .await
        .expect("start first profile");
    let second = host
        .start(spec_with_token(
            "admission-b",
            0,
            other_upstream_port,
            "local-token-b-bbbbbbbbbbbb",
        ))
        .await
        .expect("start second profile");
    let mut requests = Vec::new();
    for _ in 0..MAX_IN_FLIGHT_REQUESTS_PER_PROFILE {
        let client = client().await;
        let url = format!("http://127.0.0.1:{}/v1/responses", first.port);
        requests.push(tokio::spawn(async move {
            client
                .post(url)
                .header(header::AUTHORIZATION, "Bearer local-token-a-aaaaaaaaaaaa")
                .json(&json!({"model":"test","input":"hello"}))
                .send()
                .await
        }));
    }
    reached_upstream.await;
    // First notify means one permit is held upstream. Give the remaining in-flight
    // tasks a short window to acquire before the overflow request.
    tokio::time::sleep(Duration::from_millis(30)).await;
    let overloaded = client()
        .await
        .post(format!("http://127.0.0.1:{}/v1/responses", first.port))
        .header(header::AUTHORIZATION, "Bearer local-token-a-aaaaaaaaaaaa")
        .json(&json!({"model":"test","input":"hello"}))
        .send()
        .await
        .expect("overload request");
    assert_eq!(overloaded.status(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(
        overloaded
            .headers()
            .get(header::RETRY_AFTER)
            .and_then(|value| value.to_str().ok()),
        Some("1")
    );
    assert_eq!(
        client()
            .await
            .post(format!("http://127.0.0.1:{}/v1/responses", second.port))
            .header(header::AUTHORIZATION, "Bearer local-token-b-bbbbbbbbbbbb")
            .json(&json!({"model":"test","input":"hello"}))
            .send()
            .await
            .expect("other profile request")
            .status(),
        StatusCode::OK
    );
    release.notify_waiters();
    for request in requests {
        assert!(request.await.expect("request task").is_ok());
    }
    host.stop("admission-a").await.expect("stop first profile");
    host.stop("admission-b").await.expect("stop second profile");
    upstream_task.abort();
    other_upstream_task.abort();
}

#[tokio::test]
async fn stopping_one_profile_does_not_block_starting_or_stopping_another() {
    let started = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let reached_upstream = started.notified();
    let (upstream_port, upstream_task) = slow_upstream(started.clone(), release.clone()).await;
    let host = BridgeRuntimeHost::new();
    let first = host
        .start(spec("cross-a", 0, upstream_port))
        .await
        .expect("start first profile");
    let request_client = client().await;
    let request = tokio::spawn(async move {
        request_client
            .post(format!("http://127.0.0.1:{}/v1/responses", first.port))
            .header(header::AUTHORIZATION, "Bearer local-test-token")
            .json(&json!({"model":"test","input":"hello"}))
            .send()
            .await
    });
    reached_upstream.await;
    let draining_host = host.clone();
    let stop = tokio::spawn(async move { draining_host.stop("cross-a").await });
    assert!(tokio::time::timeout(
        Duration::from_millis(100),
        host.start(spec_with_token(
            "cross-b",
            0,
            upstream_port,
            "local-token-cross-b-bbbbbb",
        )),
    )
    .await
    .expect("unrelated profile start must not queue")
    .is_ok());
    release.notify_waiters();
    assert!(stop.await.expect("stop task").is_ok());
    assert!(request.await.expect("request task").is_ok());
    host.stop("cross-b").await.expect("stop second profile");
    upstream_task.abort();
}

#[tokio::test]
async fn repeated_shutdown_joins_the_same_inflight_cleanup() {
    let started = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let reached_upstream = started.notified();
    let (upstream_port, upstream_task) = slow_upstream(started.clone(), release.clone()).await;
    let host = BridgeRuntimeHost::new();
    let status = host
        .start(spec("shutdown-join", 0, upstream_port))
        .await
        .expect("start");
    let request_client = client().await;
    let request = tokio::spawn(async move {
        request_client
            .post(format!("http://127.0.0.1:{}/v1/responses", status.port))
            .header(header::AUTHORIZATION, "Bearer local-test-token")
            .json(&json!({"model":"test","input":"hello"}))
            .send()
            .await
    });
    reached_upstream.await;
    let first_host = host.clone();
    let first = tokio::spawn(async move { first_host.shutdown().await });
    let second_host = host.clone();
    let mut second = tokio::spawn(async move { second_host.shutdown().await });
    assert!(tokio::time::timeout(Duration::from_millis(75), &mut second)
        .await
        .is_err());
    release.notify_waiters();
    assert!(first.await.expect("first shutdown task").is_ok());
    assert!(second.await.expect("second shutdown task").is_ok());
    assert!(request.await.expect("request task").is_ok());
    upstream_task.abort();
}

#[tokio::test]
async fn shared_port_second_profile_starts_and_non_streaming_response_translates() {
    let (upstream_port, upstream_task) = upstream().await;
    let host = BridgeRuntimeHost::new();
    let first = host
        .start(spec_with_token(
            "first",
            0,
            upstream_port,
            "local-token-first-aaaaaaaa",
        ))
        .await
        .expect("first start");
    let second = host
        .start(spec_with_token(
            "second",
            first.port,
            upstream_port,
            "local-token-second-bbbbbbb",
        ))
        .await
        .expect("second profile shares the bound port");
    assert_eq!(second.port, first.port);
    let response = client()
        .await
        .post(format!("http://127.0.0.1:{}/v1/responses", first.port))
        .header(header::AUTHORIZATION, "Bearer local-token-first-aaaaaaaa")
        .json(&json!({"model":"test","input":"hello"}))
        .send()
        .await
        .expect("response request");
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.json::<Value>().await.expect("response json")["output"][0]["content"][0]["text"],
        "hello"
    );
    host.shutdown().await.expect("shutdown");
    upstream_task.abort();
}

#[tokio::test]
async fn status_and_health_report_last_observed_upstream_without_a_new_probe() {
    let (upstream_port, upstream_task) = upstream().await;
    let host = BridgeRuntimeHost::new();
    let started = host
        .start(spec("observed", 0, upstream_port))
        .await
        .expect("start");
    assert_eq!(started.upstream_status, BridgeUpstreamStatus::Unknown);

    let connected = host
        .record_upstream_outcome("observed", BridgeUpstreamStatus::Connected)
        .expect("record connected")
        .expect("instance");
    assert_eq!(connected.upstream_status, BridgeUpstreamStatus::Connected);
    let health = client()
        .await
        .get(format!("http://127.0.0.1:{}/health", started.port))
        .header(header::AUTHORIZATION, "Bearer local-test-token")
        .send()
        .await
        .expect("health")
        .json::<Value>()
        .await
        .expect("health json");
    assert_eq!(health["upstream_status"], "connected");
    assert_eq!(
        host.status("observed")
            .expect("status")
            .expect("instance")
            .upstream_status,
        BridgeUpstreamStatus::Connected
    );

    let response = client()
        .await
        .post(format!("http://127.0.0.1:{}/v1/responses", started.port))
        .header(header::AUTHORIZATION, "Bearer local-test-token")
        .json(&json!({"model":"test","input":"hello"}))
        .send()
        .await
        .expect("response request");
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        host.status("observed")
            .expect("status after success")
            .expect("instance")
            .upstream_status,
        BridgeUpstreamStatus::Connected
    );

    host.record_upstream_outcome("observed", BridgeUpstreamStatus::Degraded)
        .expect("record degraded");
    assert_eq!(
        host.status("observed")
            .expect("status after degrade")
            .expect("instance")
            .upstream_status,
        BridgeUpstreamStatus::Degraded
    );
    let degraded_health = client()
        .await
        .get(format!("http://127.0.0.1:{}/health", started.port))
        .header(header::AUTHORIZATION, "Bearer local-test-token")
        .send()
        .await
        .expect("degraded health")
        .json::<Value>()
        .await
        .expect("degraded health json");
    assert_eq!(degraded_health["upstream_status"], "degraded");

    let stopped = host.stop("observed").await.expect("stop");
    assert_eq!(stopped.state, BridgeRuntimeState::Stopped);
    assert_eq!(stopped.upstream_status, BridgeUpstreamStatus::Stopped);
    assert!(host
        .status("observed")
        .expect("status after stop")
        .is_none());
    upstream_task.abort();
}

#[tokio::test]
async fn request_failure_marks_upstream_degraded_and_status_does_not_probe() {
    let host = BridgeRuntimeHost::new();
    let started = host
        .start(spec("degraded-request", 0, 1))
        .await
        .expect("start with no live upstream");
    let response = client()
        .await
        .post(format!("http://127.0.0.1:{}/v1/responses", started.port))
        .header(header::AUTHORIZATION, "Bearer local-test-token")
        .json(&json!({"model":"test","input":"hello"}))
        .send()
        .await
        .expect("failed request");
    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
    let status = host
        .status("degraded-request")
        .expect("status")
        .expect("instance");
    assert_eq!(status.state, BridgeRuntimeState::Running);
    assert_eq!(status.upstream_status, BridgeUpstreamStatus::Degraded);
    host.stop("degraded-request").await.expect("stop");
}

async fn status_upstream(status: StatusCode) -> (u16, tokio::task::JoinHandle<()>) {
    async fn chat(State(status): State<StatusCode>) -> Response {
        (status, Json(json!({"error":{"message":"upstream-secret"}}))).into_response()
    }
    let listener =
        tokio::net::TcpListener::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
            .await
            .expect("bind status upstream");
    let port = listener.local_addr().expect("upstream addr").port();
    let task = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new()
                .route("/chat/completions", post(chat))
                .with_state(status),
        )
        .await
        .expect("serve status upstream");
    });
    (port, task)
}

#[tokio::test]
async fn upstream_429_and_5xx_are_generic_and_mark_degraded() {
    for (label, upstream_status, expected_local) in [
        (
            "too-many",
            StatusCode::TOO_MANY_REQUESTS,
            StatusCode::TOO_MANY_REQUESTS,
        ),
        (
            "server-error",
            StatusCode::INTERNAL_SERVER_ERROR,
            StatusCode::BAD_GATEWAY,
        ),
    ] {
        let (upstream_port, upstream_task) = status_upstream(upstream_status).await;
        let host = BridgeRuntimeHost::new();
        let started = host
            .start(spec(label, 0, upstream_port))
            .await
            .expect("start");
        let response = client()
            .await
            .post(format!("http://127.0.0.1:{}/v1/responses", started.port))
            .header(header::AUTHORIZATION, "Bearer local-test-token")
            .json(&json!({"model":"test","input":"hello"}))
            .send()
            .await
            .expect("upstream error request");
        assert_eq!(response.status(), expected_local);
        let body = response.text().await.expect("error body");
        assert!(body.contains("The upstream model provider returned an error."));
        assert!(!body.contains("upstream-secret"));
        assert_eq!(
            host.status(label)
                .expect("status")
                .expect("instance")
                .upstream_status,
            BridgeUpstreamStatus::Degraded
        );
        host.stop(label).await.expect("stop");
        upstream_task.abort();
    }
}

#[tokio::test]
async fn translate_failure_on_http_200_marks_upstream_degraded() {
    let (upstream_port, upstream_task) = status_upstream(StatusCode::OK).await;
    let host = BridgeRuntimeHost::new();
    let started = host
        .start(spec("translate-fail", 0, upstream_port))
        .await
        .expect("start");
    let response = client()
        .await
        .post(format!("http://127.0.0.1:{}/v1/responses", started.port))
        .header(header::AUTHORIZATION, "Bearer local-test-token")
        .json(&json!({"model":"test","input":"hello"}))
        .send()
        .await
        .expect("translate failure request");
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        host.status("translate-fail")
            .expect("status")
            .expect("instance")
            .upstream_status,
        BridgeUpstreamStatus::Degraded
    );
    host.stop("translate-fail").await.expect("stop");
    upstream_task.abort();
}

async fn anthropic_upstream() -> (u16, tokio::task::JoinHandle<()>) {
    async fn messages(
        headers: axum::http::HeaderMap,
        Json(body): Json<Value>,
    ) -> impl IntoResponse {
        let key = headers
            .get("x-api-key")
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default();
        let version = headers
            .get("anthropic-version")
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default();
        if key != "upstream-test-token" || version != "2023-06-01" {
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({ "error": { "type": "authentication_error", "message": "bad key" } })),
            );
        }
        assert_eq!(body["messages"][0]["content"][0]["text"], "hello");
        (
            StatusCode::OK,
            Json(json!({
                "id": "msg_host",
                "type": "message",
                "role": "assistant",
                "model": "claude-sonnet-4-20250514",
                "content": [{ "type": "text", "text": "你好" }],
                "stop_reason": "end_turn",
                "usage": { "input_tokens": 2, "output_tokens": 1 }
            })),
        )
    }
    let listener =
        tokio::net::TcpListener::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
            .await
            .expect("bind anthropic upstream");
    let port = listener.local_addr().expect("addr").port();
    let task = tokio::spawn(async move {
        axum::serve(listener, Router::new().route("/messages", post(messages)))
            .await
            .expect("serve anthropic upstream");
    });
    (port, task)
}

async fn codex_responses_upstream() -> (u16, tokio::task::JoinHandle<()>) {
    async fn responses(
        headers: axum::http::HeaderMap,
        Json(body): Json<Value>,
    ) -> impl IntoResponse {
        let bearer = headers
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default();
        assert_eq!(bearer, "Bearer oauth-upstream-token");
        assert!(headers.get("x-api-key").is_none());
        assert!(headers.get("anthropic-version").is_none());
        assert_eq!(body["input"][0]["content"][0]["text"], "hello");
        (
            StatusCode::OK,
            Json(json!({
                "id": "resp_codex",
                "object": "response",
                "created_at": 1,
                "model": "gpt-5",
                "status": "completed",
                "output": [{
                    "id": "msg_codex",
                    "type": "message",
                    "status": "completed",
                    "role": "assistant",
                    "content": [{ "type": "output_text", "text": "hello from codex" }]
                }],
                "usage": {
                    "input_tokens": 2,
                    "output_tokens": 3,
                    "total_tokens": 5
                }
            })),
        )
    }
    let listener =
        tokio::net::TcpListener::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
            .await
            .expect("bind Codex Responses upstream");
    let port = listener.local_addr().expect("addr").port();
    let task = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new().route("/v1/responses", post(responses)),
        )
        .await
        .expect("serve Codex Responses upstream");
    });
    (port, task)
}

fn grok_completed_response(text: &str) -> Value {
    json!({
        "id": "resp_grok",
        "object": "response",
        "created_at": 1,
        "model": "grok-4.5",
        "status": "completed",
        "output": [{
            "id": "msg_grok",
            "type": "message",
            "status": "completed",
            "role": "assistant",
            "content": [{ "type": "output_text", "text": text }]
        }],
        "usage": {
            "input_tokens": 2,
            "output_tokens": 3,
            "total_tokens": 5,
            "reasoning_tokens": 0,
            "output_tokens_details": { "reasoning_tokens": 0 }
        }
    })
}

async fn grok_responses_upstream() -> (u16, tokio::task::JoinHandle<()>) {
    async fn responses(headers: axum::http::HeaderMap) -> Json<Value> {
        let bearer = headers
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default();
        assert_eq!(bearer, "Bearer upstream-test-token");
        assert_eq!(
            headers
                .get("x-xai-token-auth")
                .and_then(|value| value.to_str().ok()),
            Some("xai-grok-cli")
        );
        assert_eq!(
            headers
                .get("x-grok-client-version")
                .and_then(|value| value.to_str().ok()),
            Some(crate::bridge::grok_cli::GROK_CLI_VERSION)
        );
        assert_eq!(
            headers
                .get("x-grok-client-identifier")
                .and_then(|value| value.to_str().ok()),
            Some(crate::bridge::grok_cli::GROK_CLI_IDENTIFIER)
        );
        assert_eq!(
            headers
                .get("x-grok-client-mode")
                .and_then(|value| value.to_str().ok()),
            Some(crate::bridge::grok_cli::GROK_CLI_MODE)
        );
        Json(grok_completed_response("hello"))
    }
    let listener =
        tokio::net::TcpListener::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
            .await
            .expect("bind Grok Responses upstream");
    let port = listener.local_addr().expect("addr").port();
    let task = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new().route("/v1/responses", post(responses)),
        )
        .await
        .expect("serve Grok Responses upstream");
    });
    (port, task)
}

async fn capturing_grok_responses_upstream(
) -> (u16, Arc<Mutex<Vec<Value>>>, tokio::task::JoinHandle<()>) {
    async fn responses(
        State(captured): State<Arc<Mutex<Vec<Value>>>>,
        Json(body): Json<Value>,
    ) -> Json<Value> {
        captured.lock().expect("lock").push(body);
        Json(grok_completed_response("hello"))
    }
    let captured = Arc::new(Mutex::new(Vec::new()));
    let listener =
        tokio::net::TcpListener::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
            .await
            .expect("bind capturing Grok Responses");
    let port = listener.local_addr().expect("addr").port();
    let captured_clone = captured.clone();
    let task = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new()
                .route("/v1/responses", post(responses))
                .with_state(captured_clone),
        )
        .await
        .expect("serve capturing Grok Responses");
    });
    (port, captured, task)
}

#[derive(Clone)]
struct CapturedGrokRequest {
    headers: axum::http::HeaderMap,
    body: Value,
}

async fn capturing_grok_requests() -> (
    u16,
    Arc<Mutex<Vec<CapturedGrokRequest>>>,
    tokio::task::JoinHandle<()>,
) {
    async fn responses(
        State(captured): State<Arc<Mutex<Vec<CapturedGrokRequest>>>>,
        headers: axum::http::HeaderMap,
        Json(body): Json<Value>,
    ) -> Json<Value> {
        captured
            .lock()
            .expect("lock")
            .push(CapturedGrokRequest { headers, body });
        Json(grok_completed_response("hello"))
    }
    let captured = Arc::new(Mutex::new(Vec::new()));
    let listener =
        tokio::net::TcpListener::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
            .await
            .expect("bind capturing Grok requests");
    let port = listener.local_addr().expect("addr").port();
    let captured_clone = captured.clone();
    let task = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new()
                .route("/v1/responses", post(responses))
                .with_state(captured_clone),
        )
        .await
        .expect("serve capturing Grok requests");
    });
    (port, captured, task)
}

#[derive(Clone)]
struct GrokCaptureReply {
    captured: Arc<Mutex<Vec<CapturedGrokRequest>>>,
    reply: Value,
}

async fn capturing_grok_requests_with_reply(
    reply: Value,
) -> (
    u16,
    Arc<Mutex<Vec<CapturedGrokRequest>>>,
    tokio::task::JoinHandle<()>,
) {
    async fn responses(
        State(state): State<GrokCaptureReply>,
        headers: axum::http::HeaderMap,
        Json(body): Json<Value>,
    ) -> Json<Value> {
        state
            .captured
            .lock()
            .expect("lock")
            .push(CapturedGrokRequest { headers, body });
        Json(state.reply.clone())
    }
    let captured = Arc::new(Mutex::new(Vec::new()));
    let listener =
        tokio::net::TcpListener::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
            .await
            .expect("bind capturing Grok reply");
    let port = listener.local_addr().expect("addr").port();
    let captured_clone = captured.clone();
    let task = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new()
                .route("/v1/responses", post(responses))
                .with_state(GrokCaptureReply {
                    captured: captured_clone,
                    reply,
                }),
        )
        .await
        .expect("serve capturing Grok reply");
    });
    (port, captured, task)
}

#[derive(Clone)]
struct GrokDecodeRetryState {
    captured: Arc<Mutex<Vec<Value>>>,
    hits: Arc<Mutex<u32>>,
}

async fn grok_decode_then_ok_upstream() -> (u16, Arc<Mutex<Vec<Value>>>, tokio::task::JoinHandle<()>)
{
    async fn responses(
        State(state): State<GrokDecodeRetryState>,
        Json(body): Json<Value>,
    ) -> Response {
        state.captured.lock().expect("lock").push(body);
        let hit = {
            let mut hits = state.hits.lock().expect("hits");
            *hits += 1;
            *hits
        };
        if hit == 1 {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": { "message": "could not decrypt the provided encrypted_content" }
                })),
            )
                .into_response();
        }
        Json(grok_completed_response("hello")).into_response()
    }
    let captured = Arc::new(Mutex::new(Vec::new()));
    let listener =
        tokio::net::TcpListener::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
            .await
            .expect("bind Grok decode retry");
    let port = listener.local_addr().expect("addr").port();
    let captured_clone = captured.clone();
    let task = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new()
                .route("/v1/responses", post(responses))
                .with_state(GrokDecodeRetryState {
                    captured: captured_clone,
                    hits: Arc::new(Mutex::new(0)),
                }),
        )
        .await
        .expect("serve Grok decode retry");
    });
    (port, captured, task)
}

async fn codex_responses_sse_upstream() -> (u16, tokio::task::JoinHandle<()>) {
    async fn responses(headers: axum::http::HeaderMap) -> Response {
        let bearer = headers
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default();
        assert_eq!(bearer, "Bearer oauth-upstream-token");
        assert!(headers.get("x-api-key").is_none());
        assert!(headers.get("anthropic-version").is_none());
        let chunks: Vec<&'static [u8]> = vec![
            br#"data: {"type":"response.created","response":{"id":"resp_stream","model":"gpt-5","status":"in_progress"}}

"#,
            br#"data: {"type":"response.output_text.delta","delta":"hello"}

"#,
            br#"data: {"type":"response.completed","response":{"id":"resp_stream","model":"gpt-5","status":"completed","output":[]}}

"#,
        ];
        let output = stream! {
            for chunk in chunks {
                yield Ok::<_, Infallible>(axum::body::Bytes::from_static(chunk));
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
            .expect("bind Codex Responses SSE upstream");
    let port = listener.local_addr().expect("addr").port();
    let task = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new().route("/v1/responses", post(responses)),
        )
        .await
        .expect("serve Codex Responses SSE upstream");
    });
    (port, task)
}

async fn capturing_codex_responses_sse_upstream(
) -> (u16, Arc<Mutex<Vec<Value>>>, tokio::task::JoinHandle<()>) {
    async fn responses(
        State(captured): State<Arc<Mutex<Vec<Value>>>>,
        headers: axum::http::HeaderMap,
        Json(body): Json<Value>,
    ) -> Response {
        let bearer = headers
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default();
        assert_eq!(bearer, "Bearer oauth-upstream-token");
        captured
            .lock()
            .expect("lock captured Codex bodies")
            .push(body.clone());
        if body.get("store").and_then(Value::as_bool) != Some(false) {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"detail": "Store must be set to false"})),
            )
                .into_response();
        }
        if body.get("stream").and_then(Value::as_bool) != Some(true) {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"detail": "Stream must be set to true"})),
            )
                .into_response();
        }
        let chunks: Vec<&'static [u8]> = vec![
            br#"data: {"type":"response.created","response":{"id":"resp_stream","model":"gpt-5","status":"in_progress"}}

"#,
            br#"data: {"type":"response.output_text.delta","delta":"pong"}

"#,
            br#"data: {"type":"response.completed","response":{"id":"resp_stream","model":"gpt-5","status":"completed","output":[]}}

"#,
        ];
        let output = stream! {
            for chunk in chunks {
                yield Ok::<_, Infallible>(axum::body::Bytes::from_static(chunk));
            }
        };
        (
            [(header::CONTENT_TYPE, "text/event-stream")],
            Body::from_stream(output),
        )
            .into_response()
    }
    let captured = Arc::new(Mutex::new(Vec::new()));
    let listener =
        tokio::net::TcpListener::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
            .await
            .expect("bind capturing Codex Responses SSE upstream");
    let port = listener.local_addr().expect("addr").port();
    let captured_clone = captured.clone();
    let task = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new()
                .route("/v1/responses", post(responses))
                .with_state(captured_clone),
        )
        .await
        .expect("serve capturing Codex Responses SSE upstream");
    });
    (port, captured, task)
}

#[tokio::test]
async fn codex_responses_oauth_messages_stream_sends_store_false_and_stream_true() {
    let (upstream_port, captured, upstream_task) = capturing_codex_responses_sse_upstream().await;
    let host = BridgeRuntimeHost::new();
    let status = host
        .start(codex_spec("codex-messages-store-stream", 0, upstream_port))
        .await
        .expect("start");
    let response = client()
        .await
        .post(format!("http://127.0.0.1:{}/v1/messages", status.port))
        .header(header::AUTHORIZATION, "Bearer local-test-token")
        .json(&json!({
            "model": "claude-sonnet-4-20250514",
            "max_tokens": 32,
            "stream": true,
            "messages": [{ "role": "user", "content": "ping" }]
        }))
        .send()
        .await
        .expect("streaming messages request");
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.text().await.expect("Anthropic SSE body");
    assert!(body.contains("pong"), "expected pong in SSE body: {body}");
    let upstream = captured.lock().expect("lock captured Codex bodies").clone();
    assert_eq!(upstream.len(), 1);
    assert_eq!(upstream[0]["store"], false);
    assert_eq!(upstream[0]["stream"], true);
    host.stop("codex-messages-store-stream")
        .await
        .expect("stop");
    upstream_task.abort();
}

#[tokio::test]
async fn anthropic_protocol_uses_messages_and_x_api_key() {
    let (upstream_port, upstream_task) = anthropic_upstream().await;
    let host = BridgeRuntimeHost::new();
    let status = host
        .start(anthropic_spec("anthropic-host", 0, upstream_port))
        .await
        .expect("start");
    let response = client()
        .await
        .post(format!("http://127.0.0.1:{}/v1/responses", status.port))
        .header(header::AUTHORIZATION, "Bearer local-test-token")
        .json(&json!({"model":"test","input":"hello"}))
        .send()
        .await
        .expect("responses request");
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.json::<Value>().await.expect("json");
    assert_eq!(body["object"], "response");
    assert_eq!(body["output"][0]["content"][0]["text"], "你好");
    assert_eq!(body["usage"]["input_tokens"], 2);
    host.stop("anthropic-host").await.expect("stop");
    upstream_task.abort();
}

#[tokio::test]
async fn codex_responses_oauth_messages_returns_anthropic_json_and_accepts_both_local_auth_headers()
{
    let (upstream_port, upstream_task) = codex_responses_upstream().await;
    let host = BridgeRuntimeHost::new();
    let configured = codex_spec("codex-messages", 0, upstream_port);
    let configured_debug = format!("{configured:?}");
    assert!(!configured_debug.contains("oauth-upstream-token"));
    assert!(!configured_debug.contains("local-test-token"));
    let status = host.start(configured).await.expect("start");
    let url = format!("http://127.0.0.1:{}/v1/messages", status.port);
    for auth_header in [
        ("authorization", "Bearer local-test-token"),
        ("x-api-key", "local-test-token"),
    ] {
        let response = client()
            .await
            .post(&url)
            .header(auth_header.0, auth_header.1)
            .json(&json!({
                "model": "claude-test",
                "max_tokens": 32,
                "messages": [{ "role": "user", "content": "hello" }]
            }))
            .send()
            .await
            .expect("messages request");
        assert_eq!(response.status(), StatusCode::OK);
        let body = response
            .json::<Value>()
            .await
            .expect("Anthropic message JSON");
        assert_eq!(body["type"], "message");
        assert_eq!(body["content"][0]["text"], "hello from codex");
    }
    host.stop("codex-messages").await.expect("stop");
    upstream_task.abort();
}

#[tokio::test]
async fn codex_responses_oauth_chat_completions_returns_chat_json_and_strips_grok_model() {
    let captured = Arc::new(Mutex::new(Vec::new()));
    async fn responses(
        State(captured): State<Arc<Mutex<Vec<Value>>>>,
        Json(body): Json<Value>,
    ) -> Json<Value> {
        captured.lock().expect("lock").push(body);
        Json(json!({
            "id": "resp_codex",
            "object": "response",
            "created_at": 1,
            "model": "gpt-5",
            "status": "completed",
            "output": [{
                "id": "msg_codex",
                "type": "message",
                "status": "completed",
                "role": "assistant",
                "content": [{ "type": "output_text", "text": "hello from codex" }]
            }],
            "usage": {
                "input_tokens": 2,
                "output_tokens": 3,
                "total_tokens": 5
            }
        }))
    }
    let listener =
        tokio::net::TcpListener::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
            .await
            .expect("bind Codex Responses chat upstream");
    let upstream_port = listener.local_addr().expect("addr").port();
    let captured_clone = captured.clone();
    let upstream_task = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new()
                .route("/v1/responses", post(responses))
                .with_state(captured_clone),
        )
        .await
        .expect("serve Codex Responses chat upstream");
    });
    let host = BridgeRuntimeHost::new();
    let status = host
        .start(codex_chat_spec("codex-chat", 0, upstream_port))
        .await
        .expect("start");
    let response = client()
        .await
        .post(format!(
            "http://127.0.0.1:{}/v1/chat/completions",
            status.port
        ))
        .header(header::AUTHORIZATION, "Bearer local-test-token")
        .json(&json!({
            "model": "grok-4.5",
            "messages": [{ "role": "user", "content": "hello" }]
        }))
        .send()
        .await
        .expect("chat request");
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.json::<Value>().await.expect("chat JSON");
    assert_eq!(body["object"], "chat.completion");
    assert_eq!(body["choices"][0]["message"]["content"], "hello from codex");
    let upstream = captured.lock().expect("lock").clone();
    assert_eq!(upstream.len(), 1);
    assert!(
        upstream[0].get("model").is_none(),
        "leftover grok-* must not be forwarded: {}",
        upstream[0]
    );
    host.stop("codex-chat").await.expect("stop");
    upstream_task.abort();
}

#[tokio::test]
async fn codex_responses_oauth_messages_returns_anthropic_json_and_strips_claude_model() {
    let captured = Arc::new(Mutex::new(Vec::new()));
    async fn responses(
        State(captured): State<Arc<Mutex<Vec<Value>>>>,
        Json(body): Json<Value>,
    ) -> Json<Value> {
        captured.lock().expect("lock").push(body);
        Json(json!({
            "id": "resp_codex",
            "object": "response",
            "created_at": 1,
            "model": "gpt-5",
            "status": "completed",
            "output": [{
                "id": "msg_codex",
                "type": "message",
                "status": "completed",
                "role": "assistant",
                "content": [{ "type": "output_text", "text": "hello from codex" }]
            }],
            "usage": {
                "input_tokens": 2,
                "output_tokens": 3,
                "total_tokens": 5
            }
        }))
    }
    let listener =
        tokio::net::TcpListener::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
            .await
            .expect("bind Codex Responses messages upstream");
    let upstream_port = listener.local_addr().expect("addr").port();
    let captured_clone = captured.clone();
    let upstream_task = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new()
                .route("/v1/responses", post(responses))
                .with_state(captured_clone),
        )
        .await
        .expect("serve Codex Responses messages upstream");
    });
    let host = BridgeRuntimeHost::new();
    let status = host
        .start(codex_spec("codex-messages-claude", 0, upstream_port))
        .await
        .expect("start");
    let response = client()
        .await
        .post(format!("http://127.0.0.1:{}/v1/messages", status.port))
        .header(header::AUTHORIZATION, "Bearer local-test-token")
        .json(&json!({
            "model": "claude-sonnet-4-20250514",
            "max_tokens": 32,
            "messages": [{ "role": "user", "content": "hello" }]
        }))
        .send()
        .await
        .expect("messages request");
    assert_eq!(response.status(), StatusCode::OK);
    let body = response
        .json::<Value>()
        .await
        .expect("Anthropic message JSON");
    assert_eq!(body["type"], "message");
    assert_eq!(body["content"][0]["text"], "hello from codex");
    let upstream = captured.lock().expect("lock").clone();
    assert_eq!(upstream.len(), 1);
    assert!(
        upstream[0].get("model").is_none(),
        "leftover claude-* must not be forwarded: {}",
        upstream[0]
    );
    host.stop("codex-messages-claude").await.expect("stop");
    upstream_task.abort();
}

#[tokio::test]
async fn codex_responses_oauth_rejects_wrong_local_auth_and_responses_downstream() {
    let (upstream_port, upstream_task) = codex_responses_upstream().await;
    let host = BridgeRuntimeHost::new();
    let status = host
        .start(codex_spec("codex-routing", 0, upstream_port))
        .await
        .expect("start");
    let wrong = client()
        .await
        .post(format!("http://127.0.0.1:{}/v1/messages", status.port))
        .header(header::AUTHORIZATION, "Bearer wrong-local-token")
        .json(&json!({
            "model": "claude-test",
            "max_tokens": 32,
            "messages": [{ "role": "user", "content": "hello" }]
        }))
        .send()
        .await
        .expect("wrong auth request");
    assert_eq!(wrong.status(), StatusCode::UNAUTHORIZED);
    let responses = client()
        .await
        .post(format!("http://127.0.0.1:{}/v1/responses", status.port))
        .header(header::AUTHORIZATION, "Bearer local-test-token")
        .json(&json!({"model":"test","input":"hello"}))
        .send()
        .await
        .expect("responses route request");
    assert_eq!(responses.status(), StatusCode::NOT_FOUND);
    host.stop("codex-routing").await.expect("stop");
    upstream_task.abort();
}

#[tokio::test]
async fn codex_responses_oauth_messages_stream_is_anthropic_sse() {
    let (upstream_port, upstream_task) = codex_responses_sse_upstream().await;
    let host = BridgeRuntimeHost::new();
    let status = host
        .start(codex_spec("codex-stream", 0, upstream_port))
        .await
        .expect("start");
    let response = client()
        .await
        .post(format!("http://127.0.0.1:{}/v1/messages", status.port))
        .header("x-api-key", "local-test-token")
        .json(&json!({
            "model": "claude-test",
            "max_tokens": 32,
            "stream": true,
            "messages": [{ "role": "user", "content": "hello" }]
        }))
        .send()
        .await
        .expect("stream request");
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("text/event-stream")
    );
    let body = response.text().await.expect("stream body");
    assert!(body.contains("event: content_block_delta"));
    assert!(body.contains("\"text\":\"hello\""));
    assert!(body.contains("event: message_stop"));
    assert!(!body.contains("response.output_text.delta"));
    host.stop("codex-stream").await.expect("stop");
    upstream_task.abort();
}

#[tokio::test]
async fn grok_claude_bridge_accepts_messages_and_404s_responses() {
    let (upstream_port, upstream_task) = grok_responses_upstream().await;
    let host = BridgeRuntimeHost::new();
    let status = host
        .start(grok_claude_spec("grok-messages", 0, upstream_port))
        .await
        .expect("start");
    let response = client()
        .await
        .post(format!("http://127.0.0.1:{}/v1/messages", status.port))
        .header("x-api-key", "local-test-token")
        .json(&json!({
            "model": "claude-test",
            "max_tokens": 32,
            "messages": [{ "role": "user", "content": "hello" }]
        }))
        .send()
        .await
        .expect("messages request");
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.expect("anthropic response");
    assert_eq!(body["type"], "message");
    assert_eq!(body["content"][0]["text"], "hello");

    let responses = client()
        .await
        .post(format!("http://127.0.0.1:{}/v1/responses", status.port))
        .header("authorization", "Bearer local-test-token")
        .json(&json!({"model": "grok-4.5", "input": "hello"}))
        .send()
        .await
        .expect("responses route request");
    assert_eq!(responses.status(), StatusCode::NOT_FOUND);
    host.stop("grok-messages").await.expect("stop");
    upstream_task.abort();
}

#[tokio::test]
async fn grok_codex_bridge_passthrough_responses_and_404s_messages() {
    let (upstream_port, upstream_task) = grok_responses_upstream().await;
    let host = BridgeRuntimeHost::new();
    let status = host
        .start(grok_codex_spec("grok-responses", 0, upstream_port))
        .await
        .expect("start");
    let response = client()
        .await
        .post(format!("http://127.0.0.1:{}/v1/responses", status.port))
        .header(header::AUTHORIZATION, "Bearer local-test-token")
        .json(&json!({"model": "grok-4.5", "input": "hello"}))
        .send()
        .await
        .expect("responses request");
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.expect("responses json");
    assert_eq!(body["object"], "response");
    assert_eq!(body["output"][0]["content"][0]["text"], "hello");

    let messages = client()
        .await
        .post(format!("http://127.0.0.1:{}/v1/messages", status.port))
        .header("x-api-key", "local-test-token")
        .json(&json!({
            "model": "claude-test",
            "max_tokens": 32,
            "messages": [{ "role": "user", "content": "hello" }]
        }))
        .send()
        .await
        .expect("messages request");
    assert_eq!(messages.status(), StatusCode::NOT_FOUND);
    host.stop("grok-responses").await.expect("stop");
    upstream_task.abort();
}

#[tokio::test]
async fn grok_codex_passthrough_keeps_reasoning_object() {
    let (upstream_port, captured, upstream_task) = capturing_grok_responses_upstream().await;
    let host = BridgeRuntimeHost::new();
    let status = host
        .start(grok_codex_spec("grok-reasoning", 0, upstream_port))
        .await
        .expect("start");
    let response = client()
        .await
        .post(format!("http://127.0.0.1:{}/v1/responses", status.port))
        .header(header::AUTHORIZATION, "Bearer local-test-token")
        .json(&json!({
            "model": "grok-4.5",
            "input": "hello",
            "reasoning": { "effort": "high", "summary": "auto" }
        }))
        .send()
        .await
        .expect("responses request");
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.expect("responses json");
    assert_eq!(body["object"], "response");
    assert_eq!(body["output"][0]["content"][0]["text"], "hello");

    let upstream_bodies = captured.lock().expect("lock captured bodies").clone();
    assert_eq!(upstream_bodies.len(), 1);
    assert_eq!(upstream_bodies[0]["reasoning"]["effort"], "high");
    assert_eq!(upstream_bodies[0]["input"], "hello");
    assert_eq!(upstream_bodies[0]["model"], "grok-4.5");

    host.stop("grok-reasoning").await.expect("stop");
    upstream_task.abort();
}

#[tokio::test]
async fn kimi_chat_responses_with_reasoning_still_returns_responses_json() {
    let (upstream_port, captured, upstream_task) = capturing_upstream().await;
    let host = BridgeRuntimeHost::new();
    let status = host
        .start(spec("kimi-reasoning", 0, upstream_port))
        .await
        .expect("start");
    let response = client()
        .await
        .post(format!("http://127.0.0.1:{}/v1/responses", status.port))
        .header(header::AUTHORIZATION, "Bearer local-test-token")
        .json(&json!({
            "model": "test",
            "input": "hello",
            "reasoning": { "effort": "high" }
        }))
        .send()
        .await
        .expect("responses request");
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.expect("responses json");
    assert_ne!(body["error"]["code"], "unsupported_reasoning");
    assert_eq!(body["object"], "response");

    let upstream_bodies = captured.lock().expect("lock captured bodies").clone();
    assert_eq!(upstream_bodies.len(), 1);
    assert!(upstream_bodies[0].get("reasoning").is_none());
    assert!(upstream_bodies[0].get("reasoning_effort").is_none());

    host.stop("kimi-reasoning").await.expect("stop");
    upstream_task.abort();
}

#[tokio::test]
async fn grok_codex_passthrough_forwards_hosted_tools() {
    let (upstream_port, captured, upstream_task) = capturing_grok_responses_upstream().await;
    let host = BridgeRuntimeHost::new();
    let status = host
        .start(grok_codex_spec("grok-hosted-tools", 0, upstream_port))
        .await
        .expect("start");
    let response = client()
        .await
        .post(format!("http://127.0.0.1:{}/v1/responses", status.port))
        .header(header::AUTHORIZATION, "Bearer local-test-token")
        .json(&json!({
            "model": "grok-4.5",
            "input": "hello",
            "tools": [
                { "type": "web_search" },
                {
                    "type": "function",
                    "name": "lookup",
                    "parameters": { "type": "object", "properties": {} }
                }
            ]
        }))
        .send()
        .await
        .expect("responses request");
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.expect("responses json");
    assert_eq!(body["object"], "response");

    let upstream_bodies = captured.lock().expect("lock captured bodies").clone();
    assert_eq!(upstream_bodies.len(), 1);
    let tools = upstream_bodies[0]["tools"]
        .as_array()
        .expect("tools forwarded");
    assert_eq!(tools.len(), 2);
    assert_eq!(tools[0]["type"], "web_search");
    assert_eq!(tools[1]["type"], "function");
    assert_eq!(tools[1]["name"], "lookup");

    host.stop("grok-hosted-tools").await.expect("stop");
    upstream_task.abort();
}

#[tokio::test]
async fn grok_codex_passthrough_keeps_hosted_tools_only() {
    let (upstream_port, captured, upstream_task) = capturing_grok_responses_upstream().await;
    let host = BridgeRuntimeHost::new();
    let status = host
        .start(grok_codex_spec("grok-hosted-only", 0, upstream_port))
        .await
        .expect("start");
    let response = client()
        .await
        .post(format!("http://127.0.0.1:{}/v1/responses", status.port))
        .header(header::AUTHORIZATION, "Bearer local-test-token")
        .json(&json!({
            "model": "grok-4.5",
            "input": "hello",
            "tools": [
                { "type": "web_search" },
                { "type": "x_search" }
            ]
        }))
        .send()
        .await
        .expect("responses request");
    assert_eq!(response.status(), StatusCode::OK);

    let upstream_bodies = captured.lock().expect("lock captured bodies").clone();
    assert_eq!(upstream_bodies.len(), 1);
    let tools = upstream_bodies[0]["tools"].as_array().expect("tools");
    assert_eq!(tools.len(), 2);
    assert_eq!(tools[0]["type"], "web_search");
    assert_eq!(tools[1]["type"], "x_search");

    host.stop("grok-hosted-only").await.expect("stop");
    upstream_task.abort();
}

#[tokio::test]
async fn grok_claude_thinking_maps_to_upstream_reasoning() {
    let (upstream_port, captured, upstream_task) = capturing_grok_requests().await;
    let host = BridgeRuntimeHost::new();
    let status = host
        .start(grok_claude_spec("grok-thinking", 0, upstream_port))
        .await
        .expect("start");
    let response = client()
        .await
        .post(format!("http://127.0.0.1:{}/v1/messages", status.port))
        .header("x-api-key", "local-test-token")
        .json(&json!({
            "model": "claude-test",
            "max_tokens": 32,
            "thinking": { "type": "enabled", "effort": "high" },
            "messages": [{ "role": "user", "content": "hello" }]
        }))
        .send()
        .await
        .expect("messages request");
    assert_eq!(response.status(), StatusCode::OK);

    let captured = captured.lock().expect("lock").clone();
    assert_eq!(captured.len(), 1);
    assert_eq!(captured[0].body["reasoning"]["effort"], "high");
    assert_eq!(captured[0].body["reasoning"]["summary"], "detailed");
    let include = captured[0].body["include"].as_array().expect("include");
    assert!(include
        .iter()
        .any(|item| item == "reasoning.encrypted_content"));
    assert!(captured[0].body.get("thinking").is_none());
    assert_eq!(captured[0].body["model"], "grok-4.5");

    host.stop("grok-thinking").await.expect("stop");
    upstream_task.abort();
}

#[tokio::test]
async fn grok_codex_local_shell_is_upgraded_for_build() {
    let (upstream_port, captured, upstream_task) = capturing_grok_requests().await;
    let host = BridgeRuntimeHost::new();
    let status = host
        .start(grok_codex_spec("grok-shell-tool", 0, upstream_port))
        .await
        .expect("start");
    let response = client()
        .await
        .post(format!("http://127.0.0.1:{}/v1/responses", status.port))
        .header(header::AUTHORIZATION, "Bearer local-test-token")
        .json(&json!({
            "model": "grok-4.5",
            "input": "hello",
            "tools": [
                { "type": "local_shell" },
                { "type": "apply_patch" }
            ]
        }))
        .send()
        .await
        .expect("responses request");
    assert_eq!(response.status(), StatusCode::OK);

    let captured = captured.lock().expect("lock").clone();
    assert_eq!(captured.len(), 1);
    let tools = captured[0].body["tools"].as_array().expect("tools");
    assert_eq!(tools.len(), 2);
    assert_eq!(tools[0]["type"], "shell");
    assert_eq!(tools[0]["environment"]["type"], "local");
    assert_eq!(tools[1]["type"], "function");
    assert_eq!(tools[1]["name"], "apply_patch");

    host.stop("grok-shell-tool").await.expect("stop");
    upstream_task.abort();
}

#[tokio::test]
async fn grok_claude_session_header_sets_stable_cli_session() {
    let (upstream_port, captured, upstream_task) = capturing_grok_requests().await;
    let host = BridgeRuntimeHost::new();
    let status = host
        .start(grok_claude_spec("grok-session", 0, upstream_port))
        .await
        .expect("start");
    let response = client()
        .await
        .post(format!("http://127.0.0.1:{}/v1/messages", status.port))
        .header("x-api-key", "local-test-token")
        .header("X-Claude-Code-Session-Id", "sess-abc")
        .json(&json!({
            "model": "claude-test",
            "max_tokens": 32,
            "messages": [{ "role": "user", "content": "hello" }]
        }))
        .send()
        .await
        .expect("messages request");
    assert_eq!(response.status(), StatusCode::OK);

    let captured = captured.lock().expect("lock").clone();
    assert_eq!(captured.len(), 1);
    let expected =
        crate::bridge::grok_cli::grok_session_id("claude:sess-abc:agent:main").expect("session id");
    assert_eq!(
        captured[0]
            .headers
            .get("x-grok-session-id")
            .and_then(|value| value.to_str().ok()),
        Some(expected.as_str())
    );
    assert_eq!(
        captured[0]
            .headers
            .get("x-grok-conv-id")
            .and_then(|value| value.to_str().ok()),
        Some(expected.as_str())
    );
    assert_eq!(
        captured[0].body["prompt_cache_key"],
        "claude:sess-abc:agent:main"
    );
    assert!(captured[0].headers.get("x-grok-turn-idx").is_none());
    assert!(captured[0].headers.get("x-grok-req-id").is_some());
    assert!(captured[0].headers.get("x-grok-agent-id").is_some());

    host.stop("grok-session").await.expect("stop");
    upstream_task.abort();
}

#[tokio::test]
async fn grok_claude_replays_encrypted_reasoning_on_next_turn() {
    let reply = json!({
        "id": "resp_grok",
        "object": "response",
        "created_at": 1,
        "model": "grok-4.5",
        "status": "completed",
        "output": [
            { "id": "rs_1", "type": "reasoning", "encrypted_content": "enc-turn-1" },
            {
                "id": "msg_grok",
                "type": "message",
                "status": "completed",
                "role": "assistant",
                "content": [{ "type": "output_text", "text": "hello" }]
            }
        ],
        "usage": {
            "input_tokens": 2,
            "output_tokens": 3,
            "total_tokens": 5,
            "reasoning_tokens": 0,
            "output_tokens_details": { "reasoning_tokens": 0 }
        }
    });
    let (upstream_port, captured, upstream_task) = capturing_grok_requests_with_reply(reply).await;
    let host = BridgeRuntimeHost::new();
    let status = host
        .start(grok_claude_spec("grok-replay", 0, upstream_port))
        .await
        .expect("start");
    let url = format!("http://127.0.0.1:{}/v1/messages", status.port);
    for _ in 0..2 {
        let response = client()
            .await
            .post(&url)
            .header("x-api-key", "local-test-token")
            .header("X-Claude-Code-Session-Id", "sess-replay")
            .json(&json!({
                "model": "claude-test",
                "max_tokens": 32,
                "messages": [{ "role": "user", "content": "hello" }]
            }))
            .send()
            .await
            .expect("messages request");
        assert_eq!(response.status(), StatusCode::OK);
    }
    let captured = captured.lock().expect("lock").clone();
    assert_eq!(captured.len(), 2);
    let second_input = captured[1].body["input"].as_array().expect("input");
    assert_eq!(second_input[0]["type"], "reasoning");
    assert_eq!(second_input[0]["encrypted_content"], "enc-turn-1");

    host.stop("grok-replay").await.expect("stop");
    upstream_task.abort();
}

#[tokio::test]
async fn grok_codex_retries_after_encrypted_reasoning_400() {
    let (upstream_port, captured, upstream_task) = grok_decode_then_ok_upstream().await;
    let host = BridgeRuntimeHost::new();
    let status = host
        .start(grok_codex_spec("grok-decode-retry", 0, upstream_port))
        .await
        .expect("start");
    let response = client()
        .await
        .post(format!("http://127.0.0.1:{}/v1/responses", status.port))
        .header(header::AUTHORIZATION, "Bearer local-test-token")
        .json(&json!({
            "model": "grok-4.5",
            "input": [
                { "type": "reasoning", "encrypted_content": "stale-blob" },
                {
                    "type": "message",
                    "role": "user",
                    "content": [{ "type": "input_text", "text": "hello" }]
                }
            ]
        }))
        .send()
        .await
        .expect("responses request");
    assert_eq!(response.status(), StatusCode::OK);
    let captured = captured.lock().expect("lock").clone();
    assert_eq!(captured.len(), 2);
    let first = captured[0]["input"].as_array().expect("first input");
    assert!(first.iter().any(|item| item["type"] == "reasoning"));
    let second = captured[1]["input"].as_array().expect("second input");
    assert!(second.iter().all(|item| item["type"] != "reasoning"));

    host.stop("grok-decode-retry").await.expect("stop");
    upstream_task.abort();
}

fn completed_usage_from_responses_sse(body: &str) -> Value {
    body.split("\n\n")
        .filter_map(|frame| {
            frame
                .lines()
                .find_map(|line| line.strip_prefix("data: "))
                .and_then(|data| serde_json::from_str::<Value>(data).ok())
        })
        .find(|event| event["type"] == "response.completed")
        .expect("completed SSE event")["response"]["usage"]
        .clone()
}

fn assert_codex_completed_usage(usage: &Value) {
    #[derive(serde::Deserialize)]
    struct CodexCompletedUsage {
        input_tokens: i64,
        output_tokens: i64,
        total_tokens: i64,
        reasoning_tokens: i64,
        #[serde(default)]
        output_tokens_details: Option<CodexOutputDetails>,
    }
    #[derive(serde::Deserialize)]
    struct CodexOutputDetails {
        reasoning_tokens: i64,
    }
    let parsed: CodexCompletedUsage = serde_json::from_value(usage.clone())
        .expect("Codex ResponseCompleted usage must include reasoning_tokens");
    assert_eq!(
        parsed
            .output_tokens_details
            .as_ref()
            .map(|details| details.reasoning_tokens),
        Some(parsed.reasoning_tokens)
    );
}

#[tokio::test]
async fn grok_codex_passthrough_completed_json_includes_reasoning_tokens() {
    let (upstream_port, upstream_task) = grok_responses_upstream().await;
    let host = BridgeRuntimeHost::new();
    let status = host
        .start(grok_codex_spec("grok-reasoning-tokens", 0, upstream_port))
        .await
        .expect("start");
    let response = client()
        .await
        .post(format!("http://127.0.0.1:{}/v1/responses", status.port))
        .header(header::AUTHORIZATION, "Bearer local-test-token")
        .json(&json!({"model": "grok-4.5", "input": "ping"}))
        .send()
        .await
        .expect("responses request");
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.expect("responses json");
    assert_eq!(body["object"], "response");
    assert_eq!(body["usage"]["reasoning_tokens"], 0);
    assert_eq!(
        body["usage"]["output_tokens_details"]["reasoning_tokens"],
        0
    );
    assert_codex_completed_usage(&body["usage"]);
    host.stop("grok-reasoning-tokens").await.expect("stop");
    upstream_task.abort();
}

async fn grok_responses_sse_upstream(
    chunks: Vec<&'static [u8]>,
) -> (u16, tokio::task::JoinHandle<()>) {
    async fn responses(State(chunks): State<Vec<&'static [u8]>>) -> Response {
        let output = stream! {
            for chunk in chunks {
                yield Ok::<_, Infallible>(axum::body::Bytes::from_static(chunk));
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

#[tokio::test]
async fn grok_codex_passthrough_sse_forwards_completed_event() {
    let (upstream_port, upstream_task) = grok_responses_sse_upstream(vec![
        b"event: response.completed\ndata: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_stream\",\"usage\":{\"input_tokens\":1,\"output_tokens\":1,\"total_tokens\":2,\"reasoning_tokens\":0,\"output_tokens_details\":{\"reasoning_tokens\":0}}}}\n\n",
    ])
    .await;
    let host = BridgeRuntimeHost::new();
    let status = host
        .start(grok_codex_spec(
            "grok-reasoning-tokens-sse",
            0,
            upstream_port,
        ))
        .await
        .expect("start");
    let body = client()
        .await
        .post(format!("http://127.0.0.1:{}/v1/responses", status.port))
        .header(header::AUTHORIZATION, "Bearer local-test-token")
        .json(&json!({"model":"grok-4.5","input":"ping","stream":true}))
        .send()
        .await
        .expect("stream request")
        .text()
        .await
        .expect("stream body");
    assert!(body.contains("response.completed"));
    let usage = completed_usage_from_responses_sse(&body);
    assert_eq!(usage["input_tokens"], 1);
    assert_eq!(usage["output_tokens"], 1);
    assert_eq!(usage["reasoning_tokens"], 0);
    host.stop("grok-reasoning-tokens-sse").await.expect("stop");
    upstream_task.abort();
}

async fn grok_401_then_ok_upstream(
    expected_retry_bearer: &'static str,
) -> (u16, Arc<Mutex<Vec<String>>>, tokio::task::JoinHandle<()>) {
    async fn responses(
        State((expected, captured)): State<(&'static str, Arc<Mutex<Vec<String>>>)>,
        headers: axum::http::HeaderMap,
    ) -> Response {
        let bearer = headers
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_owned();
        captured.lock().expect("lock").push(bearer.clone());
        if bearer != format!("Bearer {expected}") {
            return (StatusCode::UNAUTHORIZED, "expired").into_response();
        }
        Json(grok_completed_response("hello")).into_response()
    }
    let captured = Arc::new(Mutex::new(Vec::new()));
    let listener =
        tokio::net::TcpListener::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
            .await
            .expect("bind grok 401 upstream");
    let port = listener.local_addr().expect("addr").port();
    let state = (expected_retry_bearer, captured.clone());
    let task = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new()
                .route("/v1/responses", post(responses))
                .with_state(state),
        )
        .await
        .expect("serve grok 401 upstream");
    });
    (port, captured, task)
}

#[tokio::test]
async fn oauth_401_reloads_upstream_bearer_and_retries_before_first_event() {
    let (upstream_port, captured, upstream_task) =
        grok_401_then_ok_upstream("rotated-upstream-token").await;
    let hits = Arc::new(AtomicUsize::new(0));
    let hits_cb = hits.clone();
    let reload: UpstreamAuthReload = Arc::new(move || {
        hits_cb.fetch_add(1, Ordering::SeqCst);
        Some("rotated-upstream-token".into())
    });
    let host = BridgeRuntimeHost::new();
    let status = host
        .start(
            grok_claude_spec("grok-401-retry", 0, upstream_port)
                .with_reload_upstream_auth(Some(reload)),
        )
        .await
        .expect("start");
    let response = client()
        .await
        .post(format!("http://127.0.0.1:{}/v1/messages", status.port))
        .header("x-api-key", "local-test-token")
        .json(&json!({
            "model": "claude-test",
            "max_tokens": 32,
            "messages": [{ "role": "user", "content": "hello" }]
        }))
        .send()
        .await
        .expect("messages request");
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.expect("anthropic response");
    assert_eq!(body["content"][0]["text"], "hello");
    assert_eq!(hits.load(Ordering::SeqCst), 1);
    let bearers = captured.lock().expect("lock").clone();
    assert_eq!(
        bearers,
        vec![
            "Bearer upstream-test-token".to_owned(),
            "Bearer rotated-upstream-token".to_owned()
        ]
    );

    let still_local = client()
        .await
        .post(format!("http://127.0.0.1:{}/v1/messages", status.port))
        .header("x-api-key", "local-test-token")
        .json(&json!({
            "model": "claude-test",
            "max_tokens": 32,
            "messages": [{ "role": "user", "content": "hello" }]
        }))
        .send()
        .await
        .expect("local bearer still accepted");
    assert_eq!(still_local.status(), StatusCode::OK);

    host.stop("grok-401-retry").await.expect("stop");
    upstream_task.abort();
}

#[tokio::test]
async fn oauth_401_retries_after_noop_near_expiry_preload() {
    let (upstream_port, captured, upstream_task) =
        grok_401_then_ok_upstream("rotated-upstream-token").await;
    let hits = Arc::new(AtomicUsize::new(0));
    let hits_cb = hits.clone();
    let reload: UpstreamAuthReload = Arc::new(move || {
        let n = hits_cb.fetch_add(1, Ordering::SeqCst) + 1;
        if n == 1 {
            None
        } else {
            Some("rotated-upstream-token".into())
        }
    });
    let near_expiry = unsigned_jwt_exp_offset(30);
    let mut spec = grok_claude_spec("grok-401-near-exp", 0, upstream_port)
        .with_reload_upstream_auth(Some(reload));
    spec.upstream.auth = ResolvedAuth::bearer(near_expiry.clone());
    let host = BridgeRuntimeHost::new();
    let status = host.start(spec).await.expect("start");
    let response = client()
        .await
        .post(format!("http://127.0.0.1:{}/v1/messages", status.port))
        .header("x-api-key", "local-test-token")
        .json(&json!({
            "model": "claude-test",
            "max_tokens": 32,
            "messages": [{ "role": "user", "content": "hello" }]
        }))
        .send()
        .await
        .expect("messages request");
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(hits.load(Ordering::SeqCst), 2);
    let bearers = captured.lock().expect("lock").clone();
    assert_eq!(
        bearers,
        vec![
            format!("Bearer {near_expiry}"),
            "Bearer rotated-upstream-token".to_owned()
        ]
    );
    host.stop("grok-401-near-exp").await.expect("stop");
    upstream_task.abort();
}

#[tokio::test]
async fn oauth_401_without_new_token_stays_upstream_error() {
    let (upstream_port, captured, upstream_task) =
        grok_401_then_ok_upstream("rotated-upstream-token").await;
    let hits = Arc::new(AtomicUsize::new(0));
    let hits_cb = hits.clone();
    let reload: UpstreamAuthReload = Arc::new(move || {
        hits_cb.fetch_add(1, Ordering::SeqCst);
        None
    });
    let host = BridgeRuntimeHost::new();
    let status = host
        .start(
            grok_claude_spec("grok-401-none", 0, upstream_port)
                .with_reload_upstream_auth(Some(reload)),
        )
        .await
        .expect("start");
    let response = client()
        .await
        .post(format!("http://127.0.0.1:{}/v1/messages", status.port))
        .header("x-api-key", "local-test-token")
        .json(&json!({
            "model": "claude-test",
            "max_tokens": 32,
            "messages": [{ "role": "user", "content": "hello" }]
        }))
        .send()
        .await
        .expect("messages request");
    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
    let body: Value = response.json().await.expect("error json");
    assert_eq!(body["error"]["code"], "upstream_error");
    assert_eq!(hits.load(Ordering::SeqCst), 1);
    let bearers = captured.lock().expect("lock").clone();
    assert_eq!(bearers, vec!["Bearer upstream-test-token".to_owned()]);

    host.stop("grok-401-none").await.expect("stop");
    upstream_task.abort();
}

const TOKEN_A: &str = "local-token-edge-a-aaaaaaaa";
const TOKEN_B: &str = "local-token-edge-b-bbbbbbbb";

fn grok_claude_with_token(
    profile_id: &str,
    port: u16,
    upstream_port: u16,
    token: &str,
) -> BridgeStartSpec {
    let mut spec = grok_claude_spec(profile_id, port, upstream_port);
    spec.local_token = token.to_owned();
    spec
}

fn grok_codex_with_token(
    profile_id: &str,
    port: u16,
    upstream_port: u16,
    token: &str,
) -> BridgeStartSpec {
    let mut spec = grok_codex_spec(profile_id, port, upstream_port);
    spec.local_token = token.to_owned();
    spec
}

async fn free_loopback_port() -> u16 {
    let listener =
        tokio::net::TcpListener::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
            .await
            .expect("probe loopback port");
    listener.local_addr().expect("probe addr").port()
}

#[tokio::test]
async fn two_profiles_two_bearers_two_surfaces_do_not_cross() {
    let (upstream_port, upstream_task) = grok_responses_upstream().await;
    let host = BridgeRuntimeHost::new();
    let messages = host
        .start(grok_claude_with_token(
            "edge-messages",
            0,
            upstream_port,
            TOKEN_A,
        ))
        .await
        .expect("start messages edge");
    let responses = host
        .start(grok_codex_with_token(
            "edge-responses",
            0,
            upstream_port,
            TOKEN_B,
        ))
        .await
        .expect("start responses edge");
    assert_eq!(
        responses.port, messages.port,
        "new start projects onto the existing gateway socket"
    );
    let http = client().await;
    let port = messages.port;

    let served_messages = http
        .post(format!("http://127.0.0.1:{port}/v1/messages"))
        .header("x-api-key", TOKEN_A)
        .json(&json!({
            "model": "claude-test",
            "max_tokens": 32,
            "messages": [{ "role": "user", "content": "hello" }]
        }))
        .send()
        .await
        .expect("messages for A");
    assert_eq!(served_messages.status(), StatusCode::OK);

    let cross_responses = http
        .post(format!("http://127.0.0.1:{port}/v1/responses"))
        .header(header::AUTHORIZATION, format!("Bearer {TOKEN_A}"))
        .json(&json!({"model": "grok-4.5", "input": "hello"}))
        .send()
        .await
        .expect("responses for A");
    assert_eq!(cross_responses.status(), StatusCode::NOT_FOUND);
    assert!(cross_responses.text().await.expect("empty 404").is_empty());

    let served_responses = http
        .post(format!("http://127.0.0.1:{port}/v1/responses"))
        .header(header::AUTHORIZATION, format!("Bearer {TOKEN_B}"))
        .json(&json!({"model": "grok-4.5", "input": "hello"}))
        .send()
        .await
        .expect("responses for B");
    assert_eq!(served_responses.status(), StatusCode::OK);

    let cross_messages = http
        .post(format!("http://127.0.0.1:{port}/v1/messages"))
        .header("x-api-key", TOKEN_B)
        .json(&json!({
            "model": "claude-test",
            "max_tokens": 32,
            "messages": [{ "role": "user", "content": "hello" }]
        }))
        .send()
        .await
        .expect("messages for B");
    assert_eq!(cross_messages.status(), StatusCode::NOT_FOUND);

    host.shutdown().await.expect("shutdown");
    upstream_task.abort();
}

#[tokio::test]
async fn shared_port_tokens_do_not_cross_on_models() {
    let (upstream_port, upstream_task) = upstream().await;
    let host = BridgeRuntimeHost::new();
    let first = host
        .start(
            spec_with_token("models-a", 0, upstream_port, TOKEN_A)
                .with_listed_models(vec!["model-a".into()]),
        )
        .await
        .expect("start A");
    let second = host
        .start(
            spec_with_token("models-b", first.port, upstream_port, TOKEN_B)
                .with_listed_models(vec!["model-b".into()]),
        )
        .await
        .expect("start B on shared port");
    assert_eq!(second.port, first.port);
    let http = client().await;
    let listed_a = http
        .get(format!("http://127.0.0.1:{}/v1/models", first.port))
        .header(header::AUTHORIZATION, format!("Bearer {TOKEN_A}"))
        .send()
        .await
        .expect("models A")
        .json::<Value>()
        .await
        .expect("models A json");
    assert_eq!(listed_a["data"][0]["id"], "model-a");
    assert_eq!(listed_a["data"].as_array().map(Vec::len), Some(1));
    let listed_b = http
        .get(format!("http://127.0.0.1:{}/v1/models", first.port))
        .header(header::AUTHORIZATION, format!("Bearer {TOKEN_B}"))
        .send()
        .await
        .expect("models B")
        .json::<Value>()
        .await
        .expect("models B json");
    assert_eq!(listed_b["data"][0]["id"], "model-b");
    host.shutdown().await.expect("shutdown");
    upstream_task.abort();
}

#[tokio::test]
async fn alias_port_reaches_the_same_edge() {
    let (upstream_port, upstream_task) = grok_responses_upstream().await;
    let host = BridgeRuntimeHost::new();
    let primary = host
        .start(grok_claude_with_token(
            "alias-primary",
            0,
            upstream_port,
            TOKEN_A,
        ))
        .await
        .expect("start primary");
    let alias_port = free_loopback_port().await;
    let aliased = host
        .start(grok_codex_with_token(
            "alias-historical",
            alias_port,
            upstream_port,
            TOKEN_B,
        ))
        .await
        .expect("bind historical alias");
    assert_eq!(aliased.port, alias_port);
    assert_ne!(alias_port, primary.port);
    let http = client().await;

    let via_alias = http
        .post(format!("http://127.0.0.1:{alias_port}/v1/messages"))
        .header("x-api-key", TOKEN_A)
        .json(&json!({
            "model": "claude-test",
            "max_tokens": 32,
            "messages": [{ "role": "user", "content": "hello" }]
        }))
        .send()
        .await
        .expect("A via alias port");
    assert_eq!(via_alias.status(), StatusCode::OK);

    let via_primary = http
        .post(format!("http://127.0.0.1:{}/v1/responses", primary.port))
        .header(header::AUTHORIZATION, format!("Bearer {TOKEN_B}"))
        .json(&json!({"model": "grok-4.5", "input": "hello"}))
        .send()
        .await
        .expect("B via primary port");
    assert_eq!(via_primary.status(), StatusCode::OK);

    host.shutdown().await.expect("shutdown");
    upstream_task.abort();
}

#[tokio::test]
async fn missing_bearer_is_401_on_every_registered_path() {
    let (upstream_port, upstream_task) = grok_responses_upstream().await;
    let host = BridgeRuntimeHost::new();
    let status = host
        .start(grok_claude_spec("unauth-paths", 0, upstream_port))
        .await
        .expect("start");
    let http = client().await;
    let port = status.port;
    for path in [
        "/health",
        "/v1/models",
        "/models",
        "/v1/responses",
        "/v1/messages",
        "/v1/chat/completions",
        "/chat/completions",
    ] {
        let builder = if path == "/health" || path.ends_with("models") {
            http.get(format!("http://127.0.0.1:{port}{path}"))
        } else {
            http.post(format!("http://127.0.0.1:{port}{path}"))
                .json(&json!({"model": "test", "input": "hello"}))
        };
        let response = builder.send().await.expect(path);
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{path}");
        let body = response.json::<Value>().await.expect("error json");
        assert_eq!(body["error"]["code"], "invalid_api_key", "{path}");
    }
    let wrong_surface = http
        .post(format!("http://127.0.0.1:{port}/v1/responses"))
        .header(header::AUTHORIZATION, "Bearer wrong-token")
        .json(&json!({"model": "test", "input": "hello"}))
        .send()
        .await
        .expect("wrong token on unserved path");
    assert_eq!(wrong_surface.status(), StatusCode::UNAUTHORIZED);
    host.stop("unauth-paths").await.expect("stop");
    upstream_task.abort();
}

#[tokio::test]
async fn health_upstream_status_depends_on_bearer() {
    let (upstream_port, upstream_task) = upstream().await;
    let host = BridgeRuntimeHost::new();
    let first = host
        .start(spec_with_token("health-a", 0, upstream_port, TOKEN_A))
        .await
        .expect("start A");
    host.start(spec_with_token(
        "health-b",
        first.port,
        upstream_port,
        TOKEN_B,
    ))
    .await
    .expect("start B");
    host.record_upstream_outcome("health-a", BridgeUpstreamStatus::Connected)
        .expect("record A")
        .expect("A running");
    host.record_upstream_outcome("health-b", BridgeUpstreamStatus::Degraded)
        .expect("record B")
        .expect("B running");
    let http = client().await;
    let health_a = http
        .get(format!("http://127.0.0.1:{}/health", first.port))
        .header(header::AUTHORIZATION, format!("Bearer {TOKEN_A}"))
        .send()
        .await
        .expect("health A")
        .json::<Value>()
        .await
        .expect("health A json");
    assert_eq!(health_a["upstream_status"], "connected");
    let health_b = http
        .get(format!("http://127.0.0.1:{}/health", first.port))
        .header(header::AUTHORIZATION, format!("Bearer {TOKEN_B}"))
        .send()
        .await
        .expect("health B")
        .json::<Value>()
        .await
        .expect("health B json");
    assert_eq!(health_b["upstream_status"], "degraded");
    host.shutdown().await.expect("shutdown");
    upstream_task.abort();
}

#[tokio::test]
async fn new_start_projects_existing_gateway_port() {
    let (upstream_port, upstream_task) = upstream().await;
    let host = BridgeRuntimeHost::new();
    let first = host
        .start(spec_with_token("project-a", 0, upstream_port, TOKEN_A))
        .await
        .expect("start first");
    let second = host
        .start(spec_with_token("project-b", 0, upstream_port, TOKEN_B))
        .await
        .expect("start second");
    assert_eq!(second.port, first.port);
    host.shutdown().await.expect("shutdown");
    upstream_task.abort();
}
