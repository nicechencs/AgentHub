//! Sub2API login helper — open a child webview on `{site}/login` and read tokens
//! from page localStorage via an initialization script + document.title bridge.
//!
//! Remote login pages often send `X-Frame-Options: DENY`, so an iframe dialog
//! cannot host them. A top-level WebviewWindow still works. When localStorage
//! cannot be read (restricted webview / user closed the window), the GUI falls
//! back to system-browser + paste-token.

use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

const WINDOW_LABEL: &str = "sub2api-login";
const TITLE_PREFIX: &str = "AH_SUB2API_AUTH:";
const POLL_MS: u64 = 500;
const TIMEOUT_SECS: u64 = 15 * 60;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Sub2ApiLoginTokens {
    pub access_token: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<u64>,
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

