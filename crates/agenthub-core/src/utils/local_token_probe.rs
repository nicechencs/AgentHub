//! Loopback model-path check for a tokens-page entry key.
//!
//! Resolves a model from `GET /v1/models`, then `POST`s a tiny non-streaming
//! request on the row's surface. Never follows redirects off loopback.

use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::{json, Value};

use crate::utils::loopback::is_loopback_host;
use crate::utils::redact::redact_text;

const HTTP_TIMEOUT: Duration = Duration::from_secs(120);
const BODY_CHARS: usize = 2048;
const TEST_PROMPT: &str = "ping";
const TEST_MAX_TOKENS: u32 = 8;

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
    pub request_method: Option<String>,
    pub request_body: Option<String>,
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
        request_method: Option<String>,
        request_body: Option<String>,
        response_body: Option<String>,
        error_message: Option<String>,
    ) -> Self {
        Self {
            outcome,
            http_status,
            latency_ms,
            upstream_status,
            request_url,
            request_method,
            request_body,
            response_body,
            error_message,
        }
    }
}

/// Probe the conversation surface behind a loopback entry key.
///
/// `endpoint` may be `127.0.0.1:port` or a full loopback URL. `path` selects
/// `/v1/messages`, `/v1/responses`, or `/v1/chat/completions`. Remote hosts
/// fail closed as `invalid`.
pub fn probe_local_token(
    endpoint: &str,
    token: &str,
    path: &str,
    model: Option<&str>,
) -> LocalTokenProbeResult {
    let Some(origin) = loopback_origin(endpoint) else {
        return LocalTokenProbeResult::new(
            LocalTokenProbeOutcome::Invalid,
            None,
            0,
            None,
            None,
            None,
            None,
            None,
            Some("not a local endpoint".into()),
        );
    };
    let Some(surface) =
        conversation_path(path).or_else(|| conversation_path_from_endpoint(endpoint))
    else {
        return LocalTokenProbeResult::new(
            LocalTokenProbeOutcome::Invalid,
            None,
            0,
            None,
            None,
            None,
            None,
            None,
            Some("not a model path".into()),
        );
    };
    if token.trim().is_empty() {
        let url = format!("{origin}{surface}");
        return LocalTokenProbeResult::new(
            LocalTokenProbeOutcome::Invalid,
            None,
            0,
            None,
            Some(url),
            Some("POST".into()),
            None,
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
    let bearer = format!("Bearer {}", token.trim());
    let model = match model.map(str::trim).filter(|value| !value.is_empty()) {
        Some(model) => model.to_owned(),
        None => {
            let models_url = format!("{origin}/v1/models");
            let models = call_json(&agent, "GET", &models_url, &bearer, None);
            let models_latency = elapsed_ms(started);
            let models_result = classify_call(models, models_latency, &models_url, "GET", None);
            if models_result.outcome != LocalTokenProbeOutcome::Ok {
                log_probe(&models_result);
                return models_result;
            }
            match models_result
                .response_body
                .as_deref()
                .and_then(first_model_id)
            {
                Some(model) => model,
                None => {
                    let result = LocalTokenProbeResult::new(
                        LocalTokenProbeOutcome::Rejected,
                        models_result.http_status,
                        models_latency,
                        None,
                        Some(models_url),
                        Some("GET".into()),
                        None,
                        models_result.response_body,
                        Some("这条路由还没有可用模型".into()),
                    );
                    log_probe(&result);
                    return result;
                }
            }
        }
    };

    let url = format!("{origin}{surface}");
    let body = test_body(surface, &model);
    let body_text = body.to_string();
    let call = call_json(&agent, "POST", &url, &bearer, Some(&body));
    let latency_ms = elapsed_ms(started);
    let result = classify_call(call, latency_ms, &url, "POST", Some(body_text));
    log_probe(&result);
    result
}

fn call_json(
    agent: &ureq::Agent,
    method: &str,
    url: &str,
    bearer: &str,
    body: Option<&Value>,
) -> Result<ureq::Response, ureq::Error> {
    let mut request = if method == "POST" {
        agent.post(url)
    } else {
        agent.get(url)
    };
    request = request
        .set("Authorization", bearer)
        .set("Accept", "application/json");
    match body {
        Some(value) => request.send_json(value.clone()),
        None => request.call(),
    }
}

fn classify_call(
    call: Result<ureq::Response, ureq::Error>,
    latency_ms: u64,
    url: &str,
    method: &str,
    request_body: Option<String>,
) -> LocalTokenProbeResult {
    match call {
        Ok(response) => classify_status(
            response.status(),
            read_body(response.into_string().ok()),
            latency_ms,
            url,
            method,
            request_body,
        ),
        Err(ureq::Error::Status(status, response)) => classify_status(
            status as u16,
            read_body(response.into_string().ok()),
            latency_ms,
            url,
            method,
            request_body,
        ),
        Err(error) => LocalTokenProbeResult::new(
            LocalTokenProbeOutcome::Unreachable,
            None,
            latency_ms,
            None,
            Some(url.to_string()),
            Some(method.to_string()),
            request_body,
            None,
            Some(redact_text(&error.to_string())),
        ),
    }
}

fn classify_status(
    status: u16,
    body: Option<String>,
    latency_ms: u64,
    url: &str,
    method: &str,
    request_body: Option<String>,
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
        Some(url.to_string()),
        Some(method.to_string()),
        request_body,
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

fn first_model_id(raw: &str) -> Option<String> {
    let value: Value = serde_json::from_str(raw).ok()?;
    value
        .get("data")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find_map(|item| {
            item.get("id")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|id| !id.is_empty())
                .map(str::to_string)
        })
}

fn test_body(path: &str, model: &str) -> Value {
    match path {
        "/v1/messages" => json!({
            "model": model,
            "max_tokens": TEST_MAX_TOKENS,
            "stream": false,
            "messages": [{"role": "user", "content": TEST_PROMPT}]
        }),
        "/v1/responses" => json!({
            "model": model,
            "input": TEST_PROMPT,
            "max_output_tokens": TEST_MAX_TOKENS,
            "stream": false
        }),
        _ => json!({
            "model": model,
            "max_tokens": TEST_MAX_TOKENS,
            "stream": false,
            "messages": [{"role": "user", "content": TEST_PROMPT}]
        }),
    }
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

fn elapsed_ms(started: Instant) -> u64 {
    started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64
}

fn log_probe(result: &LocalTokenProbeResult) {
    tracing::info!(
        target: "core.adapter",
        op = "local_token_probe",
        url = result.request_url.as_deref().unwrap_or(""),
        method = result.request_method.as_deref().unwrap_or(""),
        outcome = ?result.outcome,
        http_status = ?result.http_status,
        latency_ms = result.latency_ms,
        "local gateway model test"
    );
}

fn conversation_path(path: &str) -> Option<&'static str> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return None;
    }
    let raw = if trimmed.contains("://") {
        reqwest::Url::parse(trimmed)
            .ok()
            .map(|url| url.path().to_string())
            .unwrap_or_else(|| trimmed.to_string())
    } else {
        trimmed.to_string()
    };
    match raw.trim().trim_end_matches('/') {
        "/v1/messages" | "v1/messages" | "/messages" | "messages" => Some("/v1/messages"),
        "/v1/responses" | "v1/responses" | "/responses" | "responses" => Some("/v1/responses"),
        "/v1/chat/completions"
        | "v1/chat/completions"
        | "/chat/completions"
        | "chat/completions" => Some("/v1/chat/completions"),
        _ => None,
    }
}

fn conversation_path_from_endpoint(endpoint: &str) -> Option<&'static str> {
    conversation_path(endpoint)
}

fn loopback_origin(endpoint: &str) -> Option<String> {
    let trimmed = endpoint.trim();
    if trimmed.is_empty() {
        return None;
    }
    let raw = if trimmed.contains("://") {
        trimmed.to_string()
    } else {
        format!("http://{trimmed}")
    };
    let Ok(url) = reqwest::Url::parse(&raw) else {
        return None;
    };
    if !matches!(url.scheme(), "http" | "https") {
        return None;
    }
    if !is_loopback_host(url.host_str()) {
        return None;
    }
    let host = url.host_str()?.to_string();
    let origin = match url.port() {
        Some(port) => format!("{}://{host}:{port}", url.scheme()),
        None => format!("{}://{host}", url.scheme()),
    };
    Some(origin)
}

#[cfg(test)]
mod tests;
