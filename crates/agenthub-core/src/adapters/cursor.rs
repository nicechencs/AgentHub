//! Cursor Agent CLI adapter (half-surface).
//!
//! Product card: **Cursor Agent**. Manages the public Agent CLI (`agent` /
//! `cursor-agent`), **not** Cursor IDE as a Claude-style full agent.
//!
//! ## Scope (honest)
//! - install / detect / uninstall (allowlisted shims; no full product uninstall claim)
//! - headless: `agent -p "…" --output-format text` (+ `--force` when dangerous)
//! - skills dir: `~/.cursor/skills-cursor`
//! - projects: read-only workspace folders under `~/.cursor/projects`
//! - auth: env `CURSOR_API_KEY` / login guidance only
//!
//! ## Explicitly out of scope
//! - Providers / Base URL templates (`write_config` fail-closed)
//! - Account pool switch via IDE private account stores (forbidden)
//! - Token usage from IDE-internal usage databases
//! - Using IDE `cursor` / `Cursor.exe` as headless entry
//!
//! ## Detect hard rule
//! PATH `agent` may resolve to another product's binary. Never treat non-Cursor
//! install trees as Cursor. Only accept binaries under `cursor-agent` install
//! trees or named `cursor-agent`, with version lines that do not look like Grok.

use std::path::{Path, PathBuf};

use crate::error::{AppError, Result};
use crate::models::{
    AgentConfig, AgentId, AuthState, Capability, CapabilityState, DetectResult, DetectStatus,
    LiveAccount, RunOptions, RunSpec,
};
use crate::runtime;
use crate::utils::paths::{agent_home, home_dir};
use crate::utils::process::{run_capture, stdout_first_line};

use super::{
    api_key_live_account, looks_like_version_line, require_api_key, AgentAdapter,
    NOT_FOUND_FIREFIGHTING_NOTE,
};

/// Official Windows native installer (PowerShell: `irm … | iex`).
pub const NATIVE_PS1_URL: &str = "https://cursor.com/install?win32=true";
/// Official Unix installer (`curl -fsSL … | bash`).
pub const NATIVE_SH_URL: &str = "https://cursor.com/install";

/// Extract the latest Cursor Agent version token from an official install script.
///
/// Handles both PowerShell (`$version = '…'`) and bash
/// (`downloads.cursor.com/lab/<ver>/…`) embed styles.
///
/// Accepted build ids (aligned with the official agent.ps1 sorter):
/// - legacy: `YYYY.MM.DD-commit`
/// - newer:  `YYYY.MM.DD-HH-MM-SS-commit`
pub fn extract_latest_version_from_install_script(body: &str) -> Option<String> {
    // Prefer the explicit PowerShell assignment — least ambiguous.
    // $version = '2026.07.23-e383d2b'  |  $version = "…"
    for line in body.lines() {
        let t = line.trim();
        let lower = t.to_ascii_lowercase();
        if !lower.contains("$version") {
            continue;
        }
        if let Some(v) = first_cursor_version_token(t) {
            return Some(v);
        }
    }
    // Bash / URL form: downloads.cursor.com/lab/<version>/
    if let Some(idx) = body.find("downloads.cursor.com/lab/") {
        let rest = &body[idx + "downloads.cursor.com/lab/".len()..];
        if let Some(v) = first_cursor_version_token(rest) {
            return Some(v);
        }
    }
    // Last resort: first matching token anywhere in the script.
    first_cursor_version_token(body)
}

fn first_cursor_version_token(s: &str) -> Option<String> {
    // Regex-free scan for YYYY.M.D[-HH-MM-SS]-hex.
    let bytes = s.as_bytes();
    let mut i = 0;
    while i + 10 < bytes.len() {
        // year 20xx
        if bytes[i] == b'2'
            && bytes[i + 1] == b'0'
            && bytes[i + 2].is_ascii_digit()
            && bytes[i + 3].is_ascii_digit()
            && bytes[i + 4] == b'.'
        {
            if let Some(end) = scan_cursor_version_end(bytes, i) {
                let token = &s[i..end];
                // Must contain a '-' (date-commit or date-time-commit).
                if token.contains('-') {
                    return Some(token.to_string());
                }
            }
        }
        i += 1;
    }
    None
}

/// From `start` at the year digit, return exclusive end index of a cursor version token.
fn scan_cursor_version_end(bytes: &[u8], start: usize) -> Option<usize> {
    // YYYY.M{1,2}.D{1,2}  then optional -HH-MM-SS  then -hex
    let mut i = start;
    // year
    for _ in 0..4 {
        if i >= bytes.len() || !bytes[i].is_ascii_digit() {
            return None;
        }
        i += 1;
    }
    if i >= bytes.len() || bytes[i] != b'.' {
        return None;
    }
    i += 1;
    // month
    let month_start = i;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i == month_start || i - month_start > 2 {
        return None;
    }
    if i >= bytes.len() || bytes[i] != b'.' {
        return None;
    }
    i += 1;
    // day
    let day_start = i;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i == day_start || i - day_start > 2 {
        return None;
    }
    if i >= bytes.len() || bytes[i] != b'-' {
        return None;
    }
    i += 1;
    // Either commit hex, or HH-MM-SS-commit.
    // Peek: if next two are digits and then '-', treat as timestamp form.
    if i + 8 <= bytes.len()
        && bytes[i].is_ascii_digit()
        && bytes[i + 1].is_ascii_digit()
        && bytes[i + 2] == b'-'
        && bytes[i + 3].is_ascii_digit()
        && bytes[i + 4].is_ascii_digit()
        && bytes[i + 5] == b'-'
        && bytes[i + 6].is_ascii_digit()
        && bytes[i + 7].is_ascii_digit()
        && i + 9 < bytes.len()
        && bytes[i + 8] == b'-'
    {
        i += 9; // skip HH-MM-SS-
    }
    let hash_start = i;
    while i < bytes.len() && bytes[i].is_ascii_hexdigit() {
        i += 1;
    }
    let hash_len = i - hash_start;
    if hash_len < 6 {
        return None;
    }
    Some(i)
}

pub struct CursorAdapter;

/// Standalone install probe used by platform detectors (no full adapter required).
pub(crate) fn detect_installation() -> DetectResult {
    let requires = crate::catalog::install::adapter_install_channels(AgentId::Cursor)
        .first()
        .map(|c| c.requires.clone())
        .unwrap_or_default();
    let env_ready = runtime::is_ready(&requires);
    let mut notes = Vec::new();

    if let Some((path, channel, via_well_known, version)) = resolve_cursor_agent_cli() {
        if via_well_known {
            notes.push(format!(
                "found via well-known path (not on process PATH): {}; \
                 restart AgentHub after installs if PATH still incomplete",
                path.display()
            ));
        }
        if let Some(ide) = detect_cursor_ide_version() {
            notes.push(format!("Cursor IDE also present ({ide})"));
        }
        tracing::info!(
            target: crate::logging::targets::DETECT,
            module = crate::logging::targets::DETECT,
            op = "detect",
            agent = "cursor",
            via = if via_well_known { "well_known" } else { "path" },
            channel = channel,
            path = %path.display(),
            version = version.as_deref().unwrap_or("-"),
            "Cursor Agent CLI detected"
        );
        return DetectResult {
            agent: AgentId::Cursor,
            status: DetectStatus::Installed,
            version,
            binary_path: Some(path),
            channel: Some(channel.into()),
            env_ready,
            notes,
            extra_copies: Vec::new(),
        };
    }

    // Not installed: optional IDE-only tip (does NOT count as Installed).
    if let Some(ide) = detect_cursor_ide_version() {
        notes.push(format!(
            "检测到 Cursor IDE ({ide})，但仍需安装 Cursor Agent CLI \
             （官方: irm '{NATIVE_PS1_URL}' | iex 或 curl {NATIVE_SH_URL} | bash）"
        ));
    } else {
        notes.push(NOT_FOUND_FIREFIGHTING_NOTE.into());
    }
    // Surface that bare PATH `agent` is not trusted when it looks like Grok.
    if let Some(rejected) = path_agent_rejected_as_non_cursor() {
        notes.push(format!(
            "ignored PATH agent at {} (not Cursor Agent CLI; e.g. Grok uses agent.exe)",
            rejected.display()
        ));
    }

    tracing::debug!(
        target: crate::logging::targets::DETECT,
        module = crate::logging::targets::DETECT,
        op = "detect",
        agent = "cursor",
        via = "not_found",
        "Cursor Agent CLI not found"
    );

    DetectResult {
        agent: AgentId::Cursor,
        status: DetectStatus::NotFound,
        version: None,
        binary_path: None,
        channel: None,
        env_ready,
        notes,
        extra_copies: Vec::new(),
    }
}

impl AgentAdapter for CursorAdapter {
    fn id(&self) -> AgentId {
        AgentId::Cursor
    }

    fn detect(&self) -> DetectResult {
        detect_installation()
    }

    fn read_config(&self) -> Result<AgentConfig> {
        let home = agent_home(AgentId::Cursor)?;
        let api_key_set = std::env::var_os("CURSOR_API_KEY")
            .map(|v| !v.is_empty())
            .unwrap_or(false);
        let ide_settings = cursor_ide_settings_path();
        let mut raw = serde_json::Map::new();
        raw.insert(
            "auth".into(),
            serde_json::json!({
                "cursorApiKeyEnvSet": api_key_set,
                "note": "Use CURSOR_API_KEY or `cursor-agent login`; no provider template file",
            }),
        );
        raw.insert(
            "paths".into(),
            serde_json::json!({
                "agentHome": home,
                "skills": home.join("skills-cursor"),
                "projects": home.join("projects"),
                "ideSettings": ide_settings,
            }),
        );
        raw.insert(
            "capabilities".into(),
            serde_json::json!({
                "providers": false,
                "accountSwitch": false,
                "usage": false,
                "skills": true,
            }),
        );
        Ok(AgentConfig {
            agent: AgentId::Cursor,
            raw: serde_json::Value::Object(raw),
        })
    }

    fn write_config(&self, _config: &AgentConfig) -> Result<()> {
        Err(AppError::Unsupported(
            "live config writes are not supported for cursor \
              (no stable models.json/config.toml provider contract; \
               use CURSOR_API_KEY or `cursor-agent login`)"
                .into(),
        ))
    }

    fn read_auth(&self) -> Result<AuthState> {
        let api_key_set = std::env::var_os("CURSOR_API_KEY")
            .map(|v| !v.is_empty())
            .unwrap_or(false);
        if api_key_set {
            let state = AuthState {
                agent: AgentId::Cursor,
                kind: Some("env-CURSOR_API_KEY".into()),
                summary: "CURSOR_API_KEY is set in the environment".into(),
                has_credentials: true,
                health: crate::models::AuthHealth::Configured,
                source: Some("env:CURSOR_API_KEY".into()),
                revision: None,
                also_present: Vec::new(),
                secret_hash: None,
            };
            return Ok(if cursor_cli_status_verified() {
                state.with_also_present(["oauth"])
            } else {
                state
            });
        }
        // Optional non-destructive status probe when CLI is present.
        if let Some((bin, _, _, _)) = resolve_cursor_agent_cli() {
            if let Ok(out) = run_capture(&bin, &["status"]) {
                let text = format!(
                    "{}{}",
                    String::from_utf8_lossy(&out.stdout),
                    String::from_utf8_lossy(&out.stderr)
                );
                let health = cursor_status_health(&text);
                if health == crate::models::AuthHealth::Verified {
                    return Ok(AuthState {
                        agent: AgentId::Cursor,
                        kind: Some("cli-status".into()),
                        summary: "cursor-agent status reports authenticated".into(),
                        has_credentials: true,
                        health,
                        source: Some("cursor-agent status".into()),
                        revision: None,
                        also_present: Vec::new(),
                        secret_hash: None,
                    });
                }
                if health == crate::models::AuthHealth::NeedsLogin {
                    return Ok(AuthState {
                        agent: AgentId::Cursor,
                        kind: Some("cli-status".into()),
                        summary: "cursor-agent status reports not authenticated; run `cursor-agent login`".into(),
                        has_credentials: false,
                        health,
                        source: Some("cursor-agent status".into()),
                        revision: None,
                        also_present: Vec::new(),
            secret_hash: None,
                    });
                }
                return Ok(AuthState {
                    agent: AgentId::Cursor,
                    kind: Some("cli-status".into()),
                    summary: "cursor-agent status could not determine authentication".into(),
                    has_credentials: false,
                    health,
                    source: Some("cursor-agent status".into()),
                    revision: None,
                    also_present: Vec::new(),
                    secret_hash: None,
                });
            }
        }
        Ok(AuthState {
            agent: AgentId::Cursor,
            kind: None,
            summary: "no CURSOR_API_KEY; run `cursor-agent login` or set CURSOR_API_KEY".into(),
            has_credentials: false,
            health: crate::models::AuthHealth::Missing,
            source: Some("cursor-agent".into()),
            revision: None,
            also_present: Vec::new(),
            secret_hash: None,
        })
    }

    fn build_api_key_account(&self, api_key: &str) -> Result<LiveAccount> {
        // Pool-only: apply to live remains Unsupported (set env / cursor-agent login).
        let key = require_api_key(api_key)?;
        Ok(api_key_live_account(
            AgentId::Cursor,
            key,
            serde_json::json!({
                "format": "api_key",
                "api_key": key,
            }),
            "CURSOR_API_KEY",
            serde_json::json!({
                "source": "manual",
                "note": "pool-only; apply live is unsupported — set CURSOR_API_KEY or run `cursor-agent login`"
            }),
        ))
    }

    fn apply_account(&self, _account: &LiveAccount) -> Result<()> {
        Err(AppError::Unsupported(
            "applying Cursor accounts to live is not supported; \
             set CURSOR_API_KEY in the environment or run `cursor-agent login` \
             (IDE private account stores are intentionally not used)"
                .into(),
        ))
    }

    fn skills_dir(&self) -> Option<PathBuf> {
        agent_home(AgentId::Cursor)
            .ok()
            .map(|h| h.join("skills-cursor"))
    }

    fn capability(&self, cap: Capability) -> CapabilityState {
        use Capability::*;
        match cap {
            Skills | DangerousMode => CapabilityState::full(),
            ConfigWrite => CapabilityState::unsupported("无稳定配置写入契约，fail-closed"),
            // UI 文案保持短句；IDE 私有库禁写见模块注释 / capability 矩阵
            AccountSwitch => CapabilityState::unsupported("账号由 Cursor 管理"),
            ApiKeyAccount => CapabilityState::partial("可用 API Key 或 cursor-agent login"),
            LiveBackup => CapabilityState::unsupported("无稳定配置/凭据文件"),
            StructuredStream => CapabilityState::unsupported("Agent CLI 仅提供 text 输出"),
            ProjectHistory => CapabilityState::partial("仅工作区目录列表，无会话 transcript"),
            ProjectDelete => CapabilityState::unsupported("无安全浅删契约"),
            ProviderPresets => CapabilityState::unsupported("无 provider 配置契约"),
            Usage => CapabilityState::unsupported("IDE 内部用量库，明确范围外"),
            Mcp | ModelSelect | SessionResume => CapabilityState::planned("待验证接入"),
        }
    }

    fn live_backup_paths(&self) -> Vec<PathBuf> {
        // No stable provider/auth config files for half-surface Cursor Agent CLI.
        // Do not back up IDE private account or usage databases.
        Vec::new()
    }

    fn build_run_spec(&self, binary: &Path, prompt: &str, opts: &RunOptions) -> Result<RunSpec> {
        // Documented headless text mode; allow_dangerous maps to --force.
        // Structured stream flags are out of scope for P0.
        let mut args = vec![
            "-p".into(),
            prompt.to_string(),
            "--output-format".into(),
            "text".into(),
        ];
        if opts.allow_dangerous {
            args.push("--force".into());
        }
        let mut env = Vec::new();
        if let Ok(key) = std::env::var("CURSOR_API_KEY") {
            if !key.is_empty() {
                env.push(("CURSOR_API_KEY".into(), key));
            }
        }
        Ok(RunSpec {
            agent: AgentId::Cursor,
            program: binary.to_path_buf(),
            args,
            cwd: opts.cwd.clone(),
            env,
        })
    }
}

/// Resolve a validated Cursor Agent CLI binary.
///
/// Returns `(path, channel, via_well_known, version)`.
fn resolve_cursor_agent_cli() -> Option<(PathBuf, &'static str, bool, Option<String>)> {
    // 1) Well-known install roots first (stable, avoids Grok PATH collision).
    for (path, channel) in well_known_cursor_agent_bins() {
        if !path.is_file() {
            continue;
        }
        match probe_cursor_binary(&path) {
            ProbeResult::Reject => continue,
            ProbeResult::Accept(version) => {
                // Bare `~/.local/bin/agent` is only accepted when version looks Cursor-ish
                // or the path itself is under cursor-agent / named cursor-agent.
                let path_ok = path_looks_like_cursor_agent(&path);
                let ver_ok = version
                    .as_deref()
                    .map(version_looks_like_cursor)
                    .unwrap_or(false);
                if path_ok || ver_ok {
                    return Some((path, channel, true, version));
                }
            }
        }
    }

    // 2) PATH: prefer explicit `cursor-agent` name.
    for name in cursor_agent_path_names() {
        if let Ok(path) = which::which(&name) {
            if path_is_rejected_non_cursor(&path) {
                continue;
            }
            match probe_cursor_binary(&path) {
                ProbeResult::Reject => continue,
                ProbeResult::Accept(version) => {
                    return Some((path, "native", false, version));
                }
            }
        }
    }

    // 3) PATH bare `agent` only if parent tree is cursor-agent AND not Grok.
    for name in expand_agent_names() {
        if let Ok(path) = which::which(&name) {
            if path_is_rejected_non_cursor(&path) {
                continue;
            }
            if !path_looks_like_cursor_agent(&path) {
                // Probe version: only accept if clearly Cursor, never Grok.
                match probe_cursor_binary(&path) {
                    ProbeResult::Reject => continue,
                    ProbeResult::Accept(Some(v)) if version_looks_like_cursor(&v) => {
                        return Some((path, "native", false, Some(v)));
                    }
                    ProbeResult::Accept(_) => continue,
                }
            }
            match probe_cursor_binary(&path) {
                ProbeResult::Reject => continue,
                ProbeResult::Accept(version) => {
                    return Some((path, "native", false, version));
                }
            }
        }
    }

    None
}

enum ProbeResult {
    /// Definitely not Cursor (e.g. Grok version line).
    Reject,
    /// Accept this binary; version may be unknown.
    Accept(Option<String>),
}

fn well_known_cursor_agent_bins() -> Vec<(PathBuf, &'static str)> {
    let mut out = Vec::new();
    #[cfg(windows)]
    {
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            let root = PathBuf::from(local).join("cursor-agent");
            for name in ["agent.exe", "agent.cmd", "agent.ps1", "cursor-agent.exe"] {
                out.push((root.join(name), "native"));
            }
            // Versioned layouts: cursor-agent/versions/<ver>/agent.exe
            let versions = root.join("versions");
            if versions.is_dir() {
                if let Ok(rd) = std::fs::read_dir(&versions) {
                    let mut dirs: Vec<PathBuf> = rd
                        .filter_map(|e| e.ok().map(|e| e.path()))
                        .filter(|p| p.is_dir())
                        .collect();
                    dirs.sort();
                    if let Some(latest) = dirs.pop() {
                        for name in ["agent.exe", "agent.cmd", "agent.ps1", "cursor-agent.exe"] {
                            out.push((latest.join(name), "native"));
                        }
                    }
                }
            }
        }
    }
    if let Ok(home) = home_dir() {
        let local_bin = home.join(".local").join("bin");
        for name in ["cursor-agent", "agent"] {
            out.push((local_bin.join(name), "native"));
            #[cfg(windows)]
            {
                out.push((local_bin.join(format!("{name}.exe")), "native"));
                out.push((local_bin.join(format!("{name}.cmd")), "native"));
            }
        }
        // Some Unix installs: ~/.local/share/cursor-agent/...
        let share = home.join(".local").join("share").join("cursor-agent");
        out.push((share.join("cursor-agent"), "native"));
        out.push((share.join("agent"), "native"));
    }
    out
}

fn cursor_agent_path_names() -> Vec<String> {
    let mut names = vec!["cursor-agent".into()];
    if cfg!(windows) {
        names.push("cursor-agent.exe".into());
        names.push("cursor-agent.cmd".into());
        names.push("cursor-agent.ps1".into());
    }
    names
}

fn expand_agent_names() -> Vec<String> {
    let mut names = vec!["agent".into()];
    if cfg!(windows) {
        names.push("agent.exe".into());
        names.push("agent.cmd".into());
        names.push("agent.ps1".into());
    }
    names
}

fn path_looks_like_cursor_agent(path: &Path) -> bool {
    let s = path.to_string_lossy().to_ascii_lowercase();
    if s.contains("cursor-agent") {
        return true;
    }
    if let Some(stem) = path.file_stem().and_then(|n| n.to_str()) {
        if stem.eq_ignore_ascii_case("cursor-agent") {
            return true;
        }
    }
    false
}

fn path_is_rejected_non_cursor(path: &Path) -> bool {
    let s = path.to_string_lossy().to_ascii_lowercase();
    // Grok Build ships agent.exe under ~/.grok/bin — never Cursor.
    if s.contains(".grok") || s.contains(r"\grok\") || s.contains("/grok/") {
        return true;
    }
    // IDE opener / Electron shell is not the Agent CLI.
    if s.contains("programs") && s.contains("cursor") && s.contains("resources") {
        // ...\Programs\cursor\resources\app\bin\cursor.cmd
        if let Some(stem) = path.file_stem().and_then(|n| n.to_str()) {
            if stem.eq_ignore_ascii_case("cursor") {
                return true;
            }
        }
    }
    false
}

fn cursor_cli_status_verified() -> bool {
    let Some((bin, _, _, _)) = resolve_cursor_agent_cli() else {
        return false;
    };
    let Ok(out) = run_capture(&bin, &["status"]) else {
        return false;
    };
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    cursor_status_health(&text) == crate::models::AuthHealth::Verified
}

/// Parse only explicit Cursor Agent status wording. Structured booleans and
/// individual status lines take precedence over prose, and any explicit false
/// wins before a positive line can be considered.
pub(crate) fn cursor_status_health(text: &str) -> crate::models::AuthHealth {
    use crate::models::AuthHealth;

    let mut explicit_true = false;
    let mut explicit_false = false;
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(text) {
        collect_cursor_auth_booleans(&value, &mut explicit_true, &mut explicit_false);
    }
    for line in text.lines() {
        if let Some(value) = cursor_status_line_boolean(line) {
            if value {
                explicit_true = true;
            } else {
                explicit_false = true;
            }
        }
    }
    if explicit_false {
        return crate::models::AuthHealth::NeedsLogin;
    }
    if explicit_true {
        return AuthHealth::Verified;
    }

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

fn collect_cursor_auth_booleans(
    value: &serde_json::Value,
    explicit_true: &mut bool,
    explicit_false: &mut bool,
) {
    let Some(object) = value.as_object() else {
        return;
    };
    for (key, value) in object {
        if is_cursor_auth_field(key) {
            if let Some(status) = cursor_auth_boolean(value) {
                if status {
                    *explicit_true = true;
                } else {
                    *explicit_false = true;
                }
            }
        }
        collect_cursor_auth_booleans(value, explicit_true, explicit_false);
    }
}

fn cursor_status_line_boolean(line: &str) -> Option<bool> {
    let (field, raw_value) = line.split_once(':').or_else(|| line.split_once('='))?;
    let field = field.trim().trim_matches(['"', '\'']);
    let raw_value = raw_value
        .trim()
        .trim_end_matches(',')
        .trim_matches(['"', '\'']);
    if is_cursor_auth_field(field) {
        return cursor_auth_boolean(&serde_json::Value::String(raw_value.to_string()));
    }
    if normalize_cursor_status_field(field) == "status" {
        return match raw_value.to_ascii_lowercase().as_str() {
            "authenticated" | "logged in" | "signed in" => Some(true),
            "not authenticated"
            | "unauthenticated"
            | "not logged in"
            | "logged out"
            | "not signed in"
            | "signed out"
            | "login required"
            | "authentication required" => Some(false),
            _ => None,
        };
    }
    None
}

fn is_cursor_auth_field(field: &str) -> bool {
    matches!(
        normalize_cursor_status_field(field).as_str(),
        "authenticated" | "isauthenticated" | "loggedin" | "isloggedin" | "signedin" | "issignedin"
    )
}

fn normalize_cursor_status_field(field: &str) -> String {
    field
        .trim()
        .to_ascii_lowercase()
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .collect()
}

fn cursor_auth_boolean(value: &serde_json::Value) -> Option<bool> {
    match value {
        serde_json::Value::Bool(value) => Some(*value),
        serde_json::Value::String(value) => match value.trim().to_ascii_lowercase().as_str() {
            "true" => Some(true),
            "false" => Some(false),
            _ => None,
        },
        _ => None,
    }
}

fn version_looks_like_grok(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    lower.contains("grok") || lower.contains("grok build")
}

fn version_looks_like_cursor(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    if version_looks_like_grok(line) {
        return false;
    }
    lower.contains("cursor")
        // Measured community versions: "2026.04.29-c83a488" style date builds.
        || (line.chars().any(|c| c.is_ascii_digit())
            && (lower.contains("agent") || line.contains('-')))
}

fn probe_cursor_binary(path: &Path) -> ProbeResult {
    match run_capture(path, &["--version"]) {
        Ok(o) => {
            let stdout = String::from_utf8_lossy(&o.stdout);
            let stderr = String::from_utf8_lossy(&o.stderr);
            let line = stdout
                .lines()
                .chain(stderr.lines())
                .map(|l| l.trim())
                .find(|l| !l.is_empty() && looks_like_version_line(l))
                .map(|l| l.to_string())
                .or_else(|| stdout_first_line(&o));
            if let Some(ref v) = line {
                if version_looks_like_grok(v) {
                    return ProbeResult::Reject;
                }
            }
            ProbeResult::Accept(line.filter(|l| looks_like_version_line(l)))
        }
        // Spawn/IO failure: still accept path-validated candidates (caller decides).
        Err(_) => ProbeResult::Accept(None),
    }
}

fn path_agent_rejected_as_non_cursor() -> Option<PathBuf> {
    for name in expand_agent_names() {
        if let Ok(path) = which::which(&name) {
            if path_is_rejected_non_cursor(&path) {
                return Some(path);
            }
            if matches!(probe_cursor_binary(&path), ProbeResult::Reject) {
                return Some(path);
            }
        }
    }
    None
}

fn detect_cursor_ide_version() -> Option<String> {
    #[cfg(windows)]
    {
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            let exe = PathBuf::from(local)
                .join("Programs")
                .join("cursor")
                .join("Cursor.exe");
            if exe.is_file() {
                // Prefer package.json next to resources when present.
                let pkg = exe
                    .parent()?
                    .join("resources")
                    .join("app")
                    .join("package.json");
                if let Ok(text) = std::fs::read_to_string(&pkg) {
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
                        if let Some(ver) = v.get("version").and_then(|x| x.as_str()) {
                            return Some(ver.to_string());
                        }
                    }
                }
                return Some("installed".into());
            }
        }
    }
    #[cfg(not(windows))]
    {
        let app = PathBuf::from("/Applications/Cursor.app");
        if app.is_dir() {
            return Some("installed".into());
        }
    }
    // PATH `cursor --version` may work but is IDE-only; still useful as presence signal.
    if let Ok(path) = which::which("cursor") {
        if let Ok(o) = run_capture(&path, &["--version"]) {
            if let Some(v) = stdout_first_line(&o) {
                if looks_like_version_line(&v) {
                    return Some(v);
                }
            }
        }
    }
    None
}

fn cursor_ide_settings_path() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        if let Ok(appdata) = std::env::var("APPDATA") {
            let p = PathBuf::from(appdata)
                .join("Cursor")
                .join("User")
                .join("settings.json");
            return Some(p);
        }
    }
    #[cfg(target_os = "macos")]
    {
        if let Ok(home) = home_dir() {
            return Some(
                home.join("Library")
                    .join("Application Support")
                    .join("Cursor")
                    .join("User")
                    .join("settings.json"),
            );
        }
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if let Ok(home) = home_dir() {
            return Some(
                home.join(".config")
                    .join("Cursor")
                    .join("User")
                    .join("settings.json"),
            );
        }
    }
    None
}

/// Public helper for install_service well-known uninstall candidates.
pub fn uninstall_bin_candidates() -> Vec<PathBuf> {
    well_known_cursor_agent_bins()
        .into_iter()
        .map(|(p, _)| p)
        .filter(|p| {
            // Never allowlist anything under .grok
            !path_is_rejected_non_cursor(p)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{AccountKind, RuntimeId};

    #[test]
    fn build_run_spec_print_mode() {
        let adapter = CursorAdapter;
        let bin = PathBuf::from("agent");
        let opts = RunOptions::default();
        let spec = adapter.build_run_spec(&bin, "hello", &opts).unwrap();
        assert_eq!(spec.agent, AgentId::Cursor);
        assert_eq!(spec.program, bin);
        assert_eq!(spec.args[0], "-p");
        assert_eq!(spec.args[1], "hello");
        assert!(spec.args.iter().any(|a| a == "--output-format"));
        assert!(spec.args.iter().any(|a| a == "text"));
        assert!(!spec.args.iter().any(|a| a == "--force"));
    }

    #[test]
    fn build_run_spec_allow_dangerous_adds_force() {
        let adapter = CursorAdapter;
        let mut opts = RunOptions::default();
        opts.allow_dangerous = true;
        let spec = adapter
            .build_run_spec(Path::new("agent"), "x", &opts)
            .unwrap();
        assert!(spec.args.iter().any(|a| a == "--force"));
    }

    #[test]
    fn install_channels_native_only() {
        let channels = CursorAdapter.install_channels();
        assert_eq!(channels.len(), 1);
        assert_eq!(channels[0].id, "native");
        #[cfg(windows)]
        assert!(channels[0].requires.contains(&RuntimeId::PowerShell));
        #[cfg(not(windows))]
        assert!(
            !channels[0].requires.contains(&RuntimeId::PowerShell),
            "macOS/Linux native channel must not require PowerShell"
        );
    }

    #[test]
    fn skills_dir_is_skills_cursor() {
        let dir = CursorAdapter.skills_dir().expect("skills_dir");
        let s = dir.to_string_lossy().replace('\\', "/");
        assert!(
            s.ends_with("/.cursor/skills-cursor") || s.contains("/skills-cursor"),
            "unexpected skills_dir: {s}"
        );
    }

    #[test]
    fn write_config_is_fail_closed() {
        let err = CursorAdapter
            .write_config(&AgentConfig {
                agent: AgentId::Cursor,
                raw: serde_json::json!({}),
            })
            .unwrap_err();
        assert_eq!(err.code(), "unsupported");
    }

    #[test]
    fn account_switch_disabled() {
        assert!(CursorAdapter
            .capability(crate::models::Capability::AccountSwitch)
            .is_blocked());
    }

    #[test]
    fn path_rejects_grok_agent() {
        let grok = PathBuf::from(r"C:\Users\demo\.grok\bin\agent.exe");
        assert!(path_is_rejected_non_cursor(&grok));
        assert!(!path_looks_like_cursor_agent(&grok));
    }

    #[test]
    fn path_accepts_cursor_agent_tree() {
        let p = PathBuf::from(r"C:\Users\demo\AppData\Local\cursor-agent\agent.cmd");
        assert!(path_looks_like_cursor_agent(&p));
        assert!(!path_is_rejected_non_cursor(&p));
    }

    #[test]
    fn version_heuristics() {
        assert!(version_looks_like_grok("grok 0.2.118 (1e1687c1cf)"));
        assert!(!version_looks_like_cursor("grok 0.2.118 (1e1687c1cf)"));
        assert!(version_looks_like_cursor("2026.04.29-c83a488"));
        assert!(version_looks_like_cursor("cursor-agent 2026.04.29"));
    }

    #[test]
    fn extract_version_from_ps1_install_script() {
        let body = r#"
$downloadUrl = 'https://downloads.cursor.com/lab/2026.07.23-e383d2b/'
$version = '2026.07.23-e383d2b'
function Get-Architecture { }
"#;
        assert_eq!(
            extract_latest_version_from_install_script(body).as_deref(),
            Some("2026.07.23-e383d2b")
        );
    }

    #[test]
    fn extract_version_from_bash_install_script() {
        let body = r#"
TEMP_EXTRACT_DIR="$HOME/.local/share/cursor-agent/versions/.tmp-2026.07.23-e383d2b-$(date +%s)"
DOWNLOAD_URL="https://downloads.cursor.com/lab/2026.07.23-e383d2b/${OS}/${ARCH}/agent-cli-package.tar.gz"
FINAL_DIR="$HOME/.local/share/cursor-agent/versions/2026.07.23-e383d2b"
"#;
        assert_eq!(
            extract_latest_version_from_install_script(body).as_deref(),
            Some("2026.07.23-e383d2b")
        );
    }

    #[test]
    fn extract_version_supports_timestamped_build_id() {
        let body = "$version = '2026.08.01-12-30-45-abcdef1'";
        assert_eq!(
            extract_latest_version_from_install_script(body).as_deref(),
            Some("2026.08.01-12-30-45-abcdef1")
        );
    }

    #[test]
    fn build_api_key_account_pool_only() {
        let acc = CursorAdapter
            .build_api_key_account("cursor-secret-key")
            .unwrap();
        assert_eq!(acc.agent, AgentId::Cursor);
        assert_eq!(acc.kind, AccountKind::ApiKey);
        let err = CursorAdapter.apply_account(&acc).unwrap_err();
        assert_eq!(err.code(), "unsupported");
    }

    #[test]
    fn detect_does_not_treat_grok_agent_as_cursor() {
        // On this developer machine PATH agent is often Grok — must not become Installed.
        let r = CursorAdapter.detect();
        if r.status == DetectStatus::Installed {
            let path = r.binary_path.as_ref().unwrap();
            assert!(
                !path_is_rejected_non_cursor(path),
                "installed path must not be Grok: {}",
                path.display()
            );
            let s = path.to_string_lossy().to_ascii_lowercase();
            assert!(
                s.contains("cursor-agent")
                    || path
                        .file_stem()
                        .and_then(|n| n.to_str())
                        .map(|n| n.eq_ignore_ascii_case("cursor-agent"))
                        .unwrap_or(false),
                "unexpected cursor binary path: {}",
                path.display()
            );
        } else {
            // Expected on machines without Cursor Agent CLI (this repo host as of 2026-08).
            assert_eq!(r.status, DetectStatus::NotFound);
            // Notes should mention IDE or firefighting; must not claim installed via Grok.
            assert!(r.binary_path.is_none());
        }
    }
}
