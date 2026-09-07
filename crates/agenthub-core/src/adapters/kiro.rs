//! Kiro CLI adapter (half-surface).
//!
//! Product card: **Kiro**. Manages the public CLI (`kiro-cli`), **not** the
//! Kiro IDE / Web / Crew surfaces.
//!
//! ## Scope
//! - install / detect (official sh + ps1; IDE.app is never Installed)
//! - Chat: new conversations use `kiro-cli acp` (ACP session/prompt, allow/deny,
//!   cancel). Legacy print path remains `kiro-cli chat --no-interactive`
//!   (HTTP text-only when CLI is missing).
//! - headless CLI: `kiro-cli chat --no-interactive --wrap never "…"`
//!   (+ `--trust-all-tools` when dangerous; TERM=dumb so Unix color does not leak)
//! - Chat model/effort: `--model` / `--effort` from live prefs (HTTP list or CLI)
//! - Chat Auto: `--agent-engine v2 --output-format stream-json` (v1 rejects it)
//! - subsequent turns: ACP `session/load`; print path still `--resume-id`
//! - auth: env `KIRO_API_KEY` / import `kiro-cli login` (sqlite + SSO cache);
//!   Connections official login spawns `kiro-cli login --license free` then imports;
//!   refresh compares expiry and can write sqlite; HTTP also refreshes OIDC/Desktop
//! - live backup of the sqlite login store and SSO cache copy
//!
//! ## Explicitly out of scope
//! - Claiming official public REST support
//! - Enterprise IdC `profileArn` / `runtime.*.kiro.dev` deep support (deferred)
//! - OpenAI loopback Routes surface (follow-up)
//! - Config write / API Key live apply
//! - Mid-turn steer (ACP has session/cancel, not turn/steer)
//! - Skills / MCP / usage / project history (no verified path yet)
//! - Using Kiro IDE as the headless entry

use std::path::{Path, PathBuf};

use crate::error::{AppError, Result};
use crate::models::{
    AccountKind, AgentConfig, AgentId, AuthHealth, AuthState, Capability, CapabilityState,
    DetectResult, DetectStatus, DetectedBinaryCopy, LiveAccount, RunOptions, RunSpec,
};
use crate::runtime;
use crate::utils::paths::agent_home;
use crate::utils::process::{run_capture, stdout_first_line};

use super::{
    api_key_live_account, detect_binary, looks_like_version_line, require_api_key, AgentAdapter,
};

mod auth;
mod chat_prefs;
pub(crate) mod http;

pub(crate) use auth::{
    kiro_grant_is_newer, kiro_login_fingerprint, read_kiro_live_account, write_kiro_live_account,
};
pub(crate) use chat_prefs::{
    kiro_live_chat_model, kiro_send_prefs, set_kiro_default_effort, set_kiro_default_model,
};

/// Official Windows native installer (PowerShell: `irm … | iex`).
pub const NATIVE_PS1_URL: &str = "https://cli.kiro.dev/install.ps1";
/// Official Unix installer (`curl -fsSL … | bash`).
pub const NATIVE_SH_URL: &str = "https://cli.kiro.dev/install";

pub struct KiroAdapter;

/// Standalone install probe used by platform detectors (no full adapter required).
pub(crate) fn detect_installation() -> DetectResult {
    let requires = crate::catalog::install::adapter_install_channels(AgentId::Kiro)
        .first()
        .map(|c| c.requires.clone())
        .unwrap_or_default();
    let env_ready = runtime::is_ready(&requires);
    let mut result = detect_binary(
        AgentId::Kiro,
        &["kiro-cli"],
        &["--version"],
        Some("native"),
        env_ready,
    );
    attach_kiro_desktop_copy(&mut result);
    if result.status == DetectStatus::Installed {
        if let Some(ide) = detect_kiro_ide_version() {
            result.notes.push(format!("Kiro 编辑器也在本机 ({ide})"));
        }
    } else if let Some(ide) = detect_kiro_ide_version() {
        result.notes.push(format!(
            "检测到 Kiro 编辑器 ({ide})，但仍需安装 kiro-cli \
             （官方: irm '{NATIVE_PS1_URL}' | iex 或 curl {NATIVE_SH_URL} | bash）"
        ));
    }
    result
}

fn attach_kiro_desktop_copy(result: &mut DetectResult) {
    let Some((path, version)) = kiro_ide_bin() else {
        return;
    };
    if result
        .binary_path
        .as_deref()
        .is_some_and(|primary| paths_equal_ignore_case(primary, &path))
    {
        return;
    }
    if result
        .extra_copies
        .iter()
        .any(|c| paths_equal_ignore_case(&c.path, &path))
    {
        return;
    }
    result.extra_copies.push(DetectedBinaryCopy::from_kind(
        AgentId::Kiro,
        path,
        "desktop",
        version,
        None,
    ));
}

fn paths_equal_ignore_case(a: &Path, b: &Path) -> bool {
    a.to_string_lossy()
        .eq_ignore_ascii_case(&b.to_string_lossy())
}

impl AgentAdapter for KiroAdapter {
    fn id(&self) -> AgentId {
        AgentId::Kiro
    }

    fn detect(&self) -> DetectResult {
        detect_installation()
    }

    fn read_config(&self) -> Result<AgentConfig> {
        let home = agent_home(AgentId::Kiro)?;
        let api_key_set = std::env::var_os("KIRO_API_KEY")
            .map(|v| !v.is_empty())
            .unwrap_or(false);
        let mut raw = serde_json::Map::new();
        raw.insert(
            "auth".into(),
            serde_json::json!({
                "kiroApiKeyEnvSet": api_key_set,
                "note": "Use KIRO_API_KEY or `kiro-cli login`; no provider template file",
            }),
        );
        raw.insert(
            "paths".into(),
            serde_json::json!({
                "agentHome": home,
            }),
        );
        raw.insert(
            "capabilities".into(),
            serde_json::json!({
                "providers": false,
                "accountSwitch": false,
                "usage": false,
                "skills": false,
            }),
        );
        Ok(AgentConfig {
            agent: AgentId::Kiro,
            raw: serde_json::Value::Object(raw),
        })
    }

    fn write_config(&self, _config: &AgentConfig) -> Result<()> {
        Err(AppError::Unsupported(
            "Kiro 暂时不能把这份登录写到本机配置。请用 Kiro 自己的登录，或设置 KIRO_API_KEY。"
                .into(),
        ))
    }

    fn read_auth(&self) -> Result<AuthState> {
        if let Some(mut state) = auth::kiro_oauth_auth_state() {
            if kiro_cli_status_verified() {
                state.health = AuthHealth::Verified;
            }
            let api_key_set = std::env::var_os("KIRO_API_KEY")
                .map(|v| !v.is_empty())
                .unwrap_or(false);
            return Ok(if api_key_set {
                state.with_also_present(["api_key"])
            } else {
                state
            });
        }
        let api_key_set = std::env::var_os("KIRO_API_KEY")
            .map(|v| !v.is_empty())
            .unwrap_or(false);
        if api_key_set {
            let state = AuthState {
                agent: AgentId::Kiro,
                kind: Some("env-KIRO_API_KEY".into()),
                summary: "KIRO_API_KEY is set in the environment".into(),
                has_credentials: true,
                health: AuthHealth::Configured,
                source: Some("env:KIRO_API_KEY".into()),
                revision: None,
                also_present: Vec::new(),
                secret_hash: None,
            };
            return Ok(if kiro_cli_status_verified() {
                state.with_also_present(["oauth"])
            } else {
                state
            });
        }
        if let Some(bin) = resolve_kiro_cli() {
            if let Some(text) = probe_kiro_login_text(&bin) {
                let health = kiro_status_health(&text);
                if health == AuthHealth::Verified {
                    return Ok(AuthState {
                        agent: AgentId::Kiro,
                        kind: Some("cli-whoami".into()),
                        summary: "kiro-cli reports authenticated".into(),
                        has_credentials: true,
                        health,
                        source: Some("kiro-cli whoami".into()),
                        revision: None,
                        also_present: Vec::new(),
                        secret_hash: None,
                    });
                }
                if health == AuthHealth::NeedsLogin {
                    return Ok(AuthState {
                        agent: AgentId::Kiro,
                        kind: Some("cli-whoami".into()),
                        summary: "kiro-cli reports not authenticated; run `kiro-cli login`".into(),
                        has_credentials: false,
                        health,
                        source: Some("kiro-cli whoami".into()),
                        revision: None,
                        also_present: Vec::new(),
                        secret_hash: None,
                    });
                }
                return Ok(AuthState {
                    agent: AgentId::Kiro,
                    kind: Some("cli-whoami".into()),
                    summary: "kiro-cli could not determine authentication".into(),
                    has_credentials: false,
                    health,
                    source: Some("kiro-cli whoami".into()),
                    revision: None,
                    also_present: Vec::new(),
                    secret_hash: None,
                });
            }
        }
        Ok(AuthState {
            agent: AgentId::Kiro,
            kind: None,
            summary: "no KIRO_API_KEY; run `kiro-cli login` or set KIRO_API_KEY".into(),
            has_credentials: false,
            health: AuthHealth::Missing,
            source: Some("kiro-cli".into()),
            revision: None,
            also_present: Vec::new(),
            secret_hash: None,
        })
    }

    fn read_account(&self) -> Result<LiveAccount> {
        auth::read_kiro_live_account()
    }

    fn identity_label(
        &self,
        kind: AccountKind,
        credentials: &serde_json::Value,
        label_hint: Option<&str>,
    ) -> Option<String> {
        if kind == AccountKind::Oauth {
            return auth::kiro_identity_label(credentials, label_hint);
        }
        super::default_identity_label(kind, credentials, label_hint)
    }

    fn build_api_key_account(&self, api_key: &str) -> Result<LiveAccount> {
        let key = require_api_key(api_key)?;
        Ok(api_key_live_account(
            AgentId::Kiro,
            key,
            serde_json::json!({
                "format": "api_key",
                "api_key": key,
            }),
            "KIRO_API_KEY",
            serde_json::json!({
                "source": "manual",
                "note": "pool-only; apply live is unsupported — set KIRO_API_KEY or run `kiro-cli login`"
            }),
        ))
    }

    fn apply_account(&self, account: &LiveAccount) -> Result<()> {
        if account.kind == AccountKind::ApiKey {
            return Err(AppError::Unsupported(
                "Kiro 暂时不能把这份登录写到本机配置。请用 Kiro 自己的登录，或设置 KIRO_API_KEY。"
                    .into(),
            ));
        }
        auth::write_kiro_live_account(account)
    }

    fn skills_dir(&self) -> Option<PathBuf> {
        None
    }

    fn capability(&self, cap: Capability) -> CapabilityState {
        use Capability::*;
        match cap {
            ConfigWrite => CapabilityState::unsupported("无稳定配置写入契约，fail-closed"),
            AccountSwitch => CapabilityState::partial("可在连接里切换，会写回 Kiro"),
            ApiKeyAccount => CapabilityState::partial("可用 API Key 或 kiro-cli login"),
            Skills => CapabilityState::planned("待路径核实"),
            LiveBackup => CapabilityState::full(),
            StructuredStream => CapabilityState::partial("对话过程走 ACP；生成时不能中途补充"),
            DangerousMode => CapabilityState::partial("映射 --trust-all-tools；请确认风险后再开"),
            ProjectHistory => CapabilityState::planned("待路径核实"),
            ProjectDelete => CapabilityState::unsupported("无安全浅删契约"),
            ProviderPresets => CapabilityState::unsupported("无 provider 配置契约"),
            Usage => CapabilityState::planned("待日志字段核实"),
            Mcp => CapabilityState::planned("待路径核实"),
            ModelSelect => CapabilityState::full(),
            SessionResume => {
                CapabilityState::partial("新对话走持续通道，可点允许/拒绝；生成时不能中途补充")
            }
        }
    }

    fn live_backup_paths(&self) -> Vec<PathBuf> {
        crate::utils::paths::kiro_cli_sqlite_path()
            .into_iter()
            .chain(crate::utils::paths::kiro_sso_cache_path())
            .collect()
    }

    fn build_run_spec(&self, binary: &Path, prompt: &str, opts: &RunOptions) -> Result<RunSpec> {
        // `--wrap never`: piped stdout on macOS/Linux still auto-wraps at 80 cols.
        // TERM/NO_COLOR: inherited Terminal.app / Linux TERM=xterm makes kiro emit CSI
        // even when stdout is a pipe. Windows kiro still colors; chat sanitizes CSI.
        let mut args = vec![
            "chat".into(),
            "--no-interactive".into(),
            "--wrap".into(),
            "never".into(),
        ];
        // v1 rejects stream-json; pin v2 (v3 dumps extra logs onto stdout).
        if super::wants_structured_for(opts.process_mode, AgentId::Kiro) {
            args.push("--agent-engine".into());
            args.push("v2".into());
            args.push("--output-format".into());
            args.push("stream-json".into());
        }
        if let Some(model) = opts
            .model
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            args.push("--model".into());
            args.push(model.to_string());
        }
        if let Some(effort) = opts
            .effort
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            args.push("--effort".into());
            args.push(effort.to_string());
        }
        if let Some(sid) = opts
            .native_session_id
            .as_deref()
            .and_then(super::session_resume::valid_session_id)
        {
            args.push("--resume-id".into());
            args.push(sid.to_string());
        }
        if opts.allow_dangerous {
            args.push("--trust-all-tools".into());
        }
        args.push(prompt.to_string());
        let mut env = vec![
            ("TERM".into(), "dumb".into()),
            ("NO_COLOR".into(), "1".into()),
            ("CLICOLOR".into(), "0".into()),
        ];
        if let Ok(key) = std::env::var("KIRO_API_KEY") {
            if !key.is_empty() {
                env.push(("KIRO_API_KEY".into(), key));
            }
        }
        Ok(RunSpec {
            agent: AgentId::Kiro,
            program: binary.to_path_buf(),
            args,
            cwd: opts.cwd.clone(),
            env,
        })
    }
}

pub(crate) fn kiro_cli_binary() -> Option<PathBuf> {
    detect_installation().binary_path
}

fn resolve_kiro_cli() -> Option<PathBuf> {
    kiro_cli_binary()
}

fn probe_kiro_login_text(bin: &Path) -> Option<String> {
    for args in [&["whoami"][..], &["login", "status"][..]] {
        if let Ok(out) = run_capture(bin, args) {
            let text = format!(
                "{}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
            if !text.trim().is_empty() {
                return Some(text);
            }
        }
    }
    None
}

fn kiro_cli_status_verified() -> bool {
    let Some(bin) = resolve_kiro_cli() else {
        return false;
    };
    let Some(text) = probe_kiro_login_text(&bin) else {
        return false;
    };
    kiro_status_health(&text) == AuthHealth::Verified
}

/// Parse only explicit login wording. Unknown output stays Unknown.
pub(crate) fn kiro_status_health(text: &str) -> AuthHealth {
    let mut has_positive = false;
    for raw_line in text.lines() {
        let line = raw_line.trim().to_ascii_lowercase();
        if matches!(
            line.as_str(),
            "not authenticated"
                | "unauthenticated"
                | "not logged in"
                | "logged out"
                | "not signed in"
                | "signed out"
                | "login required"
                | "authentication required"
        ) {
            return AuthHealth::NeedsLogin;
        }
        if matches!(line.as_str(), "authenticated" | "logged in" | "signed in")
            || line.starts_with("logged in as ")
            || line.starts_with("signed in as ")
        {
            has_positive = true;
        }
    }
    if has_positive {
        AuthHealth::Verified
    } else {
        AuthHealth::Unknown
    }
}

fn kiro_ide_bin() -> Option<(PathBuf, Option<String>)> {
    #[cfg(windows)]
    {
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            let exe = PathBuf::from(local)
                .join("Programs")
                .join("Kiro")
                .join("Kiro.exe");
            if exe.is_file() {
                return Some((exe, Some("installed".into())));
            }
        }
        for key in ["ProgramFiles", "ProgramFiles(x86)"] {
            if let Ok(root) = std::env::var(key) {
                let exe = PathBuf::from(root).join("Kiro").join("Kiro.exe");
                if exe.is_file() {
                    return Some((exe, Some("installed".into())));
                }
            }
        }
    }
    #[cfg(target_os = "macos")]
    {
        let app = PathBuf::from("/Applications/Kiro.app");
        if app.is_dir() {
            return Some((app, Some("installed".into())));
        }
    }
    None
}

fn detect_kiro_ide_version() -> Option<String> {
    kiro_ide_bin().and_then(|(_, version)| version).or_else(|| {
        let path = which::which("kiro").ok()?;
        if path_looks_like_kiro_cli(&path) {
            return None;
        }
        let o = run_capture(&path, &["--version"]).ok()?;
        stdout_first_line(&o).filter(|v| looks_like_version_line(v))
    })
}

fn path_looks_like_kiro_cli(path: &Path) -> bool {
    let s = path.to_string_lossy().to_ascii_lowercase();
    if s.contains("kiro-cli") {
        return true;
    }
    path.file_stem()
        .and_then(|n| n.to_str())
        .is_some_and(|stem| stem.eq_ignore_ascii_case("kiro-cli"))
}

/// Public helper for install_service well-known uninstall candidates.
pub fn uninstall_bin_candidates() -> Vec<PathBuf> {
    super::detect_binary::well_known_bin_paths(AgentId::Kiro)
        .into_iter()
        .map(|(p, _)| p)
        .collect()
}

#[cfg(test)]
mod tests;
