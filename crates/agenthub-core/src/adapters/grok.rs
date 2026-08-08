use std::path::{Path, PathBuf};

use crate::error::{AppError, Result};
use crate::models::{
    AccountKind, AgentConfig, AgentId, AuthState, Capability, CapabilityState, DetectResult,
    InstallChannel, LiveAccount, RunOptions, RunSpec, RuntimeId,
};
use crate::runtime;
use crate::utils::atomic::atomic_write;
use crate::utils::paths::{agent_home, home_dir};
use crate::utils::redact::mask_secret_preview;

use super::{
    api_key_live_account, detect_binary, require_api_key, write_toml_config,
    write_verified_json_object, AgentAdapter,
};

pub struct GrokAdapter;

impl AgentAdapter for GrokAdapter {
    fn id(&self) -> AgentId {
        AgentId::Grok
    }

    fn detect(&self) -> DetectResult {
        let requires = self
            .install_channels()
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

    fn install_channels(&self) -> Vec<InstallChannel> {
        // Native official script only — no public npm package for Grok Build CLI.
        vec![InstallChannel {
            id: "native".into(),
            label: "Official native binary".into(),
            requires: vec![RuntimeId::PowerShell],
            min_runtime_notes: None,
        }]
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
        let auth = home.join("auth.json");
        let config = home.join("config.toml");
        let has = auth.exists() || config.exists();
        Ok(AuthState {
            agent: AgentId::Grok,
            kind: if auth.exists() {
                Some("oauth".into())
            } else if config.exists() {
                Some("apikey-in-config".into())
            } else {
                None
            },
            summary: if auth.exists() {
                "auth.json present".into()
            } else if config.exists() {
                "config present (api_key may be inline)".into()
            } else {
                "no auth".into()
            },
            has_credentials: has,
        })
    }

    fn read_account(&self) -> Result<LiveAccount> {
        let home = agent_home(AgentId::Grok)?;
        let auth_path = home.join("auth.json");
        let config_path = home.join("config.toml");
        let api_key = read_toml_string_key(&config_path, "api_key")?;
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
                write_toml_string_key(&home.join("config.toml"), "api_key", key)?;
                verify_toml_string_key(&home.join("config.toml"), "api_key", key)?;
                Ok(())
            }
            "auth_json" => {
                let body = account.credentials.get("body").cloned().ok_or_else(|| {
                    AppError::InvalidArg("Grok account credentials.body is required".into())
                })?;
                write_verified_json_object(&home.join("auth.json"), &body)?;
                // Official OAuth must win over leftover API key (read prefers api_key).
                clear_toml_string_key(&home.join("config.toml"), "api_key")?;
                // Relay base_url would keep traffic off official endpoint.
                clear_toml_string_key(&home.join("config.toml"), "base_url")?;
                Ok(())
            }
            "grok_bundle" => {
                if let Some(key) = account.credentials.get("api_key").and_then(|v| v.as_str()) {
                    write_toml_string_key(&home.join("config.toml"), "api_key", key)?;
                    verify_toml_string_key(&home.join("config.toml"), "api_key", key)?;
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
        // text: grok -p / --single <prompt> (default plain)
        // structured (Chat): --output-format streaming-json (ACP NDJSON)
        let mut args = vec!["-p".into(), prompt.to_string()];
        if super::wants_structured_for(opts.process_mode, AgentId::Grok) {
            args.push("--output-format".into());
            args.push("streaming-json".into());
        }
        if opts.allow_dangerous {
            args.insert(0, "--always-approve".into());
        }
        Ok(RunSpec {
            agent: AgentId::Grok,
            program: binary.to_path_buf(),
            args,
            cwd: opts.cwd.clone(),
            env: vec![],
        })
    }
}

fn read_toml_string_key(path: &Path, key: &str) -> Result<Option<String>> {
    if !path.exists() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(path)?;
    let doc = text
        .parse::<toml_edit::DocumentMut>()
        .map_err(|e| AppError::InvalidArg(format!("invalid Grok config.toml: {e}")))?;
    Ok(doc
        .get(key)
        .and_then(|item| item.as_str())
        .map(|s| s.to_string()))
}

fn write_toml_string_key(path: &Path, key: &str, value: &str) -> Result<()> {
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
    doc[key] = toml_edit::value(value);
    atomic_write(path, doc.to_string().as_bytes())
}

fn verify_toml_string_key(path: &Path, key: &str, expected: &str) -> Result<()> {
    let got = read_toml_string_key(path, key)?;
    if got.as_deref() != Some(expected) {
        return Err(AppError::message(
            "account.verify",
            format!("Grok {key} verification failed after write"),
        ));
    }
    Ok(())
}

fn clear_toml_string_key(path: &Path, key: &str) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    let live = std::fs::read_to_string(path)?;
    if live.trim().is_empty() {
        return Ok(());
    }
    let mut doc = live.parse::<toml_edit::DocumentMut>().map_err(|e| {
        AppError::InvalidArg(format!("existing Grok config.toml is invalid: {e}"))
    })?;
    if doc.remove(key).is_none() {
        return Ok(());
    }
    atomic_write(path, doc.to_string().as_bytes())?;
    if read_toml_string_key(path, key)?.is_some() {
        return Err(AppError::message(
            "account.verify",
            format!("Grok {key} still present after clear"),
        ));
    }
    Ok(())
}
