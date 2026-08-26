//! Typed upstream errors for v2 indexed failover.
//!
//! HTTP status is an input, not the whole decision. v1 edges do not use this
//! classifier and keep mapping every non-401 through `map_upstream_http_error`.

use std::time::{Duration, SystemTime};

use axum::http::{HeaderValue, StatusCode};

#[cfg(test)]
mod tests;

/// Default member cooldown when `Retry-After` is missing or zero.
pub const DEFAULT_COOLDOWN: Duration = Duration::from_secs(2);

/// Proposal §8 classes. Scope is documented on each variant.
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
        || hay.contains("does not have access")
        || hay.contains("do not have access")
        || hay.contains("does not exist")
        || hay.contains("unknown model")
        || hay.contains("model not found")
        || hay.contains("not available in your")
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
