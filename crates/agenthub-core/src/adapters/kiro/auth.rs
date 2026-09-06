//! Read kiro-cli social login from the verified local stores.
//!
//! Canonical: `%LOCALAPPDATA%/Kiro-Cli/data.sqlite3` `auth_kv`.
//! Fallback: `~/.aws/sso/cache/kiro-auth-token.json`.

use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags};
use serde_json::{json, Map, Value};

use crate::error::{AppError, Result};
use crate::models::{AccountKind, AgentId, AuthHealth, AuthState, LiveAccount};
use crate::utils::paths::home_dir;
use crate::utils::redact::mask_secret_preview;

const SOCIAL_TOKEN_KEY: &str = "kirocli:social:token";

pub(super) fn read_kiro_live_account() -> Result<LiveAccount> {
    match load_kiro_token()? {
        Some((body, source)) => Ok(live_account_from_token_body(body, source)),
        None => Err(AppError::NotFound(
            "no kiro-cli login found to import".into(),
        )),
    }
}

pub(super) fn kiro_oauth_auth_state() -> Option<AuthState> {
    let (_body, source) = load_kiro_token().ok().flatten()?;
    Some(AuthState {
        agent: AgentId::Kiro,
        kind: Some("oauth".into()),
        summary: format!("kiro-cli login on this computer ({source})"),
        has_credentials: true,
        health: AuthHealth::Configured,
        source: Some(source.into()),
        revision: None,
        also_present: Vec::new(),
        secret_hash: None,
    })
}

pub(super) fn kiro_identity_label(
    credentials: &Value,
    label_hint: Option<&str>,
) -> Option<String> {
    if let Some(arn) = first_string(credentials, &["profile_arn", "profileArn"]) {
        return Some(arn);
    }
    if let Some(provider) = first_string(credentials, &["provider"]) {
        return Some(display_provider(&provider));
    }
    crate::adapters::default_identity_label(AccountKind::Oauth, credentials, label_hint)
}

pub(super) fn live_account_from_token_body(body: Value, source: &str) -> LiveAccount {
    let provider = body
        .get("provider")
        .and_then(Value::as_str)
        .map(display_provider)
        .unwrap_or_else(|| "Kiro".into());
    let access = body
        .get("access_token")
        .and_then(Value::as_str)
        .unwrap_or("");
    LiveAccount {
        agent: AgentId::Kiro,
        kind: AccountKind::Oauth,
        credentials: json!({
            "format": "auth_json",
            "body": body,
        }),
        label_hint: Some(if access.is_empty() {
            provider
        } else {
            format!("{} ({})", provider, mask_secret_preview(access))
        }),
        extra: json!({ "source": source }),
    }
}

pub(super) fn normalize_kiro_token(raw: &Value) -> Option<Value> {
    let obj = raw.as_object()?;
    let access = string_field(obj, &["access_token", "accessToken"])?;
    let mut out = Map::new();
    out.insert("access_token".into(), json!(access));
    if let Some(refresh) = string_field(obj, &["refresh_token", "refreshToken"]) {
        out.insert("refresh_token".into(), json!(refresh));
    }
    if let Some(expires) = string_field(obj, &["expires_at", "expiresAt"]) {
        out.insert("expires_at".into(), json!(expires));
    }
    if let Some(provider) = string_field(obj, &["provider"]) {
        out.insert("provider".into(), json!(provider.to_ascii_lowercase()));
    }
    if let Some(arn) = string_field(obj, &["profile_arn", "profileArn"]) {
        out.insert("profile_arn".into(), json!(arn));
    }
    if let Some(method) = string_field(obj, &["authMethod", "auth_method"]) {
        out.insert("auth_method".into(), json!(method));
    }
    Some(Value::Object(out))
}

pub(super) fn read_social_token_from_sqlite(path: &Path) -> Option<Value> {
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .ok()?;
    if let Some(value) = query_auth_kv(&conn, SOCIAL_TOKEN_KEY) {
        return normalize_kiro_token(&value);
    }
    let mut stmt = conn
        .prepare("SELECT key, value FROM auth_kv")
        .ok()?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })
    .ok()?;
    for row in rows.flatten() {
        let (key, raw) = row;
        if !key.starts_with("kirocli:") {
            continue;
        }
        let Ok(parsed) = serde_json::from_str::<Value>(&raw) else {
            continue;
        };
        if let Some(token) = normalize_kiro_token(&parsed) {
            return Some(token);
        }
    }
    None
}

fn load_kiro_token() -> Result<Option<(Value, &'static str)>> {
    for path in kiro_sqlite_candidates() {
        if !path.is_file() {
            continue;
        }
        if let Some(body) = read_social_token_from_sqlite(&path) {
            return Ok(Some((body, "data.sqlite3")));
        }
    }
    if let Some(path) = kiro_sso_cache_path() {
        if path.is_file() {
            let text = std::fs::read_to_string(&path)?;
            let raw: Value = serde_json::from_str(&text)?;
            if let Some(body) = normalize_kiro_token(&raw) {
                return Ok(Some((body, "kiro-auth-token.json")));
            }
        }
    }
    Ok(None)
}

fn kiro_sqlite_candidates() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    #[cfg(windows)]
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        paths.push(PathBuf::from(local).join("Kiro-Cli").join("data.sqlite3"));
    }
    #[cfg(target_os = "macos")]
    if let Ok(home) = home_dir() {
        paths.push(
            home.join("Library")
                .join("Application Support")
                .join("Kiro-Cli")
                .join("data.sqlite3"),
        );
    }
    #[cfg(not(windows))]
    if let Ok(home) = home_dir() {
        let xdg = std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".local").join("share"));
        paths.push(xdg.join("kiro-cli").join("data.sqlite3"));
        paths.push(home.join(".local").join("share").join("kiro-cli").join("data.sqlite3"));
    }
    paths
}

fn kiro_sso_cache_path() -> Option<PathBuf> {
    Some(
        home_dir()
            .ok()?
            .join(".aws")
            .join("sso")
            .join("cache")
            .join("kiro-auth-token.json"),
    )
}

fn query_auth_kv(conn: &Connection, key: &str) -> Option<Value> {
    let raw: String = conn
        .query_row(
            "SELECT value FROM auth_kv WHERE key = ?1",
            [key],
            |row| row.get(0),
        )
        .ok()?;
    serde_json::from_str(&raw).ok()
}

fn string_field(obj: &Map<String, Value>, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(value) = obj
            .get(*key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            return Some(value.to_string());
        }
    }
    None
}

fn first_string(credentials: &Value, keys: &[&str]) -> Option<String> {
    if let Some(obj) = credentials.as_object() {
        if let Some(value) = string_field(obj, keys) {
            return Some(value);
        }
    }
    credentials
        .get("body")
        .and_then(Value::as_object)
        .and_then(|obj| string_field(obj, keys))
}

fn display_provider(provider: &str) -> String {
    match provider.trim().to_ascii_lowercase().as_str() {
        "google" => "Google".into(),
        "github" => "GitHub".into(),
        other if other.is_empty() => "Kiro".into(),
        other => {
            let mut chars = other.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => "Kiro".into(),
            }
        }
    }
}
