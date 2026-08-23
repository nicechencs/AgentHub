use std::path::{Path, PathBuf};

use crate::error::{AppError, Result};
use crate::models::{
    AccountKind, AgentConfig, AgentId, AuthState, Capability, CapabilityState, DetectResult,
    LiveAccount, RunOptions, RunSpec,
};
use crate::runtime;
use crate::utils::atomic::atomic_write;
use crate::utils::expiry::is_expired;
use crate::utils::paths::{agent_home, home_dir};
use crate::utils::redact::mask_secret_preview;

use super::{
    api_key_live_account, auth_file_revision, detect_binary, inspect_auth_credentials,
    oauth_auth_health, require_api_key, write_json_config, AgentAdapter,
};

pub struct ClaudeAdapter;

/// Standalone install probe used by platform detectors (no full adapter required).
pub(crate) fn detect_installation() -> DetectResult {
    let channels = crate::catalog::install::adapter_install_channels(AgentId::Claude);
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

impl AgentAdapter for ClaudeAdapter {
    fn id(&self) -> AgentId {
        AgentId::Claude
    }

    fn detect(&self) -> DetectResult {
        detect_installation()
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
        let home = agent_home(AgentId::Claude)?;
        let state = claude_auth_state(&home)?;
        Ok(with_claude_keychain_also_present(state))
    }

    fn read_account(&self) -> Result<LiveAccount> {
        let home = agent_home(AgentId::Claude)?;
        let settings_path = home.join("settings.json");
        // Prefer explicit API key in settings (provider-style live config).
        if let Some(account) = read_settings_api_key_account(&settings_path)? {
            return Ok(account);
        }

        // Official OAuth — keychain / credentials file.
        if let Some(bundle) = read_claude_oauth_bundle()? {
            let mut extra = serde_json::json!({
                "source": bundle.source.as_str(),
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
                let base_url = account.credentials.get("base_url").and_then(|v| v.as_str());
                write_claude_settings_token(&home.join("settings.json"), env_key, key, base_url)?;
                Ok(())
            }
            "credentials_json" => {
                ensure_claude_oauth_file_apply_supported()?;
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
            SessionResume => {
                CapabilityState::partial("Chat 后续轮次走 print+resume；终端可复制官方续接命令")
            }
            Mcp | ModelSelect => CapabilityState::planned("待验证接入"),
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
        let mut args = vec!["-p".into()];
        if let Some(sid) = opts
            .native_session_id
            .as_deref()
            .and_then(super::session_resume::valid_session_id)
        {
            args.push("--resume".into());
            args.push(sid.to_string());
        }
        args.push(prompt.to_string());
        args.push("--output-format".into());
        args.push(format.into());
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

/// Classify Claude auth under `home` (settings + same-home credentials file).
///
/// macOS Keychain is still consulted for official OAuth / `also_present`,
/// matching production `read_auth`. File OAuth is read from
/// `home/.credentials.json` and does not re-enter `agent_home()`.
pub(crate) fn claude_auth_state(home: &Path) -> Result<AuthState> {
    let settings_path = home.join("settings.json");
    // Match read_account: an explicit settings token is the effective
    // auth mode even when stale OAuth credentials remain on disk.
    match read_claude_settings_token(&settings_path) {
        Ok(Some(_)) => {
            let state = AuthState {
                agent: AgentId::Claude,
                kind: Some("api_key".into()),
                summary: "API key present in settings.json".into(),
                has_credentials: true,
                health: crate::models::AuthHealth::Configured,
                source: Some("claude:settings.json".into()),
                revision: auth_file_revision(&settings_path),
                also_present: Vec::new(),
            };
            return Ok(if claude_file_oauth_present(home) {
                state.with_also_present(["oauth"])
            } else {
                state
            });
        }
        Ok(None) => {}
        Err(_) => {
            return Ok(AuthState {
                agent: AgentId::Claude,
                kind: None,
                summary: "Claude settings.json could not be parsed".into(),
                has_credentials: false,
                health: crate::models::AuthHealth::Unknown,
                source: Some("claude:settings.json".into()),
                revision: auth_file_revision(&settings_path),
                also_present: Vec::new(),
            });
        }
    }
    // Official OAuth: macOS Keychain first, then credentials file under
    // the given home (not a second `agent_home()` lookup).
    let credentials_path = home.join(".credentials.json");
    if let Some(bundle) = read_claude_oauth_for_home(home)? {
        let mut metadata = inspect_auth_credentials(&bundle.body);
        metadata.access_expired = metadata.access_expired.or(Some(bundle.expired));
        let health = oauth_auth_health(metadata);
        return Ok(AuthState {
            agent: AgentId::Claude,
            kind: Some("oauth".into()),
            summary: if health == crate::models::AuthHealth::NeedsLogin {
                "Claude OAuth credentials are expired; sign in again".into()
            } else {
                "Claude OAuth credentials located".into()
            },
            has_credentials: true,
            health,
            source: Some(format!("claude:{}", bundle.source.as_str())),
            revision: claude_oauth_revision(&bundle, &credentials_path),
            also_present: Vec::new(),
        });
    }
    if credentials_path.is_file() {
        return Ok(AuthState {
            agent: AgentId::Claude,
            kind: None,
            summary: "Claude credentials file could not be classified".into(),
            has_credentials: false,
            health: crate::models::AuthHealth::Unknown,
            source: Some("claude:.credentials.json".into()),
            revision: auth_file_revision(&credentials_path),
            also_present: Vec::new(),
        });
    }
    Ok(AuthState {
        agent: AgentId::Claude,
        kind: None,
        summary: "no Claude credentials found (settings API key or official login state)".into(),
        has_credentials: false,
        health: crate::models::AuthHealth::Missing,
        source: Some("claude:settings.json".into()),
        revision: auth_file_revision(&settings_path),
        also_present: Vec::new(),
    })
}

/// File OAuth under `home` only. Keychain is merged in `read_auth` so unit
/// tests against a tempdir are not polluted by the developer machine login.
fn claude_file_oauth_present(home: &Path) -> bool {
    matches!(
        read_claude_oauth_from_path(&home.join(".credentials.json")),
        Ok(Some(_))
    )
}

fn with_claude_keychain_also_present(state: AuthState) -> AuthState {
    if state.kind.as_deref() != Some("api_key") || !state.also_present.is_empty() {
        return state;
    }
    #[cfg(target_os = "macos")]
    {
        if matches!(read_claude_oauth_from_keychain(), Ok(Some(_))) {
            return state.with_also_present(["oauth"]);
        }
    }
    state
}

fn read_claude_oauth_for_home(home: &Path) -> Result<Option<ClaudeOauthBundle>> {
    #[cfg(target_os = "macos")]
    {
        if let Some(bundle) = read_claude_oauth_from_keychain()? {
            return Ok(Some(bundle));
        }
    }
    read_claude_oauth_from_path(&home.join(".credentials.json"))
}

fn read_claude_settings_json(path: &Path) -> Result<Option<serde_json::Value>> {
    if !path.exists() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(path)?;
    Ok(Some(serde_json::from_str(&text)?))
}

fn claude_settings_token(settings: &serde_json::Value) -> Option<(String, String)> {
    let env = settings.get("env")?.as_object()?;
    for key in ["ANTHROPIC_AUTH_TOKEN", "ANTHROPIC_API_KEY"] {
        if let Some(token) = env.get(key).and_then(|v| v.as_str()) {
            if !token.is_empty() {
                return Some((key.to_string(), token.to_string()));
            }
        }
    }
    None
}

fn claude_settings_base_url(settings: &serde_json::Value) -> Option<String> {
    settings
        .get("env")
        .and_then(|env| env.get("ANTHROPIC_BASE_URL"))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

fn read_claude_settings_token(path: &Path) -> Result<Option<(String, String)>> {
    let Some(settings) = read_claude_settings_json(path)? else {
        return Ok(None);
    };
    Ok(claude_settings_token(&settings))
}

fn read_settings_api_key_account(path: &Path) -> Result<Option<LiveAccount>> {
    let Some(settings) = read_claude_settings_json(path)? else {
        return Ok(None);
    };
    let Some((env_key, token)) = claude_settings_token(&settings) else {
        return Ok(None);
    };
    let mut credentials = serde_json::json!({
        "format": "api_key",
        "api_key": token,
        "env_key": env_key,
    });
    if let Some(base_url) = claude_settings_base_url(&settings) {
        credentials["base_url"] = serde_json::json!(base_url);
    }
    Ok(Some(LiveAccount {
        agent: AgentId::Claude,
        kind: AccountKind::ApiKey,
        credentials,
        label_hint: Some(format!("{} (API Key)", mask_secret_preview(&token))),
        extra: serde_json::json!({ "source": "settings.json" }),
    }))
}

fn write_claude_settings_token(
    path: &Path,
    env_key: &str,
    token: &str,
    base_url: Option<&str>,
) -> Result<()> {
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
    let base_url = base_url.map(str::trim).filter(|s| !s.is_empty());
    if let Some(base_url) = base_url {
        env_obj.insert(
            "ANTHROPIC_BASE_URL".to_string(),
            serde_json::Value::String(base_url.to_string()),
        );
    }
    let mut bytes = serde_json::to_vec_pretty(&value)?;
    bytes.push(b'\n');
    atomic_write(path, &bytes)?;
    match read_claude_settings_token(path)? {
        Some((k, v)) if k == env_key && v == token => {}
        _ => {
            return Err(AppError::message(
                "account.verify",
                "Claude settings token verification failed after write",
            ))
        }
    }
    if let Some(expected) = base_url {
        let settings = read_claude_settings_json(path)?.unwrap_or_else(|| serde_json::json!({}));
        if claude_settings_base_url(&settings).as_deref() != Some(expected) {
            return Err(AppError::message(
                "account.verify",
                "Claude settings base URL verification failed after write",
            ));
        }
    }
    Ok(())
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
    source: ClaudeOauthSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ClaudeOauthSource {
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    MacosKeychain,
    CredentialsFile,
}

impl ClaudeOauthSource {
    fn as_str(self) -> &'static str {
        match self {
            Self::MacosKeychain => "macos-keychain",
            Self::CredentialsFile => ".credentials.json",
        }
    }
}

fn claude_oauth_revision(bundle: &ClaudeOauthBundle, credentials_path: &Path) -> Option<String> {
    match bundle.source {
        ClaudeOauthSource::CredentialsFile => auth_file_revision(credentials_path),
        // Keychain is the effective source on macOS. It has no safe portable
        // revision probe here, so never pretend the lower-priority file is a
        // revision for it; OAuth apply below fails closed for the same reason.
        ClaudeOauthSource::MacosKeychain => None,
    }
}

fn ensure_claude_oauth_file_apply_supported() -> Result<()> {
    if let Some(bundle) = read_claude_oauth_bundle()? {
        ensure_claude_oauth_file_apply_source(bundle.source)?;
    }
    Ok(())
}

pub(crate) fn ensure_claude_oauth_file_apply_source(source: ClaudeOauthSource) -> Result<()> {
    match source {
        ClaudeOauthSource::CredentialsFile => Ok(()),
        ClaudeOauthSource::MacosKeychain => Err(AppError::Unsupported(
            "Claude OAuth account switching is unavailable while macOS Keychain is the active credential source; re-login through Claude Code or remove the Keychain entry before switching".into(),
        )),
    }
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
    Ok(parse_claude_oauth_json(
        json_str,
        ClaudeOauthSource::MacosKeychain,
    ))
}

fn read_claude_oauth_from_file() -> Result<Option<ClaudeOauthBundle>> {
    let path = agent_home(AgentId::Claude)?.join(".credentials.json");
    read_claude_oauth_from_path(&path)
}

fn read_claude_oauth_from_path(path: &Path) -> Result<Option<ClaudeOauthBundle>> {
    if !path.exists() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(path)?;
    Ok(parse_claude_oauth_json(
        &content,
        ClaudeOauthSource::CredentialsFile,
    ))
}

fn parse_claude_oauth_json(content: &str, source: ClaudeOauthSource) -> Option<ClaudeOauthBundle> {
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
    // Missing/unparseable expiry is treated as not expired (fail open for display).
    let expired = expires_at.as_ref().and_then(is_expired).unwrap_or(false);
    Some(ClaudeOauthBundle {
        body,
        access_token,
        expires_at,
        expired,
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_claude_oauth_accepts_camel_and_dotted_keys() {
        let a = parse_claude_oauth_json(
            r#"{"claudeAiOauth":{"accessToken":"tok-aaa","expiresAt":9999999999}}"#,
            ClaudeOauthSource::CredentialsFile,
        )
        .expect("camel key");
        assert_eq!(a.access_token, "tok-aaa");
        assert!(!a.expired);
        assert_eq!(a.source, ClaudeOauthSource::CredentialsFile);

        let b = parse_claude_oauth_json(
            r#"{"claude.ai_oauth":{"accessToken":"tok-bbb","expiresAt":1}}"#,
            ClaudeOauthSource::CredentialsFile,
        )
        .expect("dotted key");
        assert_eq!(b.access_token, "tok-bbb");
        assert!(b.expired);
    }

    #[test]
    fn parse_claude_oauth_rejects_missing_or_empty_token() {
        assert!(
            parse_claude_oauth_json(r#"{"mcpOAuth":{}}"#, ClaudeOauthSource::CredentialsFile)
                .is_none()
        );
        assert!(parse_claude_oauth_json(
            r#"{"claudeAiOauth":{"accessToken":""}}"#,
            ClaudeOauthSource::CredentialsFile
        )
        .is_none());
    }

    #[test]
    fn is_token_expired_handles_millis_and_iso() {
        assert_eq!(is_expired(&json!(1)), Some(true));
        assert_eq!(is_expired(&json!(9_999_999_999_u64)), Some(false));
        // small epoch numbers are seconds (year 1970), not millis
        assert_eq!(is_expired(&json!(1_000_u64)), Some(true));
        assert_eq!(is_expired(&json!("2099-01-01T00:00:00.000Z")), Some(false));
        assert_eq!(is_expired(&json!("2000-01-01T00:00:00Z")), Some(true));
    }

    #[test]
    fn read_account_persists_loopback_base_url_from_settings() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("settings.json");
        std::fs::write(
            &path,
            r#"{
  "env": {
    "ANTHROPIC_AUTH_TOKEN": "sk-bridge",
    "ANTHROPIC_BASE_URL": "http://127.0.0.1:43081"
  }
}
"#,
        )
        .expect("write");
        let account = read_settings_api_key_account(&path)
            .expect("read")
            .expect("api key account");
        assert_eq!(account.agent, AgentId::Claude);
        assert_eq!(account.kind, AccountKind::ApiKey);
        assert_eq!(account.credentials["format"], "api_key");
        assert_eq!(account.credentials["api_key"], "sk-bridge");
        assert_eq!(account.credentials["env_key"], "ANTHROPIC_AUTH_TOKEN");
        assert_eq!(account.credentials["base_url"], "http://127.0.0.1:43081");
    }

    #[test]
    fn write_claude_settings_token_restores_base_url_when_present() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("settings.json");
        write_claude_settings_token(
            &path,
            "ANTHROPIC_AUTH_TOKEN",
            "sk-bridge",
            Some("http://127.0.0.1:43081"),
        )
        .expect("write");
        let text = std::fs::read_to_string(&path).expect("read");
        let v: serde_json::Value = serde_json::from_str(&text).expect("json");
        assert_eq!(v["env"]["ANTHROPIC_AUTH_TOKEN"], "sk-bridge");
        assert_eq!(v["env"]["ANTHROPIC_BASE_URL"], "http://127.0.0.1:43081");

        write_claude_settings_token(&path, "ANTHROPIC_AUTH_TOKEN", "sk-plain", None)
            .expect("write without url");
        let text = std::fs::read_to_string(&path).expect("read");
        let v: serde_json::Value = serde_json::from_str(&text).expect("json");
        assert_eq!(v["env"]["ANTHROPIC_AUTH_TOKEN"], "sk-plain");
        assert_eq!(v["env"]["ANTHROPIC_BASE_URL"], "http://127.0.0.1:43081");
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
        assert!(read_claude_settings_token(&path)
            .expect("read token")
            .is_none());
    }
}
