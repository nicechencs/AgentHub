//! Node.js / npm / PowerShell / Git detection.

use std::path::{Path, PathBuf};

use which::which;

use crate::models::{EnvStatus, EnvStatusKind, RuntimeId};
use crate::utils::process::{run_capture, stdout_first_line};

use crate::catalog::limits::NODE_MIN_MAJOR;

pub fn detect_nodejs() -> EnvStatus {
    match resolve_which(&["node", "node.exe"]) {
        Some(path) => match run_capture(&path, &["-v"]) {
            Ok(out) if out.status.success() => {
                let version =
                    stdout_first_line(&out).map(|v| v.trim_start_matches('v').to_string());
                let status = match version.as_deref().and_then(parse_major) {
                    Some(major) if major >= NODE_MIN_MAJOR => EnvStatusKind::Ok,
                    Some(_) => EnvStatusKind::Outdated,
                    None => EnvStatusKind::BrokenPath,
                };
                EnvStatus {
                    id: RuntimeId::NodeJs,
                    status,
                    version,
                    path: Some(path),
                    min_required: Some(format!(">={NODE_MIN_MAJOR}")),
                    remediation: None,
                    notes: vec![],
                }
            }
            _ => EnvStatus {
                id: RuntimeId::NodeJs,
                status: EnvStatusKind::BrokenPath,
                version: None,
                path: Some(path),
                min_required: Some(format!(">={NODE_MIN_MAJOR}")),
                remediation: None,
                notes: vec![],
            },
        },
        None => EnvStatus {
            id: RuntimeId::NodeJs,
            status: EnvStatusKind::Missing,
            version: None,
            path: None,
            min_required: Some(format!(">={NODE_MIN_MAJOR}")),
            remediation: None,
            notes: vec![],
        },
    }
}

pub fn detect_npm() -> EnvStatus {
    match resolve_which(&["npm", "npm.cmd", "npm.exe"]) {
        Some(path) => match run_capture(&path, &["-v"]) {
            Ok(out) if out.status.success() => EnvStatus {
                id: RuntimeId::Npm,
                status: EnvStatusKind::Ok,
                version: stdout_first_line(&out),
                path: Some(path),
                min_required: None,
                remediation: None,
                notes: vec![],
            },
            _ => EnvStatus {
                id: RuntimeId::Npm,
                status: EnvStatusKind::BrokenPath,
                version: None,
                path: Some(path),
                min_required: None,
                remediation: None,
                notes: vec![],
            },
        },
        None => EnvStatus {
            id: RuntimeId::Npm,
            status: EnvStatusKind::Missing,
            version: None,
            path: None,
            min_required: None,
            remediation: None,
            notes: vec![],
        },
    }
}

/// Detect PowerShell availability.
///
/// - **Windows**: probes Windows PowerShell 5.1 (`powershell`) and PowerShell 7+ (`pwsh`)
///   separately; either is enough for `RuntimeId::PowerShell` readiness (native install scripts).
///   Prefer `pwsh` as the primary path/version when both work.
/// - **macOS / Linux**: PowerShell is not a shared runtime.  Doctor skips it via
///   [`super::detect::host_runtimes`]; this function only remains for explicit
///   Windows-only native install resolution and tests.
pub fn detect_powershell() -> EnvStatus {
    #[cfg(not(windows))]
    {
        return EnvStatus {
            id: RuntimeId::PowerShell,
            status: EnvStatusKind::Ok,
            version: None,
            path: None,
            min_required: None,
            remediation: None,
            notes: vec![
                "Windows PowerShell 5.1: not applicable on this platform".into(),
                "PowerShell 7 (pwsh): not required (native installers use bash/sh)".into(),
            ],
        };
    }

    #[cfg(windows)]
    {
    let mut notes = Vec::new();
    let mut last_broken: Option<PathBuf> = None;

    let ps51 = probe_powershell_candidate(
        &["powershell", "powershell.exe"],
        Some(PathBuf::from(
            r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe",
        )),
    );

    let pwsh = probe_powershell_candidate(&["pwsh", "pwsh.exe"], None);

    match &ps51 {
        PsProbe::Ok { path, version } => {
            notes.push(format!(
                "Windows PowerShell 5.1: {} @ {}",
                version.as_deref().unwrap_or("ok"),
                path.display()
            ));
        }
        PsProbe::Broken { path } => {
            notes.push(format!(
                "Windows PowerShell 5.1: broken @ {}",
                path.display()
            ));
            last_broken = Some(path.clone());
        }
        PsProbe::Missing => {
            notes.push("Windows PowerShell 5.1: missing".into());
        }
        PsProbe::NotApplicable => {
            notes.push("Windows PowerShell 5.1: not applicable on this platform".into());
        }
    }

    match &pwsh {
        PsProbe::Ok { path, version } => {
            notes.push(format!(
                "PowerShell 7 (pwsh): {} @ {}",
                version.as_deref().unwrap_or("ok"),
                path.display()
            ));
        }
        PsProbe::Broken { path } => {
            notes.push(format!("PowerShell 7 (pwsh): broken @ {}", path.display()));
            last_broken = Some(path.clone());
        }
        PsProbe::Missing => {
            notes.push("PowerShell 7 (pwsh): missing".into());
        }
        PsProbe::NotApplicable => {}
    }

    // Aggregate readiness: any working interpreter is enough for native channel.
    // Prefer pwsh as the reported path/version for install execution affinity.
    let status = if let PsProbe::Ok { path, version } = &pwsh {
        EnvStatus {
            id: RuntimeId::PowerShell,
            status: EnvStatusKind::Ok,
            version: version.clone(),
            path: Some(path.clone()),
            min_required: None,
            remediation: None,
            notes,
        }
    } else if let PsProbe::Ok { path, version } = &ps51 {
        EnvStatus {
            id: RuntimeId::PowerShell,
            status: EnvStatusKind::Ok,
            version: version.clone(),
            path: Some(path.clone()),
            min_required: None,
            remediation: None,
            notes,
        }
    } else if let Some(path) = last_broken {
        EnvStatus {
            id: RuntimeId::PowerShell,
            status: EnvStatusKind::BrokenPath,
            version: None,
            path: Some(path),
            min_required: None,
            remediation: None,
            notes,
        }
    } else {
        EnvStatus {
            id: RuntimeId::PowerShell,
            status: EnvStatusKind::Missing,
            version: None,
            path: None,
            min_required: None,
            remediation: None,
            notes,
        }
    };

    for n in &status.notes {
        tracing::debug!(
            target: crate::logging::targets::DETECT,
            module = crate::logging::targets::DETECT,
            op = "detect_powershell",
            status = ?status.status,
            "{n}"
        );
    }
    if status.status != EnvStatusKind::Ok {
        tracing::info!(
            target: crate::logging::targets::DETECT,
            module = crate::logging::targets::DETECT,
            op = "detect_powershell",
            status = ?status.status,
            path = %status
                .path
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "-".into()),
            "PowerShell runtime not fully ready (5.1 and/or 7 may be missing)"
        );
    }

    status
    } // #[cfg(windows)]
}

#[derive(Clone)]
enum PsProbe {
    Ok {
        path: PathBuf,
        version: Option<String>,
    },
    Broken {
        path: PathBuf,
    },
    Missing,
    /// Constructed only on non-Windows targets.
    #[cfg_attr(windows, allow(dead_code))]
    NotApplicable,
}

fn probe_powershell_candidate(names: &[&str], fallback: Option<PathBuf>) -> PsProbe {
    if let Some(path) = resolve_which(names) {
        return probe_ps_path(&path);
    }
    if let Some(fb) = fallback {
        if fb.is_file() {
            return probe_ps_path(&fb);
        }
    }
    PsProbe::Missing
}

fn probe_ps_path(path: &Path) -> PsProbe {
    match run_capture(
        path,
        &[
            "-NoProfile",
            "-Command",
            "$PSVersionTable.PSVersion.ToString()",
        ],
    ) {
        Ok(out) if out.status.success() => PsProbe::Ok {
            path: path.to_path_buf(),
            version: stdout_first_line(&out),
        },
        _ => PsProbe::Broken {
            path: path.to_path_buf(),
        },
    }
}

/// Resolve first existing binary from candidate names (`which`).
fn resolve_which(names: &[&str]) -> Option<PathBuf> {
    for name in names {
        if let Ok(p) = which(name) {
            return Some(p);
        }
    }
    // GUI-launched macOS apps often inherit a minimal PATH that omits the
    // Homebrew prefix. Probe both supported Homebrew locations directly so a
    // freshly installed runtime is visible without restarting the shell.
    for candidate in platform_binary_candidates(names) {
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// Well-known non-PATH binary locations for the current platform.
///
/// Kept as a pure helper so path coverage is testable without mutating PATH or
/// requiring Homebrew to be installed on the test host.
fn platform_binary_candidates(names: &[&str]) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    #[cfg(target_os = "macos")]
    for prefix in ["/opt/homebrew/bin", "/usr/local/bin"] {
        for name in names {
            candidates.push(PathBuf::from(prefix).join(name));
        }
    }
    candidates
}

/// Detect Git CLI (`git --version`).
///
/// Needed by Skills market / `skill install <git-url>` (`git clone` / `git pull`).
/// Not an Agent install-channel hard dependency, but listed as a shared runtime
/// so doctor / Agents env bar can detect and guide install.
pub fn detect_git() -> EnvStatus {
    match resolve_which(&["git", "git.exe"]) {
        Some(path) => match run_capture(&path, &["--version"]) {
            Ok(out) if out.status.success() => {
                let raw = stdout_first_line(&out);
                let version = raw.as_deref().map(parse_git_version);
                EnvStatus {
                    id: RuntimeId::Git,
                    status: EnvStatusKind::Ok,
                    version,
                    path: Some(path),
                    min_required: None,
                    remediation: None,
                    notes: vec![],
                }
            }
            _ => EnvStatus {
                id: RuntimeId::Git,
                status: EnvStatusKind::BrokenPath,
                version: None,
                path: Some(path),
                min_required: None,
                remediation: None,
                notes: vec![],
            },
        },
        None => EnvStatus {
            id: RuntimeId::Git,
            status: EnvStatusKind::Missing,
            version: None,
            path: None,
            min_required: None,
            remediation: None,
            notes: vec![],
        },
    }
}

/// Prefer PowerShell 7 (`pwsh`), else Windows PowerShell 5.1.
/// Used by native install script runner — logs should include the chosen path.
pub fn resolve_powershell_for_native() -> Option<PathBuf> {
    // Prefer 7 for modern script compatibility.
    if let Some(p) = resolve_which(&["pwsh", "pwsh.exe"]) {
        return Some(p);
    }
    if let Some(p) = resolve_which(&["powershell", "powershell.exe"]) {
        return Some(p);
    }
    #[cfg(windows)]
    {
        let fallback = PathBuf::from(r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe");
        if fallback.is_file() {
            return Some(fallback);
        }
    }
    None
}

fn parse_major(version: &str) -> Option<u64> {
    let major = version.split('.').next()?;
    major.parse().ok()
}

/// `git version 2.43.0.windows.1` → `2.43.0.windows.1`
fn parse_git_version(line: &str) -> String {
    let trimmed = line.trim();
    const PREFIX: &str = "git version ";
    if let Some(rest) = trimmed
        .strip_prefix(PREFIX)
        .or_else(|| trimmed.strip_prefix("git version"))
    {
        rest.trim().to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_major_ok() {
        assert_eq!(parse_major("22.11.0"), Some(22));
        assert_eq!(parse_major("7.6.4"), Some(7));
    }

    #[test]
    fn parse_git_version_strips_prefix() {
        assert_eq!(
            parse_git_version("git version 2.43.0.windows.1"),
            "2.43.0.windows.1"
        );
        assert_eq!(parse_git_version("git version 2.39.2"), "2.39.2");
        assert_eq!(parse_git_version("2.40.0"), "2.40.0");
    }

    #[test]
    fn detect_git_returns_git_runtime_id() {
        let st = detect_git();
        assert_eq!(st.id, RuntimeId::Git);
        // On machines with git, status should be Ok with a path; without git → Missing.
        match st.status {
            EnvStatusKind::Ok => {
                assert!(st.path.is_some());
                assert!(st.version.is_some());
            }
            EnvStatusKind::Missing => {
                assert!(st.path.is_none());
            }
            EnvStatusKind::BrokenPath => {
                assert!(st.path.is_some());
            }
            EnvStatusKind::Outdated => panic!("git has no min version yet"),
        }
    }

    #[test]
    fn detect_powershell_emits_dual_version_notes() {
        let st = detect_powershell();
        assert!(!st.notes.is_empty(), "PowerShell detect must emit notes");
        assert_eq!(st.id, RuntimeId::PowerShell);
        #[cfg(windows)]
        {
            assert!(
                st.notes.iter().any(|n| n.contains("Windows PowerShell 5.1")),
                "Windows must report 5.1 line: {:?}",
                st.notes
            );
            assert!(
                st.notes.iter().any(|n| n.contains("PowerShell 7")),
                "Windows must report pwsh line: {:?}",
                st.notes
            );
        }
        #[cfg(not(windows))]
        {
            // macOS/Linux: do not probe pwsh; mark as not required / not applicable.
            assert_eq!(st.status, EnvStatusKind::Ok);
            assert!(st.path.is_none());
            assert!(
                st.notes.iter().any(|n| n.contains("not applicable") || n.contains("not required")),
                "non-Windows must mark PowerShell as not applicable/required: {:?}",
                st.notes
            );
            assert!(
                !st.notes
                    .iter()
                    .any(|n| n.contains("Windows PowerShell 5.1:") && n.contains('@')),
                "non-Windows must not report a 5.1 binary path: {:?}",
                st.notes
            );
        }
    }

    #[test]
    fn resolve_powershell_for_native_prefers_existing_binary() {
        // On CI/dev machines at least one of System32 powershell or pwsh is common on Windows.
        let resolved = resolve_powershell_for_native();
        #[cfg(windows)]
        {
            assert!(
                resolved.is_some(),
                "Windows should resolve at least System32 powershell or pwsh"
            );
            let p = resolved.unwrap();
            assert!(p.is_file(), "resolved PowerShell must exist: {}", p.display());
        }
        #[cfg(not(windows))]
        {
            // Optional on macOS/Linux — if present must be a real file.
            if let Some(p) = resolved {
                assert!(p.is_file());
            }
        }
    }

    #[test]
    fn platform_binary_candidates_include_macos_homebrew_prefixes() {
        let candidates = platform_binary_candidates(&["node", "npm"]);
        #[cfg(target_os = "macos")]
        {
            assert!(candidates.contains(&PathBuf::from("/opt/homebrew/bin/node")));
            assert!(candidates.contains(&PathBuf::from("/opt/homebrew/bin/npm")));
            assert!(candidates.contains(&PathBuf::from("/usr/local/bin/node")));
            assert!(candidates.contains(&PathBuf::from("/usr/local/bin/npm")));
        }
        #[cfg(not(target_os = "macos"))]
        assert!(candidates.is_empty());
    }
}
