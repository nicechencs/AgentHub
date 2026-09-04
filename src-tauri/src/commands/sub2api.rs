//! Sub2API helpers — child webview login + desktop HTTP (bypasses WebView CORS).
//!
//! Remote login pages often send `X-Frame-Options: DENY`, so an iframe dialog
//! cannot host them. A top-level WebviewWindow still works. When localStorage
//! cannot be read (restricted webview / user closed the window), the GUI falls
//! back to system-browser + paste-token.
//!
//! Native password login / public settings / keys use [`sub2api_http_request`]
//! (ureq) so PinCC and similar sites that omit CORS ACAO still work from the
//! desktop shell. Never log Authorization, passwords, tokens, or captcha proofs.

use std::collections::HashMap;
use std::io::Read;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

const WINDOW_LABEL: &str = "sub2api-login";
const TITLE_PREFIX: &str = "AH_SUB2API_AUTH:";
const POLL_MS: u64 = 500;
const TIMEOUT_SECS: u64 = 15 * 60;
const HTTP_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_BODY_BYTES: usize = 2 * 1024 * 1024;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Sub2ApiLoginTokens {
    pub access_token: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Sub2ApiHttpRequestArgs {
    pub method: String,
    pub url: String,
    #[serde(default)]
    pub headers: Option<HashMap<String, String>>,
    #[serde(default)]
    pub body: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Sub2ApiHttpResponse {
    pub status: u16,
    pub body: String,
}

fn init_script() -> String {
    format!(
        r#"(function () {{
  function read() {{
    try {{
      var t = localStorage.getItem('auth_token');
      if (!t) return null;
      return {{
        access_token: t,
        refresh_token: localStorage.getItem('refresh_token') || null,
        token_expires_at: localStorage.getItem('token_expires_at') || null
      }};
    }} catch (e) {{
      return null;
    }}
  }}
  function report() {{
    var data = read();
    if (!data) return;
    try {{
      document.title = '{prefix}' + encodeURIComponent(JSON.stringify(data));
    }} catch (e) {{}}
  }}
  try {{
    var proto = Storage.prototype;
    var _set = proto.setItem;
    proto.setItem = function (k, v) {{
      _set.apply(this, arguments);
      if (k === 'auth_token' || k === 'refresh_token' || k === 'token_expires_at') report();
    }};
  }} catch (e) {{}}
  setInterval(report, 800);
  report();
}})();"#,
        prefix = TITLE_PREFIX
    )
}

fn parse_title(title: &str) -> Option<Sub2ApiLoginTokens> {
    let rest = title.strip_prefix(TITLE_PREFIX)?;
    let decoded = urlencoding::decode(rest).ok()?.into_owned();
    let value: serde_json::Value = serde_json::from_str(&decoded).ok()?;
    let access = value.get("access_token")?.as_str()?.trim().to_string();
    if access.is_empty() {
        return None;
    }
    let refresh = value
        .get("refresh_token")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let expires_at = value
        .get("token_expires_at")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<u64>().ok())
        .filter(|n| *n > 0);
    Some(Sub2ApiLoginTokens {
        access_token: access,
        refresh_token: refresh,
        expires_at,
    })
}

fn validate_login_url(url: &str) -> Result<String, String> {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return Err("login url is empty".into());
    }
    let lower = trimmed.to_ascii_lowercase();
    if !(lower.starts_with("https://") || lower.starts_with("http://")) {
        return Err(format!("only http(s) login urls are allowed: {trimmed}"));
    }
    Ok(trimmed.to_string())
}

fn validate_http_url(url: &str) -> Result<String, String> {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return Err("url is empty".into());
    }
    let lower = trimmed.to_ascii_lowercase();
    if !(lower.starts_with("https://") || lower.starts_with("http://")) {
        return Err(format!("only http(s) urls are allowed: {trimmed}"));
    }
    if trimmed.contains(char::is_whitespace) {
        return Err("url contains whitespace".into());
    }
    Ok(trimmed.to_string())
}

/// Host+path only — never query/fragment (may carry secrets).
fn safe_path_for_log(url: &str) -> String {
    let without_frag = url.split('#').next().unwrap_or(url);
    let without_query = without_frag.split('?').next().unwrap_or(without_frag);
    let rest = without_query
        .strip_prefix("https://")
        .or_else(|| without_query.strip_prefix("http://"))
        .unwrap_or(without_query);
    rest.to_string()
}

#[allow(dead_code)]
fn is_sensitive_header(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower == "authorization"
        || lower == "cookie"
        || lower == "set-cookie"
        || lower == "x-api-key"
        || lower == "proxy-authorization"
}

/// Invoke: `sub2api_open_login` — open `{base}/login` webview and return tokens.
#[tauri::command]
pub async fn sub2api_open_login(
    app: AppHandle,
    login_url: String,
) -> Result<Sub2ApiLoginTokens, String> {
    let validated = validate_login_url(&login_url)?;
    let parsed = validated
        .parse()
        .map_err(|e| format!("invalid login url: {e}"))?;

    if let Some(existing) = app.get_webview_window(WINDOW_LABEL) {
        let _ = existing.close();
        tokio::time::sleep(Duration::from_millis(150)).await;
    }

    let window = WebviewWindowBuilder::new(&app, WINDOW_LABEL, WebviewUrl::External(parsed))
        .title("Sub2API")
        .inner_size(480.0, 760.0)
        .resizable(true)
        .initialization_script(&init_script())
        .build()
        .map_err(|e| format!("open login window failed: {e}"))?;

    let deadline = std::time::Instant::now() + Duration::from_secs(TIMEOUT_SECS);
    loop {
        if std::time::Instant::now() > deadline {
            let _ = window.close();
            return Err("login timed out".into());
        }
        if app.get_webview_window(WINDOW_LABEL).is_none() {
            return Err("cancelled".into());
        }
        if let Ok(title) = window.title() {
            if let Some(tokens) = parse_title(&title) {
                let _ = window.close();
                return Ok(tokens);
            }
        }
        tokio::time::sleep(Duration::from_millis(POLL_MS)).await;
    }
}

/// Invoke: `sub2api_close_login` — close the child login webview if open (dialog cancel).
#[tauri::command]
pub async fn sub2api_close_login(app: AppHandle) -> Result<(), String> {
    if let Some(existing) = app.get_webview_window(WINDOW_LABEL) {
        let _ = existing.close();
    }
    Ok(())
}

/// Invoke: `sub2api_http_request` — perform Sub2API HTTP from Rust (no WebView CORS).
///
/// Returns status + response text. Does not log Authorization, body secrets,
/// captcha tickets, or tokens.
#[tauri::command]
pub async fn sub2api_http_request(args: Sub2ApiHttpRequestArgs) -> Result<Sub2ApiHttpResponse, String> {
    let method = args.method.trim().to_ascii_uppercase();
    if method.is_empty() {
        return Err("method is empty".into());
    }
    if !matches!(
        method.as_str(),
        "GET" | "POST" | "PUT" | "PATCH" | "DELETE" | "HEAD"
    ) {
        return Err(format!("method not allowed: {method}"));
    }
    let url = validate_http_url(&args.url)?;
    let path_log = safe_path_for_log(&url);
    let method_log = method.clone();
    let path_log_outer = path_log.clone();
    let headers = args.headers.unwrap_or_default();
    let body = args.body;

    let result = tauri::async_runtime::spawn_blocking(move || -> Result<Sub2ApiHttpResponse, String> {
        let agent = ureq::AgentBuilder::new()
            .timeout(HTTP_TIMEOUT)
            .redirects(5)
            .try_proxy_from_env(true)
            .build();

        let mut req = match method.as_str() {
            "GET" => agent.get(&url),
            "POST" => agent.post(&url),
            "PUT" => agent.put(&url),
            "PATCH" => agent.request("PATCH", &url),
            "DELETE" => agent.request("DELETE", &url),
            "HEAD" => agent.request("HEAD", &url),
            _ => return Err(format!("method not allowed: {method}")),
        };

        for (name, value) in &headers {
            let key = name.trim();
            if key.is_empty() {
                continue;
            }
            req = req.set(key, value);
        }

        let call = if let Some(ref b) = body {
            req.send_string(b)
        } else {
            req.call()
        };

        match call {
            Ok(resp) => {
                let status = resp.status();
                let mut buf = String::new();
                resp.into_reader()
                    .take(MAX_BODY_BYTES as u64)
                    .read_to_string(&mut buf)
                    .map_err(|e| format!("read body failed: {e}"))?;
                Ok(Sub2ApiHttpResponse { status, body: buf })
            }
            Err(ureq::Error::Status(status, resp)) => {
                let mut buf = String::new();
                let _ = resp
                    .into_reader()
                    .take(MAX_BODY_BYTES as u64)
                    .read_to_string(&mut buf);
                Ok(Sub2ApiHttpResponse {
                    status,
                    body: buf,
                })
            }
            Err(ureq::Error::Transport(t)) => {
                Err(format!("network error: {t}"))
            }
        }
    })
    .await
    .map_err(|e| format!("http join error: {e}"))?;

    match &result {
        Ok(resp) => {
            tracing::info!(
                target: "gui.sub2api",
                op = "sub2api_http_request",
                method = %method_log,
                path = %path_log_outer,
                status = resp.status,
                "sub2api http ok"
            );
        }
        Err(err) => {
            tracing::warn!(
                target: "gui.sub2api",
                op = "sub2api_http_request",
                method = %method_log,
                path = %path_log_outer,
                error_kind = "network",
                "sub2api http failed: {err}"
            );
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_http_urls() {
        assert!(validate_http_url("file:///etc/passwd").is_err());
        assert!(validate_http_url("javascript:alert(1)").is_err());
        assert!(validate_http_url("https://v2.pincc.ai/api/v1/settings/public").is_ok());
    }

    #[test]
    fn safe_path_omits_query() {
        let p = safe_path_for_log("https://v2.pincc.ai/api/v1/auth/login?x=1");
        assert!(p.contains("v2.pincc.ai/api/v1/auth/login"));
        assert!(!p.contains("x=1"));
    }
}
