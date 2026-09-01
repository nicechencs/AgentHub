//! Loopback-only liveness check for a tokens-page entry key.
//!
//! `GET /health` with the displayed bearer. Does not send a model request and
//! never follows redirects off loopback.

use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::Value;

use crate::utils::loopback::is_loopback_host;
use crate::utils::redact::redact_text;

const HTTP_TIMEOUT: Duration = Duration::from_secs(5);
const BODY_CHARS: usize = 2048;

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
    pub request_url: Option<String>,
    pub response_body: Option<String>,
    pub error_message: Option<String>,
}

impl LocalTokenProbeResult {
    fn new(
        outcome: LocalTokenProbeOutcome,
        http_status: Option<u16>,
        latency_ms: u64,
        upstream_status: Option<String>,
        request_url: Option<String>,
        response_body: Option<String>,
        error_message: Option<String>,
    ) -> Self {
        Self {
            outcome,
            http_status,
            latency_ms,
            upstream_status,
            request_url,
            response_body,
            error_message,
        }
    }
}

/// Probe `GET {loopback}/health` with `Authorization: Bearer`.
///
/// `endpoint` may be `127.0.0.1:port` or a full loopback URL; the request always
/// uses `/health` on that origin. Remote hosts fail closed as `invalid`.
pub fn probe_local_token(endpoint: &str, token: &str) -> LocalTokenProbeResult {
    let Some(url) = loopback_health_url(endpoint) else {
        return LocalTokenProbeResult::new(
            LocalTokenProbeOutcome::Invalid,
            None,
            0,
            None,
            None,
            None,
            Some("not a local endpoint".into()),
        );
    };
    if token.trim().is_empty() {
        return LocalTokenProbeResult::new(
            LocalTokenProbeOutcome::Invalid,
            None,
            0,
            None,
            Some(url),
            None,
            Some("entry key is empty".into()),
        );
    }

    let started = Instant::now();
    let agent = ureq::AgentBuilder::new()
        .timeout(HTTP_TIMEOUT)
        .redirects(0)
        .try_proxy_from_env(false)
        .build();
    let call = agent
        .get(&url)
        .set("Authorization", &format!("Bearer {}", token.trim()))
        .set("Accept", "application/json")
        .call();
    let latency_ms = started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;

    let result = match call {
        Ok(response) => classify_response(response, latency_ms, url.clone()),
        Err(ureq::Error::Status(status, response)) => {
            classify_status(status, read_body(response.into_string().ok()), latency_ms, url.clone())
        }
        Err(error) => LocalTokenProbeResult::new(
            LocalTokenProbeOutcome::Unreachable,
            None,
            latency_ms,
            None,
            Some(url.clone()),
            None,
            Some(redact_text(&error.to_string())),
        ),
    };

    tracing::info!(
        target: "core.adapter",
        op = "local_token_probe",
        url = %url,
        outcome = ?result.outcome,
        http_status = ?result.http_status,
        latency_ms,
        "local entry key test"
    );
    result
}

fn classify_response(
    response: ureq::Response,
    latency_ms: u64,
    url: String,
) -> LocalTokenProbeResult {
    let status = response.status();
    classify_status(status, read_body(response.into_string().ok()), latency_ms, url)
}

fn classify_status(
    status: u16,
    body: Option<String>,
    latency_ms: u64,
    url: String,
) -> LocalTokenProbeResult {
    let outcome = if (200..300).contains(&status) {
        LocalTokenProbeOutcome::Ok
    } else if status == 401 {
        LocalTokenProbeOutcome::Unauthorized
    } else {
        LocalTokenProbeOutcome::Rejected
    };
    let upstream = body.as_deref().and_then(parse_upstream_status);
    LocalTokenProbeResult::new(
        outcome,
        Some(status),
        latency_ms,
        upstream,
        Some(url),
        body,
        None,
    )
}

fn parse_upstream_status(raw: &str) -> Option<String> {
    serde_json::from_str::<Value>(raw)
        .ok()
        .as_ref()
        .and_then(|value| value.get("upstream_status"))
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn read_body(raw: Option<String>) -> Option<String> {
    let text = raw?.trim().to_string();
    if text.is_empty() {
        return None;
    }
    let redacted = redact_text(&text);
    Some(truncate_chars(&redacted, BODY_CHARS))
}

fn truncate_chars(input: &str, max_chars: usize) -> String {
    let mut chars = input.chars();
    let taken: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{taken}…")
    } else {
        taken
    }
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
