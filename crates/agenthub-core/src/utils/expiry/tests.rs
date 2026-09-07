use super::*;
use serde_json::json;

#[test]
fn normalize_credential_key_collapses_case_and_separators() {
    assert_eq!(normalize_credential_key("Access-Token"), "access_token");
    assert_eq!(normalize_credential_key("expires.at"), "expires_at");
    assert_eq!(
        normalize_credential_key("refreshExpiresAt"),
        "refreshexpiresat"
    );
}

#[test]
fn normalize_epoch_secs_detects_millis_threshold() {
    assert_eq!(normalize_epoch_secs(1_700_000_000), 1_700_000_000);
    assert_eq!(normalize_epoch_secs(1_700_000_000_000), 1_700_000_000);
    assert_eq!(normalize_epoch_secs(1_000), 1_000);
}

#[test]
fn parse_expiry_accepts_seconds_millis_rfc3339_and_naive_iso() {
    assert_eq!(parse_expiry_epoch_secs(&json!(1)), Some(1));
    assert_eq!(parse_expiry_epoch_secs(&json!(1_000_i64)), Some(1_000));
    assert_eq!(
        parse_expiry_epoch_secs(&json!(1_700_000_000_000_i64)),
        Some(1_700_000_000)
    );
    assert_eq!(
        parse_expiry_epoch_secs(&json!("1970-01-01T00:00:01Z")),
        Some(1)
    );
    assert_eq!(
        parse_expiry_epoch_secs(&json!("2000-01-01T00:00:00.000")),
        Some(946_684_800)
    );
    assert_eq!(parse_expiry_epoch_secs(&json!("")), None);
    assert_eq!(parse_expiry_epoch_secs(&json!(true)), None);
}

#[test]
fn is_expired_uses_inclusive_now_boundary() {
    assert_eq!(is_expired(&json!(1)), Some(true));
    assert_eq!(is_expired(&json!(9_999_999_999_i64)), Some(false));
    assert_eq!(is_expired(&json!("2000-01-01T00:00:00Z")), Some(true));
    assert_eq!(is_expired(&json!("2099-01-01T00:00:00.000Z")), Some(false));
    // Relative / non-absolute fields stay unparseable.
    assert_eq!(is_expired(&json!({"expires_in": 60})), None);
}

#[test]
fn remaining_secs_is_negative_when_expired() {
    let remaining = remaining_secs(&json!(1)).expect("parse");
    assert!(remaining < 0);
    let remaining = remaining_secs(&json!(9_999_999_999_i64)).expect("parse");
    assert!(remaining > 0);
}
