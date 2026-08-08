//! WorkBuddy (腾讯 CodeBuddy 桌面) adapter.
//!
//! Install shape is an Electron desktop app that **bundles** CodeBuddy Agent CLI
//! (`codebuddy` / `cbc`). Headless runs via `ELECTRON_RUN_AS_NODE` against the
//! bundled CLI (argv built in `build_run_spec`).
//!
//! Production install trees only — never depend on unpack/extract scratch paths.

use std::path::{Path, PathBuf};

use crate::error::{AppError, Result};
use crate::models::{
    AgentConfig, AgentId, AuthState, Capability, CapabilityState, DetectResult, DetectStatus,
    InstallChannel, RunOptions, RunSpec,
};
use crate::utils::paths::home_dir;
use crate::utils::redact::redact_json;

use super::AgentAdapter;

/// Official setup landing page (no npm / no allowlisted install.ps1).
pub const SETUP_URL: &str = "https://www.codebuddy.cn/work/";

pub struct WorkBuddyAdapter;

impl AgentAdapter for WorkBuddyAdapter {
    fn id(&self) -> AgentId {
        AgentId::WorkBuddy
    }

    fn detect(&self) -> DetectResult {
        let env_ready = true; // native Setup has no Node/npm runtime dependency
        let mut notes = Vec::new();

        let Some(exe) = resolve_workbuddy_exe() else {
            tracing::debug!(
                target: crate::logging::targets::DETECT,
                module = crate::logging::targets::DETECT,
                op = "detect",
                agent = "workbuddy",
                via = "not_found",
                "WorkBuddy.exe not found in default or registry paths"
            );
            notes.push(
                "WorkBuddy not found. Install via official Setup: https://www.codebuddy.cn/work/"
                    .into(),
            );
            return DetectResult {
                agent: AgentId::WorkBuddy,
                status: DetectStatus::NotFound,
                version: None,
                binary_path: None,
                channel: None,
                env_ready,
                notes,
            };
        };

        let install_dir = exe
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| exe.clone());
        let codebuddy = resolve_bundled_codebuddy(&install_dir);
        if codebuddy.is_none() {
            notes.push(
                "WorkBuddy.exe found but bundled codebuddy CLI missing under install resources"
                    .into(),
            );
        }

        let version = read_version_from_last_launch()
            .or_else(|| read_version_from_package_json(&install_dir));

        if let Some(ref cb) = codebuddy {
            tracing::info!(
                target: crate::logging::targets::DETECT,
                module = crate::logging::targets::DETECT,
                op = "detect",
                agent = "workbuddy",
                via = "native",
                path = %exe.display(),
                codebuddy = %cb.display(),
                version = version.as_deref().unwrap_or("?"),
                "WorkBuddy desktop + bundled CLI detected"
            );
        } else {
            tracing::debug!(
                target: crate::logging::targets::DETECT,
                module = crate::logging::targets::DETECT,
                op = "detect",
                agent = "workbuddy",
                path = %exe.display(),
                "WorkBuddy.exe present without bundled codebuddy"
            );
        }

        DetectResult {
            agent: AgentId::WorkBuddy,
            status: DetectStatus::Installed,
            version,
            binary_path: Some(exe),
            channel: Some("native".into()),
            env_ready,
            notes,
        }
    }

    fn install_channels(&self) -> Vec<InstallChannel> {
        // Honest channel: desktop Setup only — no npm, no install.ps1 allowlist entry.
        vec![InstallChannel {
            id: "native".into(),
            label: "WorkBuddy Setup（官网安装包）".into(),
            requires: vec![],
            min_runtime_notes: Some(
                "Download WorkBuddySetup.exe from https://www.codebuddy.cn/work/ (no Node/npm required)"
                    .into(),
            ),
        }]
    }

    fn read_config(&self) -> Result<AgentConfig> {
        let dir = workbuddy_config_dir()?;
        let settings_path = dir.join("settings.json");
        let models_path = dir.join("models.json");
        let mcp_path = dir.join(".mcp.json");

        let settings = read_json_value_or_empty(&settings_path)?;
        let models = if models_path.exists() {
            Some(read_json_value_or_empty(&models_path)?)
        } else {
            None
        };
        let mcp = if mcp_path.exists() {
            Some(read_json_value_or_empty(&mcp_path)?)
        } else {
            None
        };

        let mut raw = serde_json::Map::new();
        raw.insert("settings".into(), redact_json(&settings));
        if let Some(m) = models {
            raw.insert("models".into(), redact_json(&m));
        }
        if let Some(m) = mcp {
            raw.insert("mcp".into(), redact_json(&m));
        }
        raw.insert(
            "paths".into(),
            serde_json::json!({
                "configDir": dir,
                "settings": settings_path,
                "models": models_path,
                "mcp": mcp_path,
            }),
        );

        Ok(AgentConfig {
            agent: AgentId::WorkBuddy,
            raw: serde_json::Value::Object(raw),
        })
    }

    fn write_config(&self, _config: &AgentConfig) -> Result<()> {
        // models.json dual-shape merge not locked for P0 — fail closed.
        Err(AppError::Unsupported(
            "live config writes are not supported for workbuddy \
             (models.json / settings.json merge rules not locked; use WorkBuddy UI or edit files)"
                .into(),
        ))
    }

    fn read_auth(&self) -> Result<AuthState> {
        let path = auth_info_path();
        let has = path.as_ref().map(|p| p.is_file()).unwrap_or(false);
        Ok(AuthState {
            agent: AgentId::WorkBuddy,
            kind: if has {
                Some("desktop-login".into())
            } else {
                None
            },
            summary: if has {
                "desktop login state present (tokens not exposed)".into()
            } else {
                "no desktop login state (sign in via WorkBuddy UI)".into()
            },
            has_credentials: has,
        })
    }

    fn skills_dir(&self) -> Option<PathBuf> {
        workbuddy_config_dir().ok().map(|d| d.join("skills"))
    }

    fn capability(&self, cap: Capability) -> CapabilityState {
        use Capability::*;
        match cap {
            Skills | LiveBackup | DangerousMode | ProjectHistory | ProjectDelete => {
                CapabilityState::full()
            }
            ConfigWrite => CapabilityState::unsupported("无稳定 settings 合并契约，fail-closed"),
            AccountSwitch => CapabilityState::unsupported("暂不支持账号池切换"),
            ApiKeyAccount => CapabilityState::unsupported("暂不支持 API Key 账号池"),
            StructuredStream => {
                CapabilityState::unsupported("CLI 仅提供 text 输出，无结构化事件流")
            }
            ProviderPresets => CapabilityState::unsupported("写入契约未锁定，无内置模板"),
            Usage => CapabilityState::full(),
            Mcp | ModelSelect | SessionResume => CapabilityState::planned("待验证接入"),
        }
    }

    fn live_backup_paths(&self) -> Vec<PathBuf> {
        let mut paths = Vec::new();
        if let Ok(dir) = workbuddy_config_dir() {
            paths.push(dir.join("settings.json"));
            paths.push(dir.join("models.json"));
            paths.push(dir.join(".mcp.json"));
        }
        if let Some(auth) = auth_info_path() {
            paths.push(auth);
        }
        paths
    }

    fn build_run_spec(&self, binary: &Path, prompt: &str, opts: &RunOptions) -> Result<RunSpec> {
        // binary is WorkBuddy.exe (from detect). CLI is a separate bundled path.
        let install_dir = binary
            .parent()
            .map(Path::to_path_buf)
            .ok_or_else(|| AppError::NotFound("WorkBuddy binary has no parent directory".into()))?;
        let codebuddy = resolve_bundled_codebuddy(&install_dir).ok_or_else(|| {
            AppError::NotFound(
                "bundled codebuddy CLI not found under WorkBuddy install resources"
                    .into(),
            )
        })?;

        let mut args = vec![
            codebuddy.to_string_lossy().into_owned(),
            "-p".into(),
            prompt.to_string(),
            "--output-format".into(),
            "text".into(),
        ];
        if opts.allow_dangerous {
            args.push("--dangerously-skip-permissions".into());
        }

        let mut env = vec![("ELECTRON_RUN_AS_NODE".into(), "1".into())];
        if let Ok(dir) = workbuddy_config_dir() {
            let s = dir.to_string_lossy().into_owned();
            env.push(("WORKBUDDY_CONFIG_DIR".into(), s.clone()));
            env.push(("CODEBUDDY_CONFIG_DIR".into(), s));
        }

        tracing::debug!(
            target: crate::logging::targets::RUN,
            module = crate::logging::targets::RUN,
            op = "build_run_spec",
            agent = "workbuddy",
            program = %binary.display(),
            codebuddy = %codebuddy.display(),
            electron_run_as_node = true,
            "WorkBuddy headless run_spec ready"
        );

        Ok(RunSpec {
            agent: AgentId::WorkBuddy,
            program: binary.to_path_buf(),
            args,
            cwd: opts.cwd.clone(),
            env,
        })
    }
}

/// Config root: `WORKBUDDY_CONFIG_DIR` / `CODEBUDDY_CONFIG_DIR` or `~/.workbuddy`.
pub fn workbuddy_config_dir() -> Result<PathBuf> {
    crate::utils::paths::agent_config_dir(AgentId::WorkBuddy)
}

/// Resolve WorkBuddy.exe: fixed install dirs first, registry only as fallback.
///
/// Hot path must not spawn PowerShell when the default install is present
/// (Agents page runs detect often; registry scan was a major latency source).
pub fn resolve_workbuddy_exe() -> Option<PathBuf> {
    for p in well_known_exe_paths() {
        if p.is_file() {
            tracing::debug!(
                target: crate::logging::targets::DETECT,
                module = crate::logging::targets::DETECT,
                op = "resolve_workbuddy_exe",
                via = "well_known",
                path = %p.display(),
                "WorkBuddy.exe found in fixed install path"
            );
            return Some(p);
        }
    }
    // Slow path: HKCU Uninstall → DisplayIcon (spawns powershell once).
    if let Some(from_reg) = resolve_exe_from_uninstall_registry() {
        if from_reg.is_file() {
            tracing::debug!(
                target: crate::logging::targets::DETECT,
                module = crate::logging::targets::DETECT,
                op = "resolve_workbuddy_exe",
                via = "registry",
                path = %from_reg.display(),
                "WorkBuddy.exe found via uninstall registry"
            );
            return Some(from_reg);
        }
    }
    None
}

/// Cheap fixed candidates only — no process spawn.
fn well_known_exe_paths() -> Vec<PathBuf> {
    let mut out = Vec::new();
    #[cfg(windows)]
    {
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            out.push(
                PathBuf::from(local)
                    .join("Programs")
                    .join("WorkBuddy")
                    .join("WorkBuddy.exe"),
            );
        }
        if let Ok(home) = home_dir() {
            out.push(
                home.join("AppData")
                    .join("Local")
                    .join("Programs")
                    .join("WorkBuddy")
                    .join("WorkBuddy.exe"),
            );
        }
    }
    #[cfg(not(windows))]
    {
        // macOS app bundle (no local evidence required; NotFound if missing).
        out.push(PathBuf::from(
            "/Applications/WorkBuddy.app/Contents/MacOS/WorkBuddy",
        ));
        if let Ok(home) = home_dir() {
            out.push(
                home.join("Applications")
                    .join("WorkBuddy.app")
                    .join("Contents")
                    .join("MacOS")
                    .join("WorkBuddy"),
            );
        }
    }
    out
}

/// Production bundled CLI only (never unpack/extract scratch paths).
pub fn resolve_bundled_codebuddy(install_dir: &Path) -> Option<PathBuf> {
    let mut candidates = vec![
        install_dir
            .join("resources")
            .join("app.asar.unpacked")
            .join("cli")
            .join("bin")
            .join("codebuddy"),
    ];
    #[cfg(windows)]
    {
        candidates.push(
            install_dir
                .join("resources")
                .join("app.asar.unpacked")
                .join("cli")
                .join("bin")
                .join("codebuddy.cmd"),
        );
        // Some builds ship extension-less or .exe
        candidates.push(
            install_dir
                .join("resources")
                .join("app.asar.unpacked")
                .join("cli")
                .join("bin")
                .join("codebuddy.exe"),
        );
    }
    for p in candidates {
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

/// Allowlisted silent uninstaller (Windows).
pub fn resolve_uninstaller() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            let p = PathBuf::from(local)
                .join("Programs")
                .join("WorkBuddy")
                .join("Uninstall WorkBuddy.exe");
            if p.is_file() {
                return Some(p);
            }
        }
        if let Some(exe) = resolve_workbuddy_exe() {
            if let Some(dir) = exe.parent() {
                let p = dir.join("Uninstall WorkBuddy.exe");
                if p.is_file() {
                    return Some(p);
                }
            }
        }
    }
    None
}

fn auth_info_path() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        let local = std::env::var("LOCALAPPDATA").ok()?;
        let p = PathBuf::from(local)
            .join("CodeBuddyExtension")
            .join("Data")
            .join("Public")
            .join("auth")
            .join("workbuddy-desktop.info");
        Some(p)
    }
    #[cfg(not(windows))]
    {
        None
    }
}

fn read_version_from_last_launch() -> Option<String> {
    let dir = workbuddy_config_dir().ok()?;
    let path = dir.join("last-launch.json");
    if !path.is_file() {
        return None;
    }
    let text = std::fs::read_to_string(path).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    v.get("version")
        .and_then(|x| x.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn read_version_from_package_json(install_dir: &Path) -> Option<String> {
    let path = install_dir
        .join("resources")
        .join("app.asar.unpacked")
        .join("package.json");
    let path = if path.is_file() {
        path
    } else {
        install_dir.join("package.json")
    };
    if !path.is_file() {
        return None;
    }
    let text = std::fs::read_to_string(path).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    v.get("version")
        .and_then(|x| x.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Slow fallback: parse DisplayIcon from HKCU Uninstall keys (Windows only).
/// Call only after well-known paths miss — spawns PowerShell.
#[cfg(windows)]
fn resolve_exe_from_uninstall_registry() -> Option<PathBuf> {
    use std::os::windows::process::CommandExt;
    use std::process::Command;
    use std::time::Instant;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let started = Instant::now();
    // Query display name matching WorkBuddy and read DisplayIcon.
    let script = r#"
$ErrorActionPreference = 'SilentlyContinue'
$keys = Get-ChildItem 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall' -ErrorAction SilentlyContinue
foreach ($k in $keys) {
  $p = Get-ItemProperty $k.PSPath -ErrorAction SilentlyContinue
  if ($p.DisplayName -like 'WorkBuddy*') {
    if ($p.DisplayIcon) { Write-Output $p.DisplayIcon; exit 0 }
    if ($p.InstallLocation) {
      $exe = Join-Path $p.InstallLocation 'WorkBuddy.exe'
      if (Test-Path $exe) { Write-Output $exe; exit 0 }
    }
  }
}
"#;
    let out = Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            script,
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    tracing::debug!(
        target: crate::logging::targets::DETECT,
        module = crate::logging::targets::DETECT,
        op = "resolve_workbuddy_registry",
        elapsed_ms = started.elapsed().as_millis() as u64,
        ok = out.status.success(),
        "uninstall registry PowerShell probe finished"
    );
    if !out.status.success() {
        return None;
    }
    let line = String::from_utf8_lossy(&out.stdout)
        .lines()
        .next()?
        .trim()
        .to_string();
    if line.is_empty() {
        return None;
    }
    // DisplayIcon may be "C:\...\WorkBuddy.exe,0"
    let path_part = line.split(',').next()?.trim().trim_matches('"');
    let p = PathBuf::from(path_part);
    if p.is_file() {
        Some(p)
    } else if p.extension().is_none() {
        let exe = p.join("WorkBuddy.exe");
        if exe.is_file() {
            Some(exe)
        } else {
            None
        }
    } else {
        None
    }
}

#[cfg(not(windows))]
fn resolve_exe_from_uninstall_registry() -> Option<PathBuf> {
    None
}

fn read_json_value_or_empty(path: &Path) -> Result<serde_json::Value> {
    if !path.exists() {
        return Ok(serde_json::json!({}));
    }
    let text = std::fs::read_to_string(path)?;
    let value: serde_json::Value = serde_json::from_str(&text)?;
    // models.json may be a top-level array — accept any JSON value.
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs;

    #[test]
    fn install_channels_native_only_no_runtime() {
        let channels = WorkBuddyAdapter.install_channels();
        assert_eq!(channels.len(), 1);
        assert_eq!(channels[0].id, "native");
        assert!(channels[0].requires.is_empty());
    }

    #[test]
    fn skills_dir_under_workbuddy_home() {
        let dir = WorkBuddyAdapter.skills_dir().expect("skills_dir");
        let s = dir.to_string_lossy().replace('\\', "/");
        assert!(
            s.ends_with("/.workbuddy/skills") || s.contains("workbuddy") && s.ends_with("/skills"),
            "unexpected skills_dir: {s}"
        );
    }

    #[test]
    fn live_backup_paths_include_core_files() {
        let paths = WorkBuddyAdapter.live_backup_paths();
        assert!(!paths.is_empty());
        let names: Vec<String> = paths
            .iter()
            .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
            .collect();
        assert!(names.iter().any(|n| n == "settings.json"));
        assert!(names.iter().any(|n| n == "models.json"));
        assert!(names.iter().any(|n| n == ".mcp.json"));
    }

    #[test]
    fn build_run_spec_headless_flags() {
        let tmp = tempfile_dir();
        // layout: install dir + bundled CLI under resources (production tree)
        let bin_dir = tmp
            .join("resources")
            .join("app.asar.unpacked")
            .join("cli")
            .join("bin");
        fs::create_dir_all(&bin_dir).unwrap();
        let codebuddy = bin_dir.join("codebuddy");
        fs::write(&codebuddy, b"#!/bin/sh\n").unwrap();
        let exe = tmp.join("WorkBuddy.exe");
        fs::write(&exe, b"mz").unwrap();

        let opts = RunOptions::default();
        let spec = WorkBuddyAdapter
            .build_run_spec(&exe, "hello", &opts)
            .unwrap();
        assert_eq!(spec.agent, AgentId::WorkBuddy);
        assert_eq!(spec.program, exe);
        assert_eq!(spec.args[0], codebuddy.to_string_lossy());
        assert_eq!(spec.args[1], "-p");
        assert_eq!(spec.args[2], "hello");
        assert!(spec.args.iter().any(|a| a == "--output-format"));
        assert!(spec.args.iter().any(|a| a == "text"));
        assert!(!spec
            .args
            .iter()
            .any(|a| a == "--dangerously-skip-permissions"));
        assert!(spec
            .env
            .iter()
            .any(|(k, v)| k == "ELECTRON_RUN_AS_NODE" && v == "1"));
        let display = spec.display_command();
        assert!(display.contains("ELECTRON_RUN_AS_NODE=1"));
        assert!(display.contains("-p"));
    }

    #[test]
    fn build_run_spec_allow_dangerous() {
        let tmp = tempfile_dir();
        let bin_dir = tmp
            .join("resources")
            .join("app.asar.unpacked")
            .join("cli")
            .join("bin");
        fs::create_dir_all(&bin_dir).unwrap();
        fs::write(bin_dir.join("codebuddy"), b"x").unwrap();
        let exe = tmp.join("WorkBuddy.exe");
        fs::write(&exe, b"mz").unwrap();

        let mut opts = RunOptions::default();
        opts.allow_dangerous = true;
        let spec = WorkBuddyAdapter.build_run_spec(&exe, "x", &opts).unwrap();
        assert!(spec
            .args
            .iter()
            .any(|a| a == "--dangerously-skip-permissions"));
    }

    #[test]
    fn write_config_is_fail_closed() {
        let err = WorkBuddyAdapter
            .write_config(&AgentConfig {
                agent: AgentId::WorkBuddy,
                raw: json!({}),
            })
            .unwrap_err();
        assert_eq!(err.code(), "unsupported");
    }

    #[test]
    fn account_switch_disabled_p0() {
        assert!(
            WorkBuddyAdapter
                .capability(crate::models::Capability::AccountSwitch)
                .is_blocked()
        );
        assert!(
            WorkBuddyAdapter
                .capability(crate::models::Capability::Skills)
                .is_usable()
        );
    }

    #[test]
    fn resolve_bundled_codebuddy_ignores_extracted() {
        let tmp = tempfile_dir();
        // only extracted path — must NOT be used
        let extracted = tmp
            .join("extracted")
            .join("cli")
            .join("bin");
        fs::create_dir_all(&extracted).unwrap();
        fs::write(extracted.join("codebuddy"), b"bad").unwrap();
        assert!(resolve_bundled_codebuddy(&tmp).is_none());

        let good = tmp
            .join("resources")
            .join("app.asar.unpacked")
            .join("cli")
            .join("bin");
        fs::create_dir_all(&good).unwrap();
        let cb = good.join("codebuddy");
        fs::write(&cb, b"ok").unwrap();
        assert_eq!(resolve_bundled_codebuddy(&tmp), Some(cb));
    }

    #[test]
    fn well_known_exe_paths_are_cheap_fixed_only() {
        let paths = well_known_exe_paths();
        // Must not be empty on Windows (LOCALAPPDATA or home) or Unix (Applications).
        // Registry is intentionally not in this list.
        for p in &paths {
            let s = p.to_string_lossy().to_ascii_lowercase();
            assert!(
                s.contains("workbuddy"),
                "unexpected well-known path: {}",
                p.display()
            );
            assert!(
                !s.contains("uninstall"),
                "well-known must not be uninstaller path: {}",
                p.display()
            );
        }
    }

    fn tempfile_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "agenthub-wb-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }
}
