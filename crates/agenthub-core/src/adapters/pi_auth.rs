//! Pi `~/.pi/agent/auth.json` helpers.
//!
//! Pi stores multi-provider credentials as a top-level object:
//! ```json
//! {
//!   "anthropic": { "type": "oauth", "access": "...", "refresh": "...", "expires": 123 },
//!   "openai-codex": { "type": "oauth", "access": "...", "refresh": "...", "expires": 123 },
//!   "xai": { "type": "oauth", "access": "...", "refresh": "...", "expires": 123 },
//!   "openai": { "type": "api_key", "key": "sk-..." }
//! }
//! ```
//!
//! AgentHub keeps one pool row per provider key so Connections can show each login.

use std::path::Path;

use serde_json::{json, Map, Value};

use crate::error::{AppError, Result};
use crate::models::{AccountKind, AgentId, LiveAccount};
use crate::oauth::{
    apply_identity_to_credentials, extract_oauth_identity, identity_from_credentials, OAuthIdentity,
};
use crate::utils::atomic::atomic_write;
use crate::utils::paths::agent_config_dir;
use crate::utils::redact::mask_secret_preview;

/// Built-in Pi subscription OAuth keys (order used for UI).
pub const PI_OAUTH_PROVIDER_KEYS: &[&str] = &[
    "anthropic",
    "openai-codex",
    "xai",
    "github-copilot",
    "openrouter",
    "kimi-coding",
    "radius",
];

/// Official Pi `auth.json` slots for API-key entries
/// (https://pi.dev/docs/latest/providers). Custom providers belong in
/// `models.json`, not here.
pub const PI_AUTH_JSON_SLOTS: &[&str] = &[
    "anthropic",
    "ant-ling",
    "azure-openai-responses",
    "openai",
    "deepseek",
    "nvidia",
    "google",
    "amazon-bedrock",
];

pub fn is_pi_auth_json_slot(id: &str) -> bool {
    PI_AUTH_JSON_SLOTS.contains(&id)
}

/// `{ "type": "api_key", "key": "…" }` — official auth.json entry shape.
pub fn pi_api_key_auth_entry(key: &str) -> Value {
    json!({
        "type": "api_key",
        "key": key,
    })
}

pub fn pi_config_dir() -> Result<std::path::PathBuf> {
    agent_config_dir(AgentId::Pi)
}

pub fn pi_auth_path() -> Result<std::path::PathBuf> {
    Ok(pi_config_dir()?.join("auth.json"))
}

/// Read auth.json object (empty object when missing).
pub fn read_auth_json() -> Result<Value> {
    read_auth_json_at(&pi_auth_path()?)
}

fn read_auth_json_at(path: &Path) -> Result<Value> {
    if !path.exists() {
        return Ok(json!({}));
    }
    let text = std::fs::read_to_string(path)?;
    let body: Value = serde_json::from_str(&text)?;
    if !body.is_object() {
        return Err(AppError::InvalidArg(
            "Pi auth.json must be a JSON object".into(),
        ));
    }
    Ok(body)
}

/// Merge `patch` keys into existing auth.json (provider-level merge).
pub fn merge_auth_json(patch: &Value) -> Result<Value> {
    merge_auth_json_in_dir(&pi_config_dir()?, patch)
}

fn merge_auth_json_in_dir(dir: &Path, patch: &Value) -> Result<Value> {
    let mut base = read_auth_json_at(&dir.join("auth.json"))?;
    let patch_obj = patch
        .as_object()
        .ok_or_else(|| AppError::InvalidArg("Pi auth.json patch must be a JSON object".into()))?;
    let base_obj = base
        .as_object_mut()
        .ok_or_else(|| AppError::InvalidArg("Pi auth.json must be a JSON object".into()))?;
    for (k, v) in patch_obj {
        base_obj.insert(k.clone(), v.clone());
    }
    Ok(base)
}

pub(crate) fn write_verified_auth_json(path: &Path, body: &Value) -> Result<()> {
    let mut bytes = serde_json::to_vec_pretty(body)?;
    bytes.push(b'\n');
    atomic_write(path, &bytes)?;
    let written = std::fs::read_to_string(path)?;
    let parsed: Value = serde_json::from_str(&written)?;
    if parsed != *body {
        return Err(AppError::message(
            "account.verify",
            "Pi auth.json verification failed after write",
        ));
    }
    Ok(())
}

/// Write `{ provider: { type: api_key, key } }` into `dir/auth.json`.
/// Does not touch other provider keys. `dir` is the Pi config dir
/// (`~/.pi/agent` or `PI_CODING_AGENT_DIR`).
pub(crate) fn apply_pi_api_key_to_dir(dir: &Path, provider: &str, key: &str) -> Result<()> {
    let provider = provider.trim();
    if provider.is_empty() {
        return Err(AppError::InvalidArg(
            "Pi API key apply requires an official provider slot (anthropic/openai/…)".into(),
        ));
    }
    if !is_pi_auth_json_slot(provider) {
        return Err(AppError::InvalidArg(format!(
            "Pi API key apply does not write custom slot '{provider}'; \
             custom slots belong in models.json / provider switch, not this account apply"
        )));
    }
    let key = key.trim();
    if key.is_empty() {
        return Err(AppError::InvalidArg("Pi api_key is required".into()));
    }
    let mut patch = Map::new();
    patch.insert(provider.to_string(), pi_api_key_auth_entry(key));
    let merged = merge_auth_json_in_dir(dir, &Value::Object(patch))?;
    write_verified_auth_json(&dir.join("auth.json"), &merged)
}

/// Expand full auth.json into one LiveAccount per provider key.
pub fn expand_auth_to_live_accounts(body: &Value) -> Result<Vec<LiveAccount>> {
    let obj = body
        .as_object()
        .ok_or_else(|| AppError::InvalidArg("Pi auth.json must be a JSON object".into()))?;
    if obj.is_empty() {
        return Err(AppError::NotFound(
            "Pi auth.json has no provider credentials".into(),
        ));
    }

    let mut accounts = Vec::new();
    // Prefer stable order: known OAuth keys first, then remaining keys sorted.
    let mut keys: Vec<String> = obj.keys().cloned().collect();
    keys.sort_by(|a, b| {
        let ia = PI_OAUTH_PROVIDER_KEYS
            .iter()
            .position(|k| *k == a.as_str())
            .unwrap_or(usize::MAX);
        let ib = PI_OAUTH_PROVIDER_KEYS
            .iter()
            .position(|k| *k == b.as_str())
            .unwrap_or(usize::MAX);
        ia.cmp(&ib).then_with(|| a.cmp(b))
    });

    for key in keys {
        let entry = obj.get(&key).cloned().unwrap_or(Value::Null);
        accounts.push(live_account_for_provider(&key, &entry)?);
    }
    Ok(accounts)
}

/// Build a pool-ready LiveAccount for a single provider entry.
pub fn live_account_for_provider(provider: &str, entry: &Value) -> Result<LiveAccount> {
    let kind = infer_entry_kind(entry);
    let identity = identity_from_provider_entry(provider, entry);
    let label = identity
        .display_label()
        .map(|s| format!("pi:{provider} · {s}"))
        .unwrap_or_else(|| fallback_provider_label(provider, entry));

    let mut body = Map::new();
    body.insert(provider.to_string(), entry.clone());

    let mut cred_map = Map::new();
    cred_map.insert("format".into(), json!("auth_json"));
    cred_map.insert("provider".into(), json!(provider));
    cred_map.insert("body".into(), Value::Object(body));

    // Flatten tokens so default authorization_key / identity_label work.
    if let Some(access) = entry_access(entry) {
        cred_map.insert("access_token".into(), json!(access));
    }
    if let Some(refresh) = entry_refresh(entry) {
        cred_map.insert("refresh_token".into(), json!(refresh));
    }
    if let Some(key) = entry_api_key(entry) {
        cred_map.insert("api_key".into(), json!(key));
    }
    if let Some(exp) = entry_expires_rfc3339(entry) {
        cred_map.insert("expires_at".into(), json!(exp));
    }
    apply_identity_to_credentials(&mut cred_map, &identity);

    let mut extra = Map::new();
    extra.insert("source".into(), json!("auth.json"));
    extra.insert("provider".into(), json!(provider));
    if let Some(ref email) = identity.email {
        extra.insert("email".into(), json!(email));
        extra.insert("identityLabel".into(), json!(email));
    } else if let Some(lab) = identity.display_label() {
        extra.insert("identityLabel".into(), json!(lab));
    } else {
        extra.insert("identityLabel".into(), json!(format!("pi:{provider}")));
    }
    if let Some(ref plan) = identity.subscription {
        extra.insert("subscription".into(), json!(plan));
    }
    if let Some(exp) = entry_expires_rfc3339(entry) {
        extra.insert("expiresAt".into(), json!(exp));
    }

    Ok(LiveAccount {
        agent: AgentId::Pi,
        kind,
        credentials: Value::Object(cred_map),
        label_hint: Some(label),
        extra: Value::Object(extra),
    })
}

/// Convert a standard OAuth TokenBundle-like tokens into Pi auth.json entry.
pub fn pi_oauth_entry_from_tokens(
    access: &str,
    refresh: Option<&str>,
    expires_at_rfc3339: Option<&str>,
    expires_in_secs: Option<i64>,
) -> Value {
    let expires_ms = expires_at_to_ms(expires_at_rfc3339).or_else(|| {
        expires_in_secs.map(|s| {
            chrono::Utc::now().timestamp_millis() + s.max(0) * 1000
                - crate::catalog::limits::OAUTH_REFRESH_SKEW_MS
        })
    });
    let mut m = Map::new();
    m.insert("type".into(), json!("oauth"));
    m.insert("access".into(), json!(access));
    if let Some(r) = refresh.filter(|s| !s.is_empty()) {
        m.insert("refresh".into(), json!(r));
    }
    if let Some(exp) = expires_ms {
        m.insert("expires".into(), json!(exp));
    }
    Value::Object(m)
}

/// Build LiveAccount after a successful Pi provider OAuth login.
pub fn live_account_from_oauth_tokens(
    provider: &str,
    access: &str,
    refresh: Option<&str>,
    expires_at_rfc3339: Option<&str>,
    expires_in_secs: Option<i64>,
    id_token: Option<&str>,
) -> Result<LiveAccount> {
    let entry = pi_oauth_entry_from_tokens(access, refresh, expires_at_rfc3339, expires_in_secs);
    // Enrich identity from JWT claims before packaging.
    let mut identity = extract_oauth_identity(provider, &entry, Some(access), id_token);
    if identity.is_empty() {
        identity = identity_from_provider_entry(provider, &entry);
    }
    let mut live = live_account_for_provider(provider, &entry)?;
    if let Some(obj) = live.credentials.as_object_mut() {
        apply_identity_to_credentials(obj, &identity);
    }
    if let Some(obj) = live.extra.as_object_mut() {
        if let Some(ref email) = identity.email {
            obj.insert("email".into(), json!(email));
            obj.insert("identityLabel".into(), json!(email));
            live.label_hint = Some(format!("pi:{provider} · {email}"));
        }
        if let Some(ref plan) = identity.subscription {
            obj.insert("subscription".into(), json!(plan));
        }
        obj.insert("source".into(), json!("oauth_pkce"));
    }
    Ok(live)
}

fn identity_from_provider_entry(provider: &str, entry: &Value) -> OAuthIdentity {
    let access = entry_access(entry);
    let id_token = entry
        .get("id_token")
        .or_else(|| entry.get("idToken"))
        .and_then(|v| v.as_str());
    let mut id = extract_oauth_identity(provider, entry, access.as_deref(), id_token);
    // Also try nested account.email for some providers.
    if id.email.is_none() {
        if let Some(email) = entry
            .get("email")
            .or_else(|| entry.pointer("/account/email"))
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            id.email = Some(email.to_string());
        }
    }
    if id.is_empty() {
        id = identity_from_credentials(entry);
    }
    id
}

fn infer_entry_kind(entry: &Value) -> AccountKind {
    match entry.get("type").and_then(|t| t.as_str()) {
        Some("oauth") => AccountKind::Oauth,
        Some("api_key") | Some("apikey") | Some("api-key") => AccountKind::ApiKey,
        _ => {
            if entry_access(entry).is_some() || entry_refresh(entry).is_some() {
                AccountKind::Oauth
            } else if entry_api_key(entry).is_some() {
                AccountKind::ApiKey
            } else {
                AccountKind::Oauth
            }
        }
    }
}

fn entry_access(entry: &Value) -> Option<String> {
    entry
        .get("access")
        .or_else(|| entry.get("access_token"))
        .or_else(|| entry.get("accessToken"))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

fn entry_refresh(entry: &Value) -> Option<String> {
    entry
        .get("refresh")
        .or_else(|| entry.get("refresh_token"))
        .or_else(|| entry.get("refreshToken"))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

fn entry_api_key(entry: &Value) -> Option<String> {
    entry
        .get("key")
        .or_else(|| entry.get("api_key"))
        .or_else(|| entry.get("apiKey"))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

fn entry_expires_rfc3339(entry: &Value) -> Option<String> {
    if let Some(s) = entry
        .get("expires_at")
        .or_else(|| entry.get("expiresAt"))
        .and_then(|v| v.as_str())
    {
        return Some(s.to_string());
    }
    // Pi uses millisecond epoch under `expires`.
    if let Some(ms) = entry.get("expires").and_then(|v| v.as_i64()) {
        if ms > 0 {
            let secs = ms / 1000;
            if let Some(dt) = chrono::DateTime::from_timestamp(secs, 0) {
                return Some(dt.to_rfc3339());
            }
        }
    }
    None
}

fn expires_at_to_ms(expires_at: Option<&str>) -> Option<i64> {
    let s = expires_at?;
    chrono::DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|dt| dt.timestamp_millis())
}

fn fallback_provider_label(provider: &str, entry: &Value) -> String {
    let ty =
        entry
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or(if entry_api_key(entry).is_some() {
                "api_key"
            } else {
                "oauth"
            });
    if let Some(key) = entry_api_key(entry) {
        return format!("pi:{provider} · {} ({ty})", mask_secret_preview(&key));
    }
    format!("pi:{provider} ({ty})")
}

/// Combined live snapshot (full auth.json) for switch/apply of a whole file.
pub fn combined_live_account(body: &Value) -> Result<LiveAccount> {
    let accounts = expand_auth_to_live_accounts(body)?;
    let kind = if accounts.iter().all(|a| a.kind == AccountKind::ApiKey) {
        AccountKind::ApiKey
    } else {
        AccountKind::Oauth
    };
    let providers: Vec<&str> = body
        .as_object()
        .map(|o| o.keys().map(|s| s.as_str()).collect())
        .unwrap_or_default();
    let label = if providers.len() == 1 {
        accounts
            .first()
            .and_then(|a| a.label_hint.clone())
            .unwrap_or_else(|| format!("pi:{}", providers[0]))
    } else {
        format!("pi:{} providers", providers.len())
    };
    Ok(LiveAccount {
        agent: AgentId::Pi,
        kind,
        credentials: json!({
            "format": "auth_json",
            "body": body,
        }),
        label_hint: Some(label),
        extra: json!({
            "source": "auth.json",
            "providerCount": providers.len(),
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand_splits_multi_provider_auth() {
        let body = json!({
            "xai": { "type": "oauth", "access": "a1", "refresh": "r1", "expires": 1785682457104i64 },
            "anthropic": { "type": "oauth", "access": "a2", "refresh": "r2", "expires": 1785682457104i64 },
            "openai": { "type": "api_key", "key": "sk-test-key-123456" }
        });
        let accounts = expand_auth_to_live_accounts(&body).unwrap();
        assert_eq!(accounts.len(), 3);
        assert!(accounts
            .iter()
            .any(|a| { a.credentials.get("provider").and_then(|v| v.as_str()) == Some("xai") }));
        assert!(accounts.iter().any(|a| {
            a.credentials.get("provider").and_then(|v| v.as_str()) == Some("anthropic")
        }));
        let openai = accounts
            .iter()
            .find(|a| a.credentials.get("provider").and_then(|v| v.as_str()) == Some("openai"))
            .unwrap();
        assert_eq!(openai.kind, AccountKind::ApiKey);
    }

    #[test]
    fn oauth_entry_shape_matches_pi() {
        let entry = pi_oauth_entry_from_tokens("at", Some("rt"), None, Some(3600));
        assert_eq!(entry.get("type").and_then(|v| v.as_str()), Some("oauth"));
        assert_eq!(entry.get("access").and_then(|v| v.as_str()), Some("at"));
        assert_eq!(entry.get("refresh").and_then(|v| v.as_str()), Some("rt"));
        assert!(entry.get("expires").and_then(|v| v.as_i64()).unwrap() > 0);
    }

    #[test]
    fn live_account_flattens_refresh_for_auth_key() {
        let entry = json!({
            "type": "oauth",
            "access": "acc",
            "refresh": "ref-token-xyz",
            "expires": 1785682457104i64
        });
        let live = live_account_for_provider("xai", &entry).unwrap();
        assert_eq!(
            live.credentials
                .get("refresh_token")
                .and_then(|v| v.as_str()),
            Some("ref-token-xyz")
        );
        assert_eq!(
            live.credentials.get("provider").and_then(|v| v.as_str()),
            Some("xai")
        );
        assert!(live.label_hint.unwrap().contains("xai"));
    }
}
