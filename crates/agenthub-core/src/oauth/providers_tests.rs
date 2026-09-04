use super::*;
use serde_json::json;

#[test]
fn token_reject_error_hides_error_description_secret() {
    let secret = "sk-abcdefghijklmnopqrstuvwxyz012345";
    let body = json!({
        "error": "invalid_grant",
        "error_description": format!("client leaked {secret} in description")
    });
    let err = token_reject_error("oauth.token", "claude", 400, &body);
    let displayed = err.to_string();
    assert!(!displayed.contains(secret), "{displayed}");
    assert!(!displayed.contains("sk-"), "{displayed}");
    assert!(
        displayed.contains("token request rejected (HTTP 400)"),
        "{displayed}"
    );
    assert!(displayed.contains("invalid_grant"), "{displayed}");
    assert_eq!(err.code(), "oauth.token");
}

#[test]
fn token_reject_error_drops_unallowlisted_error_and_description() {
    let secret = "sk-unallowlistedSecretValue9999";
    let body = json!({
        "error": "totally_made_up",
        "error_description": format!("here is {secret}")
    });
    let err = token_reject_error("oauth.token", "codex", 401, &body);
    let displayed = err.to_string();
    assert!(!displayed.contains(secret), "{displayed}");
    assert!(!displayed.contains("totally_made_up"), "{displayed}");
    assert_eq!(displayed, "token request rejected (HTTP 401)");
}
