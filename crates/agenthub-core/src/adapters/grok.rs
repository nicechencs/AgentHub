use std::path::{Path, PathBuf};

use serde_json::{json, Map, Value};

use crate::error::{AppError, Result};
use crate::models::{
    AccountKind, AgentConfig, AgentId, AuthState, Capability, CapabilityState, DetectResult,
    LiveAccount, RunOptions, RunSpec,
};
use crate::runtime;
use crate::utils::atomic::atomic_write;
use crate::utils::grok_toml::{
    active_model_alias, ensure_grok_model_shape, EnsureGrokModelShapeOptions,
};
use crate::utils::paths::{agent_home, home_dir};
use crate::utils::redact::mask_secret_preview;
use toml_edit::{DocumentMut, Item};

use super::{
    api_key_live_account, auth_file_revision, auth_files_revision, detect_binary,
    inspect_auth_credentials, oauth_auth_health, require_api_key, write_toml_config,
    write_verified_json_object, AgentAdapter,
};

pub struct GrokAdapter;

/// Standalone install probe used by platform detectors (no full adapter required).
pub(crate) fn detect_installation() -> DetectResult {
    let requires = crate::catalog::install::adapter_install_channels(AgentId::Grok)
        .first()
        .map(|c| c.requires.clone())
        .unwrap_or_default();
    let env_ready = runtime::is_ready(&requires);
    detect_binary(
        AgentId::Grok,
        &["grok"],
        &["--version"],
        Some("native"),
        env_ready,
    )
}

impl AgentAdapter for GrokAdapter {
    fn id(&self) -> AgentId {
        AgentId::Grok
    }

    fn detect(&self) -> DetectResult {
        detect_installation()
    }

    fn read_config(&self) -> Result<AgentConfig> {
        let path = agent_home(AgentId::Grok)?.join("config.toml");
        let raw = if path.exists() {
            let text = std::fs::read_to_string(&path)?;
            serde_json::json!({ "format": "toml", "content": text })
        } else {
            serde_json::json!({})
        };
        Ok(AgentConfig {
            agent: AgentId::Grok,
            raw,
        })
    }

    fn write_config(&self, config: &AgentConfig) -> Result<()> {
        let path = agent_home(AgentId::Grok)?.join("config.toml");
        write_toml_config(AgentId::Grok, &path, config)
    }

    fn read_auth(&self) -> Result<AuthState> {
        let home = agent_home(AgentId::Grok)?;
        grok_auth_state(&home.join("config.toml"), &home.join("auth.json"))
    }

    fn read_account(&self) -> Result<LiveAccount> {
        let home = agent_home(AgentId::Grok)?;
        let auth_path = home.join("auth.json");
        let config_path = home.join("config.toml");
        let api_key = read_grok_api_key(&config_path)?;
        let auth_body = if auth_path.exists() {
            let text = std::fs::read_to_string(&auth_path)?;
            Some(serde_json::from_str::<serde_json::Value>(&text)?)
        } else {
            None
        };

        match (api_key.as_deref(), auth_body) {
            (Some(key), Some(body)) if !key.is_empty() => Ok(LiveAccount {
                agent: AgentId::Grok,
                kind: AccountKind::ApiKey,
                credentials: serde_json::json!({
                    "format": "grok_bundle",
                    "api_key": key,
                    "auth": body,
                }),
                label_hint: Some(format!("{} (API Key)", mask_secret_preview(key))),
                extra: serde_json::json!({ "source": "config.toml+auth.json" }),
            }),
            (Some(key), None) if !key.is_empty() => Ok(LiveAccount {
                agent: AgentId::Grok,
                kind: AccountKind::ApiKey,
                credentials: serde_json::json!({
                    "format": "api_key",
                    "api_key": key,
                }),
                label_hint: Some(format!("{} (API Key)", mask_secret_preview(key))),
                extra: serde_json::json!({ "source": "config.toml" }),
            }),
            (_, Some(body)) => Ok(LiveAccount {
                agent: AgentId::Grok,
                kind: AccountKind::Oauth,
                credentials: serde_json::json!({
                    "format": "auth_json",
                    "body": body,
                }),
                label_hint: Some("grok-oauth".into()),
                extra: serde_json::json!({ "source": "auth.json" }),
            }),
            _ => Err(AppError::NotFound(
                "no live Grok api_key or auth.json found to import".into(),
            )),
        }
    }

    fn apply_account(&self, account: &LiveAccount) -> Result<()> {
        if account.agent != AgentId::Grok {
            return Err(AppError::InvalidArg(
                "account agent mismatch for grok".into(),
            ));
        }
        let home = agent_home(AgentId::Grok)?;
        let format = account
            .credentials
            .get("format")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        match format {
            "api_key" => {
                let key = account
                    .credentials
                    .get("api_key")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| AppError::InvalidArg("Grok api_key is required".into()))?;
                write_grok_api_key(&home.join("config.toml"), key)?;
                verify_grok_field(&home.join("config.toml"), "api_key", key)?;
                Ok(())
            }
            "auth_json" | "" | "oauth" => {
                let auth_path = home.join("auth.json");
                let body = grok_auth_json_body_from_credentials(&account.credentials, &auth_path)?;
                write_verified_json_object(&auth_path, &body)?;
                // Official OAuth must win over leftover inline credentials.
                clear_grok_field(&home.join("config.toml"), "api_key")?;
                // Relay base_url would keep traffic off official endpoint.
                clear_grok_field(&home.join("config.toml"), "base_url")?;
                Ok(())
            }
            "grok_bundle" => {
                if let Some(key) = account.credentials.get("api_key").and_then(|v| v.as_str()) {
                    write_grok_api_key(&home.join("config.toml"), key)?;
                    verify_grok_field(&home.join("config.toml"), "api_key", key)?;
                }
                if let Some(body) = account.credentials.get("auth") {
                    write_verified_json_object(&home.join("auth.json"), body)?;
                }
                Ok(())
            }
            other => Err(AppError::InvalidArg(format!(
                "unsupported Grok account credential format: {other}"
            ))),
        }
    }

    fn build_api_key_account(&self, api_key: &str) -> Result<LiveAccount> {
        let key = require_api_key(api_key)?;
        Ok(api_key_live_account(
            AgentId::Grok,
            key,
            serde_json::json!({
                "format": "api_key",
                "api_key": key,
            }),
            "API Key",
            serde_json::json!({ "source": "manual" }),
        ))
    }

    fn skills_dir(&self) -> Option<PathBuf> {
        home_dir().ok().map(|h| h.join(".grok").join("skills"))
    }

    fn capability(&self, cap: Capability) -> CapabilityState {
        use Capability::*;
        match cap {
            ConfigWrite | AccountSwitch | ApiKeyAccount | Skills | LiveBackup
            | StructuredStream | DangerousMode | ProjectHistory | ProjectDelete
            | ProviderPresets => CapabilityState::full(),
            Usage => CapabilityState::full(),
            Mcp | ModelSelect | SessionResume => CapabilityState::planned("待验证接入"),
        }
    }

    fn live_backup_paths(&self) -> Vec<PathBuf> {
        let mut paths = Vec::new();
        if let Ok(home) = agent_home(AgentId::Grok) {
            paths.push(home.join("config.toml"));
            paths.push(home.join("auth.json"));
        }
        paths
    }

    fn build_run_spec(&self, binary: &Path, prompt: &str, opts: &RunOptions) -> Result<RunSpec> {
        // text: grok -p <prompt>
        // structured (Chat): --output-format streaming-json (ACP NDJSON ≥ 0.2.117)
        // --no-auto-update: same guard Grok App uses so a mid-turn CLI
        // self-update cannot kill the headless child. Old CLIs (< 0.2.117)
        // reject the flag, so only emit it when version is unknown or modern.
        let args = grok_cli_args(prompt, opts, self.detect().version.as_deref());
        Ok(RunSpec {
            agent: AgentId::Grok,
            program: binary.to_path_buf(),
            args,
            cwd: opts.cwd.clone(),
            env: vec![],
        })
    }
}

/// Flatten a Hub PKCE bundle into official `auth.json` object shape, patching
/// an existing nested profile when that file is already the same grant.
fn grok_auth_json_body_from_credentials(credentials: &Value, auth_path: &Path) -> Result<Value> {
    if let Some(body) = credentials.get("body").filter(|body| body.is_object()) {
        return Ok(body.clone());
    }
    let access = first_oauth_string(
        credentials,
        &["access_token", "accessToken", "access", "key"],
    );
    let refresh = first_oauth_string(credentials, &["refresh_token", "refreshToken", "refresh"]);
    if access.is_none() && refresh.is_none() {
        return Err(AppError::InvalidArg(
            "Grok OAuth apply requires access_token or refresh_token".into(),
        ));
    }
    let existing = if auth_path.is_file() {
        std::fs::read_to_string(auth_path)
            .ok()
            .and_then(|text| serde_json::from_str::<Value>(&text).ok())
            .filter(|body| body.is_object())
    } else {
        None
    };
    if let Some(mut body) = existing {
        patch_grok_oauth_secrets(&mut body, access.as_deref(), refresh.as_deref());
        if first_oauth_string(&body, &["refresh_token", "refreshToken", "refresh"]).is_some()
            || first_oauth_string(&body, &["access_token", "accessToken", "access", "key"])
                .is_some()
        {
            return Ok(body);
        }
    }
    let mut map = Map::new();
    if let Some(access) = access {
        map.insert("access_token".into(), json!(access));
    }
    if let Some(refresh) = refresh {
        map.insert("refresh_token".into(), json!(refresh));
    }
    for key in ["email", "user_id", "sub"] {
        if let Some(value) = credentials.get(key).cloned() {
            if !value.is_null() {
                map.insert(key.into(), value);
            }
        }
    }
    Ok(Value::Object(map))
}

fn first_oauth_string(value: &Value, keys: &[&str]) -> Option<String> {
    match value {
        Value::Object(map) => {
            for key in keys {
                if let Some(s) = map
                    .get(*key)
                    .and_then(|v| v.as_str())
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                {
                    return Some(s.to_string());
                }
            }
            for nested in map.values() {
                if nested.is_object() || nested.is_array() {
                    if let Some(found) = first_oauth_string(nested, keys) {
                        return Some(found);
                    }
                }
            }
            None
        }
        Value::Array(items) => items.iter().find_map(|item| first_oauth_string(item, keys)),
        _ => None,
    }
}

fn patch_grok_oauth_secrets(value: &mut Value, access: Option<&str>, refresh: Option<&str>) {
    let Value::Object(map) = value else {
        if let Value::Array(items) = value {
            for item in items {
                patch_grok_oauth_secrets(item, access, refresh);
            }
        }
        return;
    };
    let looks_oauth = map.keys().any(|key| {
        let lower = key.to_ascii_lowercase();
        matches!(
            lower.as_str(),
            "refresh_token"
                | "refreshtoken"
                | "refresh"
                | "email"
                | "user_id"
                | "userid"
                | "access_token"
                | "accesstoken"
        )
    });
    for (key, nested) in map.iter_mut() {
        if nested.is_string() {
            let lower = key.to_ascii_lowercase();
            if let Some(rt) = refresh {
                if lower == "refresh_token" || lower == "refreshtoken" || lower == "refresh" {
                    *nested = json!(rt);
                }
            }
            if let Some(at) = access {
                if lower == "access_token" || lower == "accesstoken" || lower == "access" {
                    *nested = json!(at);
                } else if looks_oauth && lower == "key" {
                    *nested = json!(at);
                }
            }
        } else if nested.is_object() || nested.is_array() {
            patch_grok_oauth_secrets(nested, access, refresh);
        }
    }
}

/// `--no-auto-update` exists on Grok CLI ≥ 0.2.117.
/// Unparseable / missing versions keep the modern default (include the flag).
fn grok_supports_no_auto_update(version: Option<&str>) -> bool {
    let Some(raw) = version.map(str::trim).filter(|s| !s.is_empty()) else {
        return true;
    };
    let token = crate::adapters::extract_version_token(raw);
    match semver::Version::parse(&token) {
        Ok(parsed) => parsed >= semver::Version::new(0, 2, 117),
        Err(_) => true,
    }
}

fn grok_cli_args(prompt: &str, opts: &RunOptions, version: Option<&str>) -> Vec<String> {
    let mut args = Vec::new();
    if grok_supports_no_auto_update(version) {
        args.push("--no-auto-update".into());
    }
    args.push("-p".into());
    args.push(prompt.to_string());
    if super::wants_structured_for(opts.process_mode, AgentId::Grok) {
        args.push("--output-format".into());
        args.push("streaming-json".into());
    }
    if opts.allow_dangerous {
        args.insert(0, "--always-approve".into());
    }
    args
}

pub(crate) fn grok_auth_state(config: &Path, auth: &Path) -> Result<AuthState> {
    if read_grok_api_key(config)?.is_some_and(|key| !key.is_empty()) {
        let state = AuthState {
            agent: AgentId::Grok,
            kind: Some("api_key".into()),
            summary: "API key present in config.toml".into(),
            has_credentials: true,
            health: crate::models::AuthHealth::Configured,
            source: Some("grok:config.toml".into()),
            revision: auth_files_revision(&[config, auth]),
            also_present: Vec::new(),
        };
        return Ok(if grok_auth_json_has_oauth(auth) {
            state.with_also_present(["oauth"])
        } else {
            state
        });
    }
    if !auth.is_file() {
        return Ok(AuthState {
            agent: AgentId::Grok,
            kind: None,
            summary: "no auth".into(),
            has_credentials: false,
            health: crate::models::AuthHealth::Missing,
            source: Some("grok:auth.json".into()),
            revision: None,
            also_present: Vec::new(),
        });
    }
    let body = match std::fs::read_to_string(auth)
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
    {
        Some(body) => body,
        None => {
            return Ok(AuthState {
                agent: AgentId::Grok,
                kind: None,
                summary: "auth.json could not be parsed".into(),
                has_credentials: false,
                health: crate::models::AuthHealth::Unknown,
                source: Some("grok:auth.json".into()),
                revision: auth_file_revision(auth),
                also_present: Vec::new(),
            });
        }
    };
    let metadata = inspect_auth_credentials(&body);
    if !metadata.has_access_token && !metadata.has_refresh_token {
        return Ok(AuthState {
            agent: AgentId::Grok,
            kind: None,
            summary: "auth.json present but credentials could not be classified".into(),
            has_credentials: false,
            health: crate::models::AuthHealth::Unknown,
            source: Some("grok:auth.json".into()),
            revision: auth_file_revision(auth),
            also_present: Vec::new(),
        });
    }
    let health = oauth_auth_health(metadata);
    Ok(AuthState {
        agent: AgentId::Grok,
        kind: Some("oauth".into()),
        summary: if health == crate::models::AuthHealth::NeedsLogin {
            "Grok OAuth credentials are expired; sign in again".into()
        } else {
            "auth.json credentials present".into()
        },
        has_credentials: true,
        health,
        source: Some("grok:auth.json".into()),
        revision: auth_file_revision(auth),
        also_present: Vec::new(),
    })
}

fn grok_auth_json_has_oauth(auth: &Path) -> bool {
    let Some(body) = std::fs::read_to_string(auth)
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
    else {
        return false;
    };
    let metadata = inspect_auth_credentials(&body);
    metadata.has_access_token || metadata.has_refresh_token
}

fn ensure_grok_profile<'a>(
    doc: &'a mut DocumentMut,
    alias: &str,
) -> Result<&'a mut toml_edit::Table> {
    // Account writers set api_key immediately after ensure; strip root env_key so
    // leftover env pointers cannot shadow the nested registry entry.
    ensure_grok_model_shape(
        doc,
        alias,
        EnsureGrokModelShapeOptions {
            migrate_legacy_api_key: false,
            strip_root_env_key: true,
        },
    )
}

fn read_grok_api_key(path: &Path) -> Result<Option<String>> {
    if !path.exists() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(path)?;
    let doc = text
        .parse::<toml_edit::DocumentMut>()
        .map_err(|e| AppError::InvalidArg(format!("invalid Grok config.toml: {e}")))?;
    let alias = active_model_alias(&doc);
    let entry = doc
        .get("model")
        .and_then(Item::as_table)
        .and_then(|models| models.get(&alias))
        .and_then(Item::as_table);
    if let Some(key) = entry
        .and_then(|entry| entry.get("api_key"))
        .and_then(Item::as_str)
        .map(str::trim)
        .filter(|key| !key.is_empty())
    {
        return Ok(Some(key.to_owned()));
    }
    if let Some(env_key) = entry
        .and_then(|entry| entry.get("env_key"))
        .and_then(Item::as_str)
        .map(str::trim)
        .filter(|key| !key.is_empty())
    {
        if let Ok(value) = std::env::var(env_key) {
            if !value.trim().is_empty() {
                return Ok(Some(value));
            }
        }
    }
    Ok(doc.get("api_key").and_then(Item::as_str).map(str::to_owned))
}

fn read_grok_inline_field(path: &Path, key: &str) -> Result<Option<String>> {
    if !path.exists() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(path)?;
    let doc = text
        .parse::<DocumentMut>()
        .map_err(|e| AppError::InvalidArg(format!("invalid Grok config.toml: {e}")))?;
    let alias = active_model_alias(&doc);
    Ok(doc
        .get("model")
        .and_then(Item::as_table)
        .and_then(|models| models.get(&alias))
        .and_then(Item::as_table)
        .and_then(|entry| entry.get(key))
        .and_then(Item::as_str)
        .map(str::to_owned)
        .or_else(|| doc.get(key).and_then(Item::as_str).map(str::to_owned)))
}

fn write_grok_api_key(path: &Path, value: &str) -> Result<()> {
    let live = match std::fs::read_to_string(path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(error.into()),
    };
    let mut doc = if live.trim().is_empty() {
        toml_edit::DocumentMut::new()
    } else {
        live.parse::<toml_edit::DocumentMut>().map_err(|e| {
            AppError::InvalidArg(format!("existing Grok config.toml is invalid: {e}"))
        })?
    };
    let alias = active_model_alias(&doc);
    let entry = ensure_grok_profile(&mut doc, &alias)?;
    entry["api_key"] = toml_edit::value(value);
    entry.remove("env_key");
    atomic_write(path, doc.to_string().as_bytes())
}

fn verify_grok_field(path: &Path, key: &str, expected: &str) -> Result<()> {
    let got = read_grok_inline_field(path, key)?;
    if got.as_deref() != Some(expected) {
        return Err(AppError::message(
            "account.verify",
            format!("Grok {key} verification failed after write"),
        ));
    }
    Ok(())
}

fn clear_grok_field(path: &Path, key: &str) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    let live = std::fs::read_to_string(path)?;
    if live.trim().is_empty() {
        return Ok(());
    }
    let mut doc = live
        .parse::<toml_edit::DocumentMut>()
        .map_err(|e| AppError::InvalidArg(format!("existing Grok config.toml is invalid: {e}")))?;
    let alias = active_model_alias(&doc);
    let mut changed = false;
    if doc.remove(key).is_some() {
        changed = true;
    }
    if key == "api_key" && doc.remove("env_key").is_some() {
        changed = true;
    }
    if let Some(entry) = doc
        .get_mut("model")
        .and_then(Item::as_table_mut)
        .and_then(|models| models.get_mut(&alias))
        .and_then(Item::as_table_mut)
    {
        if entry.remove(key).is_some() {
            changed = true;
        }
        if key == "api_key" && entry.remove("env_key").is_some() {
            changed = true;
        }
    }
    if !changed {
        return Ok(());
    }
    atomic_write(path, doc.to_string().as_bytes())?;
    if read_grok_inline_field(path, key)?.is_some() {
        return Err(AppError::message(
            "account.verify",
            format!("Grok {key} still present after clear"),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
