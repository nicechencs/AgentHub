//! Runtime / Agent install orchestration.
//!
//! Safety rules:
//! - Only allowlisted programs and package ids / script URLs.
//! - Never report success unless redetect confirms the expected state.
//! - Env uninstall is intentionally not provided.

use std::time::Instant;

use std::path::{Path, PathBuf};

use crate::adapters::{
    is_under_agenthub_user_npm_prefix, user_writable_npm_prefix, AdapterRegistry,
};
use crate::catalog::limits::{
    INSTALL_AGENT_TIMEOUT as AGENT_TIMEOUT, INSTALL_ENV_TIMEOUT as ENV_TIMEOUT,
    INSTALL_MAX_OUTPUT_BYTES as MAX_OUTPUT,
};
use crate::error::{AppError, Result};
use crate::logging::{self, targets};
use crate::models::{
    AgentId, DetectStatus, EnvNotReady, EnvStatusKind, InstallOutcome, Remediation, RuntimeId,
};
use crate::platform::install::{builtin_install_registry, InstallContribution};
use crate::platform::AgentKey;
use crate::runtime;
use crate::services::{LiveWriteAuthority, LiveWriteGuard};
use crate::storage::Database;
use crate::utils::command_exec::{CommandExecutor, ExecRequest, ExecResult, SystemCommandExecutor};
use crate::utils::paths::{same_path_identity, validate_default_agent_config_purge_target};
use crate::utils::redact::redact_text;

fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

/// Leftover AgentHub data-dir npm prefix (`<data>/npm`). **Not an install target.**
///
/// Installs use a user-writable prefix detect already scans (`~/.npm-global` /
/// `%APPDATA%\npm`). This path is only probed so uninstall can clean copies
/// written by older AgentHub versions.
pub(crate) fn leftover_agenthub_npm_prefix() -> Result<PathBuf> {
    Ok(crate::utils::paths::resolve_data_dir(None)?.join("npm"))
}

fn leftover_agenthub_npm_prefix_candidates() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(prefix) = leftover_agenthub_npm_prefix() {
        out.push(prefix);
    }
    if let Ok(home) = crate::utils::paths::home_dir() {
        let fallback = home.join(".agenthub").join("npm");
        if !out.iter().any(|p| p == &fallback) {
            out.push(fallback);
        }
    }
    out
}

fn npm_prefix_populated(prefix: &Path) -> bool {
    prefix.join("node_modules").is_dir() || prefix.join("lib").join("node_modules").is_dir()
}

fn leftover_prefix_populated(prefix: &Path) -> bool {
    npm_prefix_populated(prefix)
}

/// User-writable npm prefix that detect already scans. Never leftover `~/.agenthub/npm`.
pub(crate) fn detect_scanned_user_npm_prefix() -> Result<PathBuf> {
    let prefix = user_writable_npm_prefix().ok_or_else(|| {
        AppError::message(
            "install.npm_prefix",
            "cannot resolve a user-writable npm prefix",
        )
    })?;
    if is_under_agenthub_user_npm_prefix(&prefix)
        || leftover_agenthub_npm_prefix_candidates()
            .iter()
            .any(|leftover| leftover == &prefix)
    {
        return Err(AppError::message(
            "install.npm_prefix",
            "refusing leftover AgentHub data-dir npm prefix as install target",
        ));
    }
    Ok(prefix)
}

fn ensure_detect_scanned_user_npm_prefix() -> Result<PathBuf> {
    let prefix = detect_scanned_user_npm_prefix()?;
    std::fs::create_dir_all(&prefix)?;
    Ok(prefix)
}

/// Prefixes that still contain packages written by older AgentHub versions.
pub(crate) fn leftover_agenthub_npm_prefixes_present() -> Vec<PathBuf> {
    leftover_agenthub_npm_prefix_candidates()
        .into_iter()
        .filter(|p| leftover_prefix_populated(p))
        .collect()
}

/// True when an older AgentHub actually wrote packages under a leftover prefix.
#[cfg(test)]
pub(crate) fn leftover_agenthub_npm_prefix_present() -> Option<PathBuf> {
    leftover_agenthub_npm_prefixes_present().into_iter().next()
}

fn looks_like_permission_failure(res: &ExecResult) -> bool {
    let blob = format!(
        "{}
{}
{}",
        res.stderr,
        res.stdout,
        res.spawn_error.as_deref().unwrap_or("")
    );
    let lower = blob.to_ascii_lowercase();
    lower.contains("eacces")
        || lower.contains("eperm")
        || lower.contains("permission denied")
        || lower.contains("operation not permitted")
        || lower.contains("access is denied")
}

fn install_command_failure_message(agent_label: &str, res: &ExecResult) -> String {
    if res.timed_out {
        return format!("{agent_label} 安装超时");
    }
    if let Some(err) = &res.spawn_error {
        return format!("{agent_label} 安装命令无法启动：{err}");
    }
    let code = res.exit_code.unwrap_or(-1);
    if looks_like_permission_failure(res) {
        return format!("{agent_label} 安装失败：没有写入权限（EACCES，退出码 {code}）");
    }
    format!("{agent_label} 安装失败（退出码 {code}）")
}

/// Native channel that only opens an official Setup page (WorkBuddy).
const SETUP_GUIDE_CODE: &str = "setup_guide";

fn is_native_setup_guide(contribution: &dyn InstallContribution, channel: &str) -> bool {
    channel == "native"
        && contribution.native_setup_url().is_some()
        && contribution.native_ps1_url().is_none()
        && contribution.native_sh_url().is_none()
}

fn setup_guide_message(agent_label: &str) -> String {
    format!("{agent_label} 已打开官网安装页，请完成安装后重启 AgentHub")
}

fn setup_guide_diagnosis() -> &'static str {
    "诊断：该 Agent 没有脚本安装，已打开官网安装页。请完成安装后，完全退出并重启 AgentHub。"
}

fn command_failure_diagnosis(res: &ExecResult) -> String {
    if looks_like_permission_failure(res) {
        "诊断：没有写入权限，不是 PATH 问题。".into()
    } else if res.timed_out {
        "诊断：安装超时。".into()
    } else if res.spawn_error.is_some() {
        "诊断：安装命令无法启动。".into()
    } else {
        format!(
            "诊断：安装命令未成功退出（退出码 {}）。",
            res.exit_code.unwrap_or(-1)
        )
    }
}

/// Put the short human diagnosis above raw installer output.
fn prepend_diagnosis(logs: &mut Vec<String>, diagnosis: impl Into<String>) {
    let diagnosis = diagnosis.into();
    if logs.first().is_some_and(|line| line == &diagnosis) {
        return;
    }
    logs.insert(0, diagnosis);
}

fn is_installer_progress_noise(line: &str) -> bool {
    let lower = line.trim().to_ascii_lowercase();
    if lower.is_empty() {
        return false;
    }
    lower.contains("http fetch")
        || lower.contains("npm http")
        || lower.contains("npm notice")
        || (lower.starts_with("get ") && lower.contains("http"))
        || (lower.starts_with("put ") && lower.contains("http"))
        || lower.contains("content-length")
        || (lower.contains("timing") && lower.contains("http"))
        || lower.contains("cache hit")
        || lower.contains("cache miss")
}

/// Keep diagnosis / command / errors; collapse npm HTTP progress into one line.
fn summarize_installer_output_lines(
    lines: impl IntoIterator<Item = impl AsRef<str>>,
) -> Vec<String> {
    let mut out = Vec::new();
    let mut skipped_noise = 0usize;
    let flush_noise = |out: &mut Vec<String>, skipped_noise: &mut usize| {
        if *skipped_noise == 0 {
            return;
        }
        out.push(format!("（已省略 {skipped_noise} 行下载进度）"));
        *skipped_noise = 0;
    };
    for line in lines {
        let line = line.as_ref();
        if line.trim().is_empty() {
            continue;
        }
        if is_installer_progress_noise(line) {
            skipped_noise += 1;
            continue;
        }
        flush_noise(&mut out, &mut skipped_noise);
        out.push(line.to_string());
    }
    flush_noise(&mut out, &mut skipped_noise);
    const BODY_CAP: usize = 40;
    if out.len() <= BODY_CAP {
        return out;
    }
    let mut capped = Vec::with_capacity(BODY_CAP + 1);
    let keep_head = 12;
    let keep_tail = BODY_CAP - keep_head;
    capped.extend(out.iter().take(keep_head).cloned());
    capped.push(format!(
        "（已省略 {} 行安装输出）",
        out.len() - keep_head - keep_tail
    ));
    capped.extend(out.iter().rev().take(keep_tail).rev().cloned());
    capped
}

fn setup_guide_open_failed(res: &ExecResult) -> bool {
    res.spawn_error.is_some() || res.timed_out
}

/// Log start/end for install-family ops. Business failures (`Ok(outcome.ok=false)`)
/// are ERROR; hard `Err` uses structured app-error helpers.
fn log_install_result(
    op: &str,
    started: Instant,
    agent: Option<&str>,
    runtime: Option<&str>,
    result: &Result<InstallOutcome>,
) {
    let elapsed = elapsed_ms(started);
    match result {
        Ok(out) if out.ok => {
            tracing::info!(
                module = targets::INSTALL,
                op = op,
                agent = agent.unwrap_or("-"),
                runtime = runtime.unwrap_or("-"),
                action = %out.action,
                elapsed_ms = elapsed,
                "ok"
            );
        }
        Ok(out) if out.code.as_deref() == Some(SETUP_GUIDE_CODE) => {
            tracing::info!(
                module = targets::INSTALL,
                op = op,
                agent = agent.unwrap_or("-"),
                runtime = runtime.unwrap_or("-"),
                action = %out.action,
                elapsed_ms = elapsed,
                code = SETUP_GUIDE_CODE,
                "opened official setup page"
            );
        }
        Ok(out) => {
            let msg = redact_text(&out.message);
            // Firefighting: surface a few diagnostic log lines at error level (already redacted in push_exec_logs).
            let diag: Vec<&str> = out
                .logs
                .iter()
                .rev()
                .filter(|l| {
                    let t = l.trim();
                    !t.is_empty()
                        && (t.contains("重新检测")
                            || t.contains("redetect")
                            || t.contains("诊断")
                            || t.contains("PATH")
                            || t.contains("not_found")
                            || t.contains("using PowerShell")
                            || t.contains("using npm")
                            || t.starts_with("✗")
                            || t.starts_with("version:"))
                })
                .take(6)
                .map(|s| s.as_str())
                .collect();
            for line in diag.into_iter().rev() {
                let safe = redact_text(line);
                tracing::error!(
                    module = targets::INSTALL,
                    code = "install.diag",
                    op = op,
                    agent = agent.unwrap_or("-"),
                    runtime = runtime.unwrap_or("-"),
                    action = %out.action,
                    "diag: {safe}"
                );
            }
            tracing::error!(
                module = targets::INSTALL,
                code = "install.failed",
                op = op,
                agent = agent.unwrap_or("-"),
                runtime = runtime.unwrap_or("-"),
                action = %out.action,
                elapsed_ms = elapsed,
                log_lines = out.logs.len(),
                "{msg}"
            );
        }
        Err(e) => {
            if let Some(a) = agent {
                logging::log_app_error_agent(targets::INSTALL, op, a, e);
            } else {
                logging::log_app_error(targets::INSTALL, op, e);
            }
        }
    }
}

fn resolve_bin(names: &[&str]) -> Result<String> {
    runtime::resolve_binary(names)
        .map(|path| path.to_string_lossy().into_owned())
        .ok_or_else(|| AppError::NotFound(format!("command not found: {}", names.join(" | "))))
}

/// Allowlisted binary paths that may be deleted on native uninstall (never arbitrary dirs).
fn native_uninstall_bin_paths(contribution: &dyn InstallContribution) -> Vec<std::path::PathBuf> {
    contribution.native_uninstall_bin_paths()
}

/// Allowlisted external uninstallers: (program, args). Never run arbitrary paths.
fn native_uninstaller_specs(
    contribution: &dyn InstallContribution,
) -> Vec<(std::path::PathBuf, Vec<String>)> {
    contribution
        .native_uninstaller_specs()
        .into_iter()
        .map(|s| (s.program, s.args))
        .collect()
}

fn require_contribution_key(key: &AgentKey, contribution: &dyn InstallContribution) -> Result<()> {
    if contribution.agent_key() != *key {
        return Err(AppError::InvalidArg(format!(
            "install contribution key mismatch: expected {}, got {}",
            key.as_str(),
            contribution.agent_key().as_str()
        )));
    }
    Ok(())
}

fn default_channel_from_contribution(contribution: &dyn InstallContribution) -> String {
    let has_npm = contribution.npm_package().is_some();
    let has_native = contribution.native_ps1_url().is_some()
        || contribution.native_sh_url().is_some()
        || contribution.native_setup_url().is_some();
    if contribution.prefer_npm_channel_first() && has_npm {
        "npm".into()
    } else if has_native {
        "native".into()
    } else if has_npm {
        "npm".into()
    } else {
        "native".into()
    }
}

fn channel_requires_from_contribution(
    contribution: &dyn InstallContribution,
    channel: &str,
) -> Result<Vec<RuntimeId>> {
    match channel {
        "npm" => {
            if contribution.npm_package().is_none() {
                return Err(AppError::Unsupported(
                    "contribution has no npm package for channel 'npm'".into(),
                ));
            }
            Ok(vec![RuntimeId::NodeJs, RuntimeId::Npm])
        }
        "native" => {
            let has_native = contribution.native_ps1_url().is_some()
                || contribution.native_sh_url().is_some()
                || contribution.native_setup_url().is_some();
            if !has_native {
                return Err(AppError::Unsupported(
                    "contribution has no native install material for channel 'native'".into(),
                ));
            }
            // POSIX shell installers do not need PowerShell; Windows ps1 does.
            #[cfg(windows)]
            if contribution.native_ps1_url().is_some() {
                return Ok(vec![RuntimeId::PowerShell]);
            }
            let _ = contribution;
            Ok(Vec::new())
        }
        other => Err(AppError::InvalidArg(format!(
            "channel '{other}' is unsupported"
        ))),
    }
}

fn push_log(logs: &mut Vec<String>, line: impl Into<String>) {
    let line = line.into();
    logs.push(line.clone());
    // Live GUI stream (no-op when hook unset / CLI).
    crate::services::emit_install_log(&line);
}

fn push_exec_logs(logs: &mut Vec<String>, res: &ExecResult, timeout_secs: u64) {
    use crate::utils::redact::redact_text;

    let cmd = redact_text(&res.command);
    // `$ cmd` already streamed at process start; keep it in the outcome buffer.
    logs.push(format!("$ {cmd}"));
    tracing::debug!(
        target: crate::logging::targets::INSTALL,
        module = crate::logging::targets::INSTALL,
        op = "exec",
        command = %cmd,
        "install command"
    );
    if let Some(err) = &res.spawn_error {
        let line = redact_text(&format!("spawn failed: {err}"));
        push_log(logs, line.clone());
        tracing::warn!(
            target: crate::logging::targets::INSTALL,
            module = crate::logging::targets::INSTALL,
            op = "exec",
            "{line}"
        );
        return;
    }
    // Body lines were already streamed live via emit_install_log while the
    // process ran; the outcome buffer is the fail-panel body, so collapse
    // npm HTTP progress instead of dumping thousands of lines.
    let raw_body: Vec<String> = res
        .stdout
        .lines()
        .chain(res.stderr.lines())
        .map(redact_text)
        .collect();
    logs.extend(summarize_installer_output_lines(raw_body));
    if res.timed_out {
        let line = format!("✗ timed out after {timeout_secs}s");
        push_log(logs, line.clone());
        tracing::warn!(
            target: crate::logging::targets::INSTALL,
            module = crate::logging::targets::INSTALL,
            op = "exec",
            "{line}"
        );
    } else if let Some(code) = res.exit_code {
        if code == 0 {
            push_log(logs, "✓ exit 0");
            tracing::debug!(
                target: crate::logging::targets::INSTALL,
                module = crate::logging::targets::INSTALL,
                op = "exec",
                exit = 0,
                "ok"
            );
        } else {
            let line = format!("✗ exit {code}");
            push_log(logs, line.clone());
            tracing::warn!(
                target: crate::logging::targets::INSTALL,
                module = crate::logging::targets::INSTALL,
                op = "exec",
                exit = code,
                "{line}"
            );
        }
    }
}

fn channel_requires(
    registry: &AdapterRegistry,
    agent: AgentId,
    channel: &str,
) -> Result<Vec<RuntimeId>> {
    let adapter = registry
        .get(agent)
        .ok_or_else(|| AppError::NotFound(format!("unknown agent {}", agent.as_str())))?;
    let ch = adapter
        .install_channels()
        .into_iter()
        .find(|c| c.id == channel)
        .ok_or_else(|| {
            AppError::InvalidArg(format!(
                "channel '{channel}' not supported for {}",
                agent.as_str()
            ))
        })?;

    // Adapter metadata predates the platform-aware catalog and historically
    // marked every native channel as requiring PowerShell.  POSIX shell
    // installers (`install.sh`) execute with bash/sh, so carrying that
    // requirement on macOS/Linux would block an otherwise ready install.
    // Windows keeps the adapter's PowerShell requirement unchanged.
    #[cfg(not(windows))]
    if channel == "native" {
        return Ok(Vec::new());
    }

    Ok(ch.requires)
}

/// Install a shared runtime (Node.js / Git via winget on Windows or Homebrew
/// on macOS). Linux uses the `manual` channel: remediations only, no spawn.
/// Passing an empty channel selects the platform default.
pub fn install_runtime(
    id: RuntimeId,
    channel: &str,
    executor: &dyn CommandExecutor,
) -> Result<InstallOutcome> {
    install_runtime_inner(id, channel, executor)
}

/// winget package id for runtimes that support one-click install.
#[cfg_attr(not(windows), allow(dead_code))]
fn winget_package_id(id: RuntimeId) -> Option<&'static str> {
    match id {
        RuntimeId::NodeJs | RuntimeId::Npm => Some("OpenJS.NodeJS.LTS"),
        RuntimeId::Git => Some("Git.Git"),
        RuntimeId::PowerShell => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(all(not(windows), not(target_os = "macos")), allow(dead_code))]
enum RuntimePackageAction {
    Install,
    Upgrade,
}

/// Already-present Node/Git is upgraded; missing or PATH-broken is installed.
#[cfg_attr(all(not(windows), not(target_os = "macos")), allow(dead_code))]
fn runtime_package_action(status: EnvStatusKind) -> RuntimePackageAction {
    match status {
        EnvStatusKind::Ok | EnvStatusKind::Outdated => RuntimePackageAction::Upgrade,
        EnvStatusKind::Missing | EnvStatusKind::BrokenPath => RuntimePackageAction::Install,
    }
}

#[cfg_attr(all(not(windows), not(target_os = "macos")), allow(dead_code))]
fn package_manager_verb(action: RuntimePackageAction) -> &'static str {
    match action {
        RuntimePackageAction::Install => "install",
        RuntimePackageAction::Upgrade => "upgrade",
    }
}

#[cfg_attr(all(not(windows), not(target_os = "macos")), allow(dead_code))]
fn package_manager_zh(action: RuntimePackageAction) -> &'static str {
    match action {
        RuntimePackageAction::Install => "安装",
        RuntimePackageAction::Upgrade => "升级",
    }
}

/// The native runtime package manager for the current desktop platform.
///
/// Keep Windows on winget for compatibility. macOS uses Homebrew because it is
/// the standard way to install both Node.js and Git without a PowerShell
/// dependency. Linux does not spawn a package manager: the default is `manual`,
/// and `apt` / `dnf` / `pacman` / `zypper` / `apk` are the same copy-command
/// path (no sudo).
fn default_runtime_channel() -> &'static str {
    if cfg!(target_os = "macos") {
        "brew"
    } else if cfg!(windows) {
        "winget"
    } else {
        "manual"
    }
}

#[cfg(target_os = "macos")]
fn brew_formula(id: RuntimeId) -> Option<&'static str> {
    match id {
        RuntimeId::NodeJs | RuntimeId::Npm => Some("node"),
        RuntimeId::Git => Some("git"),
        RuntimeId::PowerShell => None,
    }
}

/// Resolve Homebrew even when a GUI-launched process has not inherited the
/// user's shell PATH.  The two paths cover Intel and Apple Silicon defaults.
#[cfg(target_os = "macos")]
fn resolve_brew() -> Result<String> {
    resolve_bin(&["brew"]).map_err(|_| {
        AppError::NotFound(
            "command not found: brew (install Homebrew from https://brew.sh/)".into(),
        )
    })
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
const NODEJS_DIST_INDEX_URL: &str = "https://nodejs.org/dist/index.json";

fn is_safe_node_version(version: &str) -> bool {
    let mut parts = version.split('.');
    let Some(major) = parts.next() else {
        return false;
    };
    let Some(minor) = parts.next() else {
        return false;
    };
    let Some(patch) = parts.next() else {
        return false;
    };
    parts.next().is_none()
        && !major.is_empty()
        && major.chars().all(|c| c.is_ascii_digit())
        && !minor.is_empty()
        && minor.chars().all(|c| c.is_ascii_digit())
        && !patch.is_empty()
        && patch.chars().all(|c| c.is_ascii_digit())
}

/// First LTS release in nodejs.org `index.json` (newest-first) that ships a macOS `.pkg`.
fn pick_nodejs_macos_lts_pkg(index_json: &str) -> Option<(String, String)> {
    let releases: Vec<serde_json::Value> = serde_json::from_str(index_json).ok()?;
    for rel in releases {
        let lts = &rel["lts"];
        let is_lts = lts.as_str().is_some() || lts.as_bool() == Some(true);
        if !is_lts {
            continue;
        }
        let version = rel["version"].as_str()?.trim_start_matches('v');
        if !is_safe_node_version(version) {
            continue;
        }
        let files = rel["files"].as_array()?;
        let has_pkg = files.iter().any(|file| {
            matches!(
                file.as_str(),
                Some("pkg") | Some("osx-arm64-pkg") | Some("osx-x64-pkg")
            )
        });
        if !has_pkg {
            continue;
        }
        let url = format!("https://nodejs.org/dist/v{version}/node-v{version}.pkg");
        return Some((version.to_string(), url));
    }
    None
}

#[cfg(target_os = "macos")]
fn osascript_installer_script(pkg: &Path) -> String {
    format!(
        r#"do shell script "/usr/sbin/installer -pkg " & quoted form of "{}" & " -target /" with administrator privileges"#,
        pkg.display()
    )
}

/// Official macOS Node.js `.pkg` when Homebrew is not installed.
///
/// Many machines have Node from the nodejs.org installer (`/usr/local/bin/node`)
/// and never install Homebrew. One-click upgrade must still work.
#[cfg(target_os = "macos")]
fn install_nodejs_official_macos_pkg(
    executor: &dyn CommandExecutor,
    logs: &mut Vec<String>,
) -> Result<ExecResult> {
    let curl = resolve_bin(&["curl"])?;
    push_log(logs, "正在读取 Node.js 官网版本列表…");
    let index_res = executor.run(&ExecRequest {
        program: curl.clone(),
        args: vec!["-fsSL".into(), NODEJS_DIST_INDEX_URL.into()],
        timeout: ENV_TIMEOUT,
        max_output_bytes: MAX_OUTPUT,
    });
    push_exec_logs(logs, &index_res, ENV_TIMEOUT.as_secs());
    if !index_res.success() {
        return Err(AppError::message(
            "install.node_pkg",
            "无法读取 Node.js 官网版本列表",
        ));
    }
    let (version, url) = pick_nodejs_macos_lts_pkg(&index_res.stdout).ok_or_else(|| {
        AppError::message("install.node_pkg", "无法从官网版本列表中解析 macOS 安装包")
    })?;
    if !url.starts_with("https://nodejs.org/dist/v") || !url.ends_with(".pkg") {
        return Err(AppError::message(
            "install.node_pkg",
            "官网安装包地址不在允许范围",
        ));
    }
    push_log(logs, format!("将安装 Node.js {version}（官网安装包）…"));
    let tmp = tempfile::Builder::new()
        .prefix("agenthub-node-")
        .suffix(".pkg")
        .tempfile()?;
    let pkg_path = tmp.path().to_path_buf();
    let download_res = executor.run(&ExecRequest {
        program: curl,
        args: vec![
            "-fL".into(),
            "--progress-bar".into(),
            "-o".into(),
            pkg_path.to_string_lossy().into_owned(),
            url,
        ],
        timeout: ENV_TIMEOUT,
        max_output_bytes: MAX_OUTPUT,
    });
    push_exec_logs(logs, &download_res, ENV_TIMEOUT.as_secs());
    if !download_res.success() {
        return Err(AppError::message(
            "install.node_pkg",
            "下载 Node.js 官网安装包失败",
        ));
    }

    if let Ok(osascript) = resolve_bin(&["osascript"]) {
        push_log(
            logs,
            "将请求本机密码以完成安装（系统对话框，不是 AgentHub 密码）…",
        );
        let res = executor.run(&ExecRequest {
            program: osascript,
            args: vec!["-e".into(), osascript_installer_script(&pkg_path)],
            timeout: ENV_TIMEOUT,
            max_output_bytes: MAX_OUTPUT,
        });
        push_exec_logs(logs, &res, ENV_TIMEOUT.as_secs());
        return Ok(res);
    }

    let open = resolve_bin(&["open"])?;
    push_log(
        logs,
        "将打开系统安装器。请在窗口中完成安装后回到 AgentHub 再检测。",
    );
    let res = executor.run(&ExecRequest {
        program: open,
        args: vec!["-W".into(), pkg_path.to_string_lossy().into_owned()],
        timeout: ENV_TIMEOUT,
        max_output_bytes: MAX_OUTPUT,
    });
    push_exec_logs(logs, &res, ENV_TIMEOUT.as_secs());
    Ok(res)
}

#[cfg_attr(all(not(windows), not(target_os = "macos")), allow(dead_code))]
fn looks_like_linux_package_command(command: &str) -> bool {
    let lower = command.to_ascii_lowercase();
    lower.contains("apt-get")
        || lower.contains("apt install")
        || lower.contains("dnf install")
        || lower.contains("pacman -s")
        || lower.contains("zypper")
        || lower.contains("apk add")
}

/// Drop package-manager remediations that do not apply on this host.
/// Windows never surfaces `brew` or apt; macOS never surfaces `winget` or apt;
/// Linux never surfaces `winget`/`brew`.
fn filter_host_remediations(items: Vec<Remediation>) -> Vec<Remediation> {
    items
        .into_iter()
        .filter(|r| {
            if cfg!(windows) {
                r.kind != "brew"
                    && r.command
                        .as_deref()
                        .map_or(true, |command| !looks_like_linux_package_command(command))
            } else if cfg!(target_os = "macos") {
                r.kind != "winget"
                    && r.command
                        .as_deref()
                        .map_or(true, |command| !looks_like_linux_package_command(command))
            } else {
                r.kind != "winget" && r.kind != "brew"
            }
        })
        .collect()
}

fn host_remediations(id: RuntimeId) -> Vec<Remediation> {
    filter_host_remediations(vec![runtime::remediation_for(id)])
}

fn push_remediation_logs(logs: &mut Vec<String>, remediations: &[Remediation]) {
    for rem in remediations {
        if let Some(text) = &rem.text {
            push_log(logs, text.clone());
        }
        if let Some(command) = &rem.command {
            push_log(logs, format!("可复制命令: {command}"));
        }
        if let Some(url) = &rem.url {
            push_log(logs, format!("打开页面: {url}"));
        }
    }
}

/// brew/winget missing: coded `env.not_ready` so CLI exits 3 with remediations.
fn missing_package_manager_outcome(
    action: &str,
    mut logs: Vec<String>,
    channel: &str,
    missing: RuntimeId,
    message: impl Into<String>,
) -> InstallOutcome {
    let message = message.into();
    let remediations = match channel {
        "brew" | "winget" => filter_host_remediations(
            runtime::remediations_when_installer_missing(channel, missing),
        ),
        _ => host_remediations(missing),
    };
    push_remediation_logs(&mut logs, &remediations);
    let details = serde_json::to_value(EnvNotReady {
        agent: None,
        channel: Some(channel.into()),
        missing: vec![missing],
        remediations,
        hint: Some(message.clone()),
    })
    .ok();
    InstallOutcome::failure(action, logs, message).with_code("env.not_ready", details)
}

fn unsupported_channel_outcome(action: &str, logs: Vec<String>, channel: &str) -> InstallOutcome {
    #[cfg(target_os = "macos")]
    let platform_hint = "macOS 默认使用 brew；可传 --channel brew";
    #[cfg(windows)]
    let platform_hint = "Windows 默认使用 winget";
    #[cfg(all(not(target_os = "macos"), not(windows)))]
    let platform_hint = "Linux 不自动执行包管理器；默认 --channel manual，也可传 apt/dnf/pacman/zypper/apk 以拿到可复制命令与官网";

    let supported = default_runtime_channel();
    #[cfg(target_os = "macos")]
    let suffix = "（macOS 默认使用 brew；可传 --channel brew）";
    #[cfg(windows)]
    let suffix = "";
    #[cfg(all(not(target_os = "macos"), not(windows)))]
    let suffix = "（Linux 默认 manual；apt/dnf/pacman/zypper/apk 只返回可复制命令，不自动 sudo）";

    let message = format!("不支持的安装渠道 '{channel}'（当前仅 {supported}{suffix}）");
    InstallOutcome::failure(action, logs, message).with_code(
        "unsupported",
        Some(serde_json::json!({
            "channel": channel,
            "hint": platform_hint,
        })),
    )
}

/// Complete an environment install by invalidating detection caches and
/// checking the exact requested runtime (plus Node.js for an npm request).
#[cfg_attr(not(any(windows, target_os = "macos")), allow(dead_code))]
fn finalize_runtime_install(
    id: RuntimeId,
    mut logs: Vec<String>,
    res: ExecResult,
) -> InstallOutcome {
    runtime::invalidate_cache();
    let status = runtime::detect_one(id);
    if !res.success() {
        logs.push(format!(
            "安装命令失败（exit={}）；重新检测结果不会覆盖该失败。",
            res.exit_code
                .map(|code| code.to_string())
                .unwrap_or_else(|| "unknown".into())
        ));
        return InstallOutcome {
            ok: false,
            action: "env_install".into(),
            logs,
            message: format!("{} 安装命令未成功完成", id.as_str()),
            agent: None,
            runtime: Some(status),
            ..Default::default()
        };
    }
    let node = runtime::detect_one(RuntimeId::NodeJs);
    let ok = match id {
        RuntimeId::Npm => node.status == EnvStatusKind::Ok && status.status == EnvStatusKind::Ok,
        RuntimeId::NodeJs => node.status == EnvStatusKind::Ok,
        RuntimeId::PowerShell | RuntimeId::Git => status.status == EnvStatusKind::Ok,
    };

    if ok {
        InstallOutcome {
            ok: true,
            action: "env_install".into(),
            logs,
            message: format!(
                "{} 已就绪{}",
                id.as_str(),
                if res.success() {
                    ""
                } else {
                    "（命令非 0 退出，但重新检测已通过）"
                }
            ),
            agent: None,
            runtime: Some(status),
            ..Default::default()
        }
    } else {
        logs.push(format!("重新检测: {} => {:?}", id.as_str(), status.status));
        logs.push(
            "提示: 安装成功后当前进程 PATH 可能未刷新，请完全退出并重启 AgentHub 后再检测。".into(),
        );
        InstallOutcome {
            ok: false,
            action: "env_install".into(),
            logs,
            message: format!(
                "{} 安装后检测仍未就绪（status={:?}）",
                id.as_str(),
                status.status
            ),
            agent: None,
            runtime: Some(status),
            ..Default::default()
        }
    }
}

fn install_runtime_inner(
    id: RuntimeId,
    channel: &str,
    executor: &dyn CommandExecutor,
) -> Result<InstallOutcome> {
    let started = Instant::now();
    let channel_log = if channel.is_empty() {
        default_runtime_channel()
    } else {
        channel
    };
    tracing::info!(
        module = targets::INSTALL,
        op = "install_runtime",
        runtime = id.as_str(),
        channel = channel_log,
        "start"
    );

    let result = (|| {
        let mut logs = Vec::new();
        let action = "env_install";

        // npm is bundled with Node — install Node instead.
        let target = match id {
            RuntimeId::Npm => RuntimeId::NodeJs,
            other => other,
        };

        if target == RuntimeId::PowerShell {
            let ps = runtime::detect_one(RuntimeId::PowerShell);
            for n in &ps.notes {
                logs.push(n.clone());
            }
            let msg = if cfg!(windows) {
                "PowerShell 不支持一键安装。Windows 通常自带 5.1；PowerShell 7 (pwsh) 需手动安装。任一带可用即可跑 native 安装脚本。"
            } else {
                "macOS/Linux 不需要 PowerShell：native 安装使用官方 bash/sh 脚本。AgentHub 不会在此平台检测或安装 PowerShell。"
            };
            return Ok(InstallOutcome::failure(action, logs, msg).with_code("unsupported", None));
        }

        let channel = if channel.is_empty() {
            default_runtime_channel()
        } else {
            channel
        };

        let linux_copy_channel = cfg!(all(not(windows), not(target_os = "macos")))
            && matches!(
                channel,
                "manual" | "apt" | "dnf" | "pacman" | "zypper" | "apk"
            );
        if channel == "manual" || linux_copy_channel {
            logs.push(format!(
                "# install runtime {} via {channel} remediations (no package manager spawned)",
                target.as_str()
            ));
            return Ok(missing_package_manager_outcome(
                action,
                logs,
                channel,
                target,
                "Linux 不自动执行包管理器。请按可复制命令或官网安装后，完全退出并重启 AgentHub 再检测。",
            ));
        }

        #[cfg(not(windows))]
        if channel == "winget" {
            return Ok(unsupported_channel_outcome(action, logs, channel));
        }

        #[cfg(target_os = "macos")]
        if channel == "brew" {
            let formula = brew_formula(target).ok_or_else(|| {
                AppError::Unsupported(format!("runtime {} 暂不支持 Homebrew 安装", id.as_str()))
            })?;
            let brew = match resolve_brew() {
                Ok(path) => path,
                Err(_) if target == RuntimeId::NodeJs => {
                    logs.push("未找到 Homebrew，改用 Node.js 官网安装包。".into());
                    let before = runtime::detect_one(RuntimeId::NodeJs);
                    match install_nodejs_official_macos_pkg(executor, &mut logs) {
                        Ok(res) => {
                            let out = finalize_runtime_install(id, logs, res.clone());
                            let same_version =
                                out.runtime.as_ref().and_then(|row| row.version.as_deref())
                                    == before.version.as_deref();
                            if !res.success() && same_version {
                                return Ok(InstallOutcome::failure(
                                    action,
                                    out.logs,
                                    "升级未完成。请在系统对话框中输入本机密码，或打开官网安装。完成后完全退出并重启 AgentHub 再检测。",
                                ));
                            }
                            return Ok(out);
                        }
                        Err(e) => {
                            logs.push(e.to_string());
                            return Ok(missing_package_manager_outcome(
                                action,
                                logs,
                                "brew",
                                target,
                                "未找到 Homebrew，官网安装包也未能完成。请打开 Node.js 官网安装后，完全退出并重启 AgentHub 再检测。",
                            ));
                        }
                    }
                }
                Err(_) => {
                    return Ok(missing_package_manager_outcome(
                        action,
                        logs,
                        "brew",
                        target,
                        "未找到 Homebrew，无法一键安装。请先安装 Homebrew（https://brew.sh/），或从官网手动安装。完成后完全退出并重启 AgentHub 再检测。",
                    ));
                }
            };
            let package_action = runtime_package_action(runtime::detect_one(target).status);
            let verb = package_manager_verb(package_action);
            logs.push(format!(
                "正在用 Homebrew {} {}（{formula}）…",
                package_manager_zh(package_action),
                target.as_str()
            ));
            let req = ExecRequest {
                program: brew,
                args: vec![verb.into(), formula.into()],
                timeout: ENV_TIMEOUT,
                max_output_bytes: MAX_OUTPUT,
            };
            let res = executor.run(&req);
            push_exec_logs(&mut logs, &res, ENV_TIMEOUT.as_secs());
            return Ok(finalize_runtime_install(id, logs, res));
        }

        #[cfg(not(windows))]
        {
            return Ok(unsupported_channel_outcome(action, logs, channel));
        }

        #[cfg(windows)]
        {
            if channel != "winget" {
                return Ok(unsupported_channel_outcome(action, logs, channel));
            }

            let Some(package_id) = winget_package_id(target) else {
                return Ok(InstallOutcome::failure(
                    action,
                    logs,
                    format!("runtime {} 暂不支持自动安装", id.as_str()),
                )
                .with_code("unsupported", None));
            };

            let package_action = runtime_package_action(runtime::detect_one(target).status);
            let verb = package_manager_verb(package_action);
            logs.push(format!(
                "# {verb} runtime {} via {channel} ({package_id})",
                target.as_str()
            ));

            let winget = match resolve_bin(&["winget", "winget.exe"]) {
                Ok(p) => p,
                Err(e) => {
                    logs.push(e.to_string());
                    let manual = match target {
                        RuntimeId::Git => "请手动安装 Git 后重新检测。",
                        _ => "请手动安装 Node.js LTS 后重新检测。",
                    };
                    return Ok(missing_package_manager_outcome(
                        action,
                        logs,
                        "winget",
                        target,
                        format!("未找到 winget。{manual}"),
                    ));
                }
            };

            let req = ExecRequest {
                program: winget,
                args: vec![
                    verb.into(),
                    "-e".into(),
                    "--id".into(),
                    package_id.into(),
                    "--accept-package-agreements".into(),
                    "--accept-source-agreements".into(),
                    "--disable-interactivity".into(),
                ],
                timeout: ENV_TIMEOUT,
                max_output_bytes: MAX_OUTPUT,
            };
            let res = executor.run(&req);
            push_exec_logs(&mut logs, &res, ENV_TIMEOUT.as_secs());

            Ok(finalize_runtime_install(id, logs, res))
        }
    })();

    log_install_result("install_runtime", started, None, Some(id.as_str()), &result);
    result
}

/// Install an agent via allowlisted channel (npm package or native ps1).
pub fn install_agent(
    registry: &AdapterRegistry,
    agent: AgentId,
    channel: &str,
    install_deps: bool,
    executor: &dyn CommandExecutor,
) -> Result<InstallOutcome> {
    let contribution = builtin_install_registry()
        .get_agent_id(agent)
        .ok_or_else(|| {
            AppError::NotFound(format!(
                "no install contribution for agent {}",
                agent.as_str()
            ))
        })?;
    install_agent_with_contribution(
        registry,
        agent,
        contribution.as_ref(),
        channel,
        install_deps,
        executor,
    )
}

/// Install using an explicit [`InstallContribution`] allowlist (builtin AgentId path).
///
/// Package ids / URLs / flags come from `contribution`; adapter detect remains the
/// post-install source of truth for built-in agents.
pub fn install_agent_with_contribution(
    registry: &AdapterRegistry,
    agent: AgentId,
    contribution: &dyn InstallContribution,
    channel: &str,
    install_deps: bool,
    executor: &dyn CommandExecutor,
) -> Result<InstallOutcome> {
    let key = AgentKey::from_agent_id(agent);
    require_contribution_key(&key, contribution)?;
    let started = Instant::now();
    let channel_hint = if channel.is_empty() {
        "(default)"
    } else {
        channel
    };
    tracing::info!(
        module = targets::INSTALL,
        op = "install_agent",
        agent = agent.as_str(),
        channel = channel_hint,
        install_deps = install_deps,
        "start"
    );

    let result = (|| {
        let mut logs = Vec::new();
        let action = "agent_install";
        let channel = if channel.is_empty() {
            registry
                .get(agent)
                .and_then(|a| a.install_channels().into_iter().next().map(|c| c.id))
                .unwrap_or_else(|| default_channel_from_contribution(contribution))
        } else {
            channel.to_string()
        };

        logs.push(format!(
            "# install {} channel={channel} install_deps={install_deps}",
            agent.as_str()
        ));

        let requires = channel_requires(registry, agent, &channel)?;
        if let Err(mut env_err) = runtime::ensure(&requires) {
            if !install_deps {
                env_err.agent = Some(agent.as_str().into());
                env_err.channel = Some(channel.clone());
                let msg = format!(
                    "环境未就绪: 缺少 {}。请先安装运行环境或使用 --install-deps。",
                    env_err
                        .missing
                        .iter()
                        .map(|r| r.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                );
                logs.push(msg.clone());
                let details = serde_json::to_value(&env_err).ok();
                return Ok(
                    InstallOutcome::failure(action, logs, msg).with_code("env.not_ready", details)
                );
            }
            // Bootstrap missing runtimes that we can auto-install (nodejs / git).
            for missing in &env_err.missing {
                if matches!(missing, RuntimeId::NodeJs | RuntimeId::Npm | RuntimeId::Git) {
                    logs.push(format!("# auto install runtime {}", missing.as_str()));
                    let env_out =
                        install_runtime_inner(*missing, default_runtime_channel(), executor)?;
                    logs.extend(env_out.logs);
                    if !env_out.ok {
                        return Ok(InstallOutcome::failure(
                            action,
                            logs,
                            format!(
                                "依赖 runtime {} 安装失败: {}",
                                missing.as_str(),
                                env_out.message
                            ),
                        ));
                    }
                }
            }
            if let Err(still) = runtime::ensure(&requires) {
                let msg = format!(
                    "环境仍未就绪: {}",
                    still
                        .missing
                        .iter()
                        .map(|r| r.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                );
                logs.push(msg.clone());
                return Ok(InstallOutcome::failure(action, logs, msg));
            }
        }

        let res = match channel.as_str() {
            "npm" => run_npm_install(contribution, agent.as_str(), false, executor, &mut logs)?,
            "native" => run_native_install(
                contribution,
                agent.as_str(),
                Some(agent),
                executor,
                &mut logs,
            )?,
            other => {
                return Ok(InstallOutcome::failure(
                    action,
                    logs,
                    format!("不支持的安装渠道 '{other}'"),
                ));
            }
        };

        let setup_guide = is_native_setup_guide(contribution, &channel);
        if !res.success() && !(setup_guide && !setup_guide_open_failed(&res)) {
            prepend_diagnosis(&mut logs, command_failure_diagnosis(&res));
            logs.push("安装命令未成功退出，已判定失败。".into());
            let msg = install_command_failure_message(agent.as_str(), &res);
            return Ok(InstallOutcome::failure(action, logs, msg));
        }

        runtime::invalidate_cache();
        // Agent detect cache must not show pre-install NotFound after a successful install.
        crate::services::agent_service::invalidate_detect_cache();
        let detect = registry
            .get(agent)
            .map(|a| a.detect())
            .ok_or_else(|| AppError::NotFound(format!("unknown agent {}", agent.as_str())))?;

        let installed = detect.status == DetectStatus::Installed;
        if installed {
            if let Some(p) = &detect.binary_path {
                logs.push(format!("检测：已安装 @ {}", p.display()));
            }
            for n in &detect.notes {
                logs.push(n.clone());
            }
            Ok(InstallOutcome {
                ok: true,
                action: action.into(),
                logs,
                message: format!(
                    "{} 安装完成{}",
                    agent.as_str(),
                    detect
                        .version
                        .as_deref()
                        .map(|v| format!(" (v{v})"))
                        .unwrap_or_default()
                ),
                agent: Some(detect),
                runtime: None,
                ..Default::default()
            })
        } else {
            logs.push("检测结果：未找到二进制。".into());
            for n in &detect.notes {
                logs.push(n.clone());
            }
            if setup_guide {
                prepend_diagnosis(&mut logs, setup_guide_diagnosis());
                logs.push("请在官网完成安装后，完全退出并重启 AgentHub。".into());
            } else {
                prepend_diagnosis(
                    &mut logs,
                    "诊断：命令已成功退出，但当前进程仍未找到新二进制。请完全退出并重启 AgentHub。",
                );
            }
            let mut outcome = InstallOutcome {
                ok: false,
                action: action.into(),
                logs,
                message: if setup_guide {
                    setup_guide_message(agent.as_str())
                } else {
                    format!(
                        "{} 安装命令已成功退出，但未找到二进制（请重启 AgentHub）",
                        agent.as_str()
                    )
                },
                agent: Some(detect),
                runtime: None,
                ..Default::default()
            };
            if setup_guide {
                outcome = outcome.with_code(SETUP_GUIDE_CODE, None);
            }
            Ok(outcome)
        }
    })();

    log_install_result(
        "install_agent",
        started,
        Some(agent.as_str()),
        None,
        &result,
    );
    result
}

/// Contribution-driven install for agents without a closed [`AgentId`].
///
/// Allowlist material comes solely from `contribution`. Success is based on the
/// allowlisted command exit status; lifecycle coordinator redetect remains the
/// observed-install source of truth via its detector registry.
pub fn install_from_contribution(
    key: &AgentKey,
    contribution: &dyn InstallContribution,
    channel: &str,
    install_deps: bool,
    executor: &dyn CommandExecutor,
) -> Result<InstallOutcome> {
    require_contribution_key(key, contribution)?;
    let started = Instant::now();
    let channel_hint = if channel.is_empty() {
        "(default)"
    } else {
        channel
    };
    tracing::info!(
        module = targets::INSTALL,
        op = "install_from_contribution",
        agent = key.as_str(),
        channel = channel_hint,
        install_deps = install_deps,
        "start"
    );

    let result = (|| {
        let mut logs = Vec::new();
        let action = "agent_install";
        let channel = if channel.is_empty() {
            default_channel_from_contribution(contribution)
        } else {
            channel.to_string()
        };

        logs.push(format!(
            "# install {} channel={channel} install_deps={install_deps} (contribution)",
            key.as_str()
        ));

        let requires = channel_requires_from_contribution(contribution, &channel)?;
        if let Err(mut env_err) = runtime::ensure(&requires) {
            if !install_deps {
                env_err.agent = Some(key.as_str().into());
                env_err.channel = Some(channel.clone());
                let msg = format!(
                    "环境未就绪: 缺少 {}。请先安装运行环境或使用 --install-deps。",
                    env_err
                        .missing
                        .iter()
                        .map(|r| r.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                );
                logs.push(msg.clone());
                let details = serde_json::to_value(&env_err).ok();
                return Ok(
                    InstallOutcome::failure(action, logs, msg).with_code("env.not_ready", details)
                );
            }
            for missing in &env_err.missing {
                if matches!(missing, RuntimeId::NodeJs | RuntimeId::Npm | RuntimeId::Git) {
                    logs.push(format!("# auto install runtime {}", missing.as_str()));
                    let env_out =
                        install_runtime_inner(*missing, default_runtime_channel(), executor)?;
                    logs.extend(env_out.logs);
                    if !env_out.ok {
                        return Ok(InstallOutcome::failure(
                            action,
                            logs,
                            format!(
                                "依赖 runtime {} 安装失败: {}",
                                missing.as_str(),
                                env_out.message
                            ),
                        ));
                    }
                }
            }
            if let Err(still) = runtime::ensure(&requires) {
                let msg = format!(
                    "环境仍未就绪: {}",
                    still
                        .missing
                        .iter()
                        .map(|r| r.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                );
                logs.push(msg.clone());
                return Ok(InstallOutcome::failure(action, logs, msg));
            }
        }

        let res = match channel.as_str() {
            "npm" => run_npm_install(contribution, key.as_str(), false, executor, &mut logs)?,
            "native" => run_native_install(contribution, key.as_str(), None, executor, &mut logs)?,
            other => {
                return Ok(InstallOutcome::failure(
                    action,
                    logs,
                    format!("不支持的安装渠道 '{other}'"),
                ));
            }
        };

        // No adapter redetect here — coordinator observes via DetectorRegistry.
        // Setup-guide channels intentionally return non-zero so upgrade cannot
        // claim success; opening the official page is not an install failure.
        let setup_guide = is_native_setup_guide(contribution, &channel);
        if setup_guide && !setup_guide_open_failed(&res) {
            prepend_diagnosis(&mut logs, setup_guide_diagnosis());
            return Ok(InstallOutcome {
                ok: false,
                action: action.into(),
                logs,
                message: setup_guide_message(key.as_str()),
                agent: None,
                runtime: None,
                ..Default::default()
            }
            .with_code(SETUP_GUIDE_CODE, None));
        }
        let ok = res.success();
        if !ok {
            prepend_diagnosis(&mut logs, command_failure_diagnosis(&res));
            logs.push("安装命令未成功退出，已判定失败。".into());
        }
        Ok(InstallOutcome {
            ok,
            action: action.into(),
            logs,
            message: if ok {
                format!("{} 安装命令已成功执行", key.as_str())
            } else {
                install_command_failure_message(key.as_str(), &res)
            },
            agent: None,
            runtime: None,
            ..Default::default()
        })
    })();

    log_install_result(
        "install_from_contribution",
        started,
        Some(key.as_str()),
        None,
        &result,
    );
    result
}

/// Upgrade an installed agent (npm → reinstall latest; native → re-run install.ps1).
pub fn upgrade_agent(
    registry: &AdapterRegistry,
    agent: AgentId,
    executor: &dyn CommandExecutor,
) -> Result<InstallOutcome> {
    let contribution = builtin_install_registry()
        .get_agent_id(agent)
        .ok_or_else(|| {
            AppError::NotFound(format!(
                "no install contribution for agent {}",
                agent.as_str()
            ))
        })?;
    upgrade_agent_with_contribution(registry, agent, contribution.as_ref(), executor)
}

/// Upgrade using an explicit [`InstallContribution`] allowlist (builtin AgentId path).
pub fn upgrade_agent_with_contribution(
    registry: &AdapterRegistry,
    agent: AgentId,
    contribution: &dyn InstallContribution,
    executor: &dyn CommandExecutor,
) -> Result<InstallOutcome> {
    let key = AgentKey::from_agent_id(agent);
    require_contribution_key(&key, contribution)?;
    let started = Instant::now();
    tracing::info!(
        module = targets::INSTALL,
        op = "upgrade_agent",
        agent = agent.as_str(),
        "start"
    );

    let result = (|| {
        let mut logs = Vec::new();
        let action = "agent_upgrade";

        let before = registry
            .get(agent)
            .map(|a| a.detect())
            .ok_or_else(|| AppError::NotFound(format!("unknown agent {}", agent.as_str())))?;

        if before.status != DetectStatus::Installed {
            return Ok(InstallOutcome::failure(
                action,
                logs,
                format!("{} 未安装，无法升级", agent.as_str()),
            ));
        }

        let channel = match resolve_in_app_upgrade_channel(before.channel.as_deref()) {
            Ok(ch) => ch,
            Err(message) => {
                return Ok(InstallOutcome::failure(action, logs, message));
            }
        };

        let before_ver = before.version.clone().unwrap_or_else(|| "?".into());
        push_log(
            &mut logs,
            format!(
                "# upgrade {} via {channel} (before={before_ver})",
                agent.as_str()
            ),
        );
        push_log(
            &mut logs,
            format!(
                "# 开始升级 {}：渠道={channel}，本机 v{before_ver}（下载/安装过程可能较慢）",
                agent.display_name()
            ),
        );

        let res = match channel {
            "npm" => run_npm_install(contribution, agent.as_str(), true, executor, &mut logs)?,
            _ => run_native_install(
                contribution,
                agent.as_str(),
                Some(agent),
                executor,
                &mut logs,
            )?,
        };
        // A redetected old binary is not evidence that an upgrade succeeded:
        // setup-only channels (for example WorkBuddy) and failed installers
        // intentionally leave the previous installation in place.
        let command_ok = res.success();
        if !command_ok {
            logs.push("升级命令未成功退出；即使仍检测到旧二进制，也不会报告升级完成。".into());
        }

        runtime::invalidate_cache();
        crate::services::agent_service::invalidate_detect_cache();
        let detect = registry
            .get(agent)
            .map(|a| a.detect())
            .ok_or_else(|| AppError::NotFound(format!("unknown agent {}", agent.as_str())))?;

        let after_ver = detect.version.clone().unwrap_or_else(|| "?".into());
        let ok = upgrade_succeeded(command_ok, &detect.status);
        if ok {
            logs.push(format!("version: {before_ver} → {after_ver}"));
            if before_ver == after_ver && before_ver != "?" {
                logs.push(
                    "note: version string unchanged after upgrade (already latest, or channel did not bump)"
                        .into(),
                );
            }
        } else {
            for n in &detect.notes {
                logs.push(n.clone());
            }
        }
        Ok(InstallOutcome {
            ok,
            action: action.into(),
            logs,
            message: if ok {
                format!("{} 升级完成 ({before_ver} → {after_ver})", agent.as_str())
            } else {
                format!("{} 升级后检测失败", agent.as_str())
            },
            agent: Some(detect),
            runtime: None,
            ..Default::default()
        })
    })();

    log_install_result(
        "upgrade_agent",
        started,
        Some(agent.as_str()),
        None,
        &result,
    );
    result
}

/// Contribution-driven upgrade for agents without a closed [`AgentId`].
pub fn upgrade_from_contribution(
    key: &AgentKey,
    contribution: &dyn InstallContribution,
    executor: &dyn CommandExecutor,
) -> Result<InstallOutcome> {
    require_contribution_key(key, contribution)?;
    let started = Instant::now();
    tracing::info!(
        module = targets::INSTALL,
        op = "upgrade_from_contribution",
        agent = key.as_str(),
        "start"
    );

    let result = (|| {
        let mut logs = Vec::new();
        let action = "agent_upgrade";
        let channel = default_channel_from_contribution(contribution);
        push_log(
            &mut logs,
            format!("# upgrade {} via {channel} (contribution)", key.as_str()),
        );

        let res = match channel.as_str() {
            "npm" => run_npm_install(contribution, key.as_str(), true, executor, &mut logs)?,
            _ => run_native_install(contribution, key.as_str(), None, executor, &mut logs)?,
        };
        let ok = res.success();
        if !ok {
            logs.push("升级命令未成功退出".into());
        }
        Ok(InstallOutcome {
            ok,
            action: action.into(),
            logs,
            message: if ok {
                format!("{} 升级命令已成功执行", key.as_str())
            } else {
                format!("{} 升级命令未成功", key.as_str())
            },
            agent: None,
            runtime: None,
            ..Default::default()
        })
    })();

    log_install_result(
        "upgrade_from_contribution",
        started,
        Some(key.as_str()),
        None,
        &result,
    );
    result
}

fn special_channel_kind(channel: Option<&str>) -> Option<&'static str> {
    let raw = channel.map(str::trim).filter(|c| !c.is_empty())?;
    match raw.to_ascii_lowercase().as_str() {
        "ide" => Some("ide"),
        "desktop" => Some("desktop"),
        _ => None,
    }
}

fn special_uninstall_program_message(kind: &str) -> String {
    if kind == "ide" {
        "当前是 IDE 插件安装，无法在这里卸载程序，请到 IDE 插件中卸载".into()
    } else {
        "当前是桌面应用安装，无法在这里卸载程序，请到桌面应用或 Microsoft Store 卸载".into()
    }
}

fn special_uninstall_purge_note(kind: &str) -> String {
    if kind == "ide" {
        "当前是 IDE 插件安装，程序请到 IDE 插件中卸载；将仅清理配置目录".into()
    } else {
        "当前是桌面应用安装，程序请到桌面应用或 Microsoft Store 卸载；将仅清理配置目录".into()
    }
}

/// npm / native can be upgraded here. IDE / desktop copies must be updated
/// in that product; never treat them as native and run the installer.
fn resolve_in_app_upgrade_channel(
    channel: Option<&str>,
) -> std::result::Result<&'static str, String> {
    if let Some(kind) = special_channel_kind(channel) {
        return Err(if kind == "ide" {
            "当前是 IDE 插件安装，无法在这里更新，请到 IDE 插件中更新".into()
        } else {
            "当前是桌面应用安装，无法在这里更新，请到桌面应用中更新".into()
        });
    }
    let Some(raw) = channel.map(str::trim).filter(|c| !c.is_empty()) else {
        return Ok("native");
    };
    let lower = raw.to_ascii_lowercase();
    if lower == "npm" || (lower.contains("npm") && !lower.contains("native")) {
        return Ok("npm");
    }
    Ok("native")
}

fn upgrade_succeeded(command_ok: bool, detected: &DetectStatus) -> bool {
    command_ok && *detected == DetectStatus::Installed
}

/// Uninstall agent binary when possible (npm global only).
/// Does **not** uninstall shared runtimes. Config purge is optional file delete
/// after optional backup handled by caller.
pub fn uninstall_agent(
    registry: &AdapterRegistry,
    db: &Database,
    agent: AgentId,
    purge_config: bool,
    executor: &dyn CommandExecutor,
) -> Result<InstallOutcome> {
    let authority = LiveWriteAuthority::try_from_database(db)?;
    let data_dir = if purge_config {
        crate::utils::paths::normalize_data_dir(authority.data_root())?
    } else {
        PathBuf::new()
    };
    uninstall_agent_with_authority_at_data_dir(
        registry,
        &authority,
        &data_dir,
        agent,
        purge_config,
        executor,
    )
}

/// Uninstall using a caller-composed shared live-write authority.
///
/// Lifecycle composition retains this guard across the entire purge, so the
/// destructive config-directory removal cannot race a provider bridge,
/// configuration apply, or backup restore for the same agent.
pub fn uninstall_agent_with_authority(
    registry: &AdapterRegistry,
    authority: &LiveWriteAuthority,
    agent: AgentId,
    purge_config: bool,
    executor: &dyn CommandExecutor,
) -> Result<InstallOutcome> {
    let data_dir = if purge_config {
        crate::utils::paths::normalize_data_dir(authority.data_root())?
    } else {
        PathBuf::new()
    };
    uninstall_agent_with_authority_at_data_dir(
        registry,
        authority,
        &data_dir,
        agent,
        purge_config,
        executor,
    )
}

/// Uninstall with the owning AgentHub's already-resolved data directory.
pub fn uninstall_agent_with_authority_at_data_dir(
    registry: &AdapterRegistry,
    authority: &LiveWriteAuthority,
    actual_data_dir: &Path,
    agent: AgentId,
    purge_config: bool,
    executor: &dyn CommandExecutor,
) -> Result<InstallOutcome> {
    let contribution = builtin_install_registry()
        .get_agent_id(agent)
        .ok_or_else(|| {
            AppError::NotFound(format!(
                "no install contribution for agent {}",
                agent.as_str()
            ))
        })?;
    uninstall_agent_with_contribution_and_authority_at_data_dir(
        registry,
        authority,
        &actual_data_dir,
        agent,
        contribution.as_ref(),
        purge_config,
        executor,
    )
}

/// Uninstall with an explicit contribution allowlist (builtin AgentId path).
pub fn uninstall_agent_with_contribution_and_authority(
    registry: &AdapterRegistry,
    authority: &LiveWriteAuthority,
    agent: AgentId,
    contribution: &dyn InstallContribution,
    purge_config: bool,
    executor: &dyn CommandExecutor,
) -> Result<InstallOutcome> {
    let data_dir = if purge_config {
        crate::utils::paths::normalize_data_dir(authority.data_root())?
    } else {
        PathBuf::new()
    };
    uninstall_agent_with_contribution_and_authority_at_data_dir(
        registry,
        authority,
        &data_dir,
        agent,
        contribution,
        purge_config,
        executor,
    )
}

/// Explicit-contribution uninstall using the owning AgentHub data directory.
pub fn uninstall_agent_with_contribution_and_authority_at_data_dir(
    registry: &AdapterRegistry,
    authority: &LiveWriteAuthority,
    actual_data_dir: &Path,
    agent: AgentId,
    contribution: &dyn InstallContribution,
    purge_config: bool,
    executor: &dyn CommandExecutor,
) -> Result<InstallOutcome> {
    let actual_data_dir = if purge_config {
        normalize_purge_data_dir(authority, actual_data_dir)?
    } else {
        PathBuf::new()
    };
    let key = AgentKey::from_agent_id(agent);
    require_contribution_key(&key, contribution)?;
    if purge_config {
        let guard = authority.acquire(agent)?;
        return uninstall_agent_with_contribution_and_guard_at_data_dir(
            registry,
            authority,
            &guard,
            &actual_data_dir,
            agent,
            contribution,
            true,
            executor,
        );
    }
    uninstall_agent_inner(
        registry,
        &actual_data_dir,
        agent,
        contribution,
        false,
        executor,
    )
}

/// Guarded purge counterpart for an enclosing Core lifecycle saga that has
/// already created its PreUninstall snapshot under the same authority.
pub fn uninstall_agent_with_guard(
    registry: &AdapterRegistry,
    authority: &LiveWriteAuthority,
    guard: &LiveWriteGuard,
    agent: AgentId,
    purge_config: bool,
    executor: &dyn CommandExecutor,
) -> Result<InstallOutcome> {
    let data_dir = if purge_config {
        crate::utils::paths::normalize_data_dir(authority.data_root())?
    } else {
        PathBuf::new()
    };
    uninstall_agent_with_guard_at_data_dir(
        registry,
        authority,
        guard,
        &data_dir,
        agent,
        purge_config,
        executor,
    )
}

/// Guarded uninstall using the owning AgentHub data directory.
pub fn uninstall_agent_with_guard_at_data_dir(
    registry: &AdapterRegistry,
    authority: &LiveWriteAuthority,
    guard: &LiveWriteGuard,
    actual_data_dir: &Path,
    agent: AgentId,
    purge_config: bool,
    executor: &dyn CommandExecutor,
) -> Result<InstallOutcome> {
    let actual_data_dir = if purge_config {
        crate::utils::paths::normalize_data_dir(actual_data_dir)?
    } else {
        PathBuf::new()
    };
    let contribution = builtin_install_registry()
        .get_agent_id(agent)
        .ok_or_else(|| {
            AppError::NotFound(format!(
                "no install contribution for agent {}",
                agent.as_str()
            ))
        })?;
    uninstall_agent_with_contribution_and_guard_at_data_dir(
        registry,
        authority,
        guard,
        &actual_data_dir,
        agent,
        contribution.as_ref(),
        purge_config,
        executor,
    )
}

/// Guarded purge with an explicit contribution allowlist.
pub fn uninstall_agent_with_contribution_and_guard(
    registry: &AdapterRegistry,
    authority: &LiveWriteAuthority,
    guard: &LiveWriteGuard,
    agent: AgentId,
    contribution: &dyn InstallContribution,
    purge_config: bool,
    executor: &dyn CommandExecutor,
) -> Result<InstallOutcome> {
    let data_dir = if purge_config {
        crate::utils::paths::normalize_data_dir(authority.data_root())?
    } else {
        PathBuf::new()
    };
    uninstall_agent_with_contribution_and_guard_at_data_dir(
        registry,
        authority,
        guard,
        &data_dir,
        agent,
        contribution,
        purge_config,
        executor,
    )
}

/// Explicit-contribution guarded uninstall using the owning AgentHub data dir.
pub fn uninstall_agent_with_contribution_and_guard_at_data_dir(
    registry: &AdapterRegistry,
    authority: &LiveWriteAuthority,
    guard: &LiveWriteGuard,
    actual_data_dir: &Path,
    agent: AgentId,
    contribution: &dyn InstallContribution,
    purge_config: bool,
    executor: &dyn CommandExecutor,
) -> Result<InstallOutcome> {
    let actual_data_dir = if purge_config {
        normalize_purge_data_dir(authority, actual_data_dir)?
    } else {
        PathBuf::new()
    };
    let key = AgentKey::from_agent_id(agent);
    require_contribution_key(&key, contribution)?;
    if !purge_config {
        return Err(AppError::InvalidArg(
            "live-write guard is only valid for config purge".into(),
        ));
    }
    authority.validate_guard(guard, agent)?;
    uninstall_agent_inner(
        registry,
        &actual_data_dir,
        agent,
        contribution,
        true,
        executor,
    )
}

/// A purge must use the data directory belonging to the same database-backed
/// authority.  This keeps explicit composition paths honest and prevents a
/// caller from supplying an unrelated safe-looking directory to bypass the
/// data-dir overlap policy.
fn normalize_purge_data_dir(
    authority: &LiveWriteAuthority,
    actual_data_dir: &Path,
) -> Result<PathBuf> {
    let actual = crate::utils::paths::normalize_data_dir(actual_data_dir)?;
    let authority_root = crate::utils::paths::normalize_data_dir(authority.data_root())?;
    if !same_path_identity(&actual, &authority_root)? {
        return Err(AppError::InvalidArg(format!(
            "cannot purge config: data directory {} does not match the database authority root {}",
            actual.display(),
            authority_root.display()
        )));
    }
    Ok(actual)
}

fn uninstall_agent_inner(
    registry: &AdapterRegistry,
    actual_data_dir: &Path,
    agent: AgentId,
    contribution: &dyn InstallContribution,
    purge_config: bool,
    executor: &dyn CommandExecutor,
) -> Result<InstallOutcome> {
    let started = Instant::now();
    tracing::info!(
        module = targets::INSTALL,
        op = "uninstall_agent",
        agent = agent.as_str(),
        purge_config = purge_config,
        "start"
    );

    let result = (|| {
        let mut logs = Vec::new();
        let action = "agent_uninstall";

        // Resolve and validate the fixed default target before any uninstall
        // side effect. Custom agent-owned environment roots fail closed.
        let mut purge_home = if purge_config {
            Some(validate_default_agent_config_purge_target(
                agent,
                actual_data_dir,
            )?)
        } else {
            None
        };

        let before = registry
            .get(agent)
            .map(|a| a.detect())
            .ok_or_else(|| AppError::NotFound(format!("unknown agent {}", agent.as_str())))?;

        if before.status != DetectStatus::Installed {
            return Ok(InstallOutcome::failure(
                action,
                logs,
                format!("{} 未安装", agent.as_str()),
            ));
        }

        let channel = before.channel.as_deref().unwrap_or("");
        let is_npm = channel == "npm" || (channel.contains("npm") && !channel.contains("native"));
        let special = special_channel_kind(before.channel.as_deref());
        // Never uninstall shared runtimes (Node/npm/PowerShell).
        logs.push(
            "# note: shared runtimes (nodejs/npm/powershell/git) are never uninstalled".into(),
        );

        let mut removed_program = false;
        if let Some(kind) = special {
            if !purge_config {
                return Ok(InstallOutcome::failure(
                    action,
                    logs,
                    special_uninstall_program_message(kind),
                ));
            }
            logs.push(special_uninstall_purge_note(kind));
        } else if is_npm {
            if let Some(pkg) = contribution.npm_package() {
                removed_program = npm_uninstall_global_then_leftover(pkg, executor, &mut logs)?;
            }
        } else {
            // 1) Prefer official silent uninstaller when allowlisted (e.g. WorkBuddy).
            let mut any_removed = false;
            let mut any_found = false;
            for (program, args) in native_uninstaller_specs(contribution) {
                if !program.is_file() {
                    continue;
                }
                any_found = true;
                logs.push(format!(
                    "# run allowlisted uninstaller {} {}",
                    program.display(),
                    args.join(" ")
                ));
                tracing::info!(
                    target: crate::logging::targets::INSTALL,
                    module = crate::logging::targets::INSTALL,
                    op = "uninstall",
                    agent = agent.as_str(),
                    path = %program.display(),
                    "running allowlisted native uninstaller"
                );
                let req = ExecRequest {
                    program: program.to_string_lossy().into_owned(),
                    args: args.clone(),
                    timeout: AGENT_TIMEOUT,
                    max_output_bytes: MAX_OUTPUT,
                };
                let res = executor.run(&req);
                push_exec_logs(&mut logs, &res, AGENT_TIMEOUT.as_secs());
                if res.success() {
                    any_removed = true;
                }
            }

            // 2) Otherwise only delete allowlisted binary files (never rm -rf user trees).
            let candidates = native_uninstall_bin_paths(contribution);
            for p in &candidates {
                if p.is_file() {
                    any_found = true;
                    logs.push(format!("# remove allowlisted binary {}", p.display()));
                    match std::fs::remove_file(p) {
                        Ok(()) => {
                            logs.push(format!("✓ removed {}", p.display()));
                            any_removed = true;
                        }
                        Err(e) => logs.push(format!("✗ remove failed {}: {e}", p.display())),
                    }
                }
            }
            if !any_found {
                logs.push(format!(
                    "channel={channel:?}: no allowlisted native binary/uninstaller found; \
                     manual uninstall may be required if installed outside known paths."
                ));
                if let Some(bin) = &before.binary_path {
                    logs.push(format!(
                        "detected path was {} (not deleted unless on allowlist)",
                        bin.display()
                    ));
                }
            }
            removed_program = any_removed;
            if !removed_program && !purge_config {
                return Ok(InstallOutcome::failure(
                    action,
                    logs,
                    format!(
                        "{} native 卸载失败：未删除任何白名单二进制且卸载程序未成功（可用 --purge-config 仅清配置，或手动卸载）",
                        agent.as_str()
                    ),
                ));
            }
            if !removed_program && purge_config {
                logs.push("将仅清理配置目录（程序本体未删除）…".into());
            }
        }

        // The external uninstall command may mutate the filesystem or an
        // agent-owned environment override. Resolve and validate again before
        // recursive deletion, and require the same filesystem identity.
        if purge_config {
            let initial = purge_home
                .as_deref()
                .ok_or_else(|| AppError::InvalidArg("missing purge target".into()))?;
            let revalidated = validate_default_agent_config_purge_target(agent, actual_data_dir)?;
            if !same_path_identity(initial, &revalidated)? {
                return Err(AppError::InvalidArg(format!(
                    "unsafe config purge path {}: target changed during uninstall",
                    revalidated.display()
                )));
            }
            purge_home = Some(revalidated);

            if let Some(home) = purge_home.as_deref() {
                if home.exists() {
                    logs.push(format!("# remove config dir {}", home.display()));
                    match std::fs::remove_dir_all(home) {
                        Ok(()) => logs.push(format!("✓ removed {}", home.display())),
                        Err(e) => {
                            logs.push(format!("✗ remove failed: {e}"));
                            return Ok(InstallOutcome::failure(
                                action,
                                logs,
                                format!("删除配置目录失败: {e}"),
                            ));
                        }
                    }
                } else {
                    logs.push(format!("config dir missing: {}", home.display()));
                }
            }
        }

        runtime::invalidate_cache();
        crate::services::agent_service::invalidate_detect_cache();
        let detect = registry
            .get(agent)
            .map(|a| a.detect())
            .ok_or_else(|| AppError::NotFound(format!("unknown agent {}", agent.as_str())))?;

        // Success criteria:
        // - program uninstall: redetect must be NotFound
        // - purge-only (program already gone or skipped): config gone
        let ok = if removed_program || is_npm {
            detect.status == DetectStatus::NotFound
        } else if purge_config {
            purge_home.as_deref().is_some_and(|home| !home.exists())
        } else {
            false
        };

        Ok(InstallOutcome {
            ok,
            action: action.into(),
            logs,
            message: if ok {
                if special.is_some() && purge_config && !removed_program {
                    format!(
                        "{} 已删除配置；程序仍在 IDE 插件或桌面应用中",
                        agent.as_str()
                    )
                } else {
                    format!("{} 卸载完成", agent.as_str())
                }
            } else if is_npm || removed_program {
                format!(
                    "{} 卸载后仍检测到二进制（可能 PATH 残留或安装在其他位置）",
                    agent.as_str()
                )
            } else {
                format!("{} 未能自动卸载程序本体", agent.as_str())
            },
            agent: Some(detect),
            runtime: None,
            ..Default::default()
        })
    })();

    log_install_result(
        "uninstall_agent",
        started,
        Some(agent.as_str()),
        None,
        &result,
    );
    result
}

/// Contribution-driven uninstall for agents without a closed [`AgentId`].
///
/// Config purge requires a builtin [`AgentId`] home path and is rejected here.
pub fn uninstall_from_contribution(
    key: &AgentKey,
    contribution: &dyn InstallContribution,
    purge_config: bool,
    executor: &dyn CommandExecutor,
) -> Result<InstallOutcome> {
    require_contribution_key(key, contribution)?;
    if purge_config {
        return Err(AppError::Unsupported(format!(
            "purge_config is unsupported for non-builtin agent key {}",
            key.as_str()
        )));
    }

    let started = Instant::now();
    tracing::info!(
        module = targets::INSTALL,
        op = "uninstall_from_contribution",
        agent = key.as_str(),
        "start"
    );

    let result = (|| {
        let mut logs = Vec::new();
        let action = "agent_uninstall";
        logs.push(
            "# note: shared runtimes (nodejs/npm/powershell/git) are never uninstalled".into(),
        );

        let removed_program;
        if let Some(pkg) = contribution.npm_package() {
            // Contribution path has no adapter detect; always try legacy global too.
            removed_program = npm_uninstall_global_then_leftover(pkg, executor, &mut logs)?;
        } else {
            let mut any_removed = false;
            let mut any_found = false;
            for (program, args) in native_uninstaller_specs(contribution) {
                if !program.is_file() {
                    continue;
                }
                any_found = true;
                logs.push(format!(
                    "# run allowlisted uninstaller {} {}",
                    program.display(),
                    args.join(" ")
                ));
                let req = ExecRequest {
                    program: program.to_string_lossy().into_owned(),
                    args: args.clone(),
                    timeout: AGENT_TIMEOUT,
                    max_output_bytes: MAX_OUTPUT,
                };
                let res = executor.run(&req);
                push_exec_logs(&mut logs, &res, AGENT_TIMEOUT.as_secs());
                if res.success() {
                    any_removed = true;
                }
            }
            for p in native_uninstall_bin_paths(contribution) {
                if p.is_file() {
                    any_found = true;
                    logs.push(format!("# remove allowlisted binary {}", p.display()));
                    match std::fs::remove_file(&p) {
                        Ok(()) => {
                            logs.push(format!("✓ removed {}", p.display()));
                            any_removed = true;
                        }
                        Err(e) => logs.push(format!("✗ remove failed {}: {e}", p.display())),
                    }
                }
            }
            if !any_found {
                logs.push("no allowlisted native binary/uninstaller found for contribution".into());
            }
            removed_program = any_removed;
        }

        Ok(InstallOutcome {
            ok: removed_program,
            action: action.into(),
            logs,
            message: if removed_program {
                format!("{} 卸载命令已成功执行", key.as_str())
            } else {
                format!("{} 未能自动卸载程序本体", key.as_str())
            },
            agent: None,
            runtime: None,
            ..Default::default()
        })
    })();

    log_install_result(
        "uninstall_from_contribution",
        started,
        Some(key.as_str()),
        None,
        &result,
    );
    result
}

fn npm_uninstall_global_then_leftover(
    pkg: &str,
    executor: &dyn CommandExecutor,
    logs: &mut Vec<String>,
) -> Result<bool> {
    let mut removed = false;
    // Current in-app install target. Never create this dir on uninstall.
    if let Ok(prefix) = detect_scanned_user_npm_prefix() {
        if npm_prefix_populated(&prefix) {
            let user_res = npm_uninstall_prefixed(
                pkg,
                &prefix,
                "user npm prefix detect already scans",
                executor,
                logs,
            )?;
            removed = removed || user_res.success();
        }
    }
    let global_res = npm_uninstall_legacy_global(pkg, executor, logs)?;
    removed = removed || global_res.success();
    // Older AgentHub versions wrote into `<data>/npm`. Never create that dir here.
    for prefix in leftover_agenthub_npm_prefixes_present() {
        let leftover_res = npm_uninstall_prefixed(
            pkg,
            &prefix,
            "leftover AgentHub data-dir copy",
            executor,
            logs,
        )?;
        removed = removed || leftover_res.success();
    }
    Ok(removed)
}

fn npm_uninstall_prefixed(
    pkg: &str,
    prefix: &Path,
    note: &str,
    executor: &dyn CommandExecutor,
    logs: &mut Vec<String>,
) -> Result<ExecResult> {
    let prefix_text = prefix.display().to_string();
    push_log(
        logs,
        format!("# npm uninstall -g --prefix {prefix_text} {pkg} ({note})"),
    );
    let npm = resolve_bin(&["npm", "npm.cmd"])?;
    let req = ExecRequest {
        program: npm,
        args: vec![
            "uninstall".into(),
            "-g".into(),
            "--prefix".into(),
            prefix_text,
            pkg.into(),
        ],
        timeout: AGENT_TIMEOUT,
        max_output_bytes: MAX_OUTPUT,
    };
    let res = executor.run(&req);
    push_exec_logs(logs, &res, AGENT_TIMEOUT.as_secs());
    Ok(res)
}

fn npm_uninstall_legacy_global(
    pkg: &str,
    executor: &dyn CommandExecutor,
    logs: &mut Vec<String>,
) -> Result<ExecResult> {
    push_log(logs, format!("# npm uninstall -g {pkg}"));
    let npm = resolve_bin(&["npm", "npm.cmd"])?;
    let req = ExecRequest {
        program: npm,
        args: vec!["uninstall".into(), "-g".into(), pkg.into()],
        timeout: AGENT_TIMEOUT,
        max_output_bytes: MAX_OUTPUT,
    };
    let res = executor.run(&req);
    push_exec_logs(logs, &res, AGENT_TIMEOUT.as_secs());
    Ok(res)
}

fn run_npm_install(
    contribution: &dyn InstallContribution,
    agent_label: &str,
    upgrade: bool,
    executor: &dyn CommandExecutor,
    logs: &mut Vec<String>,
) -> Result<ExecResult> {
    let pkg = contribution
        .npm_package()
        .ok_or_else(|| AppError::Unsupported(format!("{agent_label} 无 npm 安装包")))?;
    let npm = resolve_bin(&["npm", "npm.cmd", "npm.exe"])?;
    let label = if upgrade { "upgrade" } else { "install" };
    let extra = contribution.npm_install_extra_flags();
    let extra_note = if extra.is_empty() {
        String::new()
    } else {
        format!(" {}", extra.join(" "))
    };
    let prefix = ensure_detect_scanned_user_npm_prefix()?;
    let prefix_text = prefix.display().to_string();
    push_log(
        logs,
        format!("# npm {label} -g --prefix {prefix_text}{extra_note} {pkg}"),
    );
    push_log(logs, format!("使用 npm： {npm}"));
    push_log(
        logs,
        format!("安装目录：{prefix_text}（检测会扫描此目录，无需重启）"),
    );
    push_log(
        logs,
        format!("# 正在通过 npm 下载安装 {pkg}（可能需数分钟，请保持网络畅通）…"),
    );
    tracing::info!(
        target: crate::logging::targets::INSTALL,
        module = crate::logging::targets::INSTALL,
        op = "npm_install",
        agent = agent_label,
        prefix = %prefix_text,
        pkg = pkg,
        "npm install into user-writable prefix detect already scans"
    );
    let mut args = vec![
        "install".into(),
        "-g".into(),
        "--prefix".into(),
        prefix_text,
    ];
    for flag in extra {
        args.push((*flag).into());
    }
    args.push(pkg.into());
    let req = ExecRequest {
        program: npm,
        args,
        timeout: AGENT_TIMEOUT,
        max_output_bytes: MAX_OUTPUT,
    };
    let res = executor.run(&req);
    push_exec_logs(logs, &res, AGENT_TIMEOUT.as_secs());
    Ok(res)
}

/// Platform-aware native installer: Windows → allowlisted ps1; macOS/Linux → allowlisted sh.
/// Agents with only a Setup website (e.g. WorkBuddy) open the official page instead.
///
/// `builtin_agent` is only used to select the historical bash vs posix shell policy for
/// built-in agents; contribution-only installs default to bash when a sh URL is present.
fn run_native_install(
    contribution: &dyn InstallContribution,
    agent_label: &str,
    builtin_agent: Option<AgentId>,
    executor: &dyn CommandExecutor,
    logs: &mut Vec<String>,
) -> Result<ExecResult> {
    if contribution.native_setup_url().is_some()
        && contribution.native_ps1_url().is_none()
        && contribution.native_sh_url().is_none()
    {
        return run_native_setup_guide(contribution, agent_label, executor, logs);
    }
    #[cfg(windows)]
    {
        let _ = builtin_agent;
        return run_native_ps1(contribution, agent_label, executor, logs);
    }
    #[cfg(not(windows))]
    {
        return run_native_sh(contribution, agent_label, builtin_agent, executor, logs);
    }
}

/// Open official Setup page and return a non-success result so callers redetect honestly.
fn run_native_setup_guide(
    contribution: &dyn InstallContribution,
    agent_label: &str,
    executor: &dyn CommandExecutor,
    logs: &mut Vec<String>,
) -> Result<ExecResult> {
    let url = contribution
        .native_setup_url()
        .ok_or_else(|| AppError::Unsupported(format!("{agent_label} has no setup URL")))?;
    if !url.starts_with("https://") {
        return Err(AppError::InvalidArg("setup URL must be https".into()));
    }
    logs.push(format!(
        "# {agent_label} 没有脚本安装器，改为打开官网安装页"
    ));
    logs.push(format!("# 安装页：{url}"));
    tracing::info!(
        target: crate::logging::targets::INSTALL,
        module = crate::logging::targets::INSTALL,
        op = "setup_guide",
        agent = agent_label,
        url = url,
        "opening official Setup page for native install"
    );

    if let Err(err) = crate::oauth::open_in_browser(url) {
        logs.push(format!("打开安装页失败：{err}"));
    }
    let _ = executor;
    logs.push("已尝试打开官网安装页。请完成安装后，完全退出并重启 AgentHub。".into());
    Ok(ExecResult {
        command: format!("open {url}"),
        exit_code: Some(1),
        stdout: String::new(),
        stderr: String::new(),
        timed_out: false,
        spawn_error: None,
    })
}

#[cfg(windows)]
fn run_native_ps1(
    contribution: &dyn InstallContribution,
    agent_label: &str,
    executor: &dyn CommandExecutor,
    logs: &mut Vec<String>,
) -> Result<ExecResult> {
    let url = contribution.native_ps1_url().ok_or_else(|| {
        AppError::Unsupported(format!("{agent_label} 无 Windows native 安装脚本"))
    })?;
    // Allowlist: only fixed https URLs from contribution.native_ps1_url.
    if !url.starts_with("https://") {
        return Err(AppError::InvalidArg("install URL must be https".into()));
    }
    // Prefer PowerShell 7; fall back to 5.1 / System32.
    let ps = runtime::resolve_powershell_for_native()
        .map(|p| p.to_string_lossy().into_owned())
        .ok_or_else(|| {
            AppError::NotFound(
                "PowerShell not found (need Windows PowerShell 5.1 or PowerShell 7 pwsh)".into(),
            )
        })?;
    // Log interpreter identity for supportability.
    if let Ok(ver_out) = crate::utils::process::run_capture(
        std::path::Path::new(&ps),
        &[
            "-NoProfile",
            "-Command",
            "$PSVersionTable.PSVersion.ToString()",
        ],
    ) {
        if let Some(v) = crate::utils::process::stdout_first_line(&ver_out) {
            push_log(logs, format!("using PowerShell: {ps} (version {v})"));
        } else {
            push_log(logs, format!("using PowerShell: {ps}"));
        }
    } else {
        push_log(logs, format!("using PowerShell: {ps}"));
    }
    // Force unbuffered host output so download progress streams when piped.
    let script = format!(
        "$ProgressPreference='Continue'; $InformationPreference='Continue'; irm '{url}' | iex"
    );
    push_log(logs, format!("# 官方安装脚本: {url}"));
    push_log(
        logs,
        "# 正在下载并执行官方安装脚本（下载大文件时可能数分钟无新输出，请耐心等待）…",
    );
    push_log(logs, format!("# {ps} -Command {script}"));
    let req = ExecRequest {
        program: ps,
        args: vec![
            "-NoProfile".into(),
            "-ExecutionPolicy".into(),
            "Bypass".into(),
            "-Command".into(),
            script,
        ],
        timeout: AGENT_TIMEOUT,
        max_output_bytes: MAX_OUTPUT,
    };
    let res = executor.run(&req);
    push_exec_logs(logs, &res, AGENT_TIMEOUT.as_secs());
    Ok(res)
}

#[cfg(not(windows))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NativeShellRequirement {
    Bash,
    Posix,
}

#[cfg(not(windows))]
fn native_sh_shell_requirement(builtin_agent: Option<AgentId>) -> NativeShellRequirement {
    // The current allowlisted CLI setup guides publish `curl ... | bash`.
    // Keep an explicit Posix variant so a future documented sh-compatible
    // script can use the resolved shell without falling back to a hardcoded
    // `bash` inside the pipeline.
    // Contribution-only (non-AgentId) installs default to Bash.
    match builtin_agent {
        Some(AgentId::Claude | AgentId::Kimi | AgentId::Grok | AgentId::Cursor) | None => {
            NativeShellRequirement::Bash
        }
        Some(_) => NativeShellRequirement::Posix,
    }
}

#[cfg(not(windows))]
fn resolve_native_shell(requirement: NativeShellRequirement) -> Result<PathBuf> {
    let bash = runtime::resolve_binary(&["bash"]);
    if requirement == NativeShellRequirement::Bash {
        return select_native_shell(requirement, bash.as_deref(), None);
    }

    let sh = runtime::resolve_binary(&["sh"]);
    select_native_shell(requirement, bash.as_deref(), sh.as_deref())
}

#[cfg(not(windows))]
fn select_native_shell(
    requirement: NativeShellRequirement,
    bash: Option<&Path>,
    sh: Option<&Path>,
) -> Result<PathBuf> {
    match requirement {
        NativeShellRequirement::Bash => bash.map(Path::to_path_buf).ok_or_else(|| {
            AppError::NotFound(
                "bash not found; this official native installer explicitly requires bash".into(),
            )
        }),
        NativeShellRequirement::Posix => bash
            .or(sh)
            .map(Path::to_path_buf)
            .ok_or_else(|| AppError::NotFound("no POSIX shell found (need bash or sh)".into())),
    }
}

#[cfg(not(windows))]
fn native_shell_invocation(shell: &Path, url: &str) -> (Vec<String>, String) {
    let shell_text = shell.to_string_lossy();
    let script = format!(
        "curl -fL --progress-bar {} | {}",
        quote_posix_shell_word(url),
        quote_posix_shell_word(&shell_text)
    );
    let option = if shell
        .file_name()
        .map(|name| name.to_string_lossy().eq_ignore_ascii_case("bash"))
        .unwrap_or(false)
    {
        "-lc"
    } else {
        "-c"
    };
    (vec![option.into(), script], shell_text.into_owned())
}

#[cfg(not(windows))]
fn quote_posix_shell_word(word: &str) -> String {
    format!("'{}'", word.replace('\'', "'\\\"'\\\"'"))
}

#[cfg(not(windows))]
fn run_native_sh(
    contribution: &dyn InstallContribution,
    agent_label: &str,
    builtin_agent: Option<AgentId>,
    executor: &dyn CommandExecutor,
    logs: &mut Vec<String>,
) -> Result<ExecResult> {
    let url = contribution.native_sh_url().ok_or_else(|| {
        AppError::Unsupported(format!(
            "{agent_label} 在此平台无 allowlisted native sh 安装脚本；请使用 npm 渠道或手动安装"
        ))
    })?;
    if !url.starts_with("https://") {
        return Err(AppError::InvalidArg("install URL must be https".into()));
    }
    let requirement = native_sh_shell_requirement(builtin_agent);
    let shell = resolve_native_shell(requirement)?;
    let (args, shell_program) = native_shell_invocation(&shell, url);
    push_log(logs, format!("using shell: {shell_program}"));
    // The pipeline always invokes the same resolved interpreter as the outer
    // command. This prevents a sh fallback from silently requiring bash.
    let script = args[1].clone();
    push_log(logs, format!("# 官方安装脚本: {url}"));
    push_log(
        logs,
        "# 正在下载并执行官方安装脚本（下载大文件时可能数分钟，请耐心等待）…",
    );
    push_log(logs, format!("# {shell_program} {} {script}", args[0]));
    let req = ExecRequest {
        program: shell_program,
        args,
        timeout: AGENT_TIMEOUT,
        max_output_bytes: MAX_OUTPUT,
    };
    let res = executor.run(&req);
    push_exec_logs(logs, &res, AGENT_TIMEOUT.as_secs());
    Ok(res)
}

/// Convenience wrappers using the system executor.
pub fn install_runtime_system(id: RuntimeId, channel: &str) -> Result<InstallOutcome> {
    install_runtime(id, channel, &SystemCommandExecutor)
}

pub fn install_agent_system(
    registry: &AdapterRegistry,
    agent: AgentId,
    channel: &str,
    install_deps: bool,
) -> Result<InstallOutcome> {
    install_agent(
        registry,
        agent,
        channel,
        install_deps,
        &SystemCommandExecutor,
    )
}

pub fn upgrade_agent_system(registry: &AdapterRegistry, agent: AgentId) -> Result<InstallOutcome> {
    upgrade_agent(registry, agent, &SystemCommandExecutor)
}

pub fn uninstall_agent_system(
    registry: &AdapterRegistry,
    db: &Database,
    agent: AgentId,
    purge_config: bool,
) -> Result<InstallOutcome> {
    uninstall_agent(registry, db, agent, purge_config, &SystemCommandExecutor)
}

#[cfg(test)]
mod tests;
