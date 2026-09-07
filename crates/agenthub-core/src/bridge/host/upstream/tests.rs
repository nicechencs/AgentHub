use std::io::Read;
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::json;
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;

use crate::bridge::grok_cli::GrokReasoningReplay;
use crate::bridge::runtime::{
    BridgeLocalSurface, BridgeStartSpec, BridgeUpstreamConfig, BridgeUpstreamProtocol,
    BridgeUpstreamStatus, ResolvedAuth,
};

use super::super::http::EdgeState;
use super::*;

fn listener_state() -> EdgeState {
    EdgeState {
        profile_id: Arc::from("upstream-test"),
        local_token: Arc::from("local-dummy"),
        upstream: BridgeUpstreamConfig {
            base_url: "http://127.0.0.1/v1/".to_owned(),
            model: Some("m1".to_owned()),
            source_id: None,
            auth: ResolvedAuth::bearer("upstream-dummy-token"),
            protocol: BridgeUpstreamProtocol::OpenAiChatCompletions,
            local_surface: BridgeLocalSurface::Responses,
        },
        upstream_url: reqwest::Url::parse("http://127.0.0.1/v1/").expect("test url"),
        client: reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(2))
            .build()
            .expect("client"),
        force_shutdown: CancellationToken::new(),
        stopping: Arc::new(AtomicBool::new(false)),
        admission: Arc::new(Semaphore::new(1)),
        observed_upstream: Arc::new(Mutex::new(BridgeUpstreamStatus::Unknown)),
        grok_replay: Arc::new(GrokReasoningReplay::new()),
        listed_models: Arc::from(Vec::<String>::new()),
        reload_upstream_auth: None,
        mapping_source: None,
        mapping_target: None,
        downstream_responses_profile: None,
        custom_openai: false,
        route_index: None,
        auth_reload: crate::bridge::auth_reload::AuthReloadCoordinator::new(),
        codex_ingress_grok_upstream: false,
        grok_ingress_codex_upstream: false,
        continuations: std::sync::Arc::new(super::super::continuation::ContinuationBindings::new()),
        usage_spool: Default::default(),
        route_traces: Default::default(),
        member_model_denials: std::sync::Arc::new(std::sync::Mutex::new(
            std::collections::HashSet::new(),
        )),
        account_picker: BridgeStartSpec::new(
            "upstream-test",
            0,
            "local-dummy",
            BridgeUpstreamConfig {
                base_url: "http://127.0.0.1/v1/".to_owned(),
                model: Some("m1".to_owned()),
                source_id: None,
                auth: ResolvedAuth::bearer("upstream-dummy-token"),
                protocol: BridgeUpstreamProtocol::OpenAiChatCompletions,
                local_surface: BridgeLocalSurface::Responses,
            },
        )
        .account_picker(),
    }
}

fn http_request_complete(buf: &[u8]) -> bool {
    let Some(header_end) = buf.windows(4).position(|window| window == b"\r\n\r\n") else {
        return false;
    };
    let headers = std::str::from_utf8(&buf[..header_end]).unwrap_or("");
    let body = &buf[header_end + 4..];
    let content_length = headers
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().ok())
                .flatten()
        })
        .unwrap_or(0);
    body.len() >= content_length
}

fn spawn_drop_after_post() -> (u16, Arc<AtomicUsize>, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind drop server");
    let port = listener.local_addr().expect("addr").port();
    let hits = Arc::new(AtomicUsize::new(0));
    let hits_thread = hits.clone();
    let task = std::thread::spawn(move || {
        let Ok((mut stream, _)) = listener.accept() else {
            return;
        };
        hits_thread.fetch_add(1, Ordering::SeqCst);
        let mut acc = Vec::new();
        let mut buf = [0u8; 4096];
        loop {
            match stream.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    acc.extend_from_slice(&buf[..n]);
                    if http_request_complete(&acc) {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
        let _ = stream.shutdown(std::net::Shutdown::Both);
        drop(stream);
    });
    (port, hits, task)
}

#[test]
fn non_stream_header_budget_matches_body_timeout() {
    assert_eq!(
        upstream_header_timeout(false),
        UPSTREAM_NON_STREAM_TIMEOUT,
        "non-stream TTFB must use the 120s body budget, not the 30s stream TTFB"
    );
    assert_eq!(
        upstream_header_timeout(true),
        UPSTREAM_RESPONSE_HEADER_TIMEOUT
    );
    assert!(upstream_header_timeout(false) > upstream_header_timeout(true));
    assert_eq!(UPSTREAM_NON_STREAM_TIMEOUT, Duration::from_secs(120));
    assert_eq!(UPSTREAM_RESPONSE_HEADER_TIMEOUT, Duration::from_secs(30));
}

#[tokio::test]
async fn connect_error_is_unavailable_for_failover() {
    let state = listener_state();
    let builder = state
        .client
        .post("http://127.0.0.1:1/chat/completions")
        .json(&json!({"model": "m1"}));
    let error = post_upstream_attempt(&state, builder, "req-connect", Duration::from_secs(3))
        .await
        .expect_err("nothing listens on port 1");
    assert_eq!(error, UpstreamConnectError::Unavailable);
}

#[tokio::test]
async fn drop_after_post_is_unreplayable() {
    let (port, hits, server) = spawn_drop_after_post();
    let state = listener_state();
    let builder = state
        .client
        .post(format!("http://127.0.0.1:{port}/chat/completions"))
        .json(&json!({"model": "m1", "messages": [{"role": "user", "content": "hi"}]}));
    let error = post_upstream_attempt(&state, builder, "req-drop", Duration::from_secs(5))
        .await
        .expect_err("peer closed before headers");
    assert_eq!(error, UpstreamConnectError::Unreplayable);
    assert_eq!(hits.load(Ordering::SeqCst), 1);
    let _ = server.join();
}
