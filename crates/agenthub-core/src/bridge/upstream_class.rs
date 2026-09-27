//! Typed upstream errors for v2 indexed failover.
//!
//! HTTP status is an input, not the whole decision. v1 edges do not use this
//! classifier and keep mapping every non-401 through `map_upstream_http_error`.

use std::time::{Duration, SystemTime};

use axum::http::{HeaderValue, StatusCode};
use serde_json::Value;

#[cfg(test)]
mod tests;

/// Short cooldown for non-quota failures when `Retry-After` is missing or zero.
/// Transient / backoff-style errors stay here. They are not grown, and 2s is
/// under the 10 minute backoff ceiling (Magpie `longestRetry`).
pub const DEFAULT_COOLDOWN: Duration = Duration::from_secs(2);

/// Quota 429 with no `Retry-After`, body reset, or known window.
/// Magpie-inspired `quotaRest` (15m). Not a parity claim.
pub const QUOTA_DEFAULT_COOLDOWN: Duration = Duration::from_secs(15 * 60);

/// Credit exhaustion with no reset hint. Magpie-inspired `creditRest` (30m).
pub const CREDIT_DEFAULT_COOLDOWN: Duration = Duration::from_secs(30 * 60);

/// Hard cap on a vendor hint or snapshot window used as cooldown.
/// A 7d `resetAt` or a huge `Retry-After` must not park a member for days.
/// Chosen 1h, matching the spirit of Magpie `longestWait`. Class defaults
/// sit under this cap. Backoff-style stays at [`DEFAULT_COOLDOWN`] (≤10m).
pub const MAX_COOLDOWN: Duration = Duration::from_secs(60 * 60);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpstreamErrorClass {
    /// Ordinary 400 / 422, and policy 403. Request-scoped: no member switch.
    Request,
    /// Grok encrypted-reasoning 400. Same-member limited retry.
    GrokReasoningRecoverable,
    /// 401. Authorization-scoped: reload once, then isolate that authorization.
    Auth,
    /// Model / endpoint 403 or 404. Exclude this member for this model this request.
    Entitlement,
    /// Account-level 429. Member cooldown from `Retry-After`.
    QuotaAccount,
    /// Model-level 429. Cooldown only that member-model bucket.
    QuotaModel,
    /// 5xx, connect failure, timeout. Failover only before any downstream byte.
    Transient,
}

/// What the v2 loop should do with a classified attempt. Downstream commit
/// always wins: never replay after the client has any byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailoverDecision {
    ReturnToClient,
    RetrySameMember,
    ReloadThenFailover,
    ExcludeMemberModel,
    CooldownAndFailover,
    FailoverIfUncommitted,
}

impl UpstreamErrorClass {
    pub fn decision(self, downstream_committed: bool) -> FailoverDecision {
        if downstream_committed {
            return FailoverDecision::ReturnToClient;
        }
        match self {
            Self::Request => FailoverDecision::ReturnToClient,
            Self::GrokReasoningRecoverable => FailoverDecision::RetrySameMember,
            Self::Auth => FailoverDecision::ReloadThenFailover,
            Self::Entitlement => FailoverDecision::ExcludeMemberModel,
            Self::QuotaAccount | Self::QuotaModel => FailoverDecision::CooldownAndFailover,
            Self::Transient => FailoverDecision::FailoverIfUncommitted,
        }
    }

    pub fn allows_member_switch(self, downstream_committed: bool) -> bool {
        !matches!(
            self.decision(downstream_committed),
            FailoverDecision::ReturnToClient | FailoverDecision::RetrySameMember
        )
    }
}

pub fn classify_http(
    status: StatusCode,
    body: Option<&str>,
    grok_reasoning_recoverable: bool,
) -> UpstreamErrorClass {
    if grok_reasoning_recoverable && status == StatusCode::BAD_REQUEST {
        return UpstreamErrorClass::GrokReasoningRecoverable;
    }
    match status.as_u16() {
        401 => UpstreamErrorClass::Auth,
        400 | 422 => UpstreamErrorClass::Request,
        404 => UpstreamErrorClass::Entitlement,
        403 if is_entitlement_body(body) => UpstreamErrorClass::Entitlement,
        403 => UpstreamErrorClass::Request,
        429 if is_model_quota_body(body) => UpstreamErrorClass::QuotaModel,
        429 => UpstreamErrorClass::QuotaAccount,
        500..=599 => UpstreamErrorClass::Transient,
        _ => UpstreamErrorClass::Request,
    }
}

pub fn classify_connect_timeout() -> UpstreamErrorClass {
    UpstreamErrorClass::Transient
}

pub fn classify_connect_unavailable() -> UpstreamErrorClass {
    UpstreamErrorClass::Transient
}

fn haystack(body: Option<&str>) -> String {
    body.unwrap_or("").to_ascii_lowercase()
}

fn is_entitlement_body(body: Option<&str>) -> bool {
    let hay = haystack(body);
    hay.contains("model_not_found")
        || hay.contains("unknown model")
        || hay.contains("model not found")
        || hay.contains("invalid_model")
        || hay.contains("invalid model")
}

fn is_model_quota_body(body: Option<&str>) -> bool {
    let hay = haystack(body);
    hay.contains("per-model")
        || hay.contains("per model")
        || hay.contains("for this model")
        || hay.contains("tokens_per_model")
        || hay.contains("model_rate")
        || hay.contains("model rate limit")
}

pub fn parse_retry_after(value: &HeaderValue) -> Option<Duration> {
    let raw = value.to_str().ok()?.trim();
    if raw.is_empty() {
        return None;
    }
    if let Ok(seconds) = raw.parse::<u64>() {
        return Some(Duration::from_secs(seconds));
    }
    let when = parse_http_date(raw)?;
    when.duration_since(SystemTime::now()).ok()
}

fn parse_http_date(raw: &str) -> Option<SystemTime> {
    chrono::DateTime::parse_from_rfc2822(raw)
        .or_else(|_| chrono::DateTime::parse_from_str(raw, "%a, %d %b %Y %H:%M:%S GMT"))
        .ok()
        .map(|parsed| parsed.with_timezone(&chrono::Utc).into())
}

pub fn cooldown_from_retry_after(value: Option<&HeaderValue>) -> Duration {
    value
        .and_then(parse_retry_after)
        .filter(|duration| !duration.is_zero())
        .unwrap_or(DEFAULT_COOLDOWN)
}

/// Cooldown for one classified failure.
///
/// Non-quota classes keep [`cooldown_from_retry_after`] (header or 2s).
/// `QuotaAccount` / `QuotaModel` use, in order:
/// 1. HTTP `Retry-After` when it parses to a non-zero duration
/// 2. body reset hints (`resets_in_seconds` / `resets_at`, including `error.*`)
/// 3. the account snapshot reset (`reset_at`) when it is still in the future
/// 4. [`CREDIT_DEFAULT_COOLDOWN`] when the account or body is credit exhaustion,
///    otherwise [`QUOTA_DEFAULT_COOLDOWN`]
///
/// Steps 1–3 are clamped to [`MAX_COOLDOWN`]. A zero `Retry-After` falls
/// through; it does not collapse a quota failure back to 2s.
pub fn cooldown_for_class(
    class: UpstreamErrorClass,
    retry_after: Option<&HeaderValue>,
    body: Option<&str>,
    reset_at: Option<SystemTime>,
    credit_window: bool,
) -> Duration {
    if !matches!(
        class,
        UpstreamErrorClass::QuotaAccount | UpstreamErrorClass::QuotaModel
    ) {
        return cooldown_from_retry_after(retry_after);
    }
    if let Some(duration) = retry_after
        .and_then(parse_retry_after)
        .filter(|duration| !duration.is_zero())
    {
        return clamp_cooldown(duration);
    }
    if let Some(duration) = body.and_then(body_reset_after) {
        return clamp_cooldown(duration);
    }
    if let Some(reset_at) = reset_at {
        if let Ok(duration) = reset_at.duration_since(SystemTime::now()) {
            if !duration.is_zero() {
                return clamp_cooldown(duration);
            }
        }
    }
    let default = if credit_window || body_is_credit_exhaustion(body) {
        CREDIT_DEFAULT_COOLDOWN
    } else {
        QUOTA_DEFAULT_COOLDOWN
    };
    default.min(MAX_COOLDOWN)
}

fn clamp_cooldown(duration: Duration) -> Duration {
    if duration > MAX_COOLDOWN {
        MAX_COOLDOWN
    } else {
        duration
    }
}

/// Provider reset already shaped like the quota parsers: `resets_in_seconds`
/// or `resets_at` (unix seconds, unix millis, or RFC3339), at the top level
/// or under `error`.
fn body_reset_after(body: &str) -> Option<Duration> {
    let value: Value = serde_json::from_str(body.trim()).ok()?;
    reset_after_in(&value).or_else(|| value.get("error").and_then(reset_after_in))
}

fn reset_after_in(value: &Value) -> Option<Duration> {
    if let Some(secs) = json_u64(
        value
            .get("resets_in_seconds")
            .or_else(|| value.get("resetsInSeconds")),
    ) {
        if secs > 0 {
            return Some(Duration::from_secs(secs));
        }
    }
    let at = value.get("resets_at").or_else(|| value.get("resetsAt"))?;
    let when = json_reset_instant(at)?;
    let wait = when.duration_since(SystemTime::now()).ok()?;
    if wait.is_zero() {
        None
    } else {
        Some(wait)
    }
}

fn json_u64(value: Option<&Value>) -> Option<u64> {
    let value = value?;
    if let Some(n) = value.as_u64() {
        return Some(n);
    }
    if let Some(n) = value.as_i64() {
        return u64::try_from(n).ok();
    }
    if let Some(n) = value.as_f64() {
        if n.is_finite() && n > 0.0 {
            return Some(n as u64);
        }
    }
    value.as_str()?.trim().parse().ok()
}

fn json_reset_instant(value: &Value) -> Option<SystemTime> {
    if let Some(raw) = value.as_str() {
        return chrono::DateTime::parse_from_rfc3339(raw.trim())
            .ok()
            .and_then(|parsed| {
                let secs = parsed.timestamp();
                if secs < 0 {
                    None
                } else {
                    SystemTime::UNIX_EPOCH.checked_add(Duration::from_secs(secs as u64))
                }
            });
    }
    let n = value
        .as_i64()
        .or_else(|| value.as_f64().map(|n| n as i64))?;
    if n <= 0 {
        return None;
    }
    let secs = if n >= 100_000_000_000 { n / 1000 } else { n };
    if secs < 0 {
        return None;
    }
    SystemTime::UNIX_EPOCH.checked_add(Duration::from_secs(secs as u64))
}

fn body_is_credit_exhaustion(body: Option<&str>) -> bool {
    let hay = body.unwrap_or("").to_ascii_lowercase();
    hay.contains("insufficient_credit")
        || hay.contains("insufficient credit")
        || hay.contains("out of credit")
        || hay.contains("credit_exhausted")
        || hay.contains("credits exhausted")
        || hay.contains("credit balance")
}
