//! Runtime / Agent install orchestration.
//!
//! Safety rules:
//! - Only allowlisted programs and package ids / script URLs.
//! - Never report success unless redetect confirms the expected state.
//! - Env uninstall is intentionally not provided.

use std::time::{Duration, Instant};

use crate::adapters::AdapterRegistry;
use crate::catalog::install::{
    native_ps1_url, native_setup_url, native_sh_url, npm_install_extra_flags, npm_package,
};
use crate::catalog::limits::{
    INSTALL_AGENT_TIMEOUT as AGENT_TIMEOUT, INSTALL_ENV_TIMEOUT as ENV_TIMEOUT,
    INSTALL_MAX_OUTPUT_BYTES as MAX_OUTPUT,
};
use crate::error::{AppError, Result};
use crate::logging::{self, targets};
use crate::models::{AgentId, DetectStatus, EnvStatusKind, InstallOutcome, RuntimeId};
use crate::platform::install::builtin_install_registry;
use crate::runtime;
use crate::utils::command_exec::{CommandExecutor, ExecRequest, ExecResult, SystemCommandExecutor};
use crate::utils::paths::agent_home;
use crate::utils::redact::redact_text;

fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
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
    for name in names {
        if let Ok(p) = which::which(name) {
            return Ok(p.to_string_lossy().into_owned());
        }
    }
    Err(AppError::NotFound(format!(
        "command not found: {}",
        names.join(" | ")
    )))
}

/// Allowlisted binary paths that may be deleted on native uninstall (never arbitrary dirs).
fn native_uninstall_bin_paths(agent: AgentId) -> Vec<std::path::PathBuf> {
    builtin_install_registry()
        .get_agent_id(agent)
        .map(|c| c.native_uninstall_bin_paths())
        .unwrap_or_default()
}

/// Allowlisted external uninstallers: (program, args). Never run arbitrary paths.
fn native_uninstaller_specs(agent: AgentId) -> Vec<(std::path::PathBuf, Vec<String>)> {
    builtin_install_registry()
        .get_agent_id(agent)
        .map(|c| {
            c.native_uninstaller_specs()
                .into_iter()
                .map(|s| (s.program, s.args))
                .collect()
        })
        .unwrap_or_default()
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
    // process ran; still append to the outcome buffer (dedupe not required —
    // GUI replaces with final logs on complete).
    for line in res.stdout.lines().chain(res.stderr.lines()) {
        if !line.trim().is_empty() {
            let safe = redact_text(line);
            logs.push(safe);
        }
    }
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
/// on macOS). Passing an empty channel selects the platform default.
pub fn install_runtime(
    id: RuntimeId,
    channel: &str,
    executor: &dyn CommandExecutor,
) -> Result<InstallOutcome> {
    install_runtime_inner(id, channel, executor)
}

/// winget package id for runtimes that support one-click install.
fn winget_package_id(id: RuntimeId) -> Option<&'static str> {
    match id {
        RuntimeId::NodeJs | RuntimeId::Npm => Some("OpenJS.NodeJS.LTS"),
        RuntimeId::Git => Some("Git.Git"),
        RuntimeId::PowerShell => None,
    }
}

/// The native runtime package manager for the current desktop platform.
///
/// Keep Windows on winget for compatibility. macOS uses Homebrew because it is
/// the standard way to install both Node.js and Git without a PowerShell
/// dependency. Linux retains the historical winget default; callers can still
/// pass an explicit channel and will receive a clear unsupported-channel error.
fn default_runtime_channel() -> &'static str {
    if cfg!(target_os = "macos") {
        "brew"
    } else {
        "winget"
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
    if let Ok(path) = resolve_bin(&["brew"]) {
        return Ok(path);
    }
    for candidate in ["/opt/homebrew/bin/brew", "/usr/local/bin/brew"] {
        let path = std::path::Path::new(candidate);
        if path.is_file() {
            return Ok(candidate.into());
        }
    }
    Err(AppError::NotFound(
        "command not found: brew (install Homebrew from https://brew.sh/)".into(),
    ))
}

/// Complete an environment install by invalidating detection caches and
/// checking the exact requested runtime (plus Node.js for an npm request).
fn finalize_runtime_install(
    id: RuntimeId,
    mut logs: Vec<String>,
    res: ExecResult,
) -> InstallOutcome {
    runtime::invalidate_cache();
    let status = runtime::detect_one(id);
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
                "PowerShell 不支持一键安装。macOS/Linux 请自行安装 PowerShell 7 (pwsh)；native 安装优先使用官方 sh 脚本。"
            };
            return Ok(InstallOutcome::failure(action, logs, msg));
        }

        let Some(package_id) = winget_package_id(target) else {
            return Ok(InstallOutcome::failure(
                action,
                logs,
                format!("runtime {} 暂不支持自动安装", id.as_str()),
            ));
        };

        let channel = if channel.is_empty() {
            default_runtime_channel()
        } else {
            channel
        };

        #[cfg(target_os = "macos")]
        if channel == "brew" {
            let formula = brew_formula(target).ok_or_else(|| {
                AppError::Unsupported(format!("runtime {} 暂不支持 Homebrew 安装", id.as_str()))
            })?;
            logs.push(format!(
                "# install runtime {} via brew ({formula})",
                target.as_str()
            ));
            let brew = match resolve_brew() {
                Ok(path) => path,
                Err(e) => {
                    logs.push(e.to_string());
                    return Ok(InstallOutcome::failure(
                        action,
                        logs,
                        "未找到 Homebrew。请先安装 Homebrew（https://brew.sh/）后重试。",
                    ));
                }
            };
            let req = ExecRequest {
                program: brew,
                args: vec!["install".into(), formula.into()],
                timeout: ENV_TIMEOUT,
                max_output_bytes: MAX_OUTPUT,
            };
            let res = executor.run(&req);
            push_exec_logs(&mut logs, &res, ENV_TIMEOUT.as_secs());
            return Ok(finalize_runtime_install(id, logs, res));
        }

        if channel != "winget" {
            #[cfg(target_os = "macos")]
            let hint = "（macOS 默认使用 brew；可传 --channel brew）";
            #[cfg(not(target_os = "macos"))]
            let hint = "";
            return Ok(InstallOutcome::failure(
                action,
                logs,
                format!("不支持的安装渠道 '{channel}'（当前仅 winget{hint}）"),
            ));
        }

        logs.push(format!(
            "# install runtime {} via {channel} ({package_id})",
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
                return Ok(InstallOutcome::failure(
                    action,
                    logs,
                    format!("未找到 winget。{manual}"),
                ));
            }
        };

        let req = ExecRequest {
            program: winget,
            args: vec![
                "install".into(),
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
                .unwrap_or_else(|| "native".into())
        } else {
            channel.to_string()
        };

        logs.push(format!(
            "# install {} channel={channel} install_deps={install_deps}",
            agent.as_str()
        ));

        let requires = channel_requires(registry, agent, &channel)?;
        if let Err(env_err) = runtime::ensure(&requires) {
            if !install_deps {
                let msg = format!(
                    "环境未就绪: 缺少 {}。请先安装运行环境或使用 installDeps。",
                    env_err
                        .missing
                        .iter()
                        .map(|r| r.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                );
                logs.push(msg.clone());
                return Ok(InstallOutcome::failure(action, logs, msg));
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
            "npm" => run_npm_install(agent, false, executor, &mut logs)?,
            "native" => run_native_install(agent, executor, &mut logs)?,
            other => {
                return Ok(InstallOutcome::failure(
                    action,
                    logs,
                    format!("不支持的安装渠道 '{other}'"),
                ));
            }
        };

        if !res.success() && !res.timed_out {
            // still redetect — installer may return non-zero but work
            logs.push("安装命令未以 0 退出，将重新检测以确认结果…".into());
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
                logs.push(format!("redetect: Installed @ {}", p.display()));
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
            })
        } else {
            logs.push("重新检测: not_found".into());
            for n in &detect.notes {
                logs.push(n.clone());
            }
            logs.push(
                "诊断: 安装命令可能已成功，但当前进程 PATH 未包含新二进制。\
                 请完全退出并重启 AgentHub，或在终端确认 which/where 结果后再点「重新检测」。"
                    .into(),
            );
            Ok(InstallOutcome {
                ok: false,
                action: action.into(),
                logs,
                message: format!(
                    "{} 安装命令已执行，但重新检测未找到二进制（请检查 PATH 或重启 AgentHub）",
                    agent.as_str()
                ),
                agent: Some(detect),
                runtime: None,
            })
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

/// Upgrade an installed agent (npm → reinstall latest; native → re-run install.ps1).
pub fn upgrade_agent(
    registry: &AdapterRegistry,
    agent: AgentId,
    executor: &dyn CommandExecutor,
) -> Result<InstallOutcome> {
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

        let channel = before
            .channel
            .as_deref()
            .map(|c| {
                // Prefer concrete channel; legacy "npm-or-native" treated as native unless path says npm.
                if c == "npm" || (c.contains("npm") && !c.contains("native")) {
                    "npm"
                } else {
                    "native"
                }
            })
            .unwrap_or("native");

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
            "npm" => run_npm_install(agent, true, executor, &mut logs)?,
            _ => run_native_install(agent, executor, &mut logs)?,
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

fn upgrade_succeeded(command_ok: bool, detected: &DetectStatus) -> bool {
    command_ok && *detected == DetectStatus::Installed
}

/// Uninstall agent binary when possible (npm global only).
/// Does **not** uninstall shared runtimes. Config purge is optional file delete
/// after optional backup handled by caller.
pub fn uninstall_agent(
    registry: &AdapterRegistry,
    agent: AgentId,
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
        // Never uninstall shared runtimes (Node/npm/PowerShell).
        logs.push(
            "# note: shared runtimes (nodejs/npm/powershell/git) are never uninstalled".into(),
        );

        let mut removed_program = false;
        if is_npm {
            if let Some(pkg) = npm_package(agent) {
                logs.push(format!("# npm uninstall -g {pkg}"));
                let npm = resolve_bin(&["npm", "npm.cmd"])?;
                let req = ExecRequest {
                    program: npm,
                    args: vec!["uninstall".into(), "-g".into(), pkg.into()],
                    timeout: AGENT_TIMEOUT,
                    max_output_bytes: MAX_OUTPUT,
                };
                let res = executor.run(&req);
                push_exec_logs(&mut logs, &res, AGENT_TIMEOUT.as_secs());
                removed_program = res.success();
            }
        } else {
            // 1) Prefer official silent uninstaller when allowlisted (e.g. WorkBuddy).
            let mut any_removed = false;
            let mut any_found = false;
            for (program, args) in native_uninstaller_specs(agent) {
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
            let candidates = native_uninstall_bin_paths(agent);
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

        if purge_config {
            let home = agent_home(agent)?;
            if home.exists() {
                logs.push(format!("# remove config dir {}", home.display()));
                match std::fs::remove_dir_all(&home) {
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
            !agent_home(agent).map(|p| p.exists()).unwrap_or(false)
        } else {
            false
        };

        Ok(InstallOutcome {
            ok,
            action: action.into(),
            logs,
            message: if ok {
                format!("{} 卸载完成", agent.as_str())
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

fn run_npm_install(
    agent: AgentId,
    upgrade: bool,
    executor: &dyn CommandExecutor,
    logs: &mut Vec<String>,
) -> Result<ExecResult> {
    let pkg = npm_package(agent)
        .ok_or_else(|| AppError::Unsupported(format!("{} 无 npm 安装包", agent.as_str())))?;
    let npm = resolve_bin(&["npm", "npm.cmd", "npm.exe"])?;
    let label = if upgrade { "upgrade" } else { "install" };
    let extra = npm_install_extra_flags(agent);
    let extra_note = if extra.is_empty() {
        String::new()
    } else {
        format!(" {}", extra.join(" "))
    };
    push_log(logs, format!("# npm {label} -g{extra_note} {pkg}"));
    push_log(logs, format!("using npm: {npm}"));
    push_log(
        logs,
        format!("# 正在通过 npm 下载安装 {pkg}（可能需数分钟，请保持网络畅通）…"),
    );
    let mut args = vec!["install".into(), "-g".into()];
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
fn run_native_install(
    agent: AgentId,
    executor: &dyn CommandExecutor,
    logs: &mut Vec<String>,
) -> Result<ExecResult> {
    if native_setup_url(agent).is_some()
        && native_ps1_url(agent).is_none()
        && native_sh_url(agent).is_none()
    {
        return run_native_setup_guide(agent, executor, logs);
    }
    #[cfg(windows)]
    {
        return run_native_ps1(agent, executor, logs);
    }
    #[cfg(not(windows))]
    {
        return run_native_sh(agent, executor, logs);
    }
}

/// Open official Setup page and return a non-success result so callers redetect honestly.
fn run_native_setup_guide(
    agent: AgentId,
    executor: &dyn CommandExecutor,
    logs: &mut Vec<String>,
) -> Result<ExecResult> {
    let url = native_setup_url(agent)
        .ok_or_else(|| AppError::Unsupported(format!("{} has no setup URL", agent.as_str())))?;
    if !url.starts_with("https://") {
        return Err(AppError::InvalidArg("setup URL must be https".into()));
    }
    logs.push(format!(
        "# {} has no scripted installer — open official Setup page",
        agent.as_str()
    ));
    logs.push(format!("# setup url: {url}"));
    tracing::info!(
        target: crate::logging::targets::INSTALL,
        module = crate::logging::targets::INSTALL,
        op = "setup_guide",
        agent = agent.as_str(),
        url = url,
        "opening official Setup page for native install"
    );

    #[cfg(windows)]
    let req = ExecRequest {
        program: "cmd".into(),
        args: vec!["/C".into(), "start".into(), "".into(), url.into()],
        timeout: Duration::from_secs(15),
        max_output_bytes: MAX_OUTPUT,
    };
    #[cfg(target_os = "macos")]
    let req = ExecRequest {
        program: "open".into(),
        args: vec![url.into()],
        timeout: Duration::from_secs(15),
        max_output_bytes: MAX_OUTPUT,
    };
    #[cfg(all(not(windows), not(target_os = "macos")))]
    let req = ExecRequest {
        program: "xdg-open".into(),
        args: vec![url.into()],
        timeout: Duration::from_secs(15),
        max_output_bytes: MAX_OUTPUT,
    };

    let res = executor.run(&req);
    push_exec_logs(logs, &res, 15);
    logs.push(
        "已尝试打开官网安装页。请完成 WorkBuddySetup 安装后回到 AgentHub 点击重新检测。".into(),
    );
    // Always report non-success so install_agent does not claim Installed until redetect.
    Ok(ExecResult {
        command: res.command,
        exit_code: Some(1),
        stdout: res.stdout,
        stderr: res.stderr,
        timed_out: res.timed_out,
        spawn_error: res.spawn_error,
    })
}

fn run_native_ps1(
    agent: AgentId,
    executor: &dyn CommandExecutor,
    logs: &mut Vec<String>,
) -> Result<ExecResult> {
    #[cfg(not(windows))]
    {
        let _ = (agent, executor, logs);
        return Err(AppError::Unsupported(
            "native .ps1 installer is Windows-only; use npm channel or official sh install on this platform"
                .into(),
        ));
    }
    #[cfg(windows)]
    {
        let url = native_ps1_url(agent).ok_or_else(|| {
            AppError::Unsupported(format!("{} 无 Windows native 安装脚本", agent.as_str()))
        })?;
        // Allowlist: only fixed https URLs from native_ps1_url.
        if !url.starts_with("https://") {
            return Err(AppError::InvalidArg("install URL must be https".into()));
        }
        // Prefer PowerShell 7; fall back to 5.1 / System32.
        let ps = runtime::resolve_powershell_for_native()
            .map(|p| p.to_string_lossy().into_owned())
            .ok_or_else(|| {
                AppError::NotFound(
                    "PowerShell not found (need Windows PowerShell 5.1 or PowerShell 7 pwsh)"
                        .into(),
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
}

#[cfg(not(windows))]
fn run_native_sh(
    agent: AgentId,
    executor: &dyn CommandExecutor,
    logs: &mut Vec<String>,
) -> Result<ExecResult> {
    let url = native_sh_url(agent).ok_or_else(|| {
        AppError::Unsupported(format!(
            "{} 在此平台无 allowlisted native sh 安装脚本；请使用 npm 渠道或手动安装",
            agent.as_str()
        ))
    })?;
    if !url.starts_with("https://") {
        return Err(AppError::InvalidArg("install URL must be https".into()));
    }
    let shell = resolve_bin(&["bash", "sh"])?;
    push_log(logs, format!("using shell: {shell}"));
    // curl | bash with allowlisted URL only. -L follows redirects; progress to stderr.
    let script = format!("curl -fL --progress-bar {url} | bash");
    push_log(logs, format!("# 官方安装脚本: {url}"));
    push_log(
        logs,
        "# 正在下载并执行官方安装脚本（下载大文件时可能数分钟，请耐心等待）…",
    );
    push_log(logs, format!("# {shell} -lc {script}"));
    let req = ExecRequest {
        program: shell,
        args: vec!["-lc".into(), script],
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
    agent: AgentId,
    purge_config: bool,
) -> Result<InstallOutcome> {
    uninstall_agent(registry, agent, purge_config, &SystemCommandExecutor)
}

#[cfg(test)]
mod tests;
