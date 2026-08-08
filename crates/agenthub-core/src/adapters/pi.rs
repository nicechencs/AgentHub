use std::path::{Path, PathBuf};

use crate::error::{AppError, Result};
use crate::models::{
    AccountKind, AgentConfig, AgentId, AuthState, Capability, CapabilityState, DetectResult,
    InstallChannel, LiveAccount, RunOptions, RunSpec, RuntimeId,
};
use crate::runtime;
use crate::utils::atomic::atomic_write;
use super::{api_key_live_account, detect_binary, require_api_key, AgentAdapter};

pub struct PiAdapter;

impl AgentAdapter for PiAdapter {
    fn id(&self) -> AgentId {
        AgentId::Pi
    }

    fn detect(&self) -> DetectResult {
        let requires = self
            .install_channels()
            .first()
            .map(|c| c.requires.clone())
            .unwrap_or_default();
        let env_ready = runtime::is_ready(&requires);
        // Prefer PATH `pi` (npm global shim); channel inferred from path / well-known.
        detect_binary(
            AgentId::Pi,
            &["pi"],
            &["--version"],
            Some("npm"),
            env_ready,
        )
    }

    fn install_channels(&self) -> Vec<InstallChannel> {
        // npm only: upstream also documents `curl https://pi.dev/install.sh | sh`
        // and Homebrew, but AgentHub's native channel is Windows ps1 / Unix sh
        // allowlists. install.sh is Unix shell (not a Windows-native installer),
        // and Homebrew/winget are outside the current Runtime/InstallChannel
        // abstraction — keep channels honest rather than advertising dead paths.
        vec![InstallChannel {
            id: "npm".into(),
            label: "npm (@earendil-works/pi-coding-agent)".into(),
            requires: vec![RuntimeId::NodeJs, RuntimeId::Npm],
            min_runtime_notes: Some("Node.js >= 18; install uses --ignore-scripts".into()),
        }]
    }

    fn read_config(&self) -> Result<AgentConfig> {
        let dir = pi_config_dir()?;
        let settings_path = dir.join("settings.json");
        let models_path = dir.join("models.json");

        let settings = read_json_object_or_empty(&settings_path)?;
        let models = if models_path.exists() {
            Some(read_json_object_or_empty(&models_path)?)
        } else {
            None
        };

        // Raw envelope for display; write_config is fail-closed until merge rules
        // for settings/models are proven safe.
        let mut raw = serde_json::Map::new();
        raw.insert("settings".into(), settings);
        if let Some(m) = models {
            raw.insert("models".into(), m);
        }
        raw.insert(
            "paths".into(),
            serde_json::json!({
                "configDir": dir,
                "settings": settings_path,
                "models": models_path,
            }),
        );

        Ok(AgentConfig {
            agent: AgentId::Pi,
            raw: serde_json::Value::Object(raw),
        })
    }

    fn write_config(&self, _config: &AgentConfig) -> Result<()> {
        // settings.json / models.json schemas are multi-provider and partially
        // undocumented for atomic merge. Fail closed (do not invent writers).
        Err(AppError::Unsupported(
            "live config writes are not supported for pi \
             (settings.json / models.json merge rules not locked; use pi CLI or edit files)"
                .into(),
        ))
    }

    fn read_auth(&self) -> Result<AuthState> {
        let auth = pi_config_dir()?.join("auth.json");
        let has = auth.exists();
        Ok(AuthState {
            agent: AgentId::Pi,
            kind: if has {
                Some("file-auth.json".into())
            } else {
                None
            },
            summary: if has {
                "auth.json present (provider-keyed OAuth / credentials)".into()
            } else {
                "no auth.json".into()
            },
            has_credentials: has,
        })
    }

    fn read_account(&self) -> Result<LiveAccount> {
        let path = pi_config_dir()?.join("auth.json");
        if !path.exists() {
            return Err(AppError::NotFound(
                "no live Pi auth.json found to import".into(),
            ));
        }
        let text = std::fs::read_to_string(&path)?;
        let body: serde_json::Value = serde_json::from_str(&text)?;
        if !body.is_object() {
            return Err(AppError::InvalidArg(
                "Pi auth.json must be a JSON object".into(),
            ));
        }
        let label_hint = extract_pi_label(&body);
        let kind = infer_pi_account_kind(&body);
        Ok(LiveAccount {
            agent: AgentId::Pi,
            kind,
            credentials: serde_json::json!({
                "format": "auth_json",
                "body": body,
            }),
            label_hint,
            extra: serde_json::json!({ "source": "auth.json" }),
        })
    }

    fn apply_account(&self, account: &LiveAccount) -> Result<()> {
        if account.agent != AgentId::Pi {
            return Err(AppError::InvalidArg(
                "account agent mismatch for pi".into(),
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
                    AppError::InvalidArg("Pi account credentials.body is required".into())
                })?;
                if !body.is_object() {
                    return Err(AppError::InvalidArg(
                        "Pi account credentials.body must be a JSON object".into(),
                    ));
                }
                let path = pi_config_dir()?.join("auth.json");
                let mut bytes = serde_json::to_vec_pretty(&body)?;
                bytes.push(b'\n');
                atomic_write(&path, &bytes)?;
                let written = std::fs::read_to_string(&path)?;
                let parsed: serde_json::Value = serde_json::from_str(&written)?;
                if parsed != body {
                    return Err(AppError::message(
                        "account.verify",
                        "Pi auth.json verification failed after write",
                    ));
                }
                Ok(())
            }
            "api_key" => Err(AppError::Unsupported(
                "Pi live apply for standalone API key accounts is not supported; \
                 import auth.json (provider-keyed) or use `pi --api-key` / models.json"
                    .into(),
            )),
            other => Err(AppError::InvalidArg(format!(
                "unsupported Pi account credential format: {other}"
            ))),
        }
    }

    fn build_api_key_account(&self, api_key: &str) -> Result<LiveAccount> {
        // Allow storing a pool entry for future use / manual apply; live apply
        // of this format remains Unsupported (auth.json provider schema varies).
        let key = require_api_key(api_key)?;
        Ok(api_key_live_account(
            AgentId::Pi,
            key,
            serde_json::json!({
                "format": "api_key",
                "api_key": key,
            }),
            "API Key",
            serde_json::json!({
                "source": "manual",
                "note": "pool-only; apply to live auth.json is unsupported"
            }),
        ))
    }

    fn skills_dir(&self) -> Option<PathBuf> {
        // Global skills true source per pi docs: ~/.pi/agent/skills
        // (not project-local .pi/skills).
        pi_config_dir().ok().map(|d| d.join("skills"))
    }

    fn capability(&self, cap: Capability) -> CapabilityState {
        use Capability::*;
        match cap {
            AccountSwitch | Skills | LiveBackup | StructuredStream | ProjectHistory
            | ProjectDelete => CapabilityState::full(),
            ConfigWrite => CapabilityState::unsupported("无稳定 settings 合并契约，fail-closed"),
            ApiKeyAccount => CapabilityState::partial(
                "可入池；auth.json provider schema 不稳定，不写回",
            ),
            DangerousMode => CapabilityState::partial(
                "--approve 仅信任项目文件，非完全跳过确认",
            ),
            ProviderPresets => CapabilityState::unsupported("写入契约未锁定，无内置模板"),
            Usage => CapabilityState::full(),
            Mcp | ModelSelect | SessionResume => CapabilityState::planned("待验证接入"),
        }
    }

    fn live_backup_paths(&self) -> Vec<PathBuf> {
        let mut paths = Vec::new();
        if let Ok(dir) = pi_config_dir() {
            // Only stable, documented / measured files — no models-store cache.
            paths.push(dir.join("settings.json"));
            paths.push(dir.join("models.json"));
            paths.push(dir.join("auth.json"));
        }
        paths
    }

    fn build_run_spec(&self, binary: &Path, prompt: &str, opts: &RunOptions) -> Result<RunSpec> {
        // text: `pi -p "…"` `--mode text --no-session`
        // structured (Chat): `--mode json` event stream (NDJSON session events)
        // No public stable "always approve tools" flag — do not invent one.
        // `--approve` only trusts project-local files for the run (not tool YOLO).
        let mode = if super::wants_structured_for(opts.process_mode, AgentId::Pi) {
            "json"
        } else {
            "text"
        };
        let mut args = vec![
            "-p".into(),
            prompt.to_string(),
            "--mode".into(),
            mode.into(),
            "--no-session".into(),
        ];
        if opts.allow_dangerous {
            // Closest documented non-interactive trust flag for project files.
            args.push("--approve".into());
        }
        Ok(RunSpec {
            agent: AgentId::Pi,
            program: binary.to_path_buf(),
            args,
            cwd: opts.cwd.clone(),
            env: vec![],
        })
    }
}

/// Config root: `PI_CODING_AGENT_DIR` or `~/.pi/agent` (same as `agent_config_dir`).
fn pi_config_dir() -> Result<PathBuf> {
    crate::utils::paths::agent_config_dir(AgentId::Pi)
}

fn read_json_object_or_empty(path: &Path) -> Result<serde_json::Value> {
    if !path.exists() {
        return Ok(serde_json::json!({}));
    }
    let text = std::fs::read_to_string(path)?;
    let value: serde_json::Value = serde_json::from_str(&text)?;
    if !value.is_object() {
        return Err(AppError::InvalidArg(format!(
            "{} must be a JSON object",
            path.display()
        )));
    }
    Ok(value)
}

fn extract_pi_label(body: &serde_json::Value) -> Option<String> {
    let obj = body.as_object()?;
    let providers: Vec<&str> = obj.keys().map(|s| s.as_str()).collect();
    if providers.is_empty() {
        return Some("pi-auth".into());
    }
    if providers.len() == 1 {
        let name = providers[0];
        let ty = obj
            .get(name)
            .and_then(|v| v.get("type"))
            .and_then(|v| v.as_str())
            .unwrap_or("cred");
        return Some(format!("pi:{name} ({ty})"));
    }
    Some(format!("pi:{} providers", providers.len()))
}

fn infer_pi_account_kind(body: &serde_json::Value) -> AccountKind {
    let Some(obj) = body.as_object() else {
        return AccountKind::Oauth;
    };
    let mut saw_oauth = false;
    let mut saw_key = false;
    for (_k, v) in obj {
        match v.get("type").and_then(|t| t.as_str()) {
            Some("oauth") => saw_oauth = true,
            Some("api_key") | Some("apikey") | Some("api-key") => saw_key = true,
            _ => {
                if v.get("access").is_some() || v.get("refresh").is_some() {
                    saw_oauth = true;
                }
                if v.get("key").is_some() || v.get("apiKey").is_some() || v.get("api_key").is_some()
                {
                    saw_key = true;
                }
            }
        }
    }
    match (saw_oauth, saw_key) {
        (true, false) => AccountKind::Oauth,
        (false, true) => AccountKind::ApiKey,
        _ => AccountKind::Oauth,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn build_run_spec_print_mode() {
        let adapter = PiAdapter;
        let bin = PathBuf::from("pi");
        let opts = RunOptions::default();
        let spec = adapter.build_run_spec(&bin, "hello", &opts).unwrap();
        assert_eq!(spec.agent, AgentId::Pi);
        assert_eq!(spec.program, bin);
        assert_eq!(spec.args[0], "-p");
        assert_eq!(spec.args[1], "hello");
        assert!(spec.args.iter().any(|a| a == "--mode"));
        assert!(spec.args.iter().any(|a| a == "text"));
        assert!(spec.args.iter().any(|a| a == "--no-session"));
        assert!(!spec.args.iter().any(|a| a == "--approve"));
    }

    #[test]
    fn build_run_spec_allow_dangerous_adds_approve() {
        let adapter = PiAdapter;
        let mut opts = RunOptions::default();
        opts.allow_dangerous = true;
        let spec = adapter
            .build_run_spec(Path::new("pi"), "x", &opts)
            .unwrap();
        assert!(spec.args.iter().any(|a| a == "--approve"));
    }

    #[test]
    fn install_channels_npm_only() {
        let channels = PiAdapter.install_channels();
        assert_eq!(channels.len(), 1);
        assert_eq!(channels[0].id, "npm");
        assert!(channels[0].requires.contains(&RuntimeId::NodeJs));
        assert!(channels[0].requires.contains(&RuntimeId::Npm));
    }

    #[test]
    fn skills_dir_is_under_agent_config() {
        let dir = PiAdapter.skills_dir().expect("skills_dir");
        let s = dir.to_string_lossy().replace('\\', "/");
        assert!(
            s.ends_with("/.pi/agent/skills") || s.contains("/agent/skills"),
            "unexpected skills_dir: {s}"
        );
    }

    #[test]
    fn live_backup_paths_include_settings_and_auth() {
        let paths = PiAdapter.live_backup_paths();
        assert!(!paths.is_empty());
        let joined: Vec<String> = paths
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert!(joined.iter().any(|n| n == "settings.json"));
        assert!(joined.iter().any(|n| n == "auth.json"));
        assert!(joined.iter().any(|n| n == "models.json"));
    }

    #[test]
    fn extract_pi_label_single_provider() {
        let body = json!({
            "xai": { "type": "oauth", "access": "a", "refresh": "r", "expires": 1 }
        });
        let label = extract_pi_label(&body).unwrap();
        assert!(label.contains("xai"));
        assert!(label.contains("oauth"));
        assert_eq!(infer_pi_account_kind(&body), AccountKind::Oauth);
    }

    #[test]
    fn write_config_is_fail_closed() {
        let err = PiAdapter
            .write_config(&AgentConfig {
                agent: AgentId::Pi,
                raw: json!({}),
            })
            .unwrap_err();
        assert_eq!(err.code(), "unsupported");
    }
}
