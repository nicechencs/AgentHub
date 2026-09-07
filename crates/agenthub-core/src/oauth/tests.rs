use super::*;

#[test]
fn grok_login_uses_device_code_instead_of_pkce() {
    let err = start_oauth(AgentId::Grok, false, None).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("device-code"),
        "Grok must not start loopback PKCE: {msg}"
    );
    assert!(msg.contains("start_device_oauth"), "{msg}");
}

#[test]
fn kiro_login_uses_cli_instead_of_pkce() {
    let err = start_oauth(AgentId::Kiro, false, None).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("Kiro"),
        "Kiro official login must not start loopback PKCE: {msg}"
    );
    assert!(!msg.contains("start_device_oauth"), "{msg}");
    assert!(!msg.contains("PKCE"), "{msg}");
}

#[test]
fn unimplemented_pi_login_does_not_redirect_to_device_code() {
    let err = start_oauth(AgentId::Pi, false, Some("github-copilot")).unwrap_err();
    let msg = err.to_string();
    assert!(
        !msg.contains("start_device_oauth"),
        "unimplemented login must not be advertised as device-code: {msg}"
    );
    assert!(msg.contains("not available"), "{msg}");
}

#[test]
fn wait_oauth_timeout_keeps_session_waiting() {
    let state = format!("wait-keep-{}", std::process::id());
    store()
        .insert(session::OAuthSession::new(
            &state,
            AgentId::Claude,
            "verifier",
            "http://127.0.0.1/callback",
            None,
        ))
        .unwrap();
    let info = wait_oauth(&state, 1).unwrap();
    assert_eq!(info.status, OAuthStatus::Waiting);
    assert!(info.error.is_none());
    assert!(store().is_waiting(&state));
    let _ = store().mark_error(&state, "cleanup");
}
