use super::*;

#[test]
fn missing_kiro_cli_does_not_start_pkce() {
    let err = start_kiro_cli_login(None).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("Kiro"), "{msg}");
    assert!(!msg.contains("PKCE"), "{msg}");
    assert!(!msg.contains("start_device_oauth"), "{msg}");
}

#[test]
fn cancel_unknown_session_is_idempotent() {
    cancel_kiro_cli_login("missing-kiro-login");
}

#[test]
fn parse_device_prompt_url_and_code() {
    let raw =
        "Please visit https://oidc.us-east-1.amazonaws.com/device\nand enter the code ABCD-EFGH\n";
    let parsed = parse_kiro_login_prompt(raw).expect("url");
    assert_eq!(parsed.url, "https://oidc.us-east-1.amazonaws.com/device");
    assert_eq!(parsed.user_code.as_deref(), Some("ABCD-EFGH"));
}

#[test]
fn parse_ignores_loopback_and_ansi() {
    let raw = "\u{1b}[32mopen\u{1b}[0m http://127.0.0.1:1234/callback then https://prod.us-east-1.auth.desktop.kiro.dev/login?x=1,\ncode: Wxyz-9876\n";
    let parsed = parse_kiro_login_prompt(raw).expect("url");
    assert_eq!(
        parsed.url,
        "https://prod.us-east-1.auth.desktop.kiro.dev/login?x=1"
    );
    assert_eq!(parsed.user_code.as_deref(), Some("WXYZ-9876"));
}

#[test]
fn parse_without_https_is_none() {
    assert!(parse_kiro_login_prompt("waiting for login").is_none());
}

#[test]
fn already_logged_in_is_detected() {
    assert!(already_logged_in(
        "error: Already logged in, please logout with kiro-cli logout first\n"
    ));
    assert!(!already_logged_in(
        "Please visit https://example.test/login"
    ));
    assert!(already_logged_out("Not logged in\n"));
}
