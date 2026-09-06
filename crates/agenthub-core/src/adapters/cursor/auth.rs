//! Read Cursor IDE login from `state.vscdb` `ItemTable` (`cursorAuth/*`).
//!
//! Import-only: never write the IDE store. Missing file, busy WAL, or unexpected
//! schema → `None` / `NotFound`, never panic.

use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::{Connection, OpenFlags};
use serde_json::{json, Map, Value};

use crate::error::{AppError, Result};
use crate::models::{AccountKind, AgentId, AuthHealth, AuthState, LiveAccount};
#[cfg(not(windows))]
use crate::utils::paths::home_dir;
use crate::utils::redact::mask_secret_preview;

const ACCESS_KEY: &str = "cursorAuth/accessToken";
const REFRESH_KEY: &str = "cursorAuth/refreshToken";
const EMAIL_KEY: &str = "cursorAuth/cachedEmail";
const SIGNUP_KEY: &str = "cursorAuth/cachedSignUpType";
const PROFILE_KEY: &str = "cursorAuth/cachedScopedProfile";

pub(super) fn read_cursor_live_account() -> Result<LiveAccount> {
    match load_cursor_auth()? {
        Some((body, source)) => Ok(live_account_from_auth(body, source)),
        None => Err(AppError::NotFound("no Cursor login found to import".into())),
    }
}

pub(super) fn cursor_oauth_auth_state() -> Option<AuthState> {
    let (_body, source) = load_cursor_auth().ok().flatten()?;
    Some(AuthState {
        agent: AgentId::Cursor,
        kind: Some("oauth".into()),
        summary: format!("Cursor login on this computer ({source})"),
        has_credentials: true,
        health: AuthHealth::Configured,
        source: Some(source.into()),
        revision: None,
        also_present: Vec::new(),
        secret_hash: None,
    })
}

pub(super) fn cursor_identity_label(
    credentials: &Value,
    label_hint: Option<&str>,
) -> Option<String> {
    crate::adapters::default_identity_label(AccountKind::Oauth, credentials, label_hint)
}

pub(super) fn live_account_from_auth(body: Value, source: &str) -> LiveAccount {
    let email = body
        .get("email")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let access = body
        .get("access_token")
        .and_then(Value::as_str)
        .unwrap_or("");
    let label = if let Some(email) = email {
        email.to_string()
    } else if access.is_empty() {
        "Cursor".into()
    } else {
        format!("Cursor ({})", mask_secret_preview(access))
    };
    LiveAccount {
        agent: AgentId::Cursor,
        kind: AccountKind::Oauth,
        credentials: json!({
            "format": "auth_json",
            "body": body,
        }),
        label_hint: Some(label),
        extra: json!({ "source": source }),
    }
}

pub(super) fn read_cursor_auth_from_sqlite(path: &Path) -> Option<Value> {
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .ok()?;
    let _ = conn.busy_timeout(Duration::from_millis(250));
    let access = query_item(&conn, ACCESS_KEY)?;
    if access.is_empty() {
        return None;
    }
    let mut out = Map::new();
    out.insert("access_token".into(), json!(access));
    if let Some(refresh) = query_item(&conn, REFRESH_KEY).filter(|s| !s.is_empty()) {
        out.insert("refresh_token".into(), json!(refresh));
    }
    if let Some(email) = query_item(&conn, EMAIL_KEY).filter(|s| !s.is_empty()) {
        out.insert("email".into(), json!(email));
    }
    if let Some(signup) = query_item(&conn, SIGNUP_KEY).filter(|s| !s.is_empty()) {
        out.insert("signup_type".into(), json!(signup));
    }
    if let Some(name) = query_display_name(&conn) {
        out.insert("display_name".into(), json!(name));
    }
    Some(Value::Object(out))
}

fn load_cursor_auth() -> Result<Option<(Value, &'static str)>> {
    for path in cursor_state_vscdb_candidates() {
        if !path.is_file() {
            continue;
        }
        if let Some(body) = read_cursor_auth_from_sqlite(&path) {
            return Ok(Some((body, "state.vscdb")));
        }
    }
    Ok(None)
}

fn cursor_state_vscdb_candidates() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(user) = cursor_ide_user_dir() {
        paths.push(user.join("globalStorage").join("state.vscdb"));
    }
    paths
}

fn cursor_ide_user_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        let appdata = std::env::var_os("APPDATA")?;
        return Some(PathBuf::from(appdata).join("Cursor").join("User"));
    }
    #[cfg(target_os = "macos")]
    {
        let home = home_dir().ok()?;
        return Some(
            home.join("Library")
                .join("Application Support")
                .join("Cursor")
                .join("User"),
        );
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let home = home_dir().ok()?;
        return Some(home.join(".config").join("Cursor").join("User"));
    }
    #[allow(unreachable_code)]
    None
}

fn query_item(conn: &Connection, key: &str) -> Option<String> {
    if let Ok(raw) = conn.query_row("SELECT value FROM ItemTable WHERE key = ?1", [key], |row| {
        row.get::<_, String>(0)
    }) {
        return Some(unquote_item(&raw));
    }
    let bytes: Vec<u8> = conn
        .query_row("SELECT value FROM ItemTable WHERE key = ?1", [key], |row| {
            row.get(0)
        })
        .ok()?;
    let raw = String::from_utf8(bytes).ok()?;
    Some(unquote_item(&raw))
}

fn query_display_name(conn: &Connection) -> Option<String> {
    let raw = query_item(conn, PROFILE_KEY)?;
    let parsed: Value = serde_json::from_str(&raw).ok()?;
    parsed
        .get("displayName")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned)
}

fn unquote_item(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.len() >= 2 && trimmed.starts_with('"') && trimmed.ends_with('"') {
        if let Ok(s) = serde_json::from_str::<String>(trimmed) {
            return s;
        }
    }
    trimmed.to_string()
}
