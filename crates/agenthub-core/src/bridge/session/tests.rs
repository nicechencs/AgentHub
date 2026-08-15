use serde_json::json;

use crate::error::AppError;

use super::resolve_codex_subscription_auth;

#[test]
fn auth_json_credentials_resolve_access_token() {
    let credentials = json!({
        "format": "auth_json",
        "body": {
            "tokens": {
                "access_token": "access-from-body",
                "refresh_token": "refresh-must-not-be-injected"
            }
        }
    });

    let auth = resolve_codex_subscription_auth(&credentials).expect("auth_json should resolve");

    assert_eq!(auth.token(), "access-from-body");
}

#[test]
fn tokens_blob_without_format_resolves_access_token() {
    let credentials = json!({
        "tokens": {
            "access_token": "access-without-format",
            "refresh_token": "refresh-must-not-be-injected"
        }
    });

    let auth =
        resolve_codex_subscription_auth(&credentials).expect("tokens blob should be recognized");

    assert_eq!(auth.token(), "access-without-format");
}

#[test]
fn top_level_access_token_takes_priority_over_body_token() {
    let credentials = json!({
        "format": "auth_json",
        "tokens": { "access_token": "access-from-tokens" },
        "body": {
            "tokens": { "access_token": "access-from-body" }
        }
    });

    let auth = resolve_codex_subscription_auth(&credentials).expect("credentials should resolve");

    assert_eq!(auth.token(), "access-from-tokens");
}

#[test]
fn missing_access_token_returns_invalid_argument_without_secret() {
    let credentials = json!({
        "format": "auth_json",
        "body": {
            "tokens": {
                "refresh_token": "refresh-only-secret"
            }
        }
    });

    let error = resolve_codex_subscription_auth(&credentials).expect_err("access token required");

    assert!(matches!(error, AppError::InvalidArg(_)));
    assert!(format!("{error}").contains("missing access_token"));
    assert!(!format!("{error}").contains("refresh-only-secret"));
}

#[test]
fn debug_and_display_do_not_expose_credentials() {
    let credentials = json!({
        "format": "auth_json",
        "tokens": {
            "access_token": "access-secret",
            "refresh_token": "refresh-secret"
        }
    });
    let auth = resolve_codex_subscription_auth(&credentials).expect("credentials should resolve");

    let debug = format!("{auth:?}");
    assert_eq!(debug, "ResolvedAuth(REDACTED)");
    assert!(!debug.contains("access-secret"));
    assert!(!debug.contains("refresh-secret"));

    let error = resolve_codex_subscription_auth(&json!({
        "format": "auth_json",
        "tokens": { "refresh_token": "refresh-secret" }
    }))
    .expect_err("access token required");
    let display = format!("{error}");
    assert!(!display.contains("access-secret"));
    assert!(!display.contains("refresh-secret"));
}
