use std::io::Read;
use std::net::TcpListener;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use serde_json::json;
use tokio_util::sync::CancellationToken;

use crate::bridge::account::{BridgeMemberSpec, MemberHealth};
use crate::bridge::auth_reload::AuthReloadCoordinator;
use crate::bridge::route_index::{EffectiveRouteIndex, MemberCapability, MemberCapabilitySnapshot};
use crate::bridge::runtime::{
    BridgeLocalSurface, BridgeStartSpec, BridgeUpstreamConfig, BridgeUpstreamProtocol, ResolvedAuth,
};
use crate::bridge::UpstreamAuthReload;

use super::super::super::http::EdgeState;
use super::super::super::surface::DownstreamSurface;
use super::send_upstream_v2;

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

fn chat_ok() -> Response {
    Json(json!({
        "id": "chat-test",
        "model": "m1",
        "created": 1,
        "choices": [{ "message": { "role": "assistant", "content": "hello" }, "finish_reason": "stop" }],
        "usage": { "prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2 }
    }))
    .into_response()
}

async fn capturing_ok_chat() -> (u16, Arc<AtomicUsize>, tokio::task::JoinHandle<()>) {
    async fn chat(State(hits): State<Arc<AtomicUsize>>) -> Response {
        hits.fetch_add(1, Ordering::SeqCst);
        chat_ok()
    }
    let hits = Arc::new(AtomicUsize::new(0));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind capturing upstream");
    let port = listener.local_addr().expect("addr").port();
    let state = hits.clone();
    let task = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new()
                .route("/chat/completions", post(chat))
                .with_state(state),
        )
        .await
        .expect("serve capturing upstream");
    });
    (port, hits, task)
}

async fn token_gated_chat() -> (u16, tokio::task::JoinHandle<()>) {
    async fn chat(headers: HeaderMap) -> Response {
        let bearer = headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default();
        if bearer == "Bearer t1" {
            chat_ok()
        } else {
            StatusCode::UNAUTHORIZED.into_response()
        }
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind gated upstream");
    let port = listener.local_addr().expect("addr").port();
    let task = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new().route("/chat/completions", post(chat)),
        )
        .await
        .expect("serve gated upstream");
    });
    (port, task)
}

fn member_spec(
    id: &str,
    auth: ResolvedAuth,
    reload: Option<UpstreamAuthReload>,
) -> BridgeMemberSpec {
    BridgeMemberSpec::new(
        format!("account:{id}"),
        "account",
        id,
        id,
        auth,
        reload,
        MemberHealth::Renewable,
        0,
        0,
    )
}

fn snapshot(member_id: &str, port: u16) -> MemberCapabilitySnapshot {
    MemberCapabilitySnapshot {
        member_id: member_id.into(),
        public_model: "m1".into(),
        endpoint: "responses".into(),
        upstream_provider: "openai".into(),
        upstream_dialect: "generic".into(),
        upstream_model: "m1".into(),
        upstream_endpoint: format!("http://127.0.0.1:{port}"),
        transport_key: "openai:generic".into(),
        capability: MemberCapability::Supported,
    }
}

fn v2_state(
    profile_id: &str,
    members: Vec<BridgeMemberSpec>,
    snapshots: &[MemberCapabilitySnapshot],
    coordinator: AuthReloadCoordinator,
) -> EdgeState {
    let index = EffectiveRouteIndex::build(profile_id, 1, snapshots);
    let spec = BridgeStartSpec::new(
        profile_id,
        0,
        "local-token",
        BridgeUpstreamConfig {
            base_url: "http://127.0.0.1:9".to_owned(),
            model: Some("m1".to_owned()),
            source_id: Some("acc-a".into()),
            auth: ResolvedAuth::bearer("lead"),
            protocol: BridgeUpstreamProtocol::OpenAiChatCompletions,
            local_surface: BridgeLocalSurface::Responses,
        },
    )
    .with_members(members)
    .with_listed_models(vec!["m1".into()])
    .with_route_index(index);
    EdgeState::from_spec(
        &spec,
        reqwest::Url::parse("http://127.0.0.1:9/").expect("url"),
        CancellationToken::new(),
        coordinator,
        Default::default(),
        Default::default(),
    )
}

async fn send_m1(
    state: &EdgeState,
    member_id: &str,
) -> Result<reqwest::Response, axum::response::Response> {
    let member = state
        .account_picker
        .members()
        .iter()
        .find(|member| member.source_id == member_id)
        .expect("member")
        .clone();
    let candidates = state
        .route_index
        .as_ref()
        .expect("index")
        .resolve("responses", "m1")
        .expect("resolve");
    send_upstream_v2(
        state,
        DownstreamSurface::Responses,
        "req-test",
        Instant::now(),
        &HeaderMap::new(),
        &json!({"model": "m1", "input": "hello"}),
        member,
        &candidates,
        "m1",
        false,
        None,
        None,
    )
    .await
    .map(|outcome| outcome.response)
}

#[tokio::test]
async fn drop_after_post_does_not_replay_onto_second_member() {
    let (port_a, hits_a, server_a) = spawn_drop_after_post();
    let (port_b, hits_b, server_b) = capturing_ok_chat().await;
    let state = v2_state(
        "p1-006-drop",
        vec![
            member_spec("acc-a", ResolvedAuth::bearer("token-a"), None),
            member_spec("acc-b", ResolvedAuth::bearer("token-b"), None),
        ],
        &[snapshot("acc-a", port_a), snapshot("acc-b", port_b)],
        AuthReloadCoordinator::new(),
    );
    let result = send_m1(&state, "acc-a").await;
    let err = result.expect_err("must not succeed by replaying onto B");
    assert_eq!(err.status(), StatusCode::BAD_GATEWAY);
    assert_eq!(
        hits_a.load(Ordering::SeqCst),
        1,
        "A must have read the POST"
    );
    assert_eq!(
        hits_b.load(Ordering::SeqCst),
        0,
        "B must not receive a replayed request"
    );
    let _ = server_a.join();
    server_b.abort();
}

#[tokio::test]
async fn connect_failure_still_failovers_to_second_member() {
    let dead = TcpListener::bind("127.0.0.1:0").expect("bind dead");
    let port_a = dead.local_addr().expect("addr").port();
    drop(dead);
    let (port_b, hits_b, server_b) = capturing_ok_chat().await;
    let state = v2_state(
        "p1-006-connect",
        vec![
            member_spec("acc-a", ResolvedAuth::bearer("token-a"), None),
            member_spec("acc-b", ResolvedAuth::bearer("token-b"), None),
        ],
        &[snapshot("acc-a", port_a), snapshot("acc-b", port_b)],
        AuthReloadCoordinator::new(),
    );
    let response = send_m1(&state, "acc-a")
        .await
        .expect("B should serve after A connect failure");
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    assert_eq!(hits_b.load(Ordering::SeqCst), 1);
    server_b.abort();
}

#[tokio::test]
async fn late_401_on_independent_cell_adopts_current_token_without_isolate() {
    let (port, task) = token_gated_chat().await;
    let db = Arc::new(Mutex::new("t0".to_string()));
    let reload: UpstreamAuthReload = Arc::new({
        let db = db.clone();
        move || {
            let mut guard = db.lock().expect("lock");
            if guard.as_str() == "t0" {
                *guard = "t1".to_owned();
            }
            Some(guard.clone())
        }
    });
    let coordinator = AuthReloadCoordinator::new();
    let cell_a = ResolvedAuth::bearer("t0");
    let cell_b = ResolvedAuth::bearer("t0");
    let state_a = v2_state(
        "p1-007-a",
        vec![member_spec("acc-a", cell_a, Some(reload.clone()))],
        &[snapshot("acc-a", port)],
        coordinator.clone(),
    );
    let state_b = v2_state(
        "p1-007-b",
        vec![member_spec("acc-a", cell_b, Some(reload))],
        &[snapshot("acc-a", port)],
        coordinator.clone(),
    );
    let first = send_m1(&state_a, "acc-a")
        .await
        .expect("first cell rotates T0 to T1");
    assert_eq!(first.status(), reqwest::StatusCode::OK);
    let second = send_m1(&state_b, "acc-a")
        .await
        .expect("second cell must adopt T1 instead of isolating");
    assert_eq!(second.status(), reqwest::StatusCode::OK);
    assert!(
        !coordinator.is_isolated("account:acc-a"),
        "updated login must stay eligible across pools"
    );
    task.abort();
}

#[tokio::test]
async fn stale_401_does_not_isolate_cell_already_rotated_by_side_effect() {
    let (port, task) = token_gated_chat().await;
    let auth = ResolvedAuth::bearer("t0");
    let auth_cb = auth.clone();
    let reload: UpstreamAuthReload = Arc::new(move || {
        auth_cb.replace_token("t1");
        None
    });
    let coordinator = AuthReloadCoordinator::new();
    let state = v2_state(
        "p1-007-stale",
        vec![member_spec("acc-a", auth, Some(reload))],
        &[snapshot("acc-a", port)],
        coordinator.clone(),
    );
    let response = send_m1(&state, "acc-a")
        .await
        .expect("stale 401 must retry the already-rotated cell");
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    assert!(!coordinator.is_isolated("account:acc-a"));
    task.abort();
}
