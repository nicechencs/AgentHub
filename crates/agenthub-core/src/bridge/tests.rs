use std::convert::Infallible;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
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
    index_from_member_listings, protocol::responses::is_leftover_bridge_model, BridgeHostError,
    BridgeLocalSurface, BridgeMemberSpec, BridgeRuntimeHost, BridgeRuntimeState, BridgeStartSpec,
    BridgeUpstreamConfig, BridgeUpstreamProtocol, BridgeUpstreamStatus, EffectiveRouteIndex,
    MemberCapability, MemberCapabilitySnapshot, MemberHealth, MemberListing, ResolvedAuth,
    UpstreamAuthReload,
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

fn grok_chat_spec(profile_id: &str, port: u16, upstream_port: u16) -> BridgeStartSpec {
    let mut spec = grok_claude_spec(profile_id, port, upstream_port);
    spec.upstream.local_surface = BridgeLocalSurface::ChatCompletions;
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

async fn html_success_upstream(path: &'static str) -> (u16, tokio::task::JoinHandle<()>) {
    async fn html() -> Response {
        (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "text/html")],
            "<html>upstream-error-page</html>",
        )
            .into_response()
    }
    let listener =
        tokio::net::TcpListener::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
            .await
            .expect("bind html upstream");
    let port = listener.local_addr().expect("upstream addr").port();
    let task = tokio::spawn(async move {
        axum::serve(listener, Router::new().route(path, post(html)))
            .await
            .expect("serve html upstream");
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

async fn sse_upstream_with_delayed_tail(
    first: &'static [u8],
    delay: Duration,
    tail: &'static [u8],
) -> (u16, tokio::task::JoinHandle<()>) {
    async fn responses(
        State((first, delay, tail)): State<(&'static [u8], Duration, &'static [u8])>,
    ) -> Response {
        let output = stream! {
            yield Ok::<_, Infallible>(axum::body::Bytes::from_static(first));
            tokio::time::sleep(delay).await;
            yield Ok::<_, Infallible>(axum::body::Bytes::from_static(tail));
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
            .expect("bind delayed-tail mock SSE upstream");
    let port = listener.local_addr().expect("upstream addr").port();
    let task = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new()
                .route("/v1/responses", post(responses))
                .with_state((first, delay, tail)),
        )
        .await
        .expect("serve delayed-tail mock SSE upstream");
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

#[test]
fn sse_frame_delimiter_accepts_cr_only_line_endings() {
    assert_eq!(
        super::host::sse_frame_end(b"data: one\rdata: two\r\r"),
        Some((19, 2))
    );
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
async fn converted_responses_stream_error_has_exact_protocol_contract() {
    let (upstream_port, upstream_task) = sse_upstream(vec![
        b"data: {\"id\":\"chat-stream\",\"model\":\"kimi-test\",\"choices\":[{\"delta\":{\"content\":\"partial\"}}]}\n\n",
    ])
    .await;
    let host = BridgeRuntimeHost::new();
    let status = host
        .start(spec("responses-converted-error-contract", 0, upstream_port))
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

    assert_eq!(
        responses_error_data(&body),
        json!({
            "type": "error",
            "code": "upstream_error",
            "message": "The upstream model provider returned an invalid stream.",
            "param": null,
            "sequence_number": 5,
        })
    );
    assert!(!body.contains("private malformed content"));
    host.stop("responses-converted-error-contract")
        .await
        .expect("stop");
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
async fn chat_surface_relays_openai_chat_without_protocol_conversion() {
    let (upstream_port, captured, upstream_task) = capturing_upstream().await;
    let host = BridgeRuntimeHost::new();
    let mut configured = spec("chat-identity", 0, upstream_port);
    configured.upstream.local_surface = BridgeLocalSurface::ChatCompletions;
    let status = host.start(configured).await.expect("start");
    let request_body = json!({
        "model": "test",
        "messages": [{ "role": "user", "content": "hello" }],
        "response_format": { "type": "json_object" }
    });
    let response = client()
        .await
        .post(format!(
            "http://127.0.0.1:{}/v1/chat/completions",
            status.port
        ))
        .header(header::AUTHORIZATION, "Bearer local-test-token")
        .json(&request_body)
        .send()
        .await
        .expect("chat request");
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.json::<Value>().await.expect("chat response");
    assert_eq!(body["choices"][0]["message"]["content"], "hello");
    let upstream = captured.lock().expect("lock captured bodies").clone();
    assert_eq!(upstream, vec![request_body]);
    host.stop("chat-identity").await.expect("stop");
    upstream_task.abort();
}

#[tokio::test]
async fn messages_surface_relays_anthropic_messages_without_protocol_conversion() {
    let (upstream_port, upstream_task) = anthropic_upstream().await;
    let host = BridgeRuntimeHost::new();
    let mut configured = anthropic_spec("messages-identity", 0, upstream_port);
    configured.upstream.local_surface = BridgeLocalSurface::Messages;
    let status = host.start(configured).await.expect("start");
    let response = client()
        .await
        .post(format!("http://127.0.0.1:{}/v1/messages", status.port))
        .header("x-api-key", "local-test-token")
        .json(&json!({
            "model": "claude-test",
            "max_tokens": 32,
            "messages": [{
                "role": "user",
                "content": [{ "type": "text", "text": "hello" }]
            }]
        }))
        .send()
        .await
        .expect("messages request");
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.json::<Value>().await.expect("messages response");
    assert_eq!(body["type"], "message");
    assert_eq!(body["content"][0]["text"], "你好");
    host.stop("messages-identity").await.expect("stop");
    upstream_task.abort();
}

#[tokio::test]
async fn chat_surface_converts_to_and_from_anthropic_messages() {
    let (upstream_port, upstream_task) = anthropic_upstream().await;
    let host = BridgeRuntimeHost::new();
    let mut configured = anthropic_spec("chat-anthropic", 0, upstream_port);
    configured.upstream.local_surface = BridgeLocalSurface::ChatCompletions;
    let status = host.start(configured).await.expect("start");
    let response = client()
        .await
        .post(format!(
            "http://127.0.0.1:{}/v1/chat/completions",
            status.port
        ))
        .header(header::AUTHORIZATION, "Bearer local-test-token")
        .json(&json!({
            "model": "gpt-test",
            "messages": [{ "role": "user", "content": "hello" }]
        }))
        .send()
        .await
        .expect("chat request");
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.json::<Value>().await.expect("chat response");
    assert_eq!(body["choices"][0]["message"]["content"], "你好");
    assert_eq!(body["choices"][0]["finish_reason"], "stop");
    host.stop("chat-anthropic").await.expect("stop");
    upstream_task.abort();
}

#[tokio::test]
async fn identity_non_stream_rejects_non_json_success_body() {
    let (upstream_port, upstream_task) = html_success_upstream("/v1/responses").await;
    let host = BridgeRuntimeHost::new();
    let status = host
        .start(grok_codex_spec("html-passthrough", 0, upstream_port))
        .await
        .expect("start");
    let response = client()
        .await
        .post(format!("http://127.0.0.1:{}/v1/responses", status.port))
        .header(header::AUTHORIZATION, "Bearer local-test-token")
        .json(&json!({"model": "grok-4.5", "input": "hello"}))
        .send()
        .await
        .expect("request");
    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
    let body = response.text().await.expect("body");
    assert!(body.contains("upstream_error"));
    assert!(!body.contains("upstream-error-page"));
    host.stop("html-passthrough").await.expect("stop");
    upstream_task.abort();
}

#[tokio::test]
async fn chat_identity_idle_stream_uses_chat_error_frame() {
    let (upstream_port, upstream_task) =
        delayed_sse_upstream(Duration::from_millis(200), vec![b"data: [DONE]\n\n"]).await;
    let host = BridgeRuntimeHost::new();
    let mut configured = spec("chat-idle", 0, upstream_port);
    configured.upstream.local_surface = BridgeLocalSurface::ChatCompletions;
    let status = host.start(configured).await.expect("start");
    let body = client()
        .await
        .post(format!(
            "http://127.0.0.1:{}/v1/chat/completions",
            status.port
        ))
        .header(header::AUTHORIZATION, "Bearer local-test-token")
        .json(&json!({
            "model": "test",
            "messages": [{ "role": "user", "content": "hi" }],
            "stream": true
        }))
        .send()
        .await
        .expect("stream request")
        .text()
        .await
        .expect("stream body");
    assert!(body.contains("The upstream model provider returned an invalid stream."));
    assert!(body.contains("data: [DONE]"));
    assert!(!body.contains("event: error"));
    host.stop("chat-idle").await.expect("stop");
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
async fn grok_chat_replays_encrypted_reasoning_on_next_turn() {
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
        .start(grok_chat_spec("grok-chat-replay", 0, upstream_port))
        .await
        .expect("start");
    let url = format!("http://127.0.0.1:{}/v1/chat/completions", status.port);
    for _ in 0..2 {
        let response = client()
            .await
            .post(&url)
            .header(header::AUTHORIZATION, "Bearer local-test-token")
            .header("x-session-id", "sess-replay")
            .json(&json!({
                "model": "gpt-test",
                "messages": [{ "role": "user", "content": "hello" }]
            }))
            .send()
            .await
            .expect("chat request");
        assert_eq!(response.status(), StatusCode::OK);
    }
    let captured = captured.lock().expect("lock").clone();
    assert_eq!(captured.len(), 2);
    let second_input = captured[1].body["input"].as_array().expect("input");
    assert_eq!(second_input[0]["type"], "reasoning");
    assert_eq!(second_input[0]["encrypted_content"], "enc-turn-1");

    host.stop("grok-chat-replay").await.expect("stop");
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

fn responses_error_data(body: &str) -> Value {
    body.split("\n\n")
        .filter(|frame| frame.lines().any(|line| line == "event: error"))
        .find_map(|frame| {
            frame
                .lines()
                .find_map(|line| line.strip_prefix("data: "))
                .and_then(|data| serde_json::from_str::<Value>(data).ok())
        })
        .expect("Responses error SSE event")
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
        b"event: response.completed\ndata: {\"type\":\"response.completed\",\"sequence_number\":0,\"response\":{\"id\":\"resp_stream\",\"usage\":{\"input_tokens\":1,\"output_tokens\":1,\"total_tokens\":2,\"reasoning_tokens\":0,\"output_tokens_details\":{\"reasoning_tokens\":0}}}}\n\n",
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

#[tokio::test]
async fn responses_passthrough_stream_error_uses_next_upstream_sequence_number() {
    let (upstream_port, upstream_task) = sse_upstream_with_delayed_tail(
        b"event: response.created\ndata: {\"type\":\"response.created\",\"sequence_number\":7,\"response\":{\"id\":\"resp_stream\"}}\n\nevent: response.in_progress\ndata: {\"type\":\"response.in",
        Duration::from_millis(200),
        b"_progress\",\"sequence_number\":8}\n\n",
    )
    .await;
    let host = BridgeRuntimeHost::new();
    let status = host
        .start(grok_codex_spec(
            "responses-passthrough-error-contract",
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

    assert_eq!(
        responses_error_data(&body),
        json!({
            "type": "error",
            "code": "upstream_error",
            "message": "The upstream model provider returned an invalid stream.",
            "param": null,
            "sequence_number": 8,
        })
    );
    assert!(body.contains("\"sequence_number\":7"));
    assert!(!body.contains("response.in_progress"));
    assert!(!body.contains("upstream-private-error"));
    host.stop("responses-passthrough-error-contract")
        .await
        .expect("stop");
    upstream_task.abort();
}

#[tokio::test]
async fn responses_passthrough_nonterminal_eof_emits_error_frame() {
    let (upstream_port, upstream_task) = grok_responses_sse_upstream(vec![
        b"event: response.created\ndata: {\"type\":\"response.created\",\"sequence_number\":7,\"response\":{\"id\":\"resp_stream\"}}\n\n",
    ])
    .await;
    let host = BridgeRuntimeHost::new();
    let status = host
        .start(grok_codex_spec(
            "responses-passthrough-nonterminal-eof",
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

    assert!(body.contains("response.created"));
    assert_eq!(
        responses_error_data(&body),
        json!({
            "type": "error",
            "code": "upstream_error",
            "message": "The upstream model provider returned an invalid stream.",
            "param": null,
            "sequence_number": 8,
        })
    );
    host.stop("responses-passthrough-nonterminal-eof")
        .await
        .expect("stop");
    upstream_task.abort();
}

#[tokio::test]
async fn responses_passthrough_malformed_terminal_emits_error_frame() {
    let (upstream_port, upstream_task) =
        grok_responses_sse_upstream(vec![b"event: response.completed\ndata: not-json\n\n"]).await;
    let host = BridgeRuntimeHost::new();
    let status = host
        .start(grok_codex_spec(
            "responses-passthrough-malformed-terminal",
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

    assert_eq!(responses_error_data(&body)["sequence_number"], 0);
    assert!(!body.contains("response.completed"));
    host.stop("responses-passthrough-malformed-terminal")
        .await
        .expect("stop");
    upstream_task.abort();
}

#[tokio::test]
async fn responses_passthrough_mismatched_event_emits_error_frame() {
    let (upstream_port, upstream_task) = grok_responses_sse_upstream(vec![
        b"event: response.completed\ndata: {\"type\":\"response.in_progress\",\"sequence_number\":7}\n\n",
    ])
    .await;
    let host = BridgeRuntimeHost::new();
    let status = host
        .start(grok_codex_spec(
            "responses-passthrough-mismatched-event",
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

    assert_eq!(responses_error_data(&body)["sequence_number"], 0);
    assert!(!body.contains("response.in_progress"));
    host.stop("responses-passthrough-mismatched-event")
        .await
        .expect("stop");
    upstream_task.abort();
}

#[tokio::test]
async fn responses_passthrough_done_marker_emits_error_without_forwarding_it() {
    let (upstream_port, upstream_task) =
        grok_responses_sse_upstream(vec![b"data: [DONE]\n\n"]).await;
    let host = BridgeRuntimeHost::new();
    let status = host
        .start(grok_codex_spec(
            "responses-passthrough-done-marker",
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

    assert_eq!(responses_error_data(&body)["sequence_number"], 0);
    assert!(!body.contains("[DONE]"));
    host.stop("responses-passthrough-done-marker")
        .await
        .expect("stop");
    upstream_task.abort();
}

#[tokio::test]
async fn responses_passthrough_upstream_error_is_sanitized() {
    let (upstream_port, upstream_task) = grok_responses_sse_upstream(vec![
        b"event: error\ndata: {\"type\":\"error\",\"sequence_number\":7,\"code\":\"provider_error\",\"message\":\"upstream-private-error\"}\n\n",
    ])
    .await;
    let host = BridgeRuntimeHost::new();
    let status = host
        .start(grok_codex_spec(
            "responses-passthrough-sanitized-error",
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

    assert_eq!(responses_error_data(&body)["sequence_number"], 7);
    assert!(!body.contains("upstream-private-error"));
    host.stop("responses-passthrough-sanitized-error")
        .await
        .expect("stop");
    upstream_task.abort();
}

#[tokio::test]
async fn responses_passthrough_missing_sequence_emits_error_frame() {
    let (upstream_port, upstream_task) = grok_responses_sse_upstream(vec![
        b"event: response.completed\ndata: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_stream\"}}\n\n",
    ])
    .await;
    let host = BridgeRuntimeHost::new();
    let status = host
        .start(grok_codex_spec(
            "responses-passthrough-missing-sequence",
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

    assert_eq!(responses_error_data(&body)["sequence_number"], 0);
    assert!(!body.contains("response.completed"));
    host.stop("responses-passthrough-missing-sequence")
        .await
        .expect("stop");
    upstream_task.abort();
}

#[tokio::test]
async fn responses_passthrough_missing_event_emits_error_frame() {
    let (upstream_port, upstream_task) = grok_responses_sse_upstream(vec![
        b"data: {\"type\":\"response.completed\",\"sequence_number\":7,\"response\":{\"id\":\"resp_stream\"}}\n\n",
    ])
    .await;
    let host = BridgeRuntimeHost::new();
    let status = host
        .start(grok_codex_spec(
            "responses-passthrough-missing-event",
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

    assert_eq!(responses_error_data(&body)["sequence_number"], 0);
    assert!(!body.contains("response.completed"));
    host.stop("responses-passthrough-missing-event")
        .await
        .expect("stop");
    upstream_task.abort();
}

#[tokio::test]
async fn responses_passthrough_unknown_event_type_emits_error_frame() {
    let (upstream_port, upstream_task) = grok_responses_sse_upstream(vec![
        b"event: response.private\ndata: {\"type\":\"response.private\",\"sequence_number\":7,\"message\":\"upstream-private-event\"}\n\n",
    ])
    .await;
    let host = BridgeRuntimeHost::new();
    let status = host
        .start(grok_codex_spec(
            "responses-passthrough-unknown-event",
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

    assert_eq!(responses_error_data(&body)["sequence_number"], 0);
    assert!(!body.contains("upstream-private-event"));
    host.stop("responses-passthrough-unknown-event")
        .await
        .expect("stop");
    upstream_task.abort();
}

#[tokio::test]
async fn responses_passthrough_bare_event_field_emits_error_frame() {
    let (upstream_port, upstream_task) = grok_responses_sse_upstream(vec![
        b"event: response.completed\nevent\ndata: {\"type\":\"response.completed\",\"sequence_number\":7,\"response\":{\"id\":\"resp_stream\"}}\n\n",
    ])
    .await;
    let host = BridgeRuntimeHost::new();
    let status = host
        .start(grok_codex_spec(
            "responses-passthrough-bare-event",
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

    assert_eq!(responses_error_data(&body)["sequence_number"], 0);
    assert!(!body.contains("response.completed"));
    host.stop("responses-passthrough-bare-event")
        .await
        .expect("stop");
    upstream_task.abort();
}

#[tokio::test]
async fn responses_passthrough_duplicate_event_field_emits_error_frame() {
    let (upstream_port, upstream_task) = grok_responses_sse_upstream(vec![
        b"event: response.completed\nevent: response.completed\ndata: {\"type\":\"response.completed\",\"sequence_number\":7,\"response\":{\"id\":\"resp_stream\"}}\n\n",
    ])
    .await;
    let host = BridgeRuntimeHost::new();
    let status = host
        .start(grok_codex_spec(
            "responses-passthrough-duplicate-event",
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

    assert_eq!(responses_error_data(&body)["sequence_number"], 0);
    assert!(!body.contains("response.completed"));
    host.stop("responses-passthrough-duplicate-event")
        .await
        .expect("stop");
    upstream_task.abort();
}

#[tokio::test]
async fn responses_passthrough_event_whitespace_emits_error_frame() {
    let (upstream_port, upstream_task) = grok_responses_sse_upstream(vec![
        b"event: response.completed \ndata: {\"type\":\"response.completed\",\"sequence_number\":7,\"response\":{\"id\":\"resp_stream\"}}\n\n",
    ])
    .await;
    let host = BridgeRuntimeHost::new();
    let status = host
        .start(grok_codex_spec(
            "responses-passthrough-event-whitespace",
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

    assert_eq!(responses_error_data(&body)["sequence_number"], 0);
    assert!(!body.contains("response.completed"));
    host.stop("responses-passthrough-event-whitespace")
        .await
        .expect("stop");
    upstream_task.abort();
}

#[tokio::test]
async fn responses_passthrough_terminal_event_closes_before_upstream_eof() {
    let (upstream_port, upstream_task) = sse_upstream_with_delayed_tail(
        b"event: response.completed\ndata: {\"type\":\"response.completed\",\"sequence_number\":7,\"response\":{\"id\":\"resp_stream\"}}\n\n",
        Duration::from_millis(500),
        b"event: response.in_progress\ndata: {\"type\":\"response.in_progress\",\"sequence_number\":8}\n\n",
    )
    .await;
    let host = BridgeRuntimeHost::new();
    let status = host
        .start(grok_codex_spec(
            "responses-passthrough-terminal-close",
            0,
            upstream_port,
        ))
        .await
        .expect("start");
    let response = client()
        .await
        .post(format!("http://127.0.0.1:{}/v1/responses", status.port))
        .header(header::AUTHORIZATION, "Bearer local-test-token")
        .json(&json!({"model":"grok-4.5","input":"ping","stream":true}))
        .send()
        .await
        .expect("stream request");
    let body = tokio::time::timeout(Duration::from_millis(250), response.text())
        .await
        .expect("terminal stream closes")
        .expect("stream body");

    assert!(body.contains("response.completed"));
    assert!(!body.contains("response.in_progress"));
    assert!(!body.contains("event: error"));
    host.stop("responses-passthrough-terminal-close")
        .await
        .expect("stop");
    upstream_task.abort();
}

#[tokio::test]
async fn responses_passthrough_non_monotonic_sequence_emits_error_frame() {
    let (upstream_port, upstream_task) = grok_responses_sse_upstream(vec![
        b"event: response.created\ndata: {\"type\":\"response.created\",\"sequence_number\":7,\"response\":{\"id\":\"resp_stream\"}}\n\n",
        b"event: response.in_progress\ndata: {\"type\":\"response.in_progress\",\"sequence_number\":6}\n\n",
    ])
    .await;
    let host = BridgeRuntimeHost::new();
    let status = host
        .start(grok_codex_spec(
            "responses-passthrough-non-monotonic-sequence",
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

    assert_eq!(responses_error_data(&body)["sequence_number"], 8);
    assert!(body.contains("\"sequence_number\":7"));
    assert!(!body.contains("\"sequence_number\":6"));
    host.stop("responses-passthrough-non-monotonic-sequence")
        .await
        .expect("stop");
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

fn pool_member(id: &str, token: &str) -> BridgeMemberSpec {
    BridgeMemberSpec {
        ticket_id: format!("account:{id}"),
        source_kind: "account".into(),
        source_id: id.into(),
        label: id.into(),
        auth: ResolvedAuth::bearer(token),
        reload: None,
        health: MemberHealth::Renewable,
        priority: 0,
        position: 0,
    }
}

async fn grok_account_gated_upstream(
    reject_a: Arc<std::sync::atomic::AtomicBool>,
) -> (u16, Arc<Mutex<Vec<String>>>, tokio::task::JoinHandle<()>) {
    async fn responses(
        State((reject_a, captured)): State<(
            Arc<std::sync::atomic::AtomicBool>,
            Arc<Mutex<Vec<String>>>,
        )>,
        headers: axum::http::HeaderMap,
    ) -> Response {
        let bearer = headers
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_owned();
        captured.lock().expect("lock").push(bearer.clone());
        if bearer == "Bearer token-a" && reject_a.load(Ordering::SeqCst) {
            return (StatusCode::UNAUTHORIZED, "expired-a").into_response();
        }
        if bearer == "Bearer token-a" || bearer == "Bearer token-b" {
            return Json(grok_completed_response("hello")).into_response();
        }
        (StatusCode::UNAUTHORIZED, "unknown").into_response()
    }
    let captured = Arc::new(Mutex::new(Vec::new()));
    let listener =
        tokio::net::TcpListener::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
            .await
            .expect("bind gated upstream");
    let port = listener.local_addr().expect("addr").port();
    let state = (reject_a, captured.clone());
    let task = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new()
                .route("/v1/responses", post(responses))
                .with_state(state),
        )
        .await
        .expect("serve gated upstream");
    });
    (port, captured, task)
}

fn two_member_grok_spec(profile_id: &str, upstream_port: u16) -> BridgeStartSpec {
    grok_claude_spec(profile_id, 0, upstream_port)
        .with_members(vec![
            pool_member("acc-a", "token-a"),
            pool_member("acc-b", "token-b"),
        ])
        .with_multi_account(true)
}

async fn post_claude_hello(port: u16) -> reqwest::Response {
    client()
        .await
        .post(format!("http://127.0.0.1:{port}/v1/messages"))
        .header("x-api-key", "local-test-token")
        .json(&json!({
            "model": "claude-test",
            "max_tokens": 32,
            "messages": [{ "role": "user", "content": "hello" }]
        }))
        .send()
        .await
        .expect("messages request")
}

#[tokio::test]
async fn multi_account_isolates_a_then_b_serves_and_a_returns_after_restore() {
    let reject_a = Arc::new(AtomicBool::new(true));
    let (upstream_port, captured, upstream_task) =
        grok_account_gated_upstream(reject_a.clone()).await;
    let host = BridgeRuntimeHost::new();
    let status = host
        .start(two_member_grok_spec("pool-failover", upstream_port))
        .await
        .expect("start");

    let first = post_claude_hello(status.port).await;
    assert_eq!(first.status(), StatusCode::OK);
    let body: Value = first.json().await.expect("anthropic");
    assert_eq!(body["content"][0]["text"], "hello");
    assert_eq!(
        captured.lock().expect("lock").clone(),
        vec!["Bearer token-a".to_owned(), "Bearer token-b".to_owned()]
    );

    captured.lock().expect("lock").clear();
    let second = post_claude_hello(status.port).await;
    assert_eq!(second.status(), StatusCode::OK);
    assert_eq!(
        captured.lock().expect("lock").clone(),
        vec!["Bearer token-b".to_owned()]
    );

    reject_a.store(false, Ordering::SeqCst);
    host.restore_member_health("pool-failover", "acc-a", MemberHealth::Renewable)
        .expect("restore A");
    captured.lock().expect("lock").clear();
    let third = post_claude_hello(status.port).await;
    assert_eq!(third.status(), StatusCode::OK);
    assert_eq!(
        captured.lock().expect("lock").clone(),
        vec!["Bearer token-a".to_owned()]
    );

    host.stop("pool-failover").await.expect("stop");
    upstream_task.abort();
}

#[tokio::test]
async fn single_member_spec_keeps_legacy_oauth_reload_cell() {
    let (upstream_port, captured, upstream_task) =
        grok_401_then_ok_upstream("rotated-upstream-token").await;
    let reload: UpstreamAuthReload = Arc::new(|| Some("rotated-upstream-token".into()));
    let host = BridgeRuntimeHost::new();
    let status = host
        .start(
            grok_claude_spec("single-member-reload", 0, upstream_port)
                .with_reload_upstream_auth(Some(reload)),
        )
        .await
        .expect("start");
    let response = post_claude_hello(status.port).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        captured.lock().expect("lock").clone(),
        vec![
            "Bearer upstream-test-token".to_owned(),
            "Bearer rotated-upstream-token".to_owned()
        ]
    );
    host.stop("single-member-reload").await.expect("stop");
    upstream_task.abort();
}

#[tokio::test]
async fn set_gateway_port_zero_is_rejected() {
    let host = BridgeRuntimeHost::new();
    assert!(matches!(
        host.set_gateway_port(0).await,
        Err(BridgeHostError::InvalidGatewayPort)
    ));
}

#[tokio::test]
async fn primary_port_restart_does_not_drift() {
    let (upstream_port, upstream_task) = upstream().await;
    let host = BridgeRuntimeHost::new();
    let first = host
        .start(spec_with_token("restart-a", 0, upstream_port, TOKEN_A))
        .await
        .expect("start A");
    let primary = first.port;
    host.start(spec_with_token("restart-b", 0, upstream_port, TOKEN_B))
        .await
        .expect("start B on primary");
    host.stop("restart-a").await.expect("stop A");
    let restored = host
        .start(spec_with_token("restart-a", 0, upstream_port, TOKEN_A))
        .await
        .expect("restart A onto live primary");
    assert_eq!(restored.port, primary);
    assert_eq!(host.gateway_port().expect("gateway port"), Some(primary));
    host.stop("restart-a").await.expect("stop A again");
    host.stop("restart-b").await.expect("stop B");
    let rebound = host
        .start(spec_with_token(
            "restart-a",
            primary,
            upstream_port,
            TOKEN_A,
        ))
        .await
        .expect("rebind stored primary");
    assert_eq!(rebound.port, primary);
    host.shutdown().await.expect("shutdown");
    upstream_task.abort();
}

#[tokio::test]
async fn set_gateway_port_occupancy_conflict_leaves_existing_listener() {
    let (upstream_port, upstream_task) = upstream().await;
    let host = BridgeRuntimeHost::new();
    let first = host
        .start(spec_with_token("occupy-a", 0, upstream_port, TOKEN_A))
        .await
        .expect("start");
    let original = first.port;
    let blocker = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("blocker");
    let busy = blocker.local_addr().expect("blocker addr").port();
    assert!(matches!(
        host.set_gateway_port(busy).await,
        Err(BridgeHostError::Bind(_))
    ));
    assert_eq!(host.gateway_port().expect("gateway port"), Some(original));
    let health = client()
        .await
        .get(format!("http://127.0.0.1:{original}/health"))
        .header(header::AUTHORIZATION, format!("Bearer {TOKEN_A}"))
        .send()
        .await
        .expect("health on original port");
    assert_eq!(health.status(), StatusCode::OK);
    drop(blocker);
    host.shutdown().await.expect("shutdown");
    upstream_task.abort();
}

#[tokio::test]
async fn set_gateway_port_moves_primary_and_keeps_explicit_alias() {
    let (upstream_port, upstream_task) = grok_responses_upstream().await;
    let host = BridgeRuntimeHost::new();
    let _primary = host
        .start(grok_claude_with_token(
            "alias-keep-primary",
            0,
            upstream_port,
            TOKEN_A,
        ))
        .await
        .expect("start primary");
    let alias_port = free_loopback_port().await;
    let aliased = host
        .start(grok_codex_with_token(
            "alias-keep-historical",
            alias_port,
            upstream_port,
            TOKEN_B,
        ))
        .await
        .expect("bind historical alias");
    assert_eq!(aliased.port, alias_port);
    let next = free_loopback_port().await;
    let moved = host
        .set_gateway_port(next)
        .await
        .expect("move unified port");
    assert_eq!(moved, next);
    assert_eq!(host.gateway_port().expect("gateway port"), Some(next));
    assert_eq!(
        host.status("alias-keep-primary")
            .expect("status")
            .expect("primary edge")
            .port,
        next
    );
    assert_eq!(
        host.status("alias-keep-historical")
            .expect("status")
            .expect("alias edge")
            .port,
        alias_port
    );
    let http = client().await;
    let via_new = http
        .post(format!("http://127.0.0.1:{next}/v1/messages"))
        .header("x-api-key", TOKEN_A)
        .json(&json!({
            "model": "claude-test",
            "max_tokens": 32,
            "messages": [{ "role": "user", "content": "hello" }]
        }))
        .send()
        .await
        .expect("A via new primary");
    assert_eq!(via_new.status(), StatusCode::OK);
    let via_alias = http
        .post(format!("http://127.0.0.1:{alias_port}/v1/responses"))
        .header(header::AUTHORIZATION, format!("Bearer {TOKEN_B}"))
        .json(&json!({"model": "grok-4.5", "input": "hello"}))
        .send()
        .await
        .expect("B via preserved alias");
    assert_eq!(via_alias.status(), StatusCode::OK);
    host.shutdown().await.expect("shutdown");
    upstream_task.abort();
}

async fn capturing_chat_upstream() -> (u16, Arc<Mutex<Vec<String>>>, tokio::task::JoinHandle<()>) {
    async fn chat(
        State(captured): State<Arc<Mutex<Vec<String>>>>,
        headers: axum::http::HeaderMap,
    ) -> Json<Value> {
        let bearer = headers
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_owned();
        captured.lock().expect("lock").push(bearer);
        Json(json!({
            "id": "chat-test",
            "model": "exclusive",
            "created": 1,
            "choices": [{ "message": { "role": "assistant", "content": "hello" }, "finish_reason": "stop" }],
            "usage": { "prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2 }
        }))
    }
    let captured = Arc::new(Mutex::new(Vec::new()));
    let listener =
        tokio::net::TcpListener::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
            .await
            .expect("bind exclusive upstream");
    let port = listener.local_addr().expect("addr").port();
    let state = captured.clone();
    let task = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new()
                .route("/chat/completions", post(chat))
                .with_state(state),
        )
        .await
        .expect("serve exclusive upstream");
    });
    (port, captured, task)
}

fn exclusive_spec(
    profile_id: &str,
    local_token: &str,
    upstream_port: u16,
    listed: &str,
    upstream_token: &str,
) -> BridgeStartSpec {
    let mut spec = spec_with_token(profile_id, 0, upstream_port, local_token)
        .with_listed_models(vec![listed.into()])
        .with_mapping(AdapterSourceProduct::OpenaiApi, AgentId::Codex, false);
    spec.upstream.auth = ResolvedAuth::bearer(upstream_token);
    spec.upstream.model = Some(listed.into());
    spec
}

#[tokio::test]
async fn exclusive_model_m1_does_not_select_peer_b() {
    let (upstream_a, captured_a, task_a) = capturing_chat_upstream().await;
    let (upstream_b, captured_b, task_b) = capturing_chat_upstream().await;
    let host = BridgeRuntimeHost::new();
    let first = host
        .start(exclusive_spec(
            "exclusive-a",
            TOKEN_A,
            upstream_a,
            "m1",
            "upstream-a",
        ))
        .await
        .expect("start A");
    host.start({
        let mut spec = exclusive_spec("exclusive-b", TOKEN_B, upstream_b, "m2", "upstream-b");
        spec.port = first.port;
        spec
    })
    .await
    .expect("start B");
    let response = client()
        .await
        .post(format!("http://127.0.0.1:{}/v1/responses", first.port))
        .header(header::AUTHORIZATION, format!("Bearer {TOKEN_A}"))
        .json(&json!({"model": "m1", "input": "hello"}))
        .send()
        .await
        .expect("m1 on A");
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        captured_a.lock().expect("lock A").clone(),
        vec!["Bearer upstream-a".to_owned()]
    );
    assert!(
        captured_b.lock().expect("lock B").is_empty(),
        "m1 must not select B"
    );
    host.shutdown().await.expect("shutdown");
    task_a.abort();
    task_b.abort();
}

#[tokio::test]
async fn pool_tokens_do_not_cross_on_models_catalog() {
    let (upstream_port, upstream_task) = upstream().await;
    let host = BridgeRuntimeHost::new();
    let first = host
        .start(
            spec_with_token("pool-models-a", 0, upstream_port, TOKEN_A)
                .with_listed_models(vec!["m1".into()]),
        )
        .await
        .expect("start A");
    host.start(
        spec_with_token("pool-models-b", first.port, upstream_port, TOKEN_B)
            .with_listed_models(vec!["m2".into()]),
    )
    .await
    .expect("start B");
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
    assert_eq!(listed_a["data"][0]["id"], "m1");
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
    assert_eq!(listed_b["data"][0]["id"], "m2");
    assert_eq!(listed_b["data"].as_array().map(Vec::len), Some(1));
    host.shutdown().await.expect("shutdown");
    upstream_task.abort();
}

fn v2_index(route_id: &str, member_id: &str, model: &str) -> EffectiveRouteIndex {
    EffectiveRouteIndex::build(
        route_id,
        1,
        &[MemberCapabilitySnapshot {
            member_id: member_id.into(),
            public_model: model.into(),
            endpoint: "responses".into(),
            upstream_provider: "openai".into(),
            upstream_dialect: "generic".into(),
            upstream_model: model.into(),
            upstream_endpoint: "http://127.0.0.1/v1".into(),
            transport_key: "openai:generic".into(),
            capability: MemberCapability::Supported,
        }],
    )
}

#[tokio::test]
async fn v2_index_unknown_model_fails_closed_without_peer_switch() {
    let (upstream_a, captured_a, task_a) = capturing_chat_upstream().await;
    let (upstream_b, captured_b, task_b) = capturing_chat_upstream().await;
    let host = BridgeRuntimeHost::new();
    let first = host
        .start(
            exclusive_spec("v2-a", TOKEN_A, upstream_a, "m1", "upstream-a")
                .with_route_index(v2_index("v2-a", "v2-a", "m1")),
        )
        .await
        .expect("start A");
    host.start({
        let mut spec = exclusive_spec("v2-b", TOKEN_B, upstream_b, "m2", "upstream-b");
        spec.port = first.port;
        spec
    })
    .await
    .expect("start B");
    let response = client()
        .await
        .post(format!("http://127.0.0.1:{}/v1/responses", first.port))
        .header(header::AUTHORIZATION, format!("Bearer {TOKEN_A}"))
        .json(&json!({"model": "m2", "input": "hello"}))
        .send()
        .await
        .expect("m2 on v2 A");
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body: Value = response.json().await.expect("error json");
    assert_eq!(body["error"]["code"], "model_unavailable");
    assert!(captured_a.lock().expect("lock A").is_empty());
    assert!(
        captured_b.lock().expect("lock B").is_empty(),
        "v2 must not scan other profiles for a model switch"
    );
    host.shutdown().await.expect("shutdown");
    task_a.abort();
    task_b.abort();
}

#[tokio::test]
async fn v2_models_catalog_comes_from_the_same_index() {
    let (upstream_port, upstream_task) = upstream().await;
    let host = BridgeRuntimeHost::new();
    let first = host
        .start(
            spec_with_token("v2-models-a", 0, upstream_port, TOKEN_A)
                .with_listed_models(vec!["stale-lead-only".into()])
                .with_route_index(v2_index("v2-models-a", "v2-models-a", "m1")),
        )
        .await
        .expect("start A");
    host.start(
        spec_with_token("v2-models-b", first.port, upstream_port, TOKEN_B)
            .with_listed_models(vec!["m2".into()]),
    )
    .await
    .expect("start B");
    let listed_a = client()
        .await
        .get(format!("http://127.0.0.1:{}/v1/models", first.port))
        .header(header::AUTHORIZATION, format!("Bearer {TOKEN_A}"))
        .send()
        .await
        .expect("models A")
        .json::<Value>()
        .await
        .expect("models A json");
    assert_eq!(listed_a["data"].as_array().map(Vec::len), Some(1));
    assert_eq!(listed_a["data"][0]["id"], "m1");
    host.shutdown().await.expect("shutdown");
    upstream_task.abort();
}

fn production_index(members: &[(&str, &str)]) -> EffectiveRouteIndex {
    let listings: Vec<MemberListing> = members
        .iter()
        .map(|(member_id, model)| MemberListing {
            member_id: (*member_id).into(),
            listed_models: vec![(*model).into()],
            upstream_provider: "openai".into(),
            upstream_dialect: "generic".into(),
            upstream_endpoint: "http://127.0.0.1/v1".into(),
            transport_key: "openai:generic".into(),
            snapshot_ok: true,
        })
        .collect();
    index_from_member_listings("pool-v2", 1, "responses", &listings, None)
}

#[tokio::test]
async fn production_built_index_unions_models_and_never_crosses_members() {
    let (upstream_port, captured, task) = capturing_chat_upstream().await;
    let index = production_index(&[("acc-b", "m2"), ("acc-a", "m1")]);
    assert_eq!(index.list_models("responses"), vec!["m1", "m2"]);
    let host = BridgeRuntimeHost::new();
    let mut spec = spec_with_token("pool-v2", 0, upstream_port, TOKEN_A)
        .with_members(vec![
            pool_member("acc-b", "upstream-b"),
            pool_member("acc-a", "upstream-a"),
        ])
        .with_listed_models(index.list_models("responses"))
        .with_route_index(index);
    spec.upstream.model = None;
    spec.upstream.auth = ResolvedAuth::bearer("lead-unused");
    let status = host.start(spec).await.expect("start enrolled pool");
    let http = client().await;
    let listed = http
        .get(format!("http://127.0.0.1:{}/v1/models", status.port))
        .header(header::AUTHORIZATION, format!("Bearer {TOKEN_A}"))
        .send()
        .await
        .expect("models")
        .json::<Value>()
        .await
        .expect("models json");
    let ids: Vec<&str> = listed["data"]
        .as_array()
        .expect("data")
        .iter()
        .filter_map(|row| row["id"].as_str())
        .collect();
    assert_eq!(ids, vec!["m1", "m2"]);

    let m1 = http
        .post(format!("http://127.0.0.1:{}/v1/responses", status.port))
        .header(header::AUTHORIZATION, format!("Bearer {TOKEN_A}"))
        .json(&json!({"model": "m1", "input": "hello"}))
        .send()
        .await
        .expect("m1");
    assert_eq!(m1.status(), StatusCode::OK);
    assert_eq!(
        captured.lock().expect("lock").clone(),
        vec!["Bearer upstream-a".to_owned()],
        "m1 must not pick B even when B is first in the picker"
    );

    captured.lock().expect("lock").clear();
    let unknown = http
        .post(format!("http://127.0.0.1:{}/v1/responses", status.port))
        .header(header::AUTHORIZATION, format!("Bearer {TOKEN_A}"))
        .json(&json!({"model": "unknown", "input": "hello"}))
        .send()
        .await
        .expect("unknown");
    assert_eq!(unknown.status(), StatusCode::BAD_REQUEST);
    assert!(captured.lock().expect("lock").is_empty());

    captured.lock().expect("lock").clear();
    let m2 = http
        .post(format!("http://127.0.0.1:{}/v1/responses", status.port))
        .header(header::AUTHORIZATION, format!("Bearer {TOKEN_A}"))
        .json(&json!({"model": "m2", "input": "hello"}))
        .send()
        .await
        .expect("m2");
    assert_eq!(m2.status(), StatusCode::OK);
    assert_eq!(
        captured.lock().expect("lock").clone(),
        vec!["Bearer upstream-b".to_owned()]
    );

    host.shutdown().await.expect("shutdown");
    task.abort();
}

#[tokio::test]
async fn live_start_does_not_reuse_index_when_members_remap_same_models() {
    let host = BridgeRuntimeHost::new();
    let first = production_index(&[("acc-a", "m1"), ("acc-b", "m2")]);
    let remapped = production_index(&[("acc-a", "m2"), ("acc-b", "m1")]);
    assert_eq!(first.generation, remapped.generation);
    assert_eq!(
        first.list_models("responses"),
        remapped.list_models("responses")
    );
    let members = vec![
        pool_member("acc-a", "upstream-a"),
        pool_member("acc-b", "upstream-b"),
    ];
    let listed = first.list_models("responses");
    let spec = spec_with_token("pool-remap", 0, 9, TOKEN_A)
        .with_members(members.clone())
        .with_listed_models(listed.clone())
        .with_route_index(first);
    host.start(spec).await.expect("first start");
    let reused = spec_with_token("pool-remap", 0, 9, TOKEN_A)
        .with_members(members.clone())
        .with_listed_models(listed.clone())
        .with_route_index(production_index(&[("acc-a", "m1"), ("acc-b", "m2")]));
    let status = host.start(reused).await.expect("identical index is reused");
    assert!(status.running);
    let next = spec_with_token("pool-remap", 0, 9, TOKEN_A)
        .with_members(members)
        .with_listed_models(listed)
        .with_route_index(remapped);
    let error = host
        .start(next)
        .await
        .expect_err("remapped members must not reuse the live edge");
    assert!(matches!(error, BridgeHostError::ConflictingStart));
    host.shutdown().await.expect("shutdown");
}

fn chat_ok_body() -> Value {
    json!({
        "id": "chat-test",
        "model": "m1",
        "created": 1,
        "choices": [{ "message": { "role": "assistant", "content": "hello" }, "finish_reason": "stop" }],
        "usage": { "prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2 }
    })
}

fn chat_ok() -> Response {
    Json(chat_ok_body()).into_response()
}

type ChatCallback = Arc<dyn Fn(String, Value) -> Response + Send + Sync>;

async fn callback_chat_upstream(
    callback: ChatCallback,
) -> (
    u16,
    Arc<Mutex<Vec<(String, Value)>>>,
    tokio::task::JoinHandle<()>,
) {
    async fn chat(
        State((callback, captured)): State<(ChatCallback, Arc<Mutex<Vec<(String, Value)>>>)>,
        headers: axum::http::HeaderMap,
        Json(body): Json<Value>,
    ) -> Response {
        let bearer = headers
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_owned();
        captured
            .lock()
            .expect("lock")
            .push((bearer.clone(), body.clone()));
        callback(bearer, body)
    }
    let captured = Arc::new(Mutex::new(Vec::new()));
    let listener =
        tokio::net::TcpListener::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
            .await
            .expect("bind scripted chat");
    let port = listener.local_addr().expect("addr").port();
    let state = (callback, captured.clone());
    let task = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new()
                .route("/chat/completions", post(chat))
                .with_state(state),
        )
        .await
        .expect("serve scripted chat");
    });
    (port, captured, task)
}

fn p5_listing(member_id: &str, models: &[&str]) -> MemberListing {
    MemberListing {
        member_id: member_id.into(),
        listed_models: models.iter().map(|model| (*model).to_string()).collect(),
        upstream_provider: "openai".into(),
        upstream_dialect: "generic".into(),
        upstream_endpoint: "http://127.0.0.1/v1".into(),
        transport_key: "openai:generic".into(),
        snapshot_ok: true,
    }
}

fn p5_index(members: &[(&str, &[&str])]) -> EffectiveRouteIndex {
    let listings: Vec<MemberListing> = members
        .iter()
        .map(|(id, models)| p5_listing(id, models))
        .collect();
    index_from_member_listings("pool-v2-p5", 1, "responses", &listings, None)
}

fn p5_pool_spec(
    profile_id: &str,
    upstream_port: u16,
    members: Vec<BridgeMemberSpec>,
    index: EffectiveRouteIndex,
) -> BridgeStartSpec {
    spec_with_token(profile_id, 0, upstream_port, TOKEN_A)
        .with_members(members)
        .with_listed_models(index.list_models("responses"))
        .with_route_index(index)
}

fn captured_tokens(captured: &Mutex<Vec<(String, Value)>>) -> Vec<String> {
    captured
        .lock()
        .expect("lock")
        .iter()
        .map(|(bearer, _)| bearer.clone())
        .collect()
}

async fn post_p5_model(port: u16, model: &str, stream: bool) -> reqwest::Response {
    client()
        .await
        .post(format!("http://127.0.0.1:{port}/v1/responses"))
        .header(header::AUTHORIZATION, format!("Bearer {TOKEN_A}"))
        .json(&json!({"model": model, "input": "hello", "stream": stream}))
        .send()
        .await
        .expect("p5 request")
}

#[tokio::test]
async fn v2_401_after_failed_reload_failovers_to_second_member() {
    let callback: ChatCallback = Arc::new(|bearer, _body| {
        if bearer == "Bearer token-a" {
            StatusCode::UNAUTHORIZED.into_response()
        } else {
            chat_ok()
        }
    });
    let (upstream_port, captured, task) = callback_chat_upstream(callback).await;
    let host = BridgeRuntimeHost::new();
    let index = p5_index(&[("acc-a", &["m1"][..]), ("acc-b", &["m1"][..])]);
    let status = host
        .start(p5_pool_spec(
            "p5-401-failover",
            upstream_port,
            vec![
                pool_member("acc-a", "token-a"),
                pool_member("acc-b", "token-b"),
            ],
            index,
        ))
        .await
        .expect("start");
    let response = post_p5_model(status.port, "m1", false).await;
    assert_eq!(response.status(), StatusCode::OK);
    let bodies: Vec<Value> = captured
        .lock()
        .expect("lock")
        .iter()
        .map(|(_, body)| body.clone())
        .collect();
    assert_eq!(
        captured_tokens(&captured),
        vec!["Bearer token-a".to_owned(), "Bearer token-b".to_owned()]
    );
    assert_eq!(bodies.len(), 2);
    assert_eq!(bodies[0], bodies[1], "original body must be re-prepared");
    captured.lock().expect("lock").clear();
    let second = post_p5_model(status.port, "m1", false).await;
    assert_eq!(second.status(), StatusCode::OK);
    assert_eq!(
        captured_tokens(&captured),
        vec!["Bearer token-b".to_owned()],
        "A stays isolated after failed 401 reload"
    );
    host.shutdown().await.expect("shutdown");
    task.abort();
}

#[tokio::test]
async fn v2_stop_and_start_with_new_token_clears_host_isolation() {
    let callback: ChatCallback = Arc::new(|bearer, _body| {
        if bearer == "Bearer token-a" {
            StatusCode::UNAUTHORIZED.into_response()
        } else {
            chat_ok()
        }
    });
    let (upstream_port, captured, task) = callback_chat_upstream(callback).await;
    let host = BridgeRuntimeHost::new();
    let index = p5_index(&[("acc-a", &["m1"][..]), ("acc-b", &["m1"][..])]);
    let status = host
        .start(p5_pool_spec(
            "p5-401-restart-clears",
            upstream_port,
            vec![
                pool_member("acc-a", "token-a"),
                pool_member("acc-b", "token-b"),
            ],
            index.clone(),
        ))
        .await
        .expect("start");
    let first = post_p5_model(status.port, "m1", false).await;
    assert_eq!(first.status(), StatusCode::OK);
    assert_eq!(
        captured_tokens(&captured),
        vec!["Bearer token-a".to_owned(), "Bearer token-b".to_owned()]
    );
    host.stop("p5-401-restart-clears").await.expect("stop");
    captured.lock().expect("lock").clear();
    let status = host
        .start(p5_pool_spec(
            "p5-401-restart-clears",
            upstream_port,
            vec![
                pool_member("acc-a", "token-a2"),
                pool_member("acc-b", "token-b"),
            ],
            index,
        ))
        .await
        .expect("restart");
    let second = post_p5_model(status.port, "m1", false).await;
    assert_eq!(second.status(), StatusCode::OK);
    assert_eq!(
        captured_tokens(&captured),
        vec!["Bearer token-a2".to_owned()],
        "stop+start with a new login must re-admit A"
    );
    host.shutdown().await.expect("shutdown");
    task.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn v2_concurrent_401_reload_is_singleflight() {
    let hits = Arc::new(AtomicUsize::new(0));
    let hits_cb = hits.clone();
    let reload: UpstreamAuthReload = Arc::new(move || {
        hits_cb.fetch_add(1, Ordering::SeqCst);
        std::thread::sleep(Duration::from_millis(80));
        Some("rotated-token".into())
    });
    let mut member = pool_member("acc-a", "old-token");
    member.reload = Some(reload);
    let callback: ChatCallback = Arc::new(|bearer, _body| {
        if bearer == "Bearer rotated-token" {
            chat_ok()
        } else {
            StatusCode::UNAUTHORIZED.into_response()
        }
    });
    let (upstream_port, captured, task) = callback_chat_upstream(callback).await;
    let host = BridgeRuntimeHost::new();
    let index = p5_index(&[("acc-a", &["m1"][..])]);
    let status = host
        .start(p5_pool_spec(
            "p5-401-singleflight",
            upstream_port,
            vec![member],
            index,
        ))
        .await
        .expect("start");
    let port = status.port;
    let (left, right) = tokio::join!(
        post_p5_model(port, "m1", false),
        post_p5_model(port, "m1", false)
    );
    assert_eq!(left.status(), StatusCode::OK);
    assert_eq!(right.status(), StatusCode::OK);
    assert_eq!(hits.load(Ordering::SeqCst), 1);
    let tokens = captured_tokens(&captured);
    assert!(tokens.iter().any(|token| token == "Bearer old-token"));
    assert!(tokens.iter().any(|token| token == "Bearer rotated-token"));
    host.shutdown().await.expect("shutdown");
    task.abort();
}

#[tokio::test]
async fn v2_400_does_not_switch_members() {
    let callback: ChatCallback = Arc::new(|bearer, _body| {
        if bearer == "Bearer token-a" {
            (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": {"message": "bad schema"}})),
            )
                .into_response()
        } else {
            chat_ok()
        }
    });
    let (upstream_port, captured, task) = callback_chat_upstream(callback).await;
    let host = BridgeRuntimeHost::new();
    let status = host
        .start(p5_pool_spec(
            "p5-400-no-switch",
            upstream_port,
            vec![
                pool_member("acc-a", "token-a"),
                pool_member("acc-b", "token-b"),
            ],
            p5_index(&[("acc-a", &["m1"][..]), ("acc-b", &["m1"][..])]),
        ))
        .await
        .expect("start");
    let response = post_p5_model(status.port, "m1", false).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body: Value = response.json().await.expect("json");
    assert_eq!(body["error"]["code"], "invalid_request");
    assert_eq!(
        captured_tokens(&captured),
        vec!["Bearer token-a".to_owned()]
    );
    host.shutdown().await.expect("shutdown");
    task.abort();
}

#[tokio::test]
async fn v2_policy_403_does_not_switch_members() {
    let callback: ChatCallback = Arc::new(|bearer, _body| {
        if bearer == "Bearer token-a" {
            (
                StatusCode::FORBIDDEN,
                Json(json!({
                    "error": {
                        "message": "you do not have access to generate this content"
                    }
                })),
            )
                .into_response()
        } else {
            chat_ok()
        }
    });
    let (upstream_port, captured, task) = callback_chat_upstream(callback).await;
    let host = BridgeRuntimeHost::new();
    let status = host
        .start(p5_pool_spec(
            "p5-403-policy-no-switch",
            upstream_port,
            vec![
                pool_member("acc-a", "token-a"),
                pool_member("acc-b", "token-b"),
            ],
            p5_index(&[("acc-a", &["m1"][..]), ("acc-b", &["m1"][..])]),
        ))
        .await
        .expect("start");
    let response = post_p5_model(status.port, "m1", false).await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        captured_tokens(&captured),
        vec!["Bearer token-a".to_owned()],
        "policy 403 must not switch members"
    );
    host.shutdown().await.expect("shutdown");
    task.abort();
}

#[tokio::test]
async fn v2_entitlement_excludes_member_for_model_not_account() {
    let callback: ChatCallback = Arc::new(|bearer, body| {
        let model = body.get("model").and_then(Value::as_str).unwrap_or("");
        if bearer == "Bearer token-a" && model == "m1" {
            (
                StatusCode::NOT_FOUND,
                Json(json!({
                    "error": {
                        "code": "model_not_found",
                        "message": "The model does not exist or you do not have access to it."
                    }
                })),
            )
                .into_response()
        } else if bearer == "Bearer token-a" || bearer == "Bearer token-b" {
            chat_ok()
        } else {
            StatusCode::UNAUTHORIZED.into_response()
        }
    });
    let (upstream_port, captured, task) = callback_chat_upstream(callback).await;
    let host = BridgeRuntimeHost::new();
    let status = host
        .start(p5_pool_spec(
            "p5-entitlement",
            upstream_port,
            vec![
                pool_member("acc-a", "token-a"),
                pool_member("acc-b", "token-b"),
            ],
            p5_index(&[("acc-a", &["m1", "m2"][..]), ("acc-b", &["m1"][..])]),
        ))
        .await
        .expect("start");
    let first = post_p5_model(status.port, "m1", false).await;
    assert_eq!(first.status(), StatusCode::OK);
    assert_eq!(
        captured_tokens(&captured),
        vec!["Bearer token-a".to_owned(), "Bearer token-b".to_owned()]
    );
    captured.lock().expect("lock").clear();
    let second = post_p5_model(status.port, "m2", false).await;
    assert_eq!(second.status(), StatusCode::OK);
    assert_eq!(
        captured_tokens(&captured),
        vec!["Bearer token-a".to_owned()],
        "A must still serve other models and must not be NeedsLogin"
    );
    host.shutdown().await.expect("shutdown");
    task.abort();
}

#[tokio::test]
async fn v2_entitlement_403_matches_404_scope() {
    let callback: ChatCallback = Arc::new(|bearer, _body| {
        if bearer == "Bearer token-a" {
            (
                StatusCode::FORBIDDEN,
                Json(json!({
                    "error": {
                        "code": "model_not_found",
                        "message": "The model does not exist or you do not have access to it."
                    }
                })),
            )
                .into_response()
        } else {
            chat_ok()
        }
    });
    let (upstream_port, captured, task) = callback_chat_upstream(callback).await;
    let host = BridgeRuntimeHost::new();
    let status = host
        .start(p5_pool_spec(
            "p5-entitlement-403",
            upstream_port,
            vec![
                pool_member("acc-a", "token-a"),
                pool_member("acc-b", "token-b"),
            ],
            p5_index(&[("acc-a", &["m1"][..]), ("acc-b", &["m1"][..])]),
        ))
        .await
        .expect("start");
    let response = post_p5_model(status.port, "m1", false).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        captured_tokens(&captured),
        vec!["Bearer token-a".to_owned(), "Bearer token-b".to_owned()]
    );
    captured.lock().expect("lock").clear();
    let again = post_p5_model(status.port, "m1", false).await;
    assert_eq!(again.status(), StatusCode::OK);
    assert_eq!(
        captured_tokens(&captured),
        vec!["Bearer token-a".to_owned(), "Bearer token-b".to_owned()],
        "entitlement is this-request only; A is not isolated"
    );
    host.shutdown().await.expect("shutdown");
    task.abort();
}

#[tokio::test]
async fn v2_429_cools_member_and_keeps_models_catalog() {
    let callback: ChatCallback = Arc::new(|bearer, _body| {
        if bearer == "Bearer token-a" {
            (
                StatusCode::TOO_MANY_REQUESTS,
                [(header::RETRY_AFTER, "1")],
                Json(json!({"error": {"message": "rate limited"}})),
            )
                .into_response()
        } else {
            chat_ok()
        }
    });
    let (upstream_port, captured, task) = callback_chat_upstream(callback).await;
    let host = BridgeRuntimeHost::new();
    let status = host
        .start(p5_pool_spec(
            "p5-429-cooldown",
            upstream_port,
            vec![
                pool_member("acc-a", "token-a"),
                pool_member("acc-b", "token-b"),
            ],
            p5_index(&[("acc-a", &["m1"][..]), ("acc-b", &["m1"][..])]),
        ))
        .await
        .expect("start");
    let first = post_p5_model(status.port, "m1", false).await;
    assert_eq!(first.status(), StatusCode::OK);
    assert_eq!(
        captured_tokens(&captured),
        vec!["Bearer token-a".to_owned(), "Bearer token-b".to_owned()]
    );
    captured.lock().expect("lock").clear();
    let second = post_p5_model(status.port, "m1", false).await;
    assert_eq!(second.status(), StatusCode::OK);
    assert_eq!(
        captured_tokens(&captured),
        vec!["Bearer token-b".to_owned()],
        "A stays in cooldown"
    );
    let listed = client()
        .await
        .get(format!("http://127.0.0.1:{}/v1/models", status.port))
        .header(header::AUTHORIZATION, format!("Bearer {TOKEN_A}"))
        .send()
        .await
        .expect("models")
        .json::<Value>()
        .await
        .expect("models json");
    let ids: Vec<&str> = listed["data"]
        .as_array()
        .expect("data")
        .iter()
        .filter_map(|row| row["id"].as_str())
        .collect();
    assert_eq!(ids, vec!["m1"], "cooldown is availability, not capability");
    host.shutdown().await.expect("shutdown");
    task.abort();
}

#[tokio::test]
async fn v2_5xx_failovers_before_downstream_commit() {
    let callback: ChatCallback = Arc::new(|bearer, _body| {
        if bearer == "Bearer token-a" {
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        } else {
            chat_ok()
        }
    });
    let (upstream_port, captured, task) = callback_chat_upstream(callback).await;
    let host = BridgeRuntimeHost::new();
    let status = host
        .start(p5_pool_spec(
            "p5-5xx-failover",
            upstream_port,
            vec![
                pool_member("acc-a", "token-a"),
                pool_member("acc-b", "token-b"),
            ],
            p5_index(&[("acc-a", &["m1"][..]), ("acc-b", &["m1"][..])]),
        ))
        .await
        .expect("start");
    let response = post_p5_model(status.port, "m1", false).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        captured_tokens(&captured),
        vec!["Bearer token-a".to_owned(), "Bearer token-b".to_owned()]
    );
    host.shutdown().await.expect("shutdown");
    task.abort();
}

#[tokio::test]
async fn v2_does_not_replay_after_first_sse_byte() {
    let callback: ChatCallback = Arc::new(|bearer, _body| {
        if bearer == "Bearer token-a" {
            (
                [(header::CONTENT_TYPE, "text/event-stream")],
                Body::from(
                    "data: {\"id\":\"chat-stream\",\"model\":\"m1\",\"choices\":[{\"delta\":{\"content\":\"hi\"}}]}\n\n",
                ),
            )
                .into_response()
        } else {
            chat_ok()
        }
    });
    let (upstream_port, captured, task) = callback_chat_upstream(callback).await;
    let host = BridgeRuntimeHost::new();
    let status = host
        .start(p5_pool_spec(
            "p5-sse-committed",
            upstream_port,
            vec![
                pool_member("acc-a", "token-a"),
                pool_member("acc-b", "token-b"),
            ],
            p5_index(&[("acc-a", &["m1"][..]), ("acc-b", &["m1"][..])]),
        ))
        .await
        .expect("start");
    let response = post_p5_model(status.port, "m1", true).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.text().await.expect("sse");
    assert!(body.contains("hi") || body.contains("response."));
    assert_eq!(
        captured_tokens(&captured),
        vec!["Bearer token-a".to_owned()],
        "must not replay onto B after the first downstream byte"
    );
    host.shutdown().await.expect("shutdown");
    task.abort();
}

#[tokio::test]
async fn v2_pool_exhausted_when_both_members_fail() {
    let callback: ChatCallback =
        Arc::new(|_bearer, _body| StatusCode::UNAUTHORIZED.into_response());
    let (upstream_port, _captured, task) = callback_chat_upstream(callback).await;
    let host = BridgeRuntimeHost::new();
    let status = host
        .start(p5_pool_spec(
            "p5-exhausted-401",
            upstream_port,
            vec![
                pool_member("acc-a", "token-a"),
                pool_member("acc-b", "token-b"),
            ],
            p5_index(&[("acc-a", &["m1"][..]), ("acc-b", &["m1"][..])]),
        ))
        .await
        .expect("start");
    let response = post_p5_model(status.port, "m1", false).await;
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let body: Value = response.json().await.expect("json");
    assert_eq!(body["error"]["code"], "pool_exhausted");
    host.shutdown().await.expect("shutdown");
    task.abort();
}

#[tokio::test]
async fn v2_pool_exhausted_includes_retry_after_from_cooldown() {
    let callback: ChatCallback = Arc::new(|_bearer, _body| {
        (
            StatusCode::TOO_MANY_REQUESTS,
            [(header::RETRY_AFTER, "1")],
            Json(json!({"error": {"message": "rate limited"}})),
        )
            .into_response()
    });
    let (upstream_port, _captured, task) = callback_chat_upstream(callback).await;
    let host = BridgeRuntimeHost::new();
    let status = host
        .start(p5_pool_spec(
            "p5-exhausted-429",
            upstream_port,
            vec![
                pool_member("acc-a", "token-a"),
                pool_member("acc-b", "token-b"),
            ],
            p5_index(&[("acc-a", &["m1"][..]), ("acc-b", &["m1"][..])]),
        ))
        .await
        .expect("start");
    let response = post_p5_model(status.port, "m1", false).await;
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let retry_after = response
        .headers()
        .get(header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let body: Value = response.json().await.expect("json");
    assert_eq!(body["error"]["code"], "pool_exhausted");
    assert_eq!(retry_after.as_deref(), Some("1"));
    host.shutdown().await.expect("shutdown");
    task.abort();
}

#[tokio::test]
async fn v1_without_index_does_not_enter_pool_exhausted() {
    let callback: ChatCallback = Arc::new(|bearer, _body| {
        if bearer == "Bearer token-a" {
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        } else {
            chat_ok()
        }
    });
    let (upstream_port, captured, task) = callback_chat_upstream(callback).await;
    let host = BridgeRuntimeHost::new();
    let status = host
        .start(
            spec_with_token("p5-v1-no-index", 0, upstream_port, TOKEN_A)
                .with_members(vec![
                    pool_member("acc-a", "token-a"),
                    pool_member("acc-b", "token-b"),
                ])
                .with_multi_account(true)
                .with_listed_models(vec!["m1".into()]),
        )
        .await
        .expect("start");
    let response = post_p5_model(status.port, "m1", false).await;
    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
    let body: Value = response.json().await.expect("json");
    assert_eq!(body["error"]["code"], "upstream_error");
    assert_ne!(body["error"]["code"], "pool_exhausted");
    assert_eq!(
        captured_tokens(&captured),
        vec!["Bearer token-a".to_owned()],
        "v1 must not failover on 5xx"
    );
    host.shutdown().await.expect("shutdown");
    task.abort();
}

fn grok_codex_pair_spec(profile_id: &str, port: u16, upstream_port: u16) -> BridgeStartSpec {
    grok_codex_spec(profile_id, port, upstream_port)
        .with_mapping(
            AdapterSourceProduct::XaiGrokSubscription,
            AgentId::Codex,
            false,
        )
        .with_pair_adapter_flags(true, false)
        .with_listed_models(vec!["grok-4.5".into()])
}

fn codex_grok_pair_spec(profile_id: &str, port: u16, upstream_port: u16) -> BridgeStartSpec {
    let mut spec = spec(profile_id, port, upstream_port);
    spec.upstream.base_url = format!("http://127.0.0.1:{upstream_port}/v1/");
    spec.upstream.model = Some("gpt-5.4".to_owned());
    spec.upstream.protocol = BridgeUpstreamProtocol::CodexResponsesOauth;
    spec.upstream.local_surface = BridgeLocalSurface::Responses;
    spec.upstream.auth = ResolvedAuth::bearer("oauth-upstream-token");
    spec.with_mapping(
        AdapterSourceProduct::CodexChatGptSubscription,
        AgentId::Grok,
        false,
    )
    .with_pair_adapter_flags(false, true)
    .with_listed_models(vec!["gpt-5.4".into()])
}

async fn counting_responses_upstream() -> (u16, Arc<AtomicUsize>, tokio::task::JoinHandle<()>) {
    async fn responses(State(hits): State<Arc<AtomicUsize>>) -> Json<Value> {
        hits.fetch_add(1, Ordering::SeqCst);
        Json(grok_completed_response("hello"))
    }
    let hits = Arc::new(AtomicUsize::new(0));
    let listener =
        tokio::net::TcpListener::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
            .await
            .expect("bind counting upstream");
    let port = listener.local_addr().expect("addr").port();
    let state = hits.clone();
    let task = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new()
                .route("/v1/responses", post(responses))
                .with_state(state),
        )
        .await
        .expect("serve counting upstream");
    });
    (port, hits, task)
}

async fn capturing_codex_pair_upstream() -> (
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
        Json(json!({
            "id": "resp_codex_pair",
            "object": "response",
            "created_at": 1,
            "model": "gpt-5.4",
            "status": "completed",
            "store": false,
            "service_tier": "default",
            "metadata": { "codex_only": true },
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
                "total_tokens": 5,
                "reasoning_tokens": 0,
                "output_tokens_details": { "reasoning_tokens": 0 }
            }
        }))
    }
    let captured = Arc::new(Mutex::new(Vec::new()));
    let listener =
        tokio::net::TcpListener::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
            .await
            .expect("bind capturing Codex pair");
    let port = listener.local_addr().expect("addr").port();
    let state = captured.clone();
    let task = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new()
                .route("/v1/responses", post(responses))
                .with_state(state),
        )
        .await
        .expect("serve capturing Codex pair");
    });
    (port, captured, task)
}

#[tokio::test]
async fn pair_flag_on_codex_to_grok_adapts_request_and_strips_grok_identity() {
    let reply = json!({
        "id": "resp_grok",
        "object": "response",
        "created_at": 1,
        "model": "grok-4.5",
        "status": "completed",
        "prompt_cache_key": "grok-cache-secret",
        "session_id": "grok-session-secret",
        "output": [{
            "id": "msg_grok",
            "type": "message",
            "status": "completed",
            "role": "assistant",
            "content": [{ "type": "output_text", "text": "hello" }]
        }],
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
        .start(grok_codex_pair_spec("pair-c2g", 0, upstream_port))
        .await
        .expect("start");
    let response = client()
        .await
        .post(format!("http://127.0.0.1:{}/v1/responses", status.port))
        .header(header::AUTHORIZATION, "Bearer local-test-token")
        .json(&json!({
            "model": "grok-4.5",
            "store": true,
            "metadata": { "codex": true },
            "input": [
                {
                    "type": "message",
                    "role": "system",
                    "content": [{ "type": "input_text", "text": "sys" }]
                },
                {
                    "type": "message",
                    "role": "user",
                    "content": [{ "type": "input_text", "text": "hello" }]
                }
            ]
        }))
        .send()
        .await
        .expect("request");
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.expect("json");
    assert_eq!(body["output"][0]["content"][0]["text"], "hello");
    assert!(body.get("prompt_cache_key").is_none(), "{body}");
    assert!(body.get("session_id").is_none(), "{body}");

    let captured = captured.lock().expect("lock").clone();
    assert_eq!(captured.len(), 1);
    assert!(
        captured[0].body.get("store").is_none(),
        "{}",
        captured[0].body
    );
    assert!(captured[0].body.get("metadata").is_none());
    assert!(captured[0].headers.get("x-grok-client-version").is_some());

    host.stop("pair-c2g").await.expect("stop");
    upstream_task.abort();
}

#[tokio::test]
async fn pair_flag_on_grok_to_codex_uses_allowlist_without_grok_headers() {
    let (upstream_port, captured, upstream_task) = capturing_codex_pair_upstream().await;
    let host = BridgeRuntimeHost::new();
    let status = host
        .start(codex_grok_pair_spec("pair-g2c", 0, upstream_port))
        .await
        .expect("start");
    let response = client()
        .await
        .post(format!("http://127.0.0.1:{}/v1/responses", status.port))
        .header(header::AUTHORIZATION, "Bearer local-test-token")
        .json(&json!({
            "model": "gpt-5.4",
            "prompt_cache_key": "grok-cache-1",
            "store": true,
            "input": "hello"
        }))
        .send()
        .await
        .expect("request");
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.expect("json");
    assert_eq!(body["output"][0]["content"][0]["text"], "hello from codex");
    assert!(body.get("store").is_none(), "{body}");
    assert!(body.get("service_tier").is_none(), "{body}");
    assert!(body.get("metadata").is_none(), "{body}");

    let captured = captured.lock().expect("lock").clone();
    assert_eq!(captured.len(), 1);
    assert_eq!(captured[0].body["store"], false);
    assert!(captured[0].body.get("prompt_cache_key").is_none());
    assert!(captured[0].headers.get("x-grok-client-version").is_none());
    assert!(captured[0].headers.get("x-grok-session-id").is_none());

    host.stop("pair-g2c").await.expect("stop");
    upstream_task.abort();
}

#[tokio::test]
async fn pair_flag_on_missing_model_does_not_call_upstream() {
    let (upstream_port, hits, upstream_task) = counting_responses_upstream().await;
    let host = BridgeRuntimeHost::new();
    let status = host
        .start(grok_codex_pair_spec("pair-miss", 0, upstream_port))
        .await
        .expect("start");
    let response = client()
        .await
        .post(format!("http://127.0.0.1:{}/v1/responses", status.port))
        .header(header::AUTHORIZATION, "Bearer local-test-token")
        .json(&json!({
            "model": "not-a-mapped-model",
            "input": "hello"
        }))
        .send()
        .await
        .expect("request");
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body: Value = response.json().await.expect("json");
    assert_eq!(body["error"]["code"], "model_unavailable");
    assert_eq!(hits.load(Ordering::SeqCst), 0);

    host.stop("pair-miss").await.expect("stop");
    upstream_task.abort();
}

#[tokio::test]
async fn pair_flag_on_continuation_does_not_call_other_member() {
    let reject_a = Arc::new(AtomicBool::new(false));
    let (upstream_port, captured, upstream_task) =
        grok_account_gated_upstream(reject_a.clone()).await;
    let host = BridgeRuntimeHost::new();
    let status = host
        .start(
            grok_codex_pair_spec("pair-sticky", 0, upstream_port)
                .with_members(vec![
                    pool_member("acc-a", "token-a"),
                    pool_member("acc-b", "token-b"),
                ])
                .with_multi_account(true),
        )
        .await
        .expect("start");
    let first = client()
        .await
        .post(format!("http://127.0.0.1:{}/v1/responses", status.port))
        .header(header::AUTHORIZATION, "Bearer local-test-token")
        .json(&json!({ "model": "grok-4.5", "input": "hello" }))
        .send()
        .await
        .expect("first");
    assert_eq!(first.status(), StatusCode::OK);
    let first_body: Value = first.json().await.expect("first json");
    let response_id = first_body["id"].as_str().expect("id").to_owned();

    reject_a.store(true, Ordering::SeqCst);
    let second = client()
        .await
        .post(format!("http://127.0.0.1:{}/v1/responses", status.port))
        .header(header::AUTHORIZATION, "Bearer local-test-token")
        .json(&json!({
            "model": "grok-4.5",
            "previous_response_id": response_id,
            "input": "again"
        }))
        .send()
        .await
        .expect("second");
    assert_ne!(second.status(), StatusCode::OK);
    let bearers = captured.lock().expect("lock").clone();
    assert!(
        bearers.iter().all(|item| item != "Bearer token-b"),
        "continuation must not replay onto the other member: {bearers:?}"
    );
    assert!(bearers.iter().any(|item| item == "Bearer token-a"));

    host.stop("pair-sticky").await.expect("stop");
    upstream_task.abort();
}

#[tokio::test]
async fn pair_flag_off_keeps_experimental_grok_codex_passthrough() {
    let (upstream_port, captured, upstream_task) = capturing_grok_responses_upstream().await;
    let host = BridgeRuntimeHost::new();
    let status = host
        .start(grok_codex_spec("pair-off", 0, upstream_port))
        .await
        .expect("start");
    let response = client()
        .await
        .post(format!("http://127.0.0.1:{}/v1/responses", status.port))
        .header(header::AUTHORIZATION, "Bearer local-test-token")
        .json(&json!({
            "model": "grok-4.5",
            "store": true,
            "input": "hello"
        }))
        .send()
        .await
        .expect("request");
    assert_eq!(response.status(), StatusCode::OK);
    let upstream = captured.lock().expect("lock").clone();
    assert_eq!(upstream.len(), 1);
    assert_eq!(upstream[0]["store"], true);

    host.stop("pair-off").await.expect("stop");
    upstream_task.abort();
}
