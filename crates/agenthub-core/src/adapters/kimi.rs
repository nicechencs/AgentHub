use std::path::{Path, PathBuf};

use crate::error::{AppError, Result};
use crate::models::{
    AccountKind, AgentConfig, AgentId, AuthState, Capability, CapabilityState, DetectResult,
    LiveAccount, RunOptions, RunSpec,
};
use crate::runtime;
use crate::utils::atomic::atomic_write;
use crate::utils::paths::agent_home;
use crate::utils::redact::mask_secret_preview;

use super::{
    api_key_live_account, auth_file_revision, detect_binary, inspect_auth_credentials,
    oauth_auth_health, require_api_key, write_toml_config, write_verified_json_object,
    AgentAdapter,
};

pub struct KimiAdapter;

/// Standalone install probe used by platform detectors (no full adapter required).
pub(crate) fn detect_installation() -> DetectResult {
    let requires = crate::catalog::install::adapter_install_channels(AgentId::Kimi)
        .first()
        .map(|c| c.requires.clone())
        .unwrap_or_default();
    let env_ready = runtime::is_ready(&requires);
    detect_binary(
        AgentId::Kimi,
        &["kimi"],
        &["--version"],
        Some("native"),
        env_ready,
    )
}

impl AgentAdapter for KimiAdapter {
    fn id(&self) -> AgentId {
        AgentId::Kimi
    }

    fn detect(&self) -> DetectResult {
        detect_installation()
    }

    fn read_config(&self) -> Result<AgentConfig> {
        let path = agent_home(AgentId::Kimi)?.join("config.toml");
        let raw = if path.exists() {
            let text = std::fs::read_to_string(&path)?;
            serde_json::json!({ "format": "toml", "content": text })
        } else {
            serde_json::json!({})
        };
        Ok(AgentConfig {
            agent: AgentId::Kimi,
            raw,
        })
    }

    fn write_config(&self, config: &AgentConfig) -> Result<()> {
        let path = agent_home(AgentId::Kimi)?.join("config.toml");
        write_toml_config(AgentId::Kimi, &path, config)
    }

    fn read_auth(&self) -> Result<AuthState> {
        let home = agent_home(AgentId::Kimi)?;
        let cred = home.join("credentials").join("kimi-code.json");
        let config = home.join("config.toml");
        Ok(kimi_auth_state(&config, &cred))
    }

    fn read_account(&self) -> Result<LiveAccount> {
        let home = agent_home(AgentId::Kimi)?;
        let cred_path = home.join("credentials").join("kimi-code.json");
        let config_path = home.join("config.toml");
        let api_key = read_kimi_api_key(&config_path)?;
        let cred_body = if cred_path.exists() {
            let text = std::fs::read_to_string(&cred_path)?;
            Some(serde_json::from_str::<serde_json::Value>(&text)?)
        } else {
            None
        };

        match (api_key.as_deref(), cred_body) {
            (Some(key), Some(body)) if !key.is_empty() => Ok(LiveAccount {
                agent: AgentId::Kimi,
                kind: AccountKind::ApiKey,
                credentials: serde_json::json!({
                    "format": "kimi_bundle",
                    "api_key": key,
                    "credentials_file": body,
                }),
                label_hint: Some(format!("{} (API Key)", mask_secret_preview(key))),
                extra: serde_json::json!({ "source": "config.toml+credentials" }),
            }),
            (Some(key), None) if !key.is_empty() => Ok(LiveAccount {
                agent: AgentId::Kimi,
                kind: AccountKind::ApiKey,
                credentials: serde_json::json!({
                    "format": "api_key",
                    "api_key": key,
                }),
                label_hint: Some(format!("{} (API Key)", mask_secret_preview(key))),
                extra: serde_json::json!({ "source": "config.toml" }),
            }),
            (_, Some(body)) => Ok(LiveAccount {
                agent: AgentId::Kimi,
                kind: AccountKind::Oauth,
                credentials: serde_json::json!({
                    "format": "credentials_json",
                    "body": body,
                }),
                label_hint: Some("kimi-oauth".into()),
                extra: serde_json::json!({ "source": "credentials/kimi-code.json" }),
            }),
            _ => Err(AppError::NotFound(
                "no live Kimi credentials or api_key found to import".into(),
            )),
        }
    }

    fn apply_account(&self, account: &LiveAccount) -> Result<()> {
        if account.agent != AgentId::Kimi {
            return Err(AppError::InvalidArg(
                "account agent mismatch for kimi".into(),
            ));
        }
        let home = agent_home(AgentId::Kimi)?;
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
                    .ok_or_else(|| AppError::InvalidArg("Kimi api_key is required".into()))?;
                write_kimi_api_key(&home.join("config.toml"), key)?;
                Ok(())
            }
            "credentials_json" => {
                let body = account.credentials.get("body").cloned().ok_or_else(|| {
                    AppError::InvalidArg("Kimi credentials body is required".into())
                })?;
                write_verified_json_object(
                    &home.join("credentials").join("kimi-code.json"),
                    &body,
                )?;
                // Official OAuth must win over leftover API key (read prefers api_key).
                clear_kimi_api_key(&home.join("config.toml"))?;
                Ok(())
            }
            "kimi_bundle" => {
                if let Some(key) = account.credentials.get("api_key").and_then(|v| v.as_str()) {
                    write_kimi_api_key(&home.join("config.toml"), key)?;
                }
                if let Some(body) = account.credentials.get("credentials_file") {
                    write_verified_json_object(
                        &home.join("credentials").join("kimi-code.json"),
                        body,
                    )?;
                }
                Ok(())
            }
            other => Err(AppError::InvalidArg(format!(
                "unsupported Kimi account credential format: {other}"
            ))),
        }
    }

    fn build_api_key_account(&self, api_key: &str) -> Result<LiveAccount> {
        let key = require_api_key(api_key)?;
        Ok(api_key_live_account(
            AgentId::Kimi,
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
        None
    }

    fn capability(&self, cap: Capability) -> CapabilityState {
        use Capability::*;
        match cap {
            ConfigWrite | AccountSwitch | ApiKeyAccount | LiveBackup | StructuredStream
            | ProjectHistory | ProjectDelete | ProviderPresets => CapabilityState::full(),
            Skills => CapabilityState::unsupported("Kimi 不支持技能目录"),
            DangerousMode => CapabilityState::partial("-p 与 --yolo 互斥，headless 下该开关不生效"),
            Usage => CapabilityState::full(),
            Mcp | ModelSelect | SessionResume => CapabilityState::planned("待验证接入"),
        }
    }

    fn live_backup_paths(&self) -> Vec<PathBuf> {
        let mut paths = Vec::new();
        if let Ok(home) = agent_home(AgentId::Kimi) {
            paths.push(home.join("config.toml"));
            paths.push(home.join("credentials").join("kimi-code.json"));
        }
        paths
    }

    fn build_run_spec(&self, binary: &Path, prompt: &str, opts: &RunOptions) -> Result<RunSpec> {
        // text: kimi -p <prompt> --output-format text
        // structured (Chat): stream-json NDJSON events
        // Note: kimi rejects combining -p/--prompt with --auto/--yolo.
        let format = if super::wants_structured_for(opts.process_mode, AgentId::Kimi) {
            "stream-json"
        } else {
            "text"
        };
        let args = vec![
            "-p".into(),
            prompt.to_string(),
            "--output-format".into(),
            format.into(),
        ];
        if opts.allow_dangerous {
            // Prompt mode is already non-interactive; --auto is invalid with -p.
            tracing::debug!(
                module = "core.run",
                agent = "kimi",
                "kimi -p cannot take --auto/--yolo; dangerous flag ignored for headless"
            );
        }
        Ok(RunSpec {
            agent: AgentId::Kimi,
            program: binary.to_path_buf(),
            args,
            cwd: opts.cwd.clone(),
            env: vec![],
        })
    }
}

pub(crate) fn kimi_auth_state(config: &Path, cred: &Path) -> AuthState {
    match read_kimi_api_key(config) {
        Ok(Some(key)) if !key.is_empty() => {
            let state = AuthState {
                agent: AgentId::Kimi,
                kind: Some("api_key".into()),
                summary: "API key present in config.toml".into(),
                has_credentials: true,
                health: crate::models::AuthHealth::Configured,
                source: Some("kimi:config.toml".into()),
                revision: auth_file_revision(config),
                also_present: Vec::new(),
            };
            return if oauth_tokens_present(cred) {
                state.with_also_present(["oauth"])
            } else {
                state
            };
        }
        Ok(_) => {}
        Err(_) => {
            return AuthState {
                agent: AgentId::Kimi,
                kind: None,
                summary: "config.toml could not be parsed".into(),
                has_credentials: false,
                health: crate::models::AuthHealth::Unknown,
                source: Some("kimi:config.toml".into()),
                revision: auth_file_revision(config),
                also_present: Vec::new(),
            };
        }
    }

    if !cred.is_file() {
        return AuthState {
            agent: AgentId::Kimi,
            kind: None,
            summary: "no credentials file".into(),
            has_credentials: false,
            health: crate::models::AuthHealth::Missing,
            source: Some("kimi:credentials/kimi-code.json".into()),
            revision: None,
            also_present: Vec::new(),
        };
    }
    let body = match std::fs::read_to_string(cred)
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
    {
        Some(body) => body,
        None => {
            return AuthState {
                agent: AgentId::Kimi,
                kind: None,
                summary: "credentials file could not be parsed".into(),
                has_credentials: false,
                health: crate::models::AuthHealth::Unknown,
                source: Some("kimi:credentials/kimi-code.json".into()),
                revision: auth_file_revision(cred),
                also_present: Vec::new(),
            };
        }
    };
    let metadata = inspect_auth_credentials(&body);
    if !metadata.has_access_token && !metadata.has_refresh_token {
        return AuthState {
            agent: AgentId::Kimi,
            kind: None,
            summary: "credentials file present but credentials could not be classified".into(),
            has_credentials: false,
            health: crate::models::AuthHealth::Unknown,
            source: Some("kimi:credentials/kimi-code.json".into()),
            revision: auth_file_revision(cred),
            also_present: Vec::new(),
        };
    }
    let health = oauth_auth_health(metadata);
    AuthState {
        agent: AgentId::Kimi,
        kind: Some("oauth".into()),
        summary: if health == crate::models::AuthHealth::NeedsLogin {
            "Kimi OAuth credentials are expired; run `kimi login`".into()
        } else {
            "Kimi OAuth credentials present".into()
        },
        has_credentials: true,
        health,
        source: Some("kimi:credentials/kimi-code.json".into()),
        revision: auth_file_revision(cred),
        also_present: Vec::new(),
    }
}

fn oauth_tokens_present(path: &Path) -> bool {
    let Some(body) = std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
    else {
        return false;
    };
    let metadata = inspect_auth_credentials(&body);
    metadata.has_access_token || metadata.has_refresh_token
}

fn read_kimi_api_key(path: &Path) -> Result<Option<String>> {
    if !path.exists() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(path)?;
    let doc = text
        .parse::<toml_edit::DocumentMut>()
        .map_err(|e| AppError::InvalidArg(format!("invalid Kimi config.toml: {e}")))?;
    // Prefer [providers.*].api_key under default_provider; else first provider with key.
    if let Some(default) = doc
        .get("default_provider")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
    {
        if let Some(key) = doc
            .get("providers")
            .and_then(|p| p.as_table())
            .and_then(|t| t.get(default.as_str()))
            .and_then(|item| item.get("api_key"))
            .and_then(|v| v.as_str())
        {
            return Ok(Some(key.to_string()));
        }
    }
    if let Some(providers) = doc.get("providers").and_then(|p| p.as_table()) {
        for (_name, item) in providers.iter() {
            if let Some(key) = item.get("api_key").and_then(|v| v.as_str()) {
                if !key.is_empty() {
                    return Ok(Some(key.to_string()));
                }
            }
        }
    }
    Ok(None)
}

fn write_kimi_api_key(path: &Path, key: &str) -> Result<()> {
    let live = match std::fs::read_to_string(path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(error.into()),
    };
    let mut doc = if live.trim().is_empty() {
        toml_edit::DocumentMut::new()
    } else {
        live.parse::<toml_edit::DocumentMut>().map_err(|e| {
            AppError::InvalidArg(format!("existing Kimi config.toml is invalid: {e}"))
        })?
    };

    let provider_name = doc
        .get("default_provider")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .or_else(|| {
            doc.get("providers")
                .and_then(|p| p.as_table())
                .and_then(|t| t.iter().next().map(|(k, _)| k.to_string()))
        })
        .unwrap_or_else(|| "moonshot".into());

    if doc.get("default_provider").is_none() {
        doc["default_provider"] = toml_edit::value(provider_name.as_str());
    }
    if doc.get("providers").is_none() {
        doc["providers"] = toml_edit::table();
    }
    let providers = doc["providers"]
        .as_table_mut()
        .ok_or_else(|| AppError::InvalidArg("Kimi config.toml providers must be a table".into()))?;
    if providers.get(provider_name.as_str()).is_none() {
        providers.insert(provider_name.as_str(), toml_edit::table());
    }
    providers[provider_name.as_str()]["api_key"] = toml_edit::value(key);
    atomic_write(path, doc.to_string().as_bytes())?;

    let verified = read_kimi_api_key(path)?;
    if verified.as_deref() != Some(key) {
        return Err(AppError::message(
            "account.verify",
            "Kimi api_key verification failed after write",
        ));
    }
    Ok(())
}

/// Strip `api_key` under every `[providers.*]` so official OAuth can take effect.
fn clear_kimi_api_key(path: &Path) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    let live = std::fs::read_to_string(path)?;
    if live.trim().is_empty() {
        return Ok(());
    }
    let mut doc = live
        .parse::<toml_edit::DocumentMut>()
        .map_err(|e| AppError::InvalidArg(format!("existing Kimi config.toml is invalid: {e}")))?;
    let mut changed = false;
    if let Some(providers) = doc.get_mut("providers").and_then(|p| p.as_table_mut()) {
        let names: Vec<String> = providers.iter().map(|(k, _)| k.to_string()).collect();
        for name in names {
            if let Some(item) = providers.get_mut(name.as_str()) {
                if let Some(table) = item.as_table_like_mut() {
                    if table.remove("api_key").is_some() {
                        changed = true;
                    }
                }
            }
        }
    }
    if !changed {
        return Ok(());
    }
    atomic_write(path, doc.to_string().as_bytes())?;
    if read_kimi_api_key(path)?.is_some() {
        return Err(AppError::message(
            "account.verify",
            "Kimi api_key still present after clear",
        ));
    }
    Ok(())
}
