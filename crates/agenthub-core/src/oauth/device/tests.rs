use super::*;
use serde_json::json;
use std::collections::HashMap;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

fn session(state: &str, status: DeviceOAuthStatus, expires_at: Instant) -> (String, DeviceSession) {
    (
        state.into(),
        DeviceSession {
            agent: AgentId::Pi,
            provider_key: "xai".into(),
            pool_owned: false,
            device_code: "device-secret".into(),
            interval: Duration::from_secs(1),
            expires_at,
            last_poll: Instant::now(),
            access: Some("access-secret".into()),
            refresh: Some("refresh-secret".into()),
            expires_at_ms: Some(1),
            status,
            error: None,
            completing: false,
            completion_expires_at: None,
            poll_generation: 0,
            poll_claim: None,
        },
    )
}

/// Removes only this test's `state` on drop (panic, early return, or end of test).
#[must_use = "device store guard removes the session on drop"]
struct DeviceStoreGuard {
    state: String,
}

impl Drop for DeviceStoreGuard {
    fn drop(&mut self) {
        if let Ok(mut sessions) = store().lock() {
            sessions.remove(&self.state);
        }
    }
}

fn insert_session(state: &str, session: DeviceSession) -> DeviceStoreGuard {
    store()
        .lock()
        .expect("device store lock")
        .insert(state.into(), session);
    DeviceStoreGuard {
        state: state.into(),
    }
}

#[test]
fn device_store_guard_removes_only_its_own_key_on_drop() {
    let own = "guard-drop-own-key";
    let other = "guard-drop-other-key";
    let (_, own_session) = session(
        own,
        DeviceOAuthStatus::Pending,
        Instant::now() + Duration::from_secs(60),
    );
    let (_, other_session) = session(
        other,
        DeviceOAuthStatus::Pending,
        Instant::now() + Duration::from_secs(60),
    );
    let other_guard = insert_session(other, other_session);
    {
        let _guard = insert_session(own, own_session);
        assert!(store().lock().expect("device store lock").contains_key(own));
        assert!(store()
            .lock()
            .expect("device store lock")
            .contains_key(other));
    }
    {
        let sessions = store().lock().expect("device store lock");
        assert!(!sessions.contains_key(own));
        assert!(sessions.contains_key(other));
    }
    drop(other_guard);
}

#[test]
fn rfc_device_pending_body_is_preserved_from_http_400() {
    let body = parse_device_http_response(400, json!({"error": "authorization_pending"})).unwrap();
    assert_eq!(body["error"], "authorization_pending");
}

#[test]
fn rfc_device_slow_down_body_is_preserved_from_http_400() {
    let body =
        parse_device_http_response(400, json!({"error": "slow_down", "interval": 10})).unwrap();
    assert_eq!(body["error"], "slow_down");
    assert_eq!(body["interval"], 10);
}

#[test]
fn rfc_device_access_denied_and_expired_are_not_retried() {
    for error in ["access_denied", "expired_token"] {
        let body = parse_device_http_response(400, json!({"error": error})).unwrap();
        assert_eq!(body["error"], error);
    }
}

#[test]
fn transport_and_server_errors_are_retryable() {
    let server =
        parse_device_http_response(503, json!({"error": "temporarily_unavailable"})).unwrap_err();
    assert_eq!(server.code(), "oauth.device.retry");

    let html_gateway =
        ureq::Response::new(503, "Service Unavailable", "<html>bad gateway</html>").unwrap();
    let non_json = parse_device_status_response(503, html_gateway).unwrap_err();
    assert_eq!(non_json.code(), "oauth.device.retry");
}

#[test]
fn expired_and_terminal_device_sessions_are_cleaned_without_touching_active() {
    let mut sessions = HashMap::new();
    let (expired_state, expired) = session(
        "expired",
        DeviceOAuthStatus::Pending,
        Instant::now() - Duration::from_secs(1),
    );
    let (failed_state, failed) = session(
        "failed",
        DeviceOAuthStatus::Failed,
        Instant::now() + Duration::from_secs(60),
    );
    let (active_state, active) = session(
        "active",
        DeviceOAuthStatus::Pending,
        Instant::now() + Duration::from_secs(60),
    );
    sessions.insert(expired_state, expired);
    sessions.insert(failed_state, failed);
    sessions.insert(active_state.clone(), active);

    purge_locked(&mut sessions, None);

    assert!(!sessions.contains_key("expired"));
    assert!(!sessions.contains_key("failed"));
    assert!(sessions.contains_key(&active_state));
}

#[test]
fn failed_device_completion_scrubs_tokens_and_cannot_be_replayed() {
    let (_, mut session) = session(
        "completing",
        DeviceOAuthStatus::Completing,
        Instant::now() + Duration::from_secs(60),
    );
    session.completing = false;
    session.status = DeviceOAuthStatus::Failed;
    session.error = Some("device OAuth completion failed".into());
    scrub_session(&mut session);

    assert!(session.device_code.is_empty());
    assert!(session.access.is_none());
    assert!(session.refresh.is_none());
    assert!(session.expires_at_ms.is_none());
    assert_eq!(session.status, DeviceOAuthStatus::Failed);
}

#[test]
fn concurrent_poll_claim_does_not_issue_a_second_request() {
    let state = "poll-claim-in-progress";
    let (_, mut value) = session(
        state,
        DeviceOAuthStatus::Pending,
        Instant::now() + Duration::from_secs(60),
    );
    value.interval = Duration::ZERO;
    value.last_poll = Instant::now() - Duration::from_secs(1);
    value.access = None;
    value.refresh = None;
    value.expires_at_ms = None;
    let _guard = insert_session(state, value);

    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let thread_state = state.to_string();
    let first = thread::spawn(move || {
        poll_device_oauth_with(&thread_state, |_| {
            started_tx.send(()).expect("signal request start");
            release_rx.recv().expect("release request");
            Ok(json!({"error": "authorization_pending"}))
        })
    });

    started_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("first request should claim the session");
    let second = poll_device_oauth_with(state, |_| {
        panic!("a concurrent poll must not issue another request")
    })
    .expect("concurrent poll should be reported as pending");
    assert_eq!(second.status, DeviceOAuthStatus::Pending);
    assert_eq!(
        second.error.as_deref(),
        Some("device oauth poll is already in progress")
    );

    release_tx.send(()).expect("release first request");
    let first = first
        .join()
        .expect("first poll thread")
        .expect("poll result");
    assert_eq!(first.status, DeviceOAuthStatus::Pending);
}

#[test]
fn superseded_poll_response_cannot_revert_complete_session_or_clear_tokens() {
    let state = "poll-generation-superseded";
    let (_, mut value) = session(
        state,
        DeviceOAuthStatus::Pending,
        Instant::now() + Duration::from_secs(60),
    );
    value.interval = Duration::ZERO;
    value.last_poll = Instant::now() - Duration::from_secs(1);
    value.access = None;
    value.refresh = None;
    value.expires_at_ms = None;
    let _guard = insert_session(state, value);

    let result = poll_device_oauth_with(state, |_| {
        let mut guard = store().lock().expect("device store lock");
        let current = guard.get_mut(state).expect("claimed session");
        current.poll_generation = current.poll_generation.wrapping_add(1);
        current.poll_claim = None;
        current.status = DeviceOAuthStatus::Complete;
        current.completion_expires_at = Some(Instant::now() + Duration::from_secs(60));
        current.access = Some("new-access".into());
        current.refresh = Some("new-refresh".into());
        Ok(json!({"error": "authorization_pending"}))
    })
    .expect("superseded response should be ignored");
    assert_eq!(result.status, DeviceOAuthStatus::Complete);
    assert!(result.error.is_none());

    let guard = store().lock().expect("device store lock");
    let current = guard.get(state).expect("complete session remains");
    assert_eq!(current.status, DeviceOAuthStatus::Complete);
    assert_eq!(current.access.as_deref(), Some("new-access"));
    assert_eq!(current.refresh.as_deref(), Some("new-refresh"));
    assert!(current.expires_at_ms.is_none());
    assert!(current.poll_claim.is_none());
    drop(guard);
}

#[test]
fn complete_session_survives_device_code_expiry_until_completion_ttl() {
    let state = "complete-after-device-expiry";
    let (_, mut value) = session(
        state,
        DeviceOAuthStatus::Pending,
        Instant::now() + Duration::from_secs(60),
    );
    value.interval = Duration::ZERO;
    value.last_poll = Instant::now() - Duration::from_secs(1);
    value.access = None;
    value.refresh = None;
    value.expires_at_ms = None;
    let _guard = insert_session(state, value);

    let initial = poll_device_oauth_with(state, |_| {
        Ok(json!({
            "access_token": "complete-access",
            "refresh_token": "complete-refresh",
            "expires_in": 3600,
        }))
    })
    .expect("successful token response should complete the session");
    assert_eq!(initial.status, DeviceOAuthStatus::Complete);

    store()
        .lock()
        .expect("device store lock")
        .get_mut(state)
        .expect("completed session")
        .expires_at = Instant::now() - Duration::from_secs(60);

    let poll = poll_device_oauth_with(state, |_| {
        panic!("complete sessions must not poll the token endpoint")
    })
    .expect("complete session should remain available");
    assert_eq!(poll.status, DeviceOAuthStatus::Complete);
    assert_eq!(
        device_oauth_agent(state).expect("agent remains resolvable"),
        AgentId::Pi
    );
}

#[test]
fn complete_session_is_purged_after_completion_ttl() {
    let state = "complete-ttl-expired";
    let (_, mut value) = session(
        state,
        DeviceOAuthStatus::Complete,
        Instant::now() + Duration::from_secs(60),
    );
    value.completion_expires_at = Some(Instant::now() - Duration::from_secs(1));
    let _guard = insert_session(state, value);

    purge_locked(&mut store().lock().expect("device store lock"), None);
    assert!(!store()
        .lock()
        .expect("device store lock")
        .contains_key(state));
}

#[test]
fn resolve_device_oauth_target_accepts_grok_and_pi_xai() {
    let grok = resolve_device_oauth_target(AgentId::Grok, "").expect("grok");
    assert_eq!(grok.provider_key, "xai");
    assert_eq!(grok.referrer, "grok");
    assert_eq!(
        resolve_device_oauth_target(AgentId::Grok, "XAI")
            .expect("alias")
            .referrer,
        "grok"
    );
    let pi = resolve_device_oauth_target(AgentId::Pi, "xai").expect("pi");
    assert_eq!(pi.provider_key, "xai");
    assert_eq!(pi.referrer, "pi");
    assert!(resolve_device_oauth_target(AgentId::Grok, "claude").is_err());
    assert!(resolve_device_oauth_target(AgentId::Claude, "").is_err());
    assert!(resolve_device_oauth_target(AgentId::Pi, "anthropic").is_err());
}

#[test]
fn grok_device_account_input_uses_official_cli_client() {
    let (_, mut value) = session(
        "grok-input",
        DeviceOAuthStatus::Complete,
        Instant::now() + Duration::from_secs(60),
    );
    value.agent = AgentId::Grok;
    value.provider_key = "xai".into();
    let input = grok_device_account_input(&value).expect("grok input");
    assert_eq!(input.agent_id, AgentId::Grok);
    assert_eq!(input.kind, crate::models::AccountKind::Oauth);
    assert_eq!(
        input
            .credentials
            .get("access_token")
            .and_then(|v| v.as_str()),
        Some("access-secret")
    );
    assert_eq!(
        input
            .credentials
            .get("oidc_client_id")
            .and_then(|v| v.as_str()),
        Some(super::super::providers::XAI_DEVICE_CLIENT_ID)
    );
    assert_eq!(
        input
            .credentials
            .get("oidc_issuer")
            .and_then(|v| v.as_str()),
        Some(super::super::providers::XAI_DEVICE_ISSUER)
    );
    assert_eq!(
        input.extra.get("source").and_then(|v| v.as_str()),
        Some("oauth_pkce")
    );
    assert!(!crate::models::authorization_is_route_pool_home(
        &input.extra
    ));
    assert!(!input.is_current);
}

#[test]
fn pool_owned_grok_device_account_input_marks_route_pool_home() {
    let (_, mut value) = session(
        "grok-pool-input",
        DeviceOAuthStatus::Complete,
        Instant::now() + Duration::from_secs(60),
    );
    value.agent = AgentId::Grok;
    value.provider_key = "xai".into();
    value.pool_owned = true;

    let input = grok_device_account_input(&value).expect("grok pool input");
    assert_eq!(
        input.extra.get("home").and_then(|v| v.as_str()),
        Some("route_pool")
    );
    assert!(!input.is_current);
}
