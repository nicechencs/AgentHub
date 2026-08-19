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

pub fn remediation_for(id: RuntimeId) -> Remediation {
    match id {
        RuntimeId::NodeJs => {
            if cfg!(target_os = "macos") {
                Remediation {
                    kind: "brew".into(),
                    command: Some("brew install node".into()),
                    url: Some("https://nodejs.org/".into()),
                    text: Some(
                        "Install Node.js with Homebrew, then restart the shell / AgentHub so PATH refreshes."
                            .into(),
                    ),
                }
            } else if cfg!(windows) {
                Remediation {
                    kind: "winget".into(),
                    command: Some("winget install OpenJS.NodeJS.LTS".into()),
                    url: Some("https://nodejs.org/".into()),
                    text: Some(
                        "Install Node.js LTS, then restart the shell / AgentHub so PATH refreshes."
                            .into(),
                    ),
                }
            } else {
                Remediation {
                    kind: "command".into(),
                    command: linux_runtime_command(RuntimeId::NodeJs),
                    url: Some("https://nodejs.org/".into()),
                    text: Some(
                        "Linux does not one-click install Node. Use your distro package manager or the official LTS, then fully quit and restart AgentHub so PATH refreshes. Distro nodejs is often older than 18; prefer https://nodejs.org/ when unsure. Unknown distros get the official URL instead of an apt-get guess."
                            .into(),
                    ),
                }
            }
        }
        RuntimeId::Npm => Remediation {
            kind: "hint".into(),
            command: None,
            url: Some("https://nodejs.org/".into()),
            text: Some(
                "npm usually ships with Node.js. If node works but npm does not, repair PATH or reinstall Node."
                    .into(),
            ),
        },
        RuntimeId::PowerShell => {
            if cfg!(windows) {
                Remediation {
                    kind: "hint".into(),
                    command: None,
                    url: Some(
                        "https://learn.microsoft.com/powershell/scripting/install/installing-powershell"
                            .into(),
                    ),
                    text: Some(
                        "Windows usually ships PowerShell 5.1 (System32). PowerShell 7 (pwsh) is optional but preferred for native install scripts. AgentHub does not auto-install PowerShell; install pwsh manually if needed. Check ExecutionPolicy if scripts fail."
                            .into(),
                    ),
                }
            } else {
                // Should not surface in doctor on macOS/Linux; keep a honest fallback.
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
        RuntimeId::Git => {
            if cfg!(target_os = "macos") {
                Remediation {
                    kind: "brew".into(),
                    command: Some("brew install git".into()),
                    url: Some("https://git-scm.com/downloads".into()),
                    text: Some(
                        "Install Git with Homebrew, then fully quit and restart AgentHub so PATH refreshes. Skills market / git URL install need git clone."
                            .into(),
                    ),
                }
            } else if cfg!(windows) {
                Remediation {
                    kind: "winget".into(),
                    command: Some("winget install --id Git.Git -e --source winget".into()),
                    url: Some("https://git-scm.com/downloads".into()),
                    text: Some(
                        "Install Git, then fully quit and restart AgentHub so PATH refreshes. Skills market / git URL install need git clone."
                            .into(),
                    ),
                }
            } else {
                Remediation {
                    kind: "command".into(),
                    command: linux_runtime_command(RuntimeId::Git),
                    url: Some("https://git-scm.com/downloads".into()),
                    text: Some(
                        "Linux does not one-click install Git. Use your distro package manager or the official download, then fully quit and restart AgentHub so PATH refreshes. Skills market / git URL install need git clone. Unknown distros get the official URL instead of an apt-get guess."
                            .into(),
                    ),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
