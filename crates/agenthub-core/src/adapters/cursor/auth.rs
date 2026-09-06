//! Read Cursor logins from CLI `auth.json` and IDE `state.vscdb`.
//!
//! Import-only: never write either store. Missing file, busy WAL, or unexpected
//! schema → `None` / `NotFound`, never panic.
//!
//! CLI (`auth.json`) is what Cursor Agent actually runs. The window store
//! (`state.vscdb`) is a separate login and must stay labeled as such.

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
    let logins = collect_cursor_logins();
    match logins.as_slice() {
        [] => Err(AppError::NotFound("no Cursor login found to import".into())),
        [first] => Ok(live_account_from_auth(first.body.clone(), first.source)),
        _ => Ok(combined_cursor_snapshot(&logins)),
    }
}

pub(super) fn expand_cursor_live_accounts(snapshot: &LiveAccount) -> Vec<LiveAccount> {
    let Some(stores) = snapshot.extra.get("cursorStores").and_then(Value::as_array) else {
        return vec![snapshot.clone()];
    };
    let mut out = Vec::new();
    for store in stores {
        let Some(source) = store.get("source").and_then(Value::as_str) else {
            continue;
        };
        let Some(body) = store.get("body").cloned() else {
            continue;
        };
        if !body_has_access(&body) {
            continue;
        }
        out.push(live_account_from_auth(body, source));
    }
    if out.is_empty() {
        vec![snapshot.clone()]
    } else {
        out
    }
}

pub(super) fn cursor_oauth_auth_state() -> Option<AuthState> {
    let logins = collect_cursor_logins();
    let first = logins.first()?;
    let source = if logins.len() == 1 {
        first.source.to_string()
    } else {
        logins
            .iter()
            .map(|login| login.source)
            .collect::<Vec<_>>()
            .join("+")
    };
    Some(AuthState {
        agent: AgentId::Cursor,
        kind: Some("oauth".into()),
        summary: format!("Cursor login on this computer ({source})"),
        has_credentials: true,
        health: AuthHealth::Configured,
        source: Some(source),
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
            "cursorLoginKind": cursor_login_kind_from_source(source),
            "body": body,
        }),
        label_hint: Some(label),
        extra: json!({
            "source": source,
            "cursorLoginKind": cursor_login_kind_from_source(source),
        }),
    }
}

#[derive(Clone)]
struct CursorLogin {
    body: Value,
    source: &'static str,
}

fn collect_cursor_logins() -> Vec<CursorLogin> {
    let mut out = Vec::new();
    if let Some(mut body) = read_cursor_cli_auth() {
        if body_email(&body).is_none() {
            if let Some(email) = read_cursor_cli_email() {
                if let Some(obj) = body.as_object_mut() {
                    obj.insert("email".into(), json!(email));
                }
            }
        }
        out.push(CursorLogin {
            body,
            source: "auth.json",
        });
    }
    if let Some(body) = load_cursor_window_auth() {
        out.push(CursorLogin {
            body,
            source: "state.vscdb",
        });
    }
    out
}

fn combined_cursor_snapshot(logins: &[CursorLogin]) -> LiveAccount {
    if logins.len() >= 2 && same_cursor_grant(&logins[0].body, &logins[1].body) {
        let mut body = logins[0].body.clone();
        merge_cursor_identity(&mut body, &logins[1].body);
        let mut live = live_account_from_auth(body, logins[0].source);
        if let Some(obj) = live.credentials.as_object_mut() {
            obj.insert("cursorLoginKind".into(), json!("both"));
        }
        if let Some(obj) = live.extra.as_object_mut() {
            obj.insert("source".into(), json!("auth.json+state.vscdb"));
            obj.insert("cursorLoginKind".into(), json!("both"));
        }
        return live;
    }
    let mut live = live_account_from_auth(logins[0].body.clone(), logins[0].source);
    let stores: Vec<Value> = logins
        .iter()
        .map(|login| {
            json!({
                "source": login.source,
                "kind": cursor_login_kind_from_source(login.source),
                "body": login.body,
            })
        })
        .collect();
    if let Some(obj) = live.extra.as_object_mut() {
        obj.insert("cursorStores".into(), json!(stores));
    }
    live
}

fn cursor_login_kind_from_source(source: &str) -> &'static str {
    if source.contains("auth.json") && source.contains("state.vscdb") {
        "both"
    } else if source.contains("auth.json") {
        "cli"
    } else {
        "window"
    }
}

fn body_has_access(body: &Value) -> bool {
    body_string(body, &["access_token", "accessToken"])
        .map(|s| !s.is_empty())
        .unwrap_or(false)
}

fn body_email(body: &Value) -> Option<String> {
    body_string(body, &["email"]).filter(|s| !s.is_empty())
}

fn body_string(body: &Value, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(value) = body.get(*key).and_then(Value::as_str).map(str::trim) {
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

fn same_cursor_grant(left: &Value, right: &Value) -> bool {
    if let (Some(a), Some(b)) = (
        body_string(left, &["refresh_token", "refreshToken"]),
        body_string(right, &["refresh_token", "refreshToken"]),
    ) {
        return a == b;
    }
    match (
        body_string(left, &["access_token", "accessToken"]),
        body_string(right, &["access_token", "accessToken"]),
    ) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}

fn merge_cursor_identity(into: &mut Value, from: &Value) {
    let email_needed = body_email(into).is_none();
    let name_needed = body_string(into, &["display_name"]).is_none();
    let Some(obj) = into.as_object_mut() else {
        return;
    };
    if email_needed {
        if let Some(email) = body_email(from) {
            obj.insert("email".into(), json!(email));
        }
    }
    if name_needed {
        if let Some(name) = body_string(from, &["display_name"]) {
            obj.insert("display_name".into(), json!(name));
        }
    }
}

pub(super) fn read_cursor_auth_from_json(path: &Path) -> Option<Value> {
    let text = std::fs::read_to_string(path).ok()?;
    let parsed: Value = serde_json::from_str(&text).ok()?;
    let access = body_string(&parsed, &["access_token", "accessToken"])?;
    if access.is_empty() {
        return None;
    }
    let mut out = Map::new();
    out.insert("access_token".into(), json!(access));
    if let Some(refresh) = body_string(&parsed, &["refresh_token", "refreshToken"]) {
        out.insert("refresh_token".into(), json!(refresh));
    }
    if let Some(email) = body_email(&parsed) {
        out.insert("email".into(), json!(email));
    }
    Some(Value::Object(out))
}

fn read_cursor_cli_auth() -> Option<Value> {
    let path = cursor_cli_auth_json_path()?;
    if !path.is_file() {
        return None;
    }
    read_cursor_auth_from_json(&path)
}

fn read_cursor_cli_email() -> Option<String> {
    let path = crate::utils::paths::agent_home(AgentId::Cursor)
        .ok()?
        .join("cli-config.json");
    if !path.is_file() {
        return None;
    }
    let parsed: Value = serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()?;
    parsed
        .get("authInfo")
        .and_then(|info| info.get("email"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned)
}

fn cursor_cli_auth_json_path() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        let appdata = std::env::var_os("APPDATA")?;
        return Some(PathBuf::from(appdata).join("Cursor").join("auth.json"));
    }
    #[cfg(target_os = "macos")]
    {
        let home = home_dir().ok()?;
        return Some(home.join(".cursor").join("auth.json"));
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let home = home_dir().ok()?;
        let dir = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".config"));
        return Some(dir.join("cursor").join("auth.json"));
    }
    #[allow(unreachable_code)]
    None
}

fn load_cursor_window_auth() -> Option<Value> {
    for path in cursor_state_vscdb_candidates() {
        if !path.is_file() {
            continue;
        }
        if let Some(body) = read_cursor_auth_from_sqlite(&path) {
            return Some(body);
        }
    }
    None
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
