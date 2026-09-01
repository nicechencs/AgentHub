//! OpenAI-compatible `GET {base}/v1/models` helpers.
//!
//! URL normalize + JSON parse are mirrored in
//! `src/lib/provider-detect/remote-models.ts`. Live HTTP is desktop-only
//! (Tauri command); unit tests here do not touch the network.

use std::collections::HashSet;
use std::time::Duration;

use serde_json::Value;

use crate::error::{AppError, Result};
use crate::utils::redact::redact_text;

const HTTP_TIMEOUT: Duration = Duration::from_secs(8);

/// API endpoint shapes the connection form can configure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApiEndpointType {
    Messages,
    Responses,
    ChatCompletions,
}

impl ApiEndpointType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Messages => "messages",
            Self::Responses => "responses",
            Self::ChatCompletions => "chat_completions",
        }
    }

    fn path(self) -> &'static str {
        match self {
            Self::Messages => "messages",
            Self::Responses => "responses",
            Self::ChatCompletions => "chat/completions",
        }
    }
}

/// Build GET URL for `{base}/v1/models`, collapsing a trailing `/v1`.
pub fn openai_models_url(base_url: &str) -> String {
    let trimmed = base_url.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let stripped = trimmed.trim_end_matches('/').trim_end_matches("/anthropic");
    let stripped = stripped.trim_end_matches('/');
    if let Ok(url) = reqwest::Url::parse(stripped) {
        if url
            .host_str()
            .is_some_and(|host| host.eq_ignore_ascii_case("api.deepseek.com"))
        {
            return format!("{stripped}/models");
        }
    }
    let ends_with_v1 = stripped
        .rsplit_once('/')
        .is_some_and(|(_, last)| last.eq_ignore_ascii_case("v1"));
    if ends_with_v1 {
        format!("{stripped}/models")
    } else {
        format!("{stripped}/v1/models")
    }
}

fn origin_of(base_url: &str) -> Option<String> {
    let url = reqwest::Url::parse(base_url.trim()).ok()?;
    let host = url.host_str()?;
    let origin = match url.port() {
        Some(port) => format!("{}://{host}:{port}", url.scheme()),
        None => format!("{}://{host}", url.scheme()),
    };
    Some(origin)
}

fn push_models_url(urls: &mut Vec<String>, url: String) {
    if url.is_empty() {
        return;
    }
    if urls
        .iter()
        .any(|existing| existing.eq_ignore_ascii_case(&url))
    {
        return;
    }
    urls.push(url);
}

/// Candidate model-list URLs: `{base}/v1/models`, then host `/v1/models` and `/models`.
pub fn openai_models_urls(base_url: &str) -> Vec<String> {
    let mut urls = Vec::new();
    push_models_url(&mut urls, openai_models_url(base_url));
    if let Some(origin) = origin_of(base_url) {
        push_models_url(&mut urls, format!("{origin}/v1/models"));
        push_models_url(&mut urls, format!("{origin}/models"));
    }
    urls
}

fn api_endpoint_url(base_url: &str, endpoint: ApiEndpointType) -> String {
    let trimmed = base_url.trim().trim_end_matches('/');
    let has_v1 = trimmed
        .rsplit_once('/')
        .is_some_and(|(_, last)| last.eq_ignore_ascii_case("v1"));
    if has_v1 {
        format!("{trimmed}/{}", endpoint.path())
    } else {
        format!("{trimmed}/v1/{}", endpoint.path())
    }
}

fn push_model_id(out: &mut Vec<String>, seen: &mut HashSet<String>, raw: Option<&str>) {
    let Some(raw) = raw else { return };
    let id = raw.trim();
    if id.is_empty() {
        return;
    }
    if seen.insert(id.to_string()) {
        out.push(id.to_string());
    }
}

fn push_from_array(out: &mut Vec<String>, seen: &mut HashSet<String>, items: &[Value]) {
    for item in items {
        match item {
            Value::String(s) => push_model_id(out, seen, Some(s.as_str())),
            Value::Object(map) => {
                let id = map.get("id").and_then(Value::as_str);
                push_model_id(out, seen, id);
            }
            _ => {}
        }
    }
}

/// Accept `{data:[{id}]}`, `data: string[]`, `{models:string[]}`,
/// `{models:[{id}]}`, or a top-level array. Dedupe, first-seen order.
pub fn parse_openai_model_list(input: &Value) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    match input {
        Value::Array(items) => push_from_array(&mut out, &mut seen, items),
        Value::Object(map) => {
            if let Some(Value::Array(items)) = map.get("data") {
                push_from_array(&mut out, &mut seen, items);
            }
            if let Some(Value::Array(items)) = map.get("models") {
                push_from_array(&mut out, &mut seen, items);
            }
        }
        _ => {}
    }
    out
}

fn looks_http_url(base: &str) -> bool {
    let lower = base.to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://")
}

fn probe_api_endpoint(base_url: &str, api_key: &str, endpoint: ApiEndpointType) -> bool {
    let url = api_endpoint_url(base_url, endpoint);
    let mut request = ureq::post(&url)
        .set("Authorization", &format!("Bearer {api_key}"))
        .set("Accept", "application/json")
        .set("Content-Type", "application/json")
        .timeout(HTTP_TIMEOUT);
    if endpoint == ApiEndpointType::Messages {
        request = request
            .set("x-api-key", api_key)
            .set("anthropic-version", "2023-06-01");
    }
    match request.send_string("{}") {
        Ok(_) => true,
        Err(ureq::Error::Status(code, _)) => code != 404 && code != 405,
        Err(_) => false,
    }
}

/// Probe known API paths with an empty request body. A 400/401/422 means the
/// endpoint exists; only 404/405 rule it out. No model request is sent.
pub fn detect_api_endpoint_types(base_url: &str, api_key: &str) -> Result<Vec<String>> {
    let base = base_url.trim();
    if base.is_empty() {
        return Err(AppError::InvalidArg("base URL is required".into()));
    }
    if !looks_http_url(base) {
        return Err(AppError::InvalidArg("base URL must be http(s)".into()));
    }
    let key = api_key.trim();
    if key.is_empty() {
        return Err(AppError::InvalidArg("API key is required".into()));
    }

    Ok([
        ApiEndpointType::Messages,
        ApiEndpointType::Responses,
        ApiEndpointType::ChatCompletions,
    ]
    .into_iter()
    .filter(|endpoint| probe_api_endpoint(base, key, *endpoint))
    .map(ApiEndpointType::as_str)
    .map(str::to_string)
    .collect())
}

fn fetch_openai_model_list(url: &str, api_key: &str) -> Result<Vec<String>> {
    let response = ureq::get(url)
        .set("Authorization", &format!("Bearer {api_key}"))
        .set("Accept", "application/json")
        .timeout(HTTP_TIMEOUT)
        .call()
        .map_err(|err| {
            AppError::message(
                "remote_openai_models.http",
                redact_text(&format!("GET {url} failed: {err}")),
            )
        })?;
    let body: Value = response.into_json().map_err(|err| {
        AppError::message(
            "remote_openai_models.json",
            redact_text(&format!("invalid JSON from {url}: {err}")),
        )
    })?;
    Ok(parse_openai_model_list(&body))
}

/// GET `{base}/v1/models`, then host `/v1/models` and `/models`. No saved provider id.
pub fn list_remote_openai_models(base_url: &str, api_key: &str) -> Result<Vec<String>> {
    let base = base_url.trim();
    if base.is_empty() {
        return Err(AppError::InvalidArg("base URL is required".into()));
    }
    if !looks_http_url(base) {
        return Err(AppError::InvalidArg("base URL must be http(s)".into()));
    }
    let key = api_key.trim();
    if key.is_empty() {
        return Err(AppError::InvalidArg("API key is required".into()));
    }

    let urls = openai_models_urls(base);
    if urls.is_empty() {
        return Err(AppError::InvalidArg("base URL is required".into()));
    }
    let mut last_error = None;
    let mut empty = Vec::new();
    for url in urls {
        match fetch_openai_model_list(&url, key) {
            Ok(ids) if ids.is_empty() => empty = ids,
            Ok(ids) => return Ok(ids),
            Err(err) => last_error = Some(err),
        }
    }
    match last_error {
        Some(err) if empty.is_empty() => Err(err),
        _ => Ok(empty),
    }
}

#[cfg(test)]
mod tests;
