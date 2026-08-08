use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::error::{AppError, Result};
use crate::models::{
    AccountKind, AgentConfig, AgentId, AuthState, Capability, CapabilityState, DetectResult,
    InstallChannel, LiveAccount, RunOptions, RunSpec, RuntimeId,
};
use crate::runtime;
use crate::utils::atomic::atomic_write;
use crate::utils::paths::{agent_home, home_dir};

use super::{
    api_key_live_account, detect_binary, require_api_key, write_toml_config, AgentAdapter,
};

pub struct CodexAdapter;

impl AgentAdapter for CodexAdapter {
    fn id(&self) -> AgentId {
        AgentId::Codex
    }

    fn detect(&self) -> DetectResult {
        let requires = self
            .install_channels()
            .first()
            .map(|c| c.requires.clone())
            .unwrap_or_default();
        let env_ready = runtime::is_ready(&requires);
        detect_binary(
            AgentId::Codex,
            &["codex"],
            &["--version"],
            Some("npm"),
            env_ready,
        )
    }

    fn install_channels(&self) -> Vec<InstallChannel> {
        vec![
            InstallChannel {
                id: "npm".into(),
                label: "npm (@openai/codex)".into(),
                requires: vec![RuntimeId::NodeJs, RuntimeId::Npm],
                min_runtime_notes: Some("Node.js >= 18".into()),
            },
            InstallChannel {
                id: "native".into(),
                label: "Official install script".into(),
                requires: vec![RuntimeId::PowerShell],
                min_runtime_notes: None,
            },
        ]
    }

    fn read_config(&self) -> Result<AgentConfig> {
        // Dual shape settingsConfig:
        //   { "format": "toml", "content": "<config.toml>", "auth": { "OPENAI_API_KEY": "..." } }
        // `auth` is only attached when live auth holds a non-empty API key
        // (OAuth token blobs stay out of the provider pool).
        let home = agent_home(AgentId::Codex)?;
        let path = home.join("config.toml");
        let mut raw = if path.exists() {
            let text = std::fs::read_to_string(&path)?;
            json!({ "format": "toml", "content": text })
        } else {
            json!({})
        };
        if let Some(api_key) = read_live_openai_api_key(&home.join("auth.json"))? {
            if let Some(obj) = raw.as_object_mut() {
                obj.insert("auth".into(), json!({ "OPENAI_API_KEY": api_key }));
            }
        }
        Ok(AgentConfig {
            agent: AgentId::Codex,
            raw,
        })
    }

    fn write_config(&self, config: &AgentConfig) -> Result<()> {
        let home = agent_home(AgentId::Codex)?;
        let path = home.join("config.toml");
        write_toml_config(AgentId::Codex, &path, config)?;
        // API providers may also write live auth with OPENAI_API_KEY.
        if let Some(api_key) = extract_settings_openai_api_key(&config.raw) {
            write_codex_api_key_auth(&home.join("auth.json"), &api_key)?;
        }
        Ok(())
    }

    fn read_auth(&self) -> Result<AuthState> {
        let auth = agent_home(AgentId::Codex)?.join("auth.json");
        let has = auth.exists();
        Ok(AuthState {
            agent: AgentId::Codex,
            kind: if has { Some("oauth".into()) } else { None },
            summary: if has {
                "auth.json present".into()
            } else {
                "no auth.json".into()
            },
            has_credentials: has,
        })
    }

    fn read_account(&self) -> Result<LiveAccount> {
        let path = agent_home(AgentId::Codex)?.join("auth.json");
        if !path.exists() {
            return Err(AppError::NotFound(
                "no live Codex auth.json found to import".into(),
            ));
        }
        let text = std::fs::read_to_string(&path)?;
        let body: serde_json::Value = serde_json::from_str(&text)?;
        let label_hint = extract_codex_label(&body);
        Ok(LiveAccount {
            agent: AgentId::Codex,
            kind: AccountKind::Oauth,
            credentials: serde_json::json!({
                "format": "auth_json",
                "body": body,
            }),
            label_hint,
            extra: serde_json::json!({ "source": "auth.json" }),
        })
    }

    fn apply_account(&self, account: &LiveAccount) -> Result<()> {
        if account.agent != AgentId::Codex {
            return Err(AppError::InvalidArg(
                "account agent mismatch for codex".into(),
            ));
        }
        let format = account
            .credentials
            .get("format")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        match format {
            "auth_json" => {
                let body = account.credentials.get("body").cloned().ok_or_else(|| {
                    AppError::InvalidArg("Codex account credentials.body is required".into())
                })?;
                if !body.is_object() {
                    return Err(AppError::InvalidArg(
                        "Codex account credentials.body must be a JSON object".into(),
                    ));
                }
                let home = agent_home(AgentId::Codex)?;
                let path = home.join("auth.json");
                let mut bytes = serde_json::to_vec_pretty(&body)?;
                bytes.push(b'\n');
                atomic_write(&path, &bytes)?;
                let written = std::fs::read_to_string(&path)?;
                let parsed: serde_json::Value = serde_json::from_str(&written)?;
                if parsed != body {
                    return Err(AppError::message(
                        "account.verify",
                        "Codex auth.json verification failed after write",
                    ));
                }
                // Drop preferred_auth_method=apikey so OAuth auth.json is used
                // after switching back from an API provider.
                clear_codex_apikey_auth_preference(&home.join("config.toml"))?;
                Ok(())
            }
            "api_key" => Err(AppError::Unsupported(
                "Codex live apply for API key accounts is not supported; import OAuth auth.json or use provider config".into(),
            )),
            other => Err(AppError::InvalidArg(format!(
                "unsupported Codex account credential format: {other}"
            ))),
        }
    }

    fn build_api_key_account(&self, api_key: &str) -> Result<LiveAccount> {
        // Codex primary auth is OAuth via auth.json; still allow storing an
        // OpenAI API key shaped credential for pool management / future apply.
        let key = require_api_key(api_key)?;
        Ok(api_key_live_account(
            AgentId::Codex,
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
        home_dir().ok().map(|h| h.join(".codex").join("skills"))
    }

    fn capability(&self, cap: Capability) -> CapabilityState {
        use Capability::*;
        match cap {
            ConfigWrite | AccountSwitch | Skills | LiveBackup | StructuredStream
            | DangerousMode | ProjectHistory | ProjectDelete | ProviderPresets => {
                CapabilityState::full()
            }
            ApiKeyAccount => CapabilityState::partial(
                "可入池；live 应用仅支持 OAuth auth.json",
            ),
            Usage => CapabilityState::full(),
            Mcp | ModelSelect | SessionResume => CapabilityState::planned("待验证接入"),
        }
    }

    fn live_backup_paths(&self) -> Vec<PathBuf> {
        let mut paths = Vec::new();
        if let Ok(home) = agent_home(AgentId::Codex) {
            paths.push(home.join("config.toml"));
            paths.push(home.join("auth.json"));
        }
        paths
    }

    fn build_run_spec(&self, binary: &Path, prompt: &str, opts: &RunOptions) -> Result<RunSpec> {
        // text: codex exec <prompt>
        // structured (Chat): codex exec --json <prompt>  → JSONL process events
        let mut args = vec!["exec".into()];
        if super::wants_structured_for(opts.process_mode, AgentId::Codex) {
            args.push("--json".into());
        }
        if opts.allow_dangerous {
            args.push("--dangerously-bypass-approvals-and-sandbox".into());
        }
        args.push(prompt.to_string());
        Ok(RunSpec {
            agent: AgentId::Codex,
            program: binary.to_path_buf(),
            args,
            cwd: opts.cwd.clone(),
            env: vec![],
        })
    }
}

fn extract_codex_label(body: &serde_json::Value) -> Option<String> {
    body.get("account")
        .and_then(|a| a.get("email"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .or_else(|| {
            body.get("email")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
        .or_else(|| Some("codex-oauth".into()))
}

/// Read OPENAI_API_KEY from live auth.json when it is a non-empty string.
/// OAuth-shaped files often set `"OPENAI_API_KEY": null` — those are ignored.
fn read_live_openai_api_key(path: &Path) -> Result<Option<String>> {
    if !path.exists() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(path)?;
    let body: Value = serde_json::from_str(&text)?;
    Ok(body
        .get("OPENAI_API_KEY")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string()))
}

/// Pull API key from provider settings_config (AgentHub + dual-shape aliases).
fn extract_settings_openai_api_key(raw: &Value) -> Option<String> {
    let auth = raw.get("auth")?;
    auth.get("OPENAI_API_KEY")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

/// Write API-key mode auth.json. Replaces OAuth blob.
fn write_codex_api_key_auth(path: &Path, api_key: &str) -> Result<()> {
    let body = json!({ "OPENAI_API_KEY": api_key });
    let mut bytes = serde_json::to_vec_pretty(&body)?;
    bytes.push(b'\n');
    atomic_write(path, &bytes)?;
    let written = std::fs::read_to_string(path)?;
    let parsed: Value = serde_json::from_str(&written)?;
    match parsed.get("OPENAI_API_KEY").and_then(|v| v.as_str()) {
        Some(v) if v == api_key => Ok(()),
        _ => Err(AppError::message(
            "provider.verify",
            "Codex auth.json OPENAI_API_KEY verification failed after write",
        )),
    }
}

/// When switching to official OAuth, remove API-key auth preference left by provider mode.
fn clear_codex_apikey_auth_preference(path: &Path) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    let live = std::fs::read_to_string(path)?;
    if live.trim().is_empty() {
        return Ok(());
    }
    let mut doc = live.parse::<toml_edit::DocumentMut>().map_err(|e| {
        AppError::InvalidArg(format!("existing Codex config.toml is invalid: {e}"))
    })?;
    let pref = doc
        .get("preferred_auth_method")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    if pref.as_deref() != Some("apikey") {
        return Ok(());
    }
    doc.remove("preferred_auth_method");
    atomic_write(path, doc.to_string().as_bytes())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn extract_settings_openai_api_key_reads_auth_block() {
        let raw = json!({
            "format": "toml",
            "content": "model = \"gpt-5\"\n",
            "auth": { "OPENAI_API_KEY": "sk-test-key" }
        });
        assert_eq!(
            extract_settings_openai_api_key(&raw).as_deref(),
            Some("sk-test-key")
        );
        assert!(extract_settings_openai_api_key(&json!({"format":"toml","content":"x"})).is_none());
        assert!(extract_settings_openai_api_key(&json!({"auth":{"OPENAI_API_KEY":""}})).is_none());
    }

    #[test]
    fn write_codex_api_key_auth_roundtrips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("auth.json");
        write_codex_api_key_auth(&path, "sk-from-pool").unwrap();
        let auth: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(auth["OPENAI_API_KEY"], "sk-from-pool");
        assert_eq!(
            read_live_openai_api_key(&path).unwrap().as_deref(),
            Some("sk-from-pool")
        );
    }

    #[test]
    fn read_live_openai_api_key_ignores_oauth_null() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("auth.json");
        std::fs::write(
            &path,
            r#"{ "OPENAI_API_KEY": null, "auth_mode": "chatgpt", "tokens": {} }"#,
        )
        .unwrap();
        assert!(read_live_openai_api_key(&path).unwrap().is_none());
    }
}
