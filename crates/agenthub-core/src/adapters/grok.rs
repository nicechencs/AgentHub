use std::collections::HashSet;
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

const GROK_DEFAULT_AUTH_SLOT: &str = "https://auth.x.ai::client";

/// Merge this grant into official nested `auth.json`. Patch only the profile
/// whose email/`user_id` intersects; never copy tokens onto a sibling slot.
/// A stored multi-slot `format=auth_json` snapshot with no top-level identity
/// is written as-is (switch/apply of an imported file).
fn grok_auth_json_body_from_credentials(credentials: &Value, auth_path: &Path) -> Result<Value> {
    if let Some(body) = credentials.get("body") {
        if is_grok_slot_map(body) && grok_tip_is_unpinned(credentials) {
            return Ok(body.clone());
        }
    }
    let incoming = extract_incoming_grok_profile(credentials)?;
    let mut existing = if auth_path.is_file() {
        std::fs::read_to_string(auth_path)
            .ok()
            .and_then(|text| serde_json::from_str::<Value>(&text).ok())
            .filter(|body| body.is_object())
            .unwrap_or_else(|| json!({}))
    } else {
        json!({})
    };
    merge_incoming_grok_profile(&mut existing, incoming.clone(), &incoming);
    Ok(existing)
}

fn grok_tip_is_unpinned(credentials: &Value) -> bool {
    let tip = grok_top_level_grant(credentials);
    let (emails, subjects) = grok_identity_marks(&tip);
    emails.is_empty()
        && subjects.is_empty()
        && first_oauth_string(&tip, &["key", "access_token", "refresh_token"]).is_none()
}

fn extract_incoming_grok_profile(credentials: &Value) -> Result<Value> {
    let tip = grok_top_level_grant(credentials);
    if let Some(body) = credentials.get("body").and_then(|body| body.as_object()) {
        if is_grok_slot_map_object(body) {
            if let Some(slot) = body
                .values()
                .find(|slot| grok_identity_intersects(slot, &tip))
            {
                return Ok(normalize_grok_profile(slot));
            }
            if body.len() == 1 {
                if let Some(slot) = body.values().next() {
                    return Ok(normalize_grok_profile(slot));
                }
            }
        } else {
            return Ok(normalize_grok_profile(&Value::Object(body.clone())));
        }
    }
    let profile = normalize_grok_profile(&tip);
    if profile.get("key").and_then(|v| v.as_str()).is_none()
        && profile
            .get("refresh_token")
            .and_then(|v| v.as_str())
            .is_none()
    {
        return Err(AppError::InvalidArg(
            "Grok OAuth apply requires access_token or refresh_token".into(),
        ));
    }
    Ok(profile)
}

fn grok_top_level_grant(credentials: &Value) -> Value {
    let Some(obj) = credentials.as_object() else {
        return credentials.clone();
    };
    let mut map = Map::new();
    for key in [
        "email",
        "user_id",
        "userId",
        "sub",
        "access_token",
        "refresh_token",
        "key",
        "type",
        "provider",
    ] {
        if let Some(value) = obj.get(key) {
            map.insert(key.into(), value.clone());
        }
    }
    if map.is_empty() {
        // Do not return the full tree: a multi-slot `body` would union every
        // profile identity and match the first slot.
        return json!({});
    }
    Value::Object(map)
}

fn normalize_grok_profile(value: &Value) -> Value {
    let mut map = Map::new();
    if let Some(access) =
        first_oauth_string(value, &["key", "access_token", "accessToken", "access"])
    {
        map.insert("key".into(), json!(access));
    }
    if let Some(refresh) = first_oauth_string(value, &["refresh_token", "refreshToken", "refresh"])
    {
        map.insert("refresh_token".into(), json!(refresh));
    }
    if let Some(email) = first_oauth_string(value, &["email"]) {
        map.insert("email".into(), json!(email));
    }
    if let Some(user_id) = first_oauth_string(value, &["user_id", "userId", "sub"]) {
        map.insert("user_id".into(), json!(user_id));
    }
    Value::Object(map)
}

fn is_grok_slot_map(value: &Value) -> bool {
    value.as_object().is_some_and(is_grok_slot_map_object)
}

fn is_grok_slot_map_object(obj: &Map<String, Value>) -> bool {
    !obj.is_empty()
        && obj.values().all(|value| value.is_object())
        && !obj.contains_key("refresh_token")
        && !obj.contains_key("access_token")
        && obj
            .keys()
            .all(|key| key.contains("auth.x.ai") || key.contains("://") || key == "xai")
}

fn merge_incoming_grok_profile(existing: &mut Value, incoming: Value, identity: &Value) {
    if existing.as_object().is_none() {
        *existing = json!({});
    }
    if existing.as_object().is_some_and(|obj| obj.is_empty()) {
        *existing = json!({ GROK_DEFAULT_AUTH_SLOT: incoming });
        return;
    }
    if is_grok_slot_map(existing) {
        let obj = existing.as_object_mut().expect("slot map");
        let matched = obj.iter().find_map(|(key, slot)| {
            grok_identity_intersects(slot, identity)
                .then(|| key.clone())
                .or_else(|| grok_identity_intersects(slot, &incoming).then(|| key.clone()))
        });
        if let Some(key) = matched {
            if let Some(slot) = obj.get_mut(&key) {
                patch_one_grok_profile(slot, &incoming);
            }
            return;
        }
        obj.entry(GROK_DEFAULT_AUTH_SLOT.to_string())
            .or_insert(incoming);
        return;
    }
    let (emails, subjects) = grok_identity_marks(existing);
    if grok_identity_intersects(existing, identity) || (emails.is_empty() && subjects.is_empty()) {
        patch_one_grok_profile(existing, &incoming);
        return;
    }
    *existing = json!({ GROK_DEFAULT_AUTH_SLOT: incoming });
}

fn patch_one_grok_profile(slot: &mut Value, incoming: &Value) {
    let Some(map) = slot.as_object_mut() else {
        *slot = incoming.clone();
        return;
    };
    if let Some(key) = incoming.get("key").cloned() {
        map.insert("key".into(), key);
    }
    if let Some(rt) = incoming.get("refresh_token").cloned() {
        map.insert("refresh_token".into(), rt);
    }
}

fn grok_identity_marks(value: &Value) -> (HashSet<String>, HashSet<String>) {
    let mut emails = HashSet::new();
    let mut subjects = HashSet::new();
    collect_grok_identity(value, &mut emails, &mut subjects);
    (emails, subjects)
}

fn collect_grok_identity(
    value: &Value,
    emails: &mut HashSet<String>,
    subjects: &mut HashSet<String>,
) {
    match value {
        Value::Object(map) => {
            for (key, nested) in map {
                if let Some(raw) = nested.as_str().map(str::trim).filter(|s| !s.is_empty()) {
                    let lower = key.to_ascii_lowercase();
                    if matches!(lower.as_str(), "email" | "email_address" | "emailaddress") {
                        emails.insert(raw.to_ascii_lowercase());
                    } else if matches!(
                        lower.as_str(),
                        "user_id" | "userid" | "sub" | "subject" | "account_id" | "accountid"
                    ) {
                        subjects.insert(raw.to_owned());
                    }
                }
            }
            for nested in map.values() {
                if nested.is_object() || nested.is_array() {
                    collect_grok_identity(nested, emails, subjects);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_grok_identity(item, emails, subjects);
            }
        }
        _ => {}
    }
}

fn grok_identity_intersects(left: &Value, right: &Value) -> bool {
    let (le, ls) = grok_identity_marks(left);
    let (re, rs) = grok_identity_marks(right);
    if (le.is_empty() && ls.is_empty()) || (re.is_empty() && rs.is_empty()) {
        return false;
    }
    !le.is_disjoint(&re) || !ls.is_disjoint(&rs)
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
