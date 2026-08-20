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

#[test]
fn oauth_other_top_level_access_token_resolves() {
    let auth = resolve_codex_subscription_auth(&json!({
        "access_token": "at-other"
    }))
    .expect("top-level OauthOther access_token should resolve");

    assert_eq!(auth.token(), "at-other");
}

#[test]
fn oauth_other_raw_access_token_resolves() {
    let auth = resolve_codex_subscription_auth(&json!({
        "raw": { "access_token": "at-from-raw" }
    }))
    .expect("/raw/access_token should resolve");

    assert_eq!(auth.token(), "at-from-raw");
}

#[test]
fn oauth_other_body_access_token_resolves() {
    let auth = resolve_codex_subscription_auth(&json!({
        "body": { "access_token": "at-from-body" }
    }))
    .expect("/body/access_token should resolve");

    assert_eq!(auth.token(), "at-from-body");
}

#[test]
fn empty_access_token_is_rejected() {
    let error = resolve_codex_subscription_auth(&json!({
        "access_token": ""
    }))
    .expect_err("empty access_token must fail");

    assert!(matches!(error, AppError::InvalidArg(_)));
    assert!(
        format!("{error}").contains("not auth_json"),
        "blank top-level token is not a Codex credential shape: {error}"
    );

    let error = resolve_codex_subscription_auth(&json!({
        "raw": { "access_token": "" },
        "body": { "access_token": "" }
    }))
    .expect_err("empty OauthOther pointers must fail");

    assert!(matches!(error, AppError::InvalidArg(_)));
}

#[test]
fn top_level_access_token_beats_tokens_access_token() {
    let auth = resolve_codex_subscription_auth(&json!({
        "access_token": "at-top",
        "tokens": { "access_token": "at-nested" }
    }))
    .expect("top-level pointer is first, matching normalize_oauth_credentials");

    assert_eq!(auth.token(), "at-top");
}

#[test]
fn whitespace_token_is_trimmed_and_blank_rejected() {
    let auth = resolve_codex_subscription_auth(&json!({
        "access_token": "  at-trimmed  "
    }))
    .expect("leading/trailing whitespace should be trimmed");
    assert_eq!(auth.token(), "at-trimmed");

    let error = resolve_codex_subscription_auth(&json!({
        "access_token": "   "
    }))
    .expect_err("whitespace-only token must fail");
    assert!(matches!(error, AppError::InvalidArg(_)));
}
