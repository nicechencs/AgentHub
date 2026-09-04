use super::*;
use serde_json::json;

use crate::catalog::limits::OAUTH_REFRESH_SKEW_MS;

#[test]
fn empty_access_token_is_err() {
    let err = access_token_from_json(&json!({ "access_token": "" })).unwrap_err();
    assert_eq!(err.code(), "oauth.refresh");
    assert!(err.to_string().contains("missing access"), "{err}");
}

#[test]
fn empty_access_alias_is_err() {
    let err = access_token_from_json(&json!({ "access": "" })).unwrap_err();
    assert!(err.to_string().contains("missing access"), "{err}");
}

#[test]
fn missing_access_token_is_err() {
    let err = access_token_from_json(&json!({ "refresh_token": "rt" })).unwrap_err();
    assert!(err.to_string().contains("missing access"), "{err}");
}

#[test]
fn huge_expires_in_is_clamped_and_does_not_overflow() {
    let expires_in = expires_in_secs_from_json(&json!({ "expires_in": i64::MAX }));
    assert_eq!(expires_in, MAX_EXPIRES_IN_SECS);

    let before = chrono::Utc::now().timestamp_millis();
    let ms = expires_ms_from_secs(expires_in);
    let after = chrono::Utc::now().timestamp_millis();
    let year_ms = MAX_EXPIRES_IN_SECS.saturating_mul(1000);
    let min = before
        .saturating_add(year_ms)
        .saturating_sub(OAUTH_REFRESH_SKEW_MS);
    let max = after
        .saturating_add(year_ms)
        .saturating_sub(OAUTH_REFRESH_SKEW_MS);
    assert!(ms >= min && ms <= max, "ms={ms} min={min} max={max}");

    let _ = expires_ms_from_secs(i64::MAX);
}

#[test]
fn parse_token_status_body_hides_error_description_secret() {
    let secret = "sk-abcdefghijklmnopqrstuvwxyz012345";
    let body = json!({
        "error": "invalid_grant",
        "error_description": format!("refresh token {secret} is revoked")
    });
    let err = parse_token_status_body("anthropic", 400, body).unwrap_err();
    let displayed = err.to_string();
    assert!(!displayed.contains(secret), "{displayed}");
    assert!(!displayed.contains("sk-"), "{displayed}");
    assert!(displayed.contains("HTTP 400"), "{displayed}");
    assert!(displayed.contains("invalid_grant"), "{displayed}");
    assert_eq!(err.code(), "oauth.refresh");
}
