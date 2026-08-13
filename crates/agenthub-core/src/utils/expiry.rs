//! Shared token-expiry parsing and credential JSON key normalization.
//!
//! Accepts unix seconds/millis (number or numeric string), RFC3339, and naive
//! ISO datetimes. Values above the millis threshold are treated as milliseconds.

use serde_json::Value;

/// Epoch values with magnitude above this are treated as milliseconds.
const MILLIS_THRESHOLD: u64 = 1_000_000_000_000;

/// Normalize a credential JSON object key for case-/separator-insensitive matching.
///
/// `Access-Token` / `access.token` / `access_token` → `access_token`.
pub fn normalize_credential_key(raw_key: &str) -> String {
    raw_key.to_ascii_lowercase().replace(['-', '.'], "_")
}

/// Convert a unix timestamp that may be seconds or milliseconds into seconds.
pub fn normalize_epoch_secs(timestamp: i64) -> i64 {
    if timestamp.unsigned_abs() > MILLIS_THRESHOLD {
        timestamp / 1000
    } else {
        timestamp
    }
}

/// Current unix time in seconds.
pub fn now_unix_secs() -> i64 {
    chrono::Utc::now().timestamp()
}

/// Parse an expiry JSON value into unix seconds.
///
/// Returns `None` when the value is empty or unparseable.
pub fn parse_expiry_epoch_secs(value: &Value) -> Option<i64> {
    match value {
        Value::Number(number) => {
            // Prefer integer; fall back to f64 for JSON numbers that lost integer form.
            let timestamp = number
                .as_i64()
                .or_else(|| number.as_f64().map(|n| n as i64))?;
            Some(normalize_epoch_secs(timestamp))
        }
        Value::String(text) => {
            let text = text.trim();
            if text.is_empty() {
                return None;
            }
            if let Ok(number) = text.parse::<i64>() {
                return Some(normalize_epoch_secs(number));
            }
            if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(text) {
                return Some(dt.timestamp());
            }
            // Claude credentials sometimes store naive ISO timestamps.
            if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(text, "%Y-%m-%dT%H:%M:%S%.f") {
                return Some(dt.and_utc().timestamp());
            }
            None
        }
        _ => None,
    }
}

/// Whether `value` is an absolute expiry that has already passed (`<= now`).
///
/// Returns `None` when the value cannot be interpreted as an absolute expiry
/// (e.g. relative `expires_in`, empty string, non-timestamp types).
pub fn is_expired(value: &Value) -> Option<bool> {
    let secs = parse_expiry_epoch_secs(value)?;
    Some(secs <= now_unix_secs())
}

/// Seconds remaining until expiry (`expires - now`). Negative when already expired.
pub fn remaining_secs(value: &Value) -> Option<i64> {
    let secs = parse_expiry_epoch_secs(value)?;
    Some(secs - now_unix_secs())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn normalize_credential_key_collapses_case_and_separators() {
        assert_eq!(normalize_credential_key("Access-Token"), "access_token");
        assert_eq!(normalize_credential_key("expires.at"), "expires_at");
        assert_eq!(normalize_credential_key("refreshExpiresAt"), "refreshexpiresat");
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
}
