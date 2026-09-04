//! Remediation plans for missing runtimes (no auto-install in P0).

use crate::models::{Remediation, RuntimeId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LinuxFamily {
    Debian,
    Fedora,
    Arch,
    Suse,
    Alpine,
    Other,
}

fn os_release_field(text: &str, key: &str) -> Option<String> {
    for line in text.lines() {
        let line = line.trim();
        let Some(rest) = line
            .strip_prefix(key)
            .and_then(|rest| rest.strip_prefix('='))
        else {
            continue;
        };
        let value = rest.trim().trim_matches('"').trim_matches('\'');
        if !value.is_empty() {
            return Some(value.to_ascii_lowercase());
        }
    }
    None
}

fn hay_has_token(hay: &str, needles: &[&str]) -> bool {
    hay.split_whitespace().any(|token| needles.contains(&token))
}

fn linux_family_from_os_release(text: &str) -> LinuxFamily {
    let id = os_release_field(text, "ID").unwrap_or_default();
    let like = os_release_field(text, "ID_LIKE").unwrap_or_default();
    let hay = format!("{id} {like}");
    if hay_has_token(&hay, &["debian", "ubuntu", "linuxmint"]) {
        LinuxFamily::Debian
    } else if hay_has_token(&hay, &["fedora", "rhel", "centos", "rocky", "almalinux"]) {
        LinuxFamily::Fedora
    } else if hay_has_token(&hay, &["arch", "archlinux", "manjaro"]) {
        LinuxFamily::Arch
    } else if hay_has_token(&hay, &["suse", "opensuse", "sles"]) {
        LinuxFamily::Suse
    } else if hay_has_token(&hay, &["alpine"]) {
        LinuxFamily::Alpine
    } else {
        LinuxFamily::Other
    }
}

#[cfg_attr(any(windows, target_os = "macos"), allow(dead_code))]
fn linux_family() -> LinuxFamily {
    match std::fs::read_to_string("/etc/os-release") {
        Ok(text) => linux_family_from_os_release(&text),
        Err(_) => LinuxFamily::Other,
    }
}

#[cfg_attr(any(windows, target_os = "macos"), allow(dead_code))]
fn linux_runtime_command_for(family: LinuxFamily, id: RuntimeId) -> Option<String> {
    let packages = match id {
        RuntimeId::NodeJs | RuntimeId::Npm => "nodejs npm",
        RuntimeId::Git => "git",
        RuntimeId::PowerShell => return None,
    };
    match family {
        LinuxFamily::Debian => Some(format!("sudo apt-get install -y {packages}")),
        LinuxFamily::Fedora => Some(format!("sudo dnf install -y {packages}")),
        LinuxFamily::Arch => Some(format!("sudo pacman -S --needed {packages}")),
        LinuxFamily::Suse => Some(format!("sudo zypper install -y {packages}")),
        LinuxFamily::Alpine => Some(format!("sudo apk add {packages}")),
        LinuxFamily::Other => None,
    }
}

#[cfg_attr(any(windows, target_os = "macos"), allow(dead_code))]
fn linux_runtime_command(id: RuntimeId) -> Option<String> {
    linux_runtime_command_for(linux_family(), id)
}

const HOMEBREW_URL: &str = "https://brew.sh/";
const HOMEBREW_INSTALL_COMMAND: &str = r#"/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)""#;
const NODEJS_URL: &str = "https://nodejs.org/";
const GIT_URL: &str = "https://git-scm.com/downloads";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HostFamily {
    Macos,
    Windows,
    Linux,
}

fn host_family() -> HostFamily {
    if cfg!(target_os = "macos") {
        HostFamily::Macos
    } else if cfg!(windows) {
        HostFamily::Windows
    } else {
        HostFamily::Linux
    }
}

fn host_package_manager_present() -> bool {
    match host_family() {
        HostFamily::Macos => super::resolve_binary(&["brew"]).is_some(),
        HostFamily::Windows => super::resolve_binary(&["winget", "winget.exe"]).is_some(),
        HostFamily::Linux => false,
    }
}

fn official_runtime_url(id: RuntimeId) -> &'static str {
    match id {
        RuntimeId::NodeJs | RuntimeId::Npm => NODEJS_URL,
        RuntimeId::Git => GIT_URL,
        RuntimeId::PowerShell => {
            "https://learn.microsoft.com/powershell/scripting/install/installing-powershell"
        }
    }
}

fn runtime_display_name(id: RuntimeId) -> &'static str {
    match id {
        RuntimeId::NodeJs | RuntimeId::Npm => "Node.js",
        RuntimeId::Git => "Git",
        RuntimeId::PowerShell => "PowerShell",
    }
}

fn brew_missing_text(id: RuntimeId) -> String {
    format!(
        "未找到 Homebrew，无法一键安装 {}。请先安装 Homebrew，或打开官网手动安装。完成后完全退出并重启 AgentHub 再检测。",
        runtime_display_name(id)
    )
}

fn winget_missing_text(id: RuntimeId) -> String {
    format!(
        "未找到 winget，无法一键安装 {}。请打开官网手动安装，完成后完全退出并重启 AgentHub 再检测。",
        runtime_display_name(id)
    )
}

fn brew_missing_primary(id: RuntimeId) -> Remediation {
    Remediation {
        kind: "url".into(),
        command: Some(HOMEBREW_INSTALL_COMMAND.into()),
        url: Some(official_runtime_url(id).into()),
        text: Some(brew_missing_text(id)),
    }
}

fn brew_missing_remediations(id: RuntimeId) -> Vec<Remediation> {
    vec![
        brew_missing_primary(id),
        Remediation {
            kind: "url".into(),
            command: None,
            url: Some(HOMEBREW_URL.into()),
            text: Some("安装 Homebrew 后，完全退出并重启 AgentHub，即可使用一键安装。".into()),
        },
    ]
}

fn winget_missing_primary(id: RuntimeId) -> Remediation {
    Remediation {
        kind: "url".into(),
        command: None,
        url: Some(official_runtime_url(id).into()),
        text: Some(winget_missing_text(id)),
    }
}

/// Remediation after brew/winget was already missing at install time.
///
/// Do not re-probe the host: a GUI PATH miss must still explain how to install
/// the package manager, not repeat `brew install …` / `winget install …`.
pub fn remediations_when_installer_missing(channel: &str, id: RuntimeId) -> Vec<Remediation> {
    match channel {
        "brew" => brew_missing_remediations(id),
        "winget" => vec![winget_missing_primary(id)],
        _ => vec![remediation_for(id)],
    }
}

pub fn remediation_for(id: RuntimeId) -> Remediation {
    remediation_for_host(id, host_family(), host_package_manager_present())
}

fn remediation_for_host(
    id: RuntimeId,
    family: HostFamily,
    package_manager_present: bool,
) -> Remediation {
    match id {
        RuntimeId::Npm => Remediation {
            kind: "hint".into(),
            command: None,
            url: Some(NODEJS_URL.into()),
            text: Some(
                "npm usually ships with Node.js. If node works but npm does not, repair PATH or reinstall Node."
                    .into(),
            ),
        },
        RuntimeId::PowerShell => powershell_remediation(family),
        RuntimeId::NodeJs | RuntimeId::Git => match family {
            HostFamily::Macos => macos_runtime_remediation(id, package_manager_present),
            HostFamily::Windows => windows_runtime_remediation(id, package_manager_present),
            HostFamily::Linux => linux_runtime_remediation(id),
        },
    }
}

fn powershell_remediation(family: HostFamily) -> Remediation {
    if family == HostFamily::Windows {
        Remediation {
            kind: "hint".into(),
            command: None,
            url: Some(official_runtime_url(RuntimeId::PowerShell).into()),
            text: Some(
                "Windows usually ships PowerShell 5.1 (System32). PowerShell 7 (pwsh) is optional but preferred for native install scripts. AgentHub does not auto-install PowerShell; install pwsh manually if needed. Check ExecutionPolicy if scripts fail."
                    .into(),
            ),
        }
    } else {
        Remediation {
            kind: "hint".into(),
            command: None,
            url: None,
            text: Some(
                "PowerShell is not required on macOS/Linux. Native agent installers use official bash/sh scripts."
                    .into(),
            ),
        }
    }
}

fn macos_runtime_remediation(id: RuntimeId, brew_present: bool) -> Remediation {
    if !brew_present {
        return brew_missing_primary(id);
    }
    match id {
        RuntimeId::Git => Remediation {
            kind: "brew".into(),
            command: Some("brew install git".into()),
            url: Some(GIT_URL.into()),
            text: Some(
                "Install Git with Homebrew, then fully quit and restart AgentHub so PATH refreshes. Skills market / git URL install need git clone."
                    .into(),
            ),
        },
        _ => Remediation {
            kind: "brew".into(),
            command: Some("brew install node".into()),
            url: Some(NODEJS_URL.into()),
            text: Some(
                "Install Node.js with Homebrew, then restart the shell / AgentHub so PATH refreshes."
                    .into(),
            ),
        },
    }
}

fn windows_runtime_remediation(id: RuntimeId, winget_present: bool) -> Remediation {
    if !winget_present {
        return winget_missing_primary(id);
    }
    match id {
        RuntimeId::Git => Remediation {
            kind: "winget".into(),
            command: Some("winget install --id Git.Git -e --source winget".into()),
            url: Some(GIT_URL.into()),
            text: Some(
                "Install Git, then fully quit and restart AgentHub so PATH refreshes. Skills market / git URL install need git clone."
                    .into(),
            ),
        },
        _ => Remediation {
            kind: "winget".into(),
            command: Some("winget install OpenJS.NodeJS.LTS".into()),
            url: Some(NODEJS_URL.into()),
            text: Some(
                "Install Node.js LTS, then restart the shell / AgentHub so PATH refreshes."
                    .into(),
            ),
        },
    }
}

fn linux_runtime_remediation(id: RuntimeId) -> Remediation {
    match id {
        RuntimeId::Git => Remediation {
            kind: "command".into(),
            command: linux_runtime_command(RuntimeId::Git),
            url: Some(GIT_URL.into()),
            text: Some(
                "Linux does not one-click install Git. Use your distro package manager or the official download, then fully quit and restart AgentHub so PATH refreshes. Skills market / git URL install need git clone. Unknown distros get the official URL instead of an apt-get guess."
                    .into(),
            ),
        },
        _ => Remediation {
            kind: "command".into(),
            command: linux_runtime_command(RuntimeId::NodeJs),
            url: Some(NODEJS_URL.into()),
            text: Some(
                "Linux does not one-click install Node. Use your distro package manager or the official LTS, then fully quit and restart AgentHub so PATH refreshes. Distro nodejs is often older than 18; prefer https://nodejs.org/ when unsure. Unknown distros get the official URL instead of an apt-get guess."
                    .into(),
            ),
        },
    }
}

#[cfg(test)]
mod tests;
