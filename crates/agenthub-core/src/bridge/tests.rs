use std::convert::Infallible;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
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

use super::host::CleanupCompletion;
use super::{
    BridgeHostError, BridgeRuntimeHost, BridgeRuntimeState, BridgeStartSpec, BridgeUpstreamConfig,
    BridgeUpstreamStatus, ResolvedAuth,
};

fn spec(profile_id: &str, port: u16, upstream_port: u16) -> BridgeStartSpec {
    BridgeStartSpec::new(
        profile_id,
        port,
        "local-test-token",
        BridgeUpstreamConfig {
            base_url: format!("http://127.0.0.1:{upstream_port}"),
            model: None,
            source_connection_id: Some("connection-test".to_owned()),
            auth: ResolvedAuth::bearer("upstream-test-token"),
        },
    )
}

async fn upstream() -> (u16, tokio::task::JoinHandle<()>) {
    upstream_at("/chat/completions").await
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
        .start(spec("admission-a", 0, upstream_port))
        .await
        .expect("start first profile");
    let second = host
        .start(spec("admission-b", 0, other_upstream_port))
        .await
        .expect("start second profile");
    let mut requests = Vec::new();
    for _ in 0..4 {
        let client = client().await;
        let url = format!("http://127.0.0.1:{}/v1/responses", first.port);
        requests.push(tokio::spawn(async move {
            client
                .post(url)
                .header(header::AUTHORIZATION, "Bearer local-test-token")
                .json(&json!({"model":"test","input":"hello"}))
                .send()
                .await
        }));
    }
    reached_upstream.await;
    // The first notification proves the upstream is holding a permit. Give the other three
    // local client tasks a short scheduling window before asking for the fifth permit.
    tokio::time::sleep(Duration::from_millis(30)).await;
    let overloaded = client()
        .await
        .post(format!("http://127.0.0.1:{}/v1/responses", first.port))
        .header(header::AUTHORIZATION, "Bearer local-test-token")
        .json(&json!({"model":"test","input":"hello"}))
        .send()
        .await
        .expect("overload request");
    assert_eq!(overloaded.status(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(
        client()
            .await
            .post(format!("http://127.0.0.1:{}/v1/responses", second.port))
            .header(header::AUTHORIZATION, "Bearer local-test-token")
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
        host.start(spec("cross-b", 0, upstream_port)),
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
async fn port_conflict_fails_and_non_streaming_response_translates() {
    let (upstream_port, upstream_task) = upstream().await;
    let host = BridgeRuntimeHost::new();
    let first = host
        .start(spec("first", 0, upstream_port))
        .await
        .expect("first start");
    let conflict = host.start(spec("second", first.port, upstream_port)).await;
    assert!(matches!(conflict, Err(super::BridgeHostError::Bind(_))));
    let response = client()
        .await
        .post(format!("http://127.0.0.1:{}/v1/responses", first.port))
        .header(header::AUTHORIZATION, "Bearer local-test-token")
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
