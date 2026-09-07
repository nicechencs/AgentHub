//! Obtain usable Kiro HTTP auth from Builder ID login and/or `KIRO_API_KEY`.
//!
//! Sources (first usable wins for chat):
//! 1. `KIRO_API_KEY` (`ksk_…`) — no refresh, `tokentype: API_KEY`
//! 2. kiro-cli sqlite `auth_kv` — social / odic tokens (+ device-registration for OIDC refresh)
//! 3. `~/.aws/sso/cache/kiro-auth-token.json` — desktop-style JSON (optional)

use std::path::Path;
use std::time::Duration;

use chrono::{DateTime, Utc};
use rusqlite::{Connection, OpenFlags};
use serde_json::{json, Map, Value};

use crate::error::{AppError, Result};
use crate::utils::paths::{kiro_cli_sqlite_path, kiro_sso_cache_path};

const SOCIAL_TOKEN_KEY: &str = "kirocli:social:token";
const ODIC_TOKEN_KEYS: &[&str] = &["kirocli:odic:token", "codewhisperer:odic:token"];
const ODIC_REG_KEYS: &[&str] = &[
    "kirocli:odic:device-registration",
    "codewhisperer:odic:device-registration",
];
const PROFILE_STATE_KEY: &str = "api.codewhisperer.profile";

const REFRESH_SKEW_SECS: i64 = 120;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum KiroAuthKind {
    ApiKey,
    /// Builder ID / social desktop refresh (`prod.*.auth.desktop.kiro.dev`).
    Desktop,
    /// Device-code / IdC OIDC refresh (`oidc.*.amazonaws.com/token`).
    Oidc,
}

#[derive(Debug, Clone)]
pub(crate) struct KiroHttpCreds {
    pub auth_kind: KiroAuthKind,
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub region: String,
    pub profile_arn: Option<String>,
    pub client_id: Option<String>,
    pub client_secret: Option<String>,
    pub origin: String,
    /// SQLite key to merge-write after refresh (odic/social).
    pub sqlite_token_key: Option<String>,
    #[allow(dead_code)]
    pub source: String,
}

impl KiroHttpCreds {
    pub(crate) fn needs_refresh(&self) -> bool {
        match self.auth_kind {
            KiroAuthKind::ApiKey => false,
            KiroAuthKind::Desktop | KiroAuthKind::Oidc => match self.expires_at {
                Some(exp) => {
                    let skew = chrono::Duration::seconds(REFRESH_SKEW_SECS);
                    Utc::now() + skew >= exp
                }
                None => true,
            },
        }
    }

    pub(crate) fn token_type_header(&self) -> Option<&'static str> {
        match self.auth_kind {
            KiroAuthKind::ApiKey => Some("API_KEY"),
            KiroAuthKind::Desktop | KiroAuthKind::Oidc => None,
        }
    }
}

/// Load the best available HTTP credential. Prefers `KIRO_API_KEY`, else local login.
pub(crate) fn load_kiro_http_creds() -> Result<KiroHttpCreds> {
    if let Ok(key) = std::env::var("KIRO_API_KEY") {
        let key = key.trim().to_string();
        if !key.is_empty() {
            let region = std::env::var("KIRO_API_REGION")
                .ok()
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
                .unwrap_or_else(|| "us-east-1".into());
            return Ok(KiroHttpCreds {
                auth_kind: KiroAuthKind::ApiKey,
                access_token: key,
                refresh_token: None,
                expires_at: None,
                region,
                profile_arn: None,
                client_id: None,
                client_secret: None,
                origin: "AI_EDITOR".into(),
                sqlite_token_key: None,
                source: "env:KIRO_API_KEY".into(),
            });
        }
    }

    if let Some(path) = kiro_cli_sqlite_path() {
        if path.is_file() {
            if let Some(creds) = load_from_sqlite(&path)? {
                return Ok(creds);
            }
        }
    }

    if let Some(path) = kiro_sso_cache_path() {
        if path.is_file() {
            if let Some(creds) = load_from_sso_cache(&path)? {
                return Ok(creds);
            }
        }
    }

    Err(AppError::NotFound(
        "no Kiro login or KIRO_API_KEY for HTTP".into(),
    ))
}

fn load_from_sqlite(path: &Path) -> Result<Option<KiroHttpCreds>> {
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;

    let mut token_key: Option<String> = None;
    let mut token_raw: Option<Value> = None;
    if let Some(v) = query_auth_kv(&conn, SOCIAL_TOKEN_KEY) {
        token_key = Some(SOCIAL_TOKEN_KEY.into());
        token_raw = Some(v);
    } else {
        for key in ODIC_TOKEN_KEYS {
            if let Some(v) = query_auth_kv(&conn, key) {
                token_key = Some((*key).into());
                token_raw = Some(v);
                break;
            }
        }
    }
    let Some(token_raw) = token_raw else {
        return Ok(None);
    };
    let Some(token_key) = token_key else {
        return Ok(None);
    };

    let access = string_field(&token_raw, &["access_token", "accessToken"])
        .ok_or_else(|| AppError::InvalidArg("Kiro login missing access_token".into()))?;
    let refresh = string_field(&token_raw, &["refresh_token", "refreshToken"]);
    let expires_at = string_field(&token_raw, &["expires_at", "expiresAt"]).and_then(|s| {
        parse_expires(&s)
    });
    let region = string_field(&token_raw, &["region"]).unwrap_or_else(|| "us-east-1".into());
    let mut profile_arn = string_field(&token_raw, &["profile_arn", "profileArn"]);

    let mut client_id = None;
    let mut client_secret = None;
    for key in ODIC_REG_KEYS {
        if let Some(reg) = query_auth_kv(&conn, key) {
            client_id = string_field(&reg, &["client_id", "clientId"]);
            client_secret = string_field(&reg, &["client_secret", "clientSecret"]);
            if client_id.is_some() && client_secret.is_some() {
                break;
            }
        }
    }

    if profile_arn.is_none() {
        if let Some(arn) = read_profile_arn_from_state(&conn) {
            profile_arn = Some(arn);
        }
    }

    let auth_kind = if client_id.is_some() && client_secret.is_some() {
        KiroAuthKind::Oidc
    } else {
        KiroAuthKind::Desktop
    };
    let origin = match auth_kind {
        KiroAuthKind::Oidc => "KIRO_CLI",
        KiroAuthKind::Desktop => "AI_EDITOR",
        KiroAuthKind::ApiKey => "AI_EDITOR",
    }
    .to_string();

    Ok(Some(KiroHttpCreds {
        auth_kind,
        access_token: access,
        refresh_token: refresh,
        expires_at,
        region,
        profile_arn,
        client_id,
        client_secret,
        origin,
        sqlite_token_key: Some(token_key.clone()),
        source: format!("data.sqlite3:{token_key}"),
    }))
}

fn load_from_sso_cache(path: &Path) -> Result<Option<KiroHttpCreds>> {
    let text = std::fs::read_to_string(path)?;
    let raw: Value = serde_json::from_str(&text)?;
    let access = match string_field(&raw, &["access_token", "accessToken"]) {
        Some(v) => v,
        None => return Ok(None),
    };
    let refresh = string_field(&raw, &["refresh_token", "refreshToken"]);
    let expires_at = string_field(&raw, &["expires_at", "expiresAt"]).and_then(|s| parse_expires(&s));
    let region = string_field(&raw, &["region"]).unwrap_or_else(|| "us-east-1".into());
    let profile_arn = string_field(&raw, &["profile_arn", "profileArn"]);
    let client_id = string_field(&raw, &["client_id", "clientId"]);
    let client_secret = string_field(&raw, &["client_secret", "clientSecret"]);
    let auth_kind = if client_id.is_some() && client_secret.is_some() {
        KiroAuthKind::Oidc
    } else {
        KiroAuthKind::Desktop
    };
    let origin = match auth_kind {
        KiroAuthKind::Oidc => "KIRO_CLI",
        _ => "AI_EDITOR",
    }
    .to_string();
    Ok(Some(KiroHttpCreds {
        auth_kind,
        access_token: access,
        refresh_token: refresh,
        expires_at,
        region,
        profile_arn,
        client_id,
        client_secret,
        origin,
        sqlite_token_key: None,
        source: "kiro-auth-token.json".into(),
    }))
}

pub(crate) fn persist_refreshed_token(creds: &KiroHttpCreds) -> Result<()> {
    let Some(key) = creds.sqlite_token_key.as_deref() else {
        return Ok(());
    };
    let Some(path) = kiro_cli_sqlite_path() else {
        return Ok(());
    };
    if !path.is_file() {
        return Ok(());
    }
    let conn = Connection::open(&path)?;
    let existing: String = match conn.query_row(
        "SELECT value FROM auth_kv WHERE key = ?1",
        [key],
        |row| row.get(0),
    ) {
        Ok(v) => v,
        Err(_) => return Ok(()),
    };
    let mut obj: Map<String, Value> = match serde_json::from_str::<Value>(&existing) {
        Ok(Value::Object(m)) => m,
        _ => Map::new(),
    };
    obj.insert("access_token".into(), json!(creds.access_token));
    if let Some(refresh) = &creds.refresh_token {
        obj.insert("refresh_token".into(), json!(refresh));
    }
    if let Some(exp) = creds.expires_at {
        obj.insert(
            "expires_at".into(),
            json!(exp.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)),
        );
    }
    obj.insert("region".into(), json!(creds.region));
    conn.execute(
        "UPDATE auth_kv SET value = ?1 WHERE key = ?2",
        rusqlite::params![Value::Object(obj).to_string(), key],
    )?;
    Ok(())
}

fn read_profile_arn_from_state(conn: &Connection) -> Option<String> {
    let raw: Vec<u8> = conn
        .query_row(
            "SELECT value FROM state WHERE key = ?1",
            [PROFILE_STATE_KEY],
            |row| row.get(0),
        )
        .ok()?;
    let text = String::from_utf8_lossy(&raw);
    let value: Value = serde_json::from_str(text.trim()).ok()?;
    string_field(&value, &["arn", "profileArn", "profile_arn"])
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

fn string_field(value: &Value, keys: &[&str]) -> Option<String> {
    let obj = value.as_object()?;
    for key in keys {
        if let Some(v) = obj
            .get(*key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            return Some(v.to_string());
        }
    }
    None
}

fn parse_expires(raw: &str) -> Option<DateTime<Utc>> {
    let trimmed = raw.trim();
    DateTime::parse_from_rfc3339(trimmed)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
        .or_else(|| {
            // kiro-cli may write nanoseconds; chrono RFC3339 maxes at micros.
            let truncated = truncate_frac_to_micros(trimmed);
            DateTime::parse_from_rfc3339(&truncated)
                .ok()
                .map(|dt| dt.with_timezone(&Utc))
        })
        .or_else(|| {
            chrono::NaiveDateTime::parse_from_str(trimmed, "%Y-%m-%dT%H:%M:%S%.fZ")
                .ok()
                .map(|dt| dt.and_utc())
        })
}

fn truncate_frac_to_micros(raw: &str) -> String {
    // 2026-09-07T01:08:11.937696853Z → …937696Z
    if let Some(dot) = raw.find('.') {
        let (head, rest) = raw.split_at(dot + 1);
        let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        let suffix: String = rest.chars().skip_while(|c| c.is_ascii_digit()).collect();
        if digits.len() > 6 {
            return format!("{head}{}{suffix}", &digits[..6]);
        }
    }
    raw.to_string()
}

pub(crate) fn http_timeout() -> Duration {
    Duration::from_secs(120)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_nanoseconds_for_chrono() {
        let s = truncate_frac_to_micros("2026-09-07T01:08:11.937696853Z");
        assert_eq!(s, "2026-09-07T01:08:11.937696Z");
        assert!(parse_expires("2026-09-07T01:08:11.937696853Z").is_some());
    }

    #[test]
    fn api_key_needs_no_refresh() {
        let creds = KiroHttpCreds {
            auth_kind: KiroAuthKind::ApiKey,
            access_token: "ksk_test".into(),
            refresh_token: None,
            expires_at: None,
            region: "us-east-1".into(),
            profile_arn: None,
            client_id: None,
            client_secret: None,
            origin: "AI_EDITOR".into(),
            sqlite_token_key: None,
            source: "env".into(),
        };
        assert!(!creds.needs_refresh());
        assert_eq!(creds.token_type_header(), Some("API_KEY"));
    }
}
