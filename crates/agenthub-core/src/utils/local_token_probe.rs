//! Loopback-only liveness check for a tokens-page entry key.
//!
//! `GET /health` with the displayed bearer. Does not send a model request and
//! never follows redirects off loopback.

use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::Value;

use crate::utils::loopback::is_loopback_host;

const HTTP_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalTokenProbeOutcome {
    Ok,
    Unauthorized,
    Unreachable,
    Rejected,
    Invalid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalTokenProbeResult {
    pub outcome: LocalTokenProbeOutcome,
    pub http_status: Option<u16>,
    pub latency_ms: u64,
    pub upstream_status: Option<String>,
}

impl LocalTokenProbeResult {
    fn new(
        outcome: LocalTokenProbeOutcome,
        http_status: Option<u16>,
        latency_ms: u64,
        upstream_status: Option<String>,
    ) -> Self {
        Self {
            outcome,
            http_status,
            latency_ms,
            upstream_status,
        }
    }
}

/// Probe `GET {loopback}/health` with `Authorization: Bearer`.
///
/// `endpoint` may be `127.0.0.1:port` or a full loopback URL; the request always
/// uses `/health` on that origin. Remote hosts fail closed as `invalid`.
pub fn probe_local_token(endpoint: &str, token: &str) -> LocalTokenProbeResult {
    let Some(url) = loopback_health_url(endpoint) else {
        return LocalTokenProbeResult::new(LocalTokenProbeOutcome::Invalid, None, 0, None);
    };
    if token.trim().is_empty() {
        return LocalTokenProbeResult::new(LocalTokenProbeOutcome::Invalid, None, 0, None);
    }

    let started = Instant::now();
    let agent = ureq::AgentBuilder::new()
        .timeout(HTTP_TIMEOUT)
        .redirects(0)
        .build();
    let call = agent
        .get(&url)
        .set("Authorization", &format!("Bearer {}", token.trim()))
        .set("Accept", "application/json")
        .call();
    let latency_ms = started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;

    match call {
        Ok(response) => {
            let status = response.status();
            let upstream = parse_upstream_status(response.into_json().ok());
            classify_http(status, latency_ms, upstream)
        }
        Err(ureq::Error::Status(status, response)) => {
            let upstream = parse_upstream_status(response.into_json().ok());
            classify_http(status, latency_ms, upstream)
        }
        Err(_) => LocalTokenProbeResult::new(
            LocalTokenProbeOutcome::Unreachable,
            None,
            latency_ms,
            None,
        ),
    }
}

fn classify_http(
    status: u16,
    latency_ms: u64,
    upstream_status: Option<String>,
) -> LocalTokenProbeResult {
    let outcome = if (200..300).contains(&status) {
        LocalTokenProbeOutcome::Ok
    } else if status == 401 {
        LocalTokenProbeOutcome::Unauthorized
    } else {
        LocalTokenProbeOutcome::Rejected
    };
    LocalTokenProbeResult::new(outcome, Some(status), latency_ms, upstream_status)
}

fn parse_upstream_status(body: Option<Value>) -> Option<String> {
    body.as_ref()
        .and_then(|value| value.get("upstream_status"))
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn loopback_health_url(endpoint: &str) -> Option<String> {
    let trimmed = endpoint.trim();
    if trimmed.is_empty() {
        return None;
    }
    let raw = if trimmed.contains("://") {
        trimmed.to_string()
    } else {
        format!("http://{trimmed}")
    };
    let Ok(mut url) = reqwest::Url::parse(&raw) else {
        return None;
    };
    if !matches!(url.scheme(), "http" | "https") {
        return None;
    }
    if !is_loopback_host(url.host_str()) {
        return None;
    }
    url.set_path("/health");
    url.set_query(None);
    url.set_fragment(None);
    Some(url.to_string())
}

#[cfg(test)]
mod tests;
