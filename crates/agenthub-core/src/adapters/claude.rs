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
    api_key_live_account, detect_binary, require_api_key, write_json_config, AgentAdapter,
};

pub struct ClaudeAdapter;

impl AgentAdapter for ClaudeAdapter {
    fn id(&self) -> AgentId {
        AgentId::Claude
    }

    fn detect(&self) -> DetectResult {
        let channels = self.install_channels();
        let default_requires = channels
            .first()
            .map(|c| c.requires.as_slice())
            .unwrap_or(&[]);
        let env_ready = runtime::is_ready(default_requires);
        // Prefer `claude` on PATH (npm or native); channel inferred from path.
        detect_binary(
            AgentId::Claude,
            &["claude"],
            &["--version"],
            None,
            env_ready,
        )
    }

    fn install_channels(&self) -> Vec<InstallChannel> {
        // native first — aligns with frontend `src/config/agents.ts` and common
        // Windows install path; `env_ready` is computed from channels[0].
        vec![
            InstallChannel {
                id: "native".into(),
                label: "Official native installer".into(),
                requires: vec![RuntimeId::PowerShell],
                min_runtime_notes: None,
            },
            InstallChannel {
                id: "npm".into(),
                label: "npm (@anthropic-ai/claude-code)".into(),
                requires: vec![RuntimeId::NodeJs, RuntimeId::Npm],
                min_runtime_notes: Some("Node.js >= 18".into()),
            },
        ]
    }

    fn read_config(&self) -> Result<AgentConfig> {
        let path = agent_home(AgentId::Claude)?.join("settings.json");
        let raw = if path.exists() {
            let text = std::fs::read_to_string(&path)?;
            serde_json::from_str(&text)?
        } else {
            serde_json::json!({})
        };
        Ok(AgentConfig {
            agent: AgentId::Claude,
            raw,
        })
    }

    fn write_config(&self, config: &AgentConfig) -> Result<()> {
        let path = agent_home(AgentId::Claude)?.join("settings.json");
        write_json_config(&path, config)
    }

    fn read_auth(&self) -> Result<AuthState> {
        // Official OAuth: macOS Keychain first, then credentials file under agent home.
        let oauth = read_claude_oauth_bundle()?;
        if oauth.is_some() {
            return Ok(AuthState {
                agent: AgentId::Claude,
                kind: Some("oauth".into()),
                summary: "Claude OAuth credentials located".into(),
                has_credentials: true,
            });
        }
        let home = agent_home(AgentId::Claude)?;
        let settings_path = home.join("settings.json");
        if read_claude_settings_token(&settings_path)?.is_some() {
            return Ok(AuthState {
                agent: AgentId::Claude,
                kind: Some("api_key".into()),
                summary: "API key present in settings.json".into(),
                has_credentials: true,
            });
        }
        Ok(AuthState {
            agent: AgentId::Claude,
            kind: None,
            summary: "no Claude credentials found (settings API key or official login state)"
                .into(),
            has_credentials: false,
        })
    }

    fn read_account(&self) -> Result<LiveAccount> {
        let home = agent_home(AgentId::Claude)?;
        let settings_path = home.join("settings.json");
        // Prefer explicit API key in settings (provider-style live config).
        if let Some((env_key, token)) = read_claude_settings_token(&settings_path)? {
            return Ok(LiveAccount {
                agent: AgentId::Claude,
                kind: AccountKind::ApiKey,
                credentials: serde_json::json!({
                    "format": "api_key",
                    "api_key": token,
                    "env_key": env_key,
                }),
                label_hint: Some(format!("{} (API Key)", mask_secret_preview(&token))),
                extra: serde_json::json!({ "source": "settings.json" }),
            });
        }

        // Official OAuth — keychain / credentials file.
        if let Some(bundle) = read_claude_oauth_bundle()? {
            let mut extra = serde_json::json!({
                "source": bundle.source,
            });
            if let Some(obj) = extra.as_object_mut() {
                if let Some(exp) = bundle.expires_at {
                    obj.insert("expiresAt".into(), exp);
                }
                if bundle.expired {
                    obj.insert("tokenExpired".into(), serde_json::json!(true));
                }
            }
            let label = format!(
                "Claude OAuth ({})",
                mask_secret_preview(&bundle.access_token)
            );
            return Ok(LiveAccount {
                agent: AgentId::Claude,
                kind: AccountKind::Oauth,
                credentials: serde_json::json!({
                    "format": "credentials_json",
                    "body": bundle.body,
                }),
                label_hint: Some(label),
                extra,
            });
        }

        Err(AppError::Unsupported(
            "Claude live OAuth credentials not found. Official login state may live outside \
             a single config file. Use API Key accounts, import after CLI login, \
             or re-login via `claude` so credentials become available"
                .into(),
        ))
    }

    fn apply_account(&self, account: &LiveAccount) -> Result<()> {
        if account.agent != AgentId::Claude {
            return Err(AppError::InvalidArg(
                "account agent mismatch for claude".into(),
            ));
        }
        let home = agent_home(AgentId::Claude)?;
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
                    .ok_or_else(|| AppError::InvalidArg("Claude api_key is required".into()))?;
                let env_key = account
                    .credentials
                    .get("env_key")
                    .and_then(|v| v.as_str())
                    .unwrap_or("ANTHROPIC_AUTH_TOKEN");
                write_claude_settings_token(&home.join("settings.json"), env_key, key)?;
                Ok(())
            }
            "credentials_json" => {
                let body = account.credentials.get("body").cloned().ok_or_else(|| {
                    AppError::InvalidArg("Claude credentials body is required".into())
                })?;
                if !body.is_object() {
                    return Err(AppError::InvalidArg(
                        "Claude credentials body must be a JSON object".into(),
                    ));
                }
                let path = home.join(".credentials.json");
                let mut bytes = serde_json::to_vec_pretty(&body)?;
                bytes.push(b'\n');
                atomic_write(&path, &bytes)?;
                let written = std::fs::read_to_string(&path)?;
                let parsed: serde_json::Value = serde_json::from_str(&written)?;
                if parsed != body {
                    return Err(AppError::message(
                        "account.verify",
                        "Claude .credentials.json verification failed after write",
                    ));
                }
                // Official OAuth must win over leftover API/relay env from provider mode.
                // read_account prefers settings.json tokens when present.
                clear_claude_settings_api_auth(&home.join("settings.json"))?;
                Ok(())
            }
            other => Err(AppError::InvalidArg(format!(
                "unsupported Claude account credential format: {other}"
            ))),
        }
    }

    fn build_api_key_account(&self, api_key: &str) -> Result<LiveAccount> {
        let key = require_api_key(api_key)?;
        Ok(api_key_live_account(
            AgentId::Claude,
            key,
            serde_json::json!({
                "format": "api_key",
                "api_key": key,
                "env_key": "ANTHROPIC_AUTH_TOKEN",
            }),
            "API Key",
            serde_json::json!({ "source": "manual" }),
        ))
    }

    fn skills_dir(&self) -> Option<PathBuf> {
        agent_home(AgentId::Claude).ok().map(|h| h.join("skills"))
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
        // Prefer agent_home so CLAUDE_CONFIG_DIR is honored for settings / credentials.
        // ~/.claude.json (MCP / global) stays under the user home root (not under config dir).
        let mut paths = Vec::new();
        if let Ok(claude) = agent_home(AgentId::Claude) {
            paths.push(claude.join("settings.json"));
            paths.push(claude.join(".credentials.json"));
        }
        if let Ok(home) = home_dir() {
            paths.push(home.join(".claude.json"));
        }
        paths
    }

    fn build_run_spec(&self, binary: &Path, prompt: &str, opts: &RunOptions) -> Result<RunSpec> {
        // text: claude -p <prompt> --output-format text
        // structured (Chat): stream-json + verbose for tool/turn events
        let format = if super::wants_structured_for(opts.process_mode, AgentId::Claude) {
            "stream-json"
        } else {
            "text"
        };
        let mut args = vec![
            "-p".into(),
            prompt.to_string(),
            "--output-format".into(),
            format.into(),
        ];
        if format == "stream-json" {
            // Full turn-by-turn stream (tool_use / result); partial tokens optional later.
            args.push("--verbose".into());
        }
        if opts.allow_dangerous {
            args.push("--dangerously-skip-permissions".into());
        }
        Ok(RunSpec {
            agent: AgentId::Claude,
            program: binary.to_path_buf(),
            args,
            cwd: opts.cwd.clone(),
            env: vec![],
        })
    }
}

fn read_claude_settings_token(path: &Path) -> Result<Option<(String, String)>> {
    if !path.exists() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(path)?;
    let value: serde_json::Value = serde_json::from_str(&text)?;
    let env = value.get("env").and_then(|v| v.as_object());
    let Some(env) = env else {
        return Ok(None);
    };
    for key in ["ANTHROPIC_AUTH_TOKEN", "ANTHROPIC_API_KEY"] {
        if let Some(token) = env.get(key).and_then(|v| v.as_str()) {
            if !token.is_empty() {
                return Ok(Some((key.to_string(), token.to_string())));
            }
        }
    }
    Ok(None)
}

fn write_claude_settings_token(path: &Path, env_key: &str, token: &str) -> Result<()> {
    let mut value = if path.exists() {
        let text = std::fs::read_to_string(path)?;
        serde_json::from_str::<serde_json::Value>(&text)?
    } else {
        serde_json::json!({})
    };
    if !value.is_object() {
        return Err(AppError::InvalidArg(
            "Claude settings.json must be a JSON object".into(),
        ));
    }
    let obj = value.as_object_mut().expect("object");
    let env = obj.entry("env").or_insert_with(|| serde_json::json!({}));
    let env_obj = env
        .as_object_mut()
        .ok_or_else(|| AppError::InvalidArg("Claude settings.json env must be an object".into()))?;
    // Keep one auth field active.
    for k in ["ANTHROPIC_AUTH_TOKEN", "ANTHROPIC_API_KEY"] {
        if k != env_key {
            env_obj.remove(k);
        }
    }
    env_obj.insert(env_key.to_string(), serde_json::Value::String(token.into()));
    let mut bytes = serde_json::to_vec_pretty(&value)?;
    bytes.push(b'\n');
    atomic_write(path, &bytes)?;
    match read_claude_settings_token(path)? {
        Some((k, v)) if k == env_key && v == token => Ok(()),
        _ => Err(AppError::message(
            "account.verify",
            "Claude settings token verification failed after write",
        )),
    }
}

/// Remove API/relay auth env so official OAuth credentials take effect.
///
/// Keys cleared: `ANTHROPIC_AUTH_TOKEN`, `ANTHROPIC_API_KEY`, `ANTHROPIC_BASE_URL`.
/// Missing file / empty env is a no-op. Other settings (model, etc.) are preserved.
fn clear_claude_settings_api_auth(path: &Path) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    let text = std::fs::read_to_string(path)?;
    let mut value: serde_json::Value = serde_json::from_str(&text)?;
    let Some(obj) = value.as_object_mut() else {
        return Ok(());
    };
    let Some(env) = obj.get_mut("env") else {
        return Ok(());
    };
    let Some(env_obj) = env.as_object_mut() else {
        return Ok(());
    };
    let mut changed = false;
    for k in [
        "ANTHROPIC_AUTH_TOKEN",
        "ANTHROPIC_API_KEY",
        "ANTHROPIC_BASE_URL",
    ] {
        if env_obj.remove(k).is_some() {
            changed = true;
        }
    }
    if !changed {
        return Ok(());
    }
    if env_obj.is_empty() {
        obj.remove("env");
    }
    let mut bytes = serde_json::to_vec_pretty(&value)?;
    bytes.push(b'\n');
    atomic_write(path, &bytes)?;
    if read_claude_settings_token(path)?.is_some() {
        return Err(AppError::message(
            "account.verify",
            "Claude settings API auth still present after clear",
        ));
    }
    Ok(())
}

// ── Claude OAuth credential discovery ───────────────────────────────────────

struct ClaudeOauthBundle {
    /// Full credentials JSON body to persist / re-apply.
    body: serde_json::Value,
    access_token: String,
    expires_at: Option<serde_json::Value>,
    expired: bool,
    source: &'static str,
}

/// Read Claude official OAuth credentials.
///
/// Priority:
/// 1. macOS Keychain service `Claude Code-credentials` (when available)
/// 2. credentials file under Claude agent home
///
/// JSON keys accepted: `claudeAiOauth` | `claude.ai_oauth`
/// with `accessToken` / `expiresAt` (and optional `refreshToken`).
fn read_claude_oauth_bundle() -> Result<Option<ClaudeOauthBundle>> {
    #[cfg(target_os = "macos")]
    {
        if let Some(bundle) = read_claude_oauth_from_keychain()? {
            return Ok(Some(bundle));
        }
    }
    read_claude_oauth_from_file()
}

#[cfg(target_os = "macos")]
fn read_claude_oauth_from_keychain() -> Result<Option<ClaudeOauthBundle>> {
    let output = std::process::Command::new("security")
        .args([
            "find-generic-password",
            "-s",
            "Claude Code-credentials",
            "-w",
        ])
        .output();
    let output = match output {
        Ok(o) if o.status.success() => o,
        _ => return Ok(None),
    };
    let json_str = match String::from_utf8(output.stdout) {
        Ok(s) => s,
        Err(_) => return Ok(None),
    };
    let json_str = json_str.trim();
    if json_str.is_empty() {
        return Ok(None);
    }
    Ok(parse_claude_oauth_json(json_str, "macos-keychain"))
}

fn read_claude_oauth_from_file() -> Result<Option<ClaudeOauthBundle>> {
    let path = agent_home(AgentId::Claude)?.join(".credentials.json");
    if !path.exists() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(&path)?;
    Ok(parse_claude_oauth_json(&content, ".credentials.json"))
}

fn parse_claude_oauth_json(content: &str, source: &'static str) -> Option<ClaudeOauthBundle> {
    let body: serde_json::Value = serde_json::from_str(content).ok()?;
    let entry = body
        .get("claudeAiOauth")
        .or_else(|| body.get("claude.ai_oauth"))?;
    let access_token = entry
        .get("accessToken")
        .or_else(|| entry.get("access_token"))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())?
        .to_string();
    let expires_at = entry
        .get("expiresAt")
        .or_else(|| entry.get("expires_at"))
        .cloned();
    let expired = expires_at
        .as_ref()
        .map(is_claude_token_expired)
        .unwrap_or(false);
    Some(ClaudeOauthBundle {
        body,
        access_token,
        expires_at,
        expired,
        source,
    })
}

/// Accept unix seconds/millis or RFC3339 / naive ISO strings.
fn is_claude_token_expired(expires_at: &serde_json::Value) -> bool {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    match expires_at {
        serde_json::Value::Number(n) => {
            let Some(ts) = n.as_u64() else {
                return false;
            };
            let ts_secs = if ts > 1_000_000_000_000 { ts / 1000 } else { ts };
            ts_secs < now_secs
        }
        serde_json::Value::String(s) => {
            if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
                (dt.timestamp() as u64) < now_secs
            } else if let Ok(dt) =
                chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f")
            {
                (dt.and_utc().timestamp() as u64) < now_secs
            } else {
                false
            }
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_claude_oauth_accepts_camel_and_dotted_keys() {
        let a = parse_claude_oauth_json(
            r#"{"claudeAiOauth":{"accessToken":"tok-aaa","expiresAt":9999999999}}"#,
            "test",
        )
        .expect("camel key");
        assert_eq!(a.access_token, "tok-aaa");
        assert!(!a.expired);
        assert_eq!(a.source, "test");

        let b = parse_claude_oauth_json(
            r#"{"claude.ai_oauth":{"accessToken":"tok-bbb","expiresAt":1}}"#,
            "test",
        )
        .expect("dotted key");
        assert_eq!(b.access_token, "tok-bbb");
        assert!(b.expired);
    }

    #[test]
    fn parse_claude_oauth_rejects_missing_or_empty_token() {
        assert!(parse_claude_oauth_json(r#"{"mcpOAuth":{}}"#, "t").is_none());
        assert!(parse_claude_oauth_json(
            r#"{"claudeAiOauth":{"accessToken":""}}"#,
            "t"
        )
        .is_none());
    }

    #[test]
    fn is_token_expired_handles_millis_and_iso() {
        assert!(is_claude_token_expired(&json!(1)));
        assert!(!is_claude_token_expired(&json!(9_999_999_999_u64)));
        // millis
        assert!(is_claude_token_expired(&json!(1_000_u64)));
        assert!(!is_claude_token_expired(&json!(
            "2099-01-01T00:00:00.000Z"
        )));
        assert!(is_claude_token_expired(&json!("2000-01-01T00:00:00Z")));
    }

    #[test]
    fn clear_claude_settings_api_auth_removes_token_and_base_url() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("settings.json");
        std::fs::write(
            &path,
            r#"{
  "model": "sonnet",
  "env": {
    "ANTHROPIC_AUTH_TOKEN": "sk-test",
    "ANTHROPIC_BASE_URL": "https://relay.example.com",
    "OTHER": "keep"
  }
}
"#,
        )
        .expect("write");
        clear_claude_settings_api_auth(&path).expect("clear");
        let text = std::fs::read_to_string(&path).expect("read");
        let v: serde_json::Value = serde_json::from_str(&text).expect("json");
        assert_eq!(v["model"], "sonnet");
        assert_eq!(v["env"]["OTHER"], "keep");
        assert!(v["env"].get("ANTHROPIC_AUTH_TOKEN").is_none());
        assert!(v["env"].get("ANTHROPIC_BASE_URL").is_none());
        assert!(read_claude_settings_token(&path).expect("read token").is_none());
    }
}
