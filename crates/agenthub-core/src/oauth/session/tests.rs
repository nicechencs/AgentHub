use super::*;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

impl SessionStore {
    /// Test helper: inspect a session as of an explicit clock instant so expiry
    /// tests never subtract `TTL` from `Instant::now()` (that panics when the
    /// clock origin is within one TTL of now). Lives in the test module per the
    /// repo rule that test code stays out of production files.
    fn get_info_at(&self, state: &str, now: Instant) -> Result<OAuthSessionInfo> {
        let mut g = self
            .inner
            .lock()
            .map_err(|_| AppError::message("oauth.store", "session store poisoned"))?;
        purge_at(&mut g, now);
        let s = g
            .get(state)
            .ok_or_else(|| AppError::NotFound("oauth session not found".into()))?;
        Ok(OAuthSessionInfo {
            state: s.state.clone(),
            agent_id: s.agent,
            status: s.status,
            error: s.error.clone(),
        })
    }
}

fn session(state: &str, created_at: Instant) -> OAuthSession {
    OAuthSession {
        state: state.into(),
        agent: AgentId::Claude,
        verifier: "verifier".into(),
        redirect_uri: "http://127.0.0.1/callback".into(),
        provider_key: None,
        status: OAuthStatus::Waiting,
        code: None,
        error: None,
        created_at,
        completing: false,
    }
}

#[test]
fn expired_sessions_are_purged_on_read() {
    let store = SessionStore::new();
    let state = "expired-session";
    let created_at = Instant::now();
    store.insert(session(state, created_at)).unwrap();

    // Adding TTL never underflows. Subtracting TTL from Instant::now() panics
    // when the clock origin is within one TTL of now (fresh boot / short-lived
    // hosts) and must not be used to construct an expired session.
    let expired_at = created_at
        .checked_add(TTL + Duration::from_secs(1))
        .expect("OAuth TTL offset should fit in Instant");
    let error = store.get_info_at(state, expired_at).unwrap_err();
    assert_eq!(error.code(), "not_found");
}

#[test]
fn completion_claim_is_single_use_and_failure_is_terminal() {
    let store = SessionStore::new();
    let state = "single-use-session";
    store.insert(session(state, Instant::now())).unwrap();
    assert_eq!(
        store
            .insert(session(state, Instant::now()))
            .unwrap_err()
            .code(),
        "oauth.state"
    );
    store.set_code(state, "authorization-code".into()).unwrap();
    assert_eq!(
        store
            .set_code(state, "replacement-code".into())
            .unwrap_err()
            .code(),
        "oauth.replay"
    );

    let claimed = store.take_ready(state).unwrap();
    assert_eq!(claimed.code.as_deref(), Some("authorization-code"));
    assert_eq!(store.take_ready(state).unwrap_err().code(), "oauth.replay");

    store.mark_completion_failed(state).unwrap();
    let info = store.get_info(state).unwrap();
    assert_eq!(info.status, OAuthStatus::Failed);
    assert_eq!(info.error.as_deref(), Some("OAuth completion failed"));
    assert_eq!(store.take_ready(state).unwrap_err().code(), "oauth.replay");
}

#[test]
fn timeout_failure_scrubs_callback_code_and_blocks_completion() {
    let store = SessionStore::new();
    let state = "timed-out-session";
    store.insert(session(state, Instant::now())).unwrap();
    store.set_code(state, "authorization-code".into()).unwrap();

    store.mark_error(state, "untrusted callback text").unwrap();

    let info = store.get_info(state).unwrap();
    assert_eq!(info.status, OAuthStatus::Failed);
    assert_eq!(info.error.as_deref(), Some("OAuth authorization failed"));
    assert_eq!(store.take_ready(state).unwrap_err().code(), "oauth.replay");
}

#[test]
fn concurrent_completion_claims_have_one_winner() {
    let store = Arc::new(SessionStore::new());
    let state = "concurrent-session";
    store.insert(session(state, Instant::now())).unwrap();
    store.set_code(state, "authorization-code".into()).unwrap();

    let mut workers = Vec::new();
    for _ in 0..8 {
        let store = Arc::clone(&store);
        workers.push(thread::spawn(move || store.take_ready(state)));
    }

    let mut successes = 0;
    let mut replays = 0;
    for worker in workers {
        match worker.join().unwrap() {
            Ok(_) => successes += 1,
            Err(error) if error.code() == "oauth.replay" => replays += 1,
            Err(error) => panic!("unexpected claim error: {error}"),
        }
    }
    assert_eq!(successes, 1);
    assert_eq!(replays, 7);
}

#[test]
fn cancel_waiting_on_port_fails_only_matching_sessions() {
    let store = SessionStore::new();
    store
        .insert(session("keep", Instant::now()))
        .unwrap();
    let mut other = session("drop", Instant::now());
    other.redirect_uri = "http://127.0.0.1:1455/callback".into();
    store.insert(other).unwrap();

    assert_eq!(store.cancel_waiting_on_port(1455).unwrap(), 1);
    assert_eq!(store.get_info("keep").unwrap().status, OAuthStatus::Waiting);
    assert_eq!(store.get_info("drop").unwrap().status, OAuthStatus::Failed);
    assert!(!store.is_waiting("drop"));
}
