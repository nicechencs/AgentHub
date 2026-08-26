use super::*;
use crate::models::AgentId;
use crate::oauth::session::{OAuthSession, OAuthStatus, SessionStore};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

fn waiting_session(state: &str, redirect_uri: &str) -> OAuthSession {
    OAuthSession {
        state: state.into(),
        agent: AgentId::Claude,
        verifier: "verifier".into(),
        redirect_uri: redirect_uri.into(),
        provider_key: None,
        status: OAuthStatus::Waiting,
        code: None,
        error: None,
        created_at: Instant::now(),
        completing: false,
    }
}

fn http_get(addr: std::net::SocketAddr, path: &str) {
    for _ in 0..50 {
        if let Ok(mut stream) = TcpStream::connect(addr) {
            let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
            let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
            let req = format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n");
            if stream.write_all(req.as_bytes()).is_ok() {
                let mut buf = [0u8; 512];
                let _ = stream.read(&mut buf);
                return;
            }
        }
        thread::sleep(Duration::from_millis(20));
    }
    panic!("could not GET {path} on {addr}");
}

#[test]
fn listener_ignores_probe_then_accepts_matching_callback() {
    let store = Arc::new(SessionStore::new());
    let listener = bind_listener(None).unwrap();
    let addr = listener.local_addr().unwrap();
    let state = "probe-state";
    let redirect = format!("http://127.0.0.1:{}/callback", addr.port());
    store.insert(waiting_session(state, &redirect)).unwrap();

    let store_thread = Arc::clone(&store);
    let handle = thread::spawn(move || {
        spawn_callback_listener(listener, store_thread, state, "/callback")
    });

    http_get(addr, "/favicon.ico");
    assert_eq!(store.get_info(state).unwrap().status, OAuthStatus::Waiting);

    http_get(addr, "/callback?state=wrong&code=stolen");
    assert_eq!(store.get_info(state).unwrap().status, OAuthStatus::Waiting);

    http_get(addr, &format!("/callback?code=ok-code&state={state}"));
    handle.join().unwrap().unwrap();
    let info = store.get_info(state).unwrap();
    assert_eq!(info.status, OAuthStatus::CallbackReceived);
}

#[test]
fn listener_exits_when_session_is_cancelled() {
    let store = Arc::new(SessionStore::new());
    let listener = bind_listener(None).unwrap();
    let addr = listener.local_addr().unwrap();
    let state = "cancel-state";
    store
        .insert(waiting_session(
            state,
            &format!("http://127.0.0.1:{}/callback", addr.port()),
        ))
        .unwrap();

    let store_thread = Arc::clone(&store);
    let handle = thread::spawn(move || {
        spawn_callback_listener(listener, store_thread, state, "/callback")
    });

    store.mark_error(state, "cancelled").unwrap();
    handle
        .join()
        .expect("listener thread")
        .expect("listener exit");
    assert_eq!(store.get_info(state).unwrap().status, OAuthStatus::Failed);
}
