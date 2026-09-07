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
mod tests;
