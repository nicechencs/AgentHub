//! DeepSeek Harness (`dsh`) adapter.
//!
//! Product card: **DeepSeek Harness**. Manages the npm `dsh` CLI, not the
//! DeepSeek API ticket and not the Python SDK / source checkout.
//!
//! ## Scope
//! - detect / npm install (`@deepseek-ai/dsh`)
//! - home `$DSH_HOME` or `~/.dsh`
//! - skills projection root `$DSH_HOME/skills`
//! - API Key pool + credentials-file apply (reference name in patch, value in credentials)
//! - home-level `cordis.patch.yml` merge for the official DeepSeek LLM plugin row
//! - headless text run: `dsh --profile headless "<prompt>"`
//!
//! ## Honest limits
//! - Generic `write_config` only accepts the projected LLM-row shape; unknown
//!   plugin trees fail closed (`ConfigWrite = Partial`).
//! - Structured stream / native session resume are Planned until a local
//!   NDJSON contract is verified.
//! - No OAuth. No invented danger flags.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::{json, Map, Value};

use crate::error::{AppError, Result};
use crate::models::{
    AgentConfig, AgentId, AuthHealth, AuthState, Capability, CapabilityState, DetectResult,
    LiveAccount, RunOptions, RunSpec,
};
use crate::runtime;
use crate::utils::atomic::{atomic_write, with_restored_files};
use crate::utils::paths::{agent_home, first_env_path};

use super::{
    api_key_live_account, auth_file_revision, detect_binary, require_api_key, AgentAdapter,
};

pub const NPM_PACKAGE: &str = "@deepseek-ai/dsh";
pub const HOME_PATCH_FILE: &str = "cordis.patch.yml";
pub const CREDENTIALS_FILE: &str = ".credentials.yaml";
pub const LLM_PLUGIN_ID: &str = "@deepseek-ai/dsh-llm-deepseek";
pub const DEFAULT_API_KEY_ENV: &str = "DEEPSEEK_API_KEY";
pub const DEFAULT_PROVIDER: &str = "deepseek-official";
pub const DEFAULT_MODEL: &str = "deepseek-v4-flash";
pub const DEFAULT_BASE_URL: &str = "https://api.deepseek.com";

pub struct DshAdapter;

/// Standalone install probe used by platform detectors (no full adapter required).
pub(crate) fn detect_installation() -> DetectResult {
    let requires = crate::catalog::install::adapter_install_channels(AgentId::Dsh)
        .first()
        .map(|c| c.requires.clone())
        .unwrap_or_default();
    let env_ready = runtime::is_ready(&requires);
    detect_binary(
        AgentId::Dsh,
        &["dsh"],
        &["--version"],
        Some("npm"),
        env_ready,
    )
}

impl AgentAdapter for DshAdapter {
    fn id(&self) -> AgentId {
        AgentId::Dsh
    }

    fn detect(&self) -> DetectResult {
        detect_installation()
    }

    fn read_config(&self) -> Result<AgentConfig> {
        let home = agent_home(AgentId::Dsh)?;
        let patch_path = home.join(HOME_PATCH_FILE);
        let creds_path = home.join(CREDENTIALS_FILE);
        let fields = read_llm_fields(&patch_path)?;
        Ok(AgentConfig {
            agent: AgentId::Dsh,
            raw: json!({
                "provider": DEFAULT_PROVIDER,
                "model": fields.model,
                "apiKeyEnv": fields.api_key_env,
                "baseURL": fields.base_url,
                "thinking": fields.thinking,
                "reasoningEffort": fields.reasoning_effort,
                "maxTokens": fields.max_tokens,
                "paths": {
                    "home": home,
                    "patch": patch_path,
                    "credentials": creds_path,
                }
            }),
        })
    }

    fn write_config(&self, config: &AgentConfig) -> Result<()> {
        write_dsh_config(config)
    }

    fn read_auth(&self) -> Result<AuthState> {
        let home = agent_home(AgentId::Dsh)?;
        let creds = home.join(CREDENTIALS_FILE);
        let env_set = std::env::var(DEFAULT_API_KEY_ENV)
            .ok()
            .is_some_and(|v| !v.trim().is_empty());
        let file_key = read_credential_value(&creds, DEFAULT_API_KEY_ENV)?;
        let has_file = file_key.as_ref().is_some_and(|v| !v.is_empty());
        if env_set && has_file {
            return Ok(AuthState {
                agent: AgentId::Dsh,
                kind: Some("api_key".into()),
                summary: "API key present (env shadows credentials file)".into(),
                has_credentials: true,
                health: AuthHealth::Configured,
                source: Some("dsh:credentials+env".into()),
                revision: auth_file_revision(&creds),
                also_present: Vec::new(),
                secret_hash: None,
            });
        }
        if env_set {
            return Ok(AuthState {
                agent: AgentId::Dsh,
                kind: Some("api_key".into()),
                summary: "API key present in process environment".into(),
                has_credentials: true,
                health: AuthHealth::Configured,
                source: Some("dsh:env".into()),
                revision: None,
                also_present: Vec::new(),
                secret_hash: None,
            });
        }
        if has_file {
            return Ok(AuthState {
                agent: AgentId::Dsh,
                kind: Some("api_key".into()),
                summary: "API key present in credentials file".into(),
                has_credentials: true,
                health: AuthHealth::Configured,
                source: Some("dsh:credentials".into()),
                revision: auth_file_revision(&creds),
                also_present: Vec::new(),
                secret_hash: None,
            });
        }
        Ok(AuthState {
            agent: AgentId::Dsh,
            kind: None,
            summary: "no DeepSeek API key".into(),
            has_credentials: false,
            health: AuthHealth::Missing,
            source: Some("dsh:credentials".into()),
            revision: auth_file_revision(&creds),
            also_present: Vec::new(),
            secret_hash: None,
        })
    }

    fn read_account(&self) -> Result<LiveAccount> {
        let home = agent_home(AgentId::Dsh)?;
        let creds = home.join(CREDENTIALS_FILE);
        let key = read_credential_value(&creds, DEFAULT_API_KEY_ENV)?.filter(|v| !v.is_empty());
        let key = key.ok_or_else(|| AppError::NotFound("no live DSH API key to import".into()))?;
        let patch_path = home.join(HOME_PATCH_FILE);
        let fields = read_llm_fields(&patch_path).unwrap_or_default();
        let content = std::fs::read_to_string(&patch_path).unwrap_or_default();
        let mut cred = json!({
            "format": "api_key",
            "api_key": key,
            "provider": "deepseek",
        });
        if !content.is_empty() {
            cred["content"] = json!(content);
        }
        if !fields.model.is_empty() {
            cred["model"] = json!(fields.model);
        }
        if !fields.base_url.is_empty() {
            cred["base_url"] = json!(fields.base_url);
        }
        let mut extra = json!({
            "source": "live",
            "provider": "deepseek",
        });
        if !fields.base_url.is_empty() {
            extra["endpoint"] = json!(fields.base_url);
        }
        Ok(api_key_live_account(
            AgentId::Dsh,
            &key,
            cred,
            "API Key",
            extra,
        ))
    }

    fn apply_account(&self, account: &LiveAccount) -> Result<()> {
        if account.agent != AgentId::Dsh {
            return Err(AppError::InvalidArg(
                "account agent mismatch for dsh".into(),
            ));
        }
        let key = account
            .credentials
            .get("api_key")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                AppError::InvalidArg("DSH account requires credentials.api_key".into())
            })?;
        write_credential_value(
            &agent_home(AgentId::Dsh)?.join(CREDENTIALS_FILE),
            DEFAULT_API_KEY_ENV,
            key,
        )?;
        let patch = agent_home(AgentId::Dsh)?.join(HOME_PATCH_FILE);
        if !patch.exists() {
            if let Some(content) = account
                .credentials
                .get("content")
                .and_then(Value::as_str)
                .filter(|s| !s.trim().is_empty())
            {
                crate::utils::atomic::atomic_write(&patch, content.as_bytes())?;
            }
        }
        let mut fields = read_llm_fields(&patch)?;
        fields.api_key_env = DEFAULT_API_KEY_ENV.to_string();
        if let Some(model) = account
            .credentials
            .get("model")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            fields.model = model.to_string();
        }
        if let Some(base) = account
            .credentials
            .get("base_url")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            fields.base_url = base.to_string();
        }
        write_llm_fields(&patch, &fields)
    }

    fn build_api_key_account(&self, api_key: &str) -> Result<LiveAccount> {
        let key = require_api_key(api_key)?;
        Ok(api_key_live_account(
            AgentId::Dsh,
            key,
            json!({
                "format": "api_key",
                "api_key": key,
                "provider": "deepseek",
            }),
            "API Key",
            json!({
                "source": "manual",
                "provider": "deepseek",
            }),
        ))
    }

    fn skills_dir(&self) -> Option<PathBuf> {
        agent_home(AgentId::Dsh)
            .ok()
            .map(|home| home.join("skills"))
    }

    fn live_backup_paths(&self) -> Vec<PathBuf> {
        let Ok(home) = agent_home(AgentId::Dsh) else {
            return Vec::new();
        };
        let mut paths = vec![home.join(HOME_PATCH_FILE), home.join(CREDENTIALS_FILE)];
        let profiles = home.join("profiles");
        if let Ok(entries) = std::fs::read_dir(&profiles) {
            for entry in entries.flatten() {
                let dir = entry.path();
                if !dir.is_dir() {
                    continue;
                }
                paths.push(dir.join("package.json"));
                paths.push(dir.join(HOME_PATCH_FILE));
            }
        }
        paths
    }

    fn build_run_spec(&self, binary: &Path, prompt: &str, opts: &RunOptions) -> Result<RunSpec> {
        // Official launcher: `dsh --profile headless "<job>"`.
        // No documented always-approve flag — do not invent one.
        let _ = opts.allow_dangerous;
        let mut env = Vec::new();
        if let Ok(home) = agent_home(AgentId::Dsh) {
            env.push(("DSH_HOME".into(), home.to_string_lossy().into_owned()));
        }
        Ok(RunSpec {
            agent: AgentId::Dsh,
            program: binary.to_path_buf(),
            args: vec!["--profile".into(), "headless".into(), prompt.to_string()],
            cwd: opts.cwd.clone(),
            env,
        })
    }

    fn capability(&self, cap: Capability) -> CapabilityState {
        use Capability::*;
        match cap {
            ApiKeyAccount | Skills | LiveBackup | ProjectHistory => CapabilityState::full(),
            ConfigWrite => CapabilityState::partial(
                "只合并 home 级 DeepSeek LLM 插件行；整棵 Cordis 树 fail-closed",
            ),
            AccountSwitch => CapabilityState::partial("仅 API Key 引用切换，无 OAuth"),
            DangerousMode => {
                CapabilityState::partial("存在 danger composition；未验证官方非交互 flag")
            }
            ProjectDelete => CapabilityState::partial("仅删除单会话 JSONL，不删 SQLite 整库"),
            ProviderPresets => CapabilityState::partial("内置 deepseek-official，不是通用预设商店"),
            Usage => CapabilityState::full(),
            StructuredStream => CapabilityState::planned("headless 事件契约未验证"),
            Mcp | ModelSelect | SessionResume => CapabilityState::planned("待验证接入"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DshLlmFields {
    pub api_key_env: String,
    pub base_url: String,
    pub thinking: String,
    pub reasoning_effort: String,
    pub max_tokens: Option<u64>,
    pub model: String,
}

impl Default for DshLlmFields {
    fn default() -> Self {
        Self {
            api_key_env: DEFAULT_API_KEY_ENV.into(),
            base_url: DEFAULT_BASE_URL.into(),
            thinking: "enabled".into(),
            reasoning_effort: "high".into(),
            max_tokens: None,
            model: DEFAULT_MODEL.into(),
        }
    }
}

pub(crate) fn write_dsh_config(config: &AgentConfig) -> Result<()> {
    if config.agent != AgentId::Dsh {
        return Err(AppError::InvalidArg(format!(
            "config agent mismatch: expected dsh, got {}",
            config.agent.as_str()
        )));
    }
    let raw = config
        .raw
        .as_object()
        .ok_or_else(|| AppError::InvalidArg("DSH settings_config must be a JSON object".into()))?;
    let peeled_key = raw
        .get("api_key")
        .or_else(|| raw.get("apiKey"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let home = agent_home(AgentId::Dsh)?;
    let patch = home.join(HOME_PATCH_FILE);
    let mut fields = read_llm_fields(&patch)?;
    if let Some(v) = string_field(raw, "apiKeyEnv") {
        fields.api_key_env = v;
    }
    if let Some(v) = string_field(raw, "baseURL").or_else(|| string_field(raw, "baseUrl")) {
        fields.base_url = v;
    }
    if let Some(v) = string_field(raw, "thinking") {
        fields.thinking = v;
    }
    if let Some(v) = string_field(raw, "reasoningEffort") {
        fields.reasoning_effort = v;
    }
    if let Some(v) = string_field(raw, "model") {
        fields.model = v;
    }
    if let Some(n) = raw.get("maxTokens").and_then(Value::as_u64) {
        fields.max_tokens = Some(n);
    }
    let creds = home.join(CREDENTIALS_FILE);
    with_restored_files(&[&patch, &creds], || {
        write_llm_fields(&patch, &fields)?;
        if let Some(key) = peeled_key {
            if key != "$AGENTHUB_CONNECTION_SECRET$" {
                write_credential_value(&creds, &fields.api_key_env, &key)?;
            }
        }
        Ok(())
    })
}

fn string_field(raw: &Map<String, Value>, key: &str) -> Option<String> {
    raw.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

pub(crate) fn read_llm_fields(path: &Path) -> Result<DshLlmFields> {
    let mut fields = DshLlmFields::default();
    if !path.exists() {
        return Ok(fields);
    }
    let text = std::fs::read_to_string(path)?;
    let Some(row) = find_plugin_row(&text, LLM_PLUGIN_ID) else {
        return Ok(fields);
    };
    if let Some(v) = row.get("apiKeyEnv") {
        fields.api_key_env = v.clone();
    }
    if let Some(v) = row.get("baseURL").or_else(|| row.get("baseUrl")) {
        fields.base_url = v.clone();
    }
    if let Some(v) = row.get("thinking") {
        fields.thinking = v.clone();
    }
    if let Some(v) = row.get("reasoningEffort") {
        fields.reasoning_effort = v.clone();
    }
    if let Some(v) = row.get("maxTokens") {
        if let Ok(n) = v.parse::<u64>() {
            fields.max_tokens = Some(n);
        }
    }
    if let Some(v) = row.get("model") {
        fields.model = v.clone();
    }
    Ok(fields)
}

pub(crate) fn write_llm_fields(path: &Path, fields: &DshLlmFields) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let existing = if path.exists() {
        std::fs::read_to_string(path)?
    } else {
        String::new()
    };
    let rendered = upsert_llm_row(&existing, fields)?;
    let mut bytes = rendered.into_bytes();
    if !bytes.ends_with(b"\n") {
        bytes.push(b'\n');
    }
    atomic_write(path, &bytes)
}

pub(crate) fn read_credential_value(path: &Path, key: &str) -> Result<Option<String>> {
    if !path.exists() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(path)?;
    Ok(parse_flat_yaml_map(&text)?.remove(key))
}

pub(crate) fn write_credential_value(path: &Path, key: &str, value: &str) -> Result<()> {
    if value.trim().is_empty() {
        return Err(AppError::InvalidArg(
            "credential value must not be empty".into(),
        ));
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut map = if path.exists() {
        parse_flat_yaml_map(&std::fs::read_to_string(path)?)?
    } else {
        BTreeMap::new()
    };
    map.insert(key.to_string(), value.to_string());
    let rendered = render_flat_yaml_map(&map)?;
    atomic_write(path, rendered.as_bytes())
}

fn parse_flat_yaml_map(text: &str) -> Result<BTreeMap<String, String>> {
    if text.trim().is_empty() {
        return Ok(BTreeMap::new());
    }
    let value: serde_yml::Value = serde_yml::from_str(text).map_err(|err| {
        AppError::InvalidArg(format!("DSH credentials YAML must be a string map: {err}"))
    })?;
    if value.is_null() {
        return Ok(BTreeMap::new());
    }
    serde_yml::from_value(value).map_err(|err| {
        AppError::InvalidArg(format!("DSH credentials YAML must be a string map: {err}"))
    })
}

fn render_flat_yaml_map(map: &BTreeMap<String, String>) -> Result<String> {
    serde_yml::to_string(map)
        .map_err(|err| AppError::InvalidArg(format!("failed to write DSH credentials YAML: {err}")))
}

fn find_plugin_row(
    text: &str,
    plugin_id: &str,
) -> Option<std::collections::BTreeMap<String, String>> {
    let mut current_id: Option<String> = None;
    let mut in_config = false;
    let mut row = std::collections::BTreeMap::new();
    let mut found = None;
    for raw in text.lines() {
        let indent = raw.len() - raw.trim_start().len();
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with("- id:") || line.starts_with("-id:") {
            if current_id.as_deref() == Some(plugin_id) {
                found = Some(row.clone());
            }
            row.clear();
            in_config = false;
            current_id = line
                .split_once(':')
                .map(|(_, rest)| unquote(rest.trim()))
                .filter(|s| !s.is_empty());
            continue;
        }
        if indent == 0 && line.starts_with("id:") {
            if current_id.as_deref() == Some(plugin_id) {
                found = Some(row.clone());
            }
            row.clear();
            in_config = false;
            current_id = Some(unquote(line[3..].trim()));
            continue;
        }
        if line == "config:" {
            in_config = true;
            continue;
        }
        if in_config && indent >= 2 {
            if let Some((key, rest)) = line.split_once(':') {
                row.insert(key.trim().to_string(), unquote(rest.trim()));
            }
        }
    }
    if current_id.as_deref() == Some(plugin_id) {
        found = Some(row);
    }
    found
}

fn upsert_llm_row(existing: &str, fields: &DshLlmFields) -> Result<String> {
    let mut max_tokens_line = String::new();
    if let Some(n) = fields.max_tokens {
        max_tokens_line = format!("    maxTokens: {n}\n");
    }
    let new_row = format!(
        "- id: {id}\n  config:\n    apiKeyEnv: {env}\n    baseURL: {base}\n    thinking: {thinking}\n    reasoningEffort: {effort}\n    model: {model}\n{max_tokens}",
        id = LLM_PLUGIN_ID,
        env = yaml_quote(&fields.api_key_env),
        base = yaml_quote(&fields.base_url),
        thinking = yaml_quote(&fields.thinking),
        effort = yaml_quote(&fields.reasoning_effort),
        model = yaml_quote(&fields.model),
        max_tokens = max_tokens_line,
    );
    if existing.trim().is_empty() {
        return Ok(new_row);
    }
    if existing.contains(LLM_PLUGIN_ID)
        && (existing.contains("apiKey") && !existing.contains("apiKeyEnv")
            || existing.to_ascii_lowercase().contains("sk-"))
    {
        return Err(AppError::InvalidArg(
            "refusing to rewrite a DSH patch that may contain a secret".into(),
        ));
    }
    if let Some(replaced) = replace_plugin_row(existing, LLM_PLUGIN_ID, &new_row) {
        return Ok(replaced);
    }
    let mut out = existing.trim_end().to_string();
    out.push('\n');
    out.push_str(&new_row);
    Ok(out)
}

fn replace_plugin_row(existing: &str, plugin_id: &str, new_row: &str) -> Option<String> {
    let needle = format!("id: {plugin_id}");
    let lines: Vec<&str> = existing.lines().collect();
    let mut start = None;
    for (idx, line) in lines.iter().enumerate() {
        if line.contains(&needle) {
            start = Some(if line.trim_start().starts_with("- ") {
                idx
            } else if idx > 0 && lines[idx - 1].trim_start().starts_with('-') {
                idx - 1
            } else {
                idx
            });
            break;
        }
    }
    let start = start?;
    let mut end = lines.len();
    for (idx, line) in lines.iter().enumerate().skip(start + 1) {
        if line.starts_with("- ") {
            end = idx;
            break;
        }
    }
    let mut out = String::new();
    for line in &lines[..start] {
        out.push_str(line);
        out.push('\n');
    }
    out.push_str(new_row);
    if !out.ends_with('\n') {
        out.push('\n');
    }
    for line in &lines[end..] {
        out.push_str(line);
        out.push('\n');
    }
    Some(out)
}

fn unquote(raw: &str) -> String {
    let s = raw.trim();
    if (s.starts_with('"') && s.ends_with('"') && s.len() >= 2)
        || (s.starts_with('\'') && s.ends_with('\'') && s.len() >= 2)
    {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

fn yaml_quote(value: &str) -> String {
    if value.is_empty()
        || value.contains(':')
        || value.contains('#')
        || value.contains(' ')
        || value.contains('"')
        || value.contains('\'')
    {
        format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        value.to_string()
    }
}

pub fn resolve_dsh_home() -> Result<PathBuf> {
    if let Some(dir) = first_env_path("DSH_HOME") {
        return Ok(dir);
    }
    Ok(crate::utils::paths::home_dir()?.join(".dsh"))
}

#[cfg(test)]
mod tests;
