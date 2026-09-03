use super::{
    linux_family_from_os_release, linux_runtime_command_for, os_release_field, remediation_for,
    remediation_for_host, remediations_when_installer_missing, HostFamily, LinuxFamily,
    HOMEBREW_INSTALL_COMMAND, HOMEBREW_URL,
};
use crate::models::RuntimeId;

#[test]
fn os_release_field_reads_unquoted_and_quoted_values() {
    let text = "NAME=\"Ubuntu\"\nID=ubuntu\nID_LIKE=\"debian\"\n";
    assert_eq!(os_release_field(text, "ID").as_deref(), Some("ubuntu"));
    assert_eq!(os_release_field(text, "ID_LIKE").as_deref(), Some("debian"));
    assert_eq!(os_release_field(text, "VERSION_ID"), None);
}

#[test]
fn linux_family_from_os_release_classifies_common_distros() {
    assert_eq!(
        linux_family_from_os_release("ID=ubuntu\nID_LIKE=debian\n"),
        LinuxFamily::Debian
    );
    assert_eq!(
        linux_family_from_os_release("ID=fedora\n"),
        LinuxFamily::Fedora
    );
    assert_eq!(linux_family_from_os_release("ID=arch\n"), LinuxFamily::Arch);
    assert_eq!(
        linux_family_from_os_release("ID=alpine\n"),
        LinuxFamily::Alpine
    );
    assert_eq!(
        linux_family_from_os_release("ID=opensuse-tumbleweed\nID_LIKE=\"suse\"\n"),
        LinuxFamily::Suse
    );
    assert_eq!(
        linux_family_from_os_release("ID=gentoo\n"),
        LinuxFamily::Other
    );
}

#[test]
fn linux_runtime_command_matches_family_and_never_guesses_apt_for_other() {
    assert_eq!(
        linux_runtime_command_for(LinuxFamily::Debian, RuntimeId::Git).as_deref(),
        Some("sudo apt-get install -y git")
    );
    assert_eq!(
        linux_runtime_command_for(LinuxFamily::Fedora, RuntimeId::Git).as_deref(),
        Some("sudo dnf install -y git")
    );
    assert_eq!(
        linux_runtime_command_for(LinuxFamily::Arch, RuntimeId::Git).as_deref(),
        Some("sudo pacman -S --needed git")
    );
    assert_eq!(
        linux_runtime_command_for(LinuxFamily::Suse, RuntimeId::NodeJs).as_deref(),
        Some("sudo zypper install -y nodejs npm")
    );
    assert_eq!(
        linux_runtime_command_for(LinuxFamily::Alpine, RuntimeId::Git).as_deref(),
        Some("sudo apk add git")
    );
    assert_eq!(
        linux_runtime_command_for(LinuxFamily::Other, RuntimeId::NodeJs),
        None
    );
    assert_eq!(
        linux_runtime_command_for(LinuxFamily::Other, RuntimeId::Git),
        None
    );
}

#[test]
fn linux_nodejs_remediation_is_manual_not_winget() {
    if !cfg!(target_os = "linux") {
        return;
    }
    let rem = remediation_for(RuntimeId::NodeJs);
    assert_ne!(rem.kind, "winget");
    assert_ne!(rem.kind, "brew");
    assert_eq!(rem.kind, "command");
    if let Some(command) = rem.command.as_deref() {
        assert!(
            command.contains("apt-get")
                || command.contains("dnf")
                || command.contains("pacman")
                || command.contains("zypper")
                || command.contains("apk add"),
            "expected a distro package command, got {command}"
        );
        assert!(!command.to_ascii_lowercase().contains("winget"));
        assert!(!command.to_ascii_lowercase().contains("brew"));
        assert!(command.contains("nodejs") || command.contains("npm"));
    } else {
        assert_eq!(rem.url.as_deref(), Some("https://nodejs.org/"));
        assert!(rem
            .text
            .as_deref()
            .unwrap_or_default()
            .to_ascii_lowercase()
            .contains("official"));
    }
    assert_eq!(rem.url.as_deref(), Some("https://nodejs.org/"));
    assert!(rem
        .text
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase()
        .contains("linux"));
}

#[test]
fn linux_git_remediation_is_manual_not_winget() {
    if !cfg!(target_os = "linux") {
        return;
    }
    let rem = remediation_for(RuntimeId::Git);
    assert_eq!(rem.kind, "command");
    if let Some(command) = rem.command.as_deref() {
        assert!(command.contains("git"));
        assert!(!command.to_ascii_lowercase().contains("winget"));
        assert!(!command.to_ascii_lowercase().contains("brew"));
    }
    assert_eq!(rem.url.as_deref(), Some("https://git-scm.com/downloads"));
}

#[test]
fn macos_without_brew_does_not_suggest_brew_install() {
    for id in [RuntimeId::NodeJs, RuntimeId::Git] {
        let rem = remediation_for_host(id, HostFamily::Macos, false);
        assert_ne!(rem.kind, "brew", "{}", id.as_str());
        assert_ne!(rem.kind, "winget", "{}", id.as_str());
        let command = rem.command.as_deref().unwrap_or_default();
        assert_eq!(command, HOMEBREW_INSTALL_COMMAND);
        assert!(!command.contains("brew install"));
        assert_eq!(rem.url.as_deref(), Some(official_url(id)));
        let text = rem.text.as_deref().unwrap_or_default();
        assert!(text.contains("Homebrew"), "{text}");
        assert!(text.contains("一键安装") || text.contains("官网"), "{text}");
    }
}

#[test]
fn macos_with_brew_keeps_formula_install() {
    let node = remediation_for_host(RuntimeId::NodeJs, HostFamily::Macos, true);
    assert_eq!(node.kind, "brew");
    assert_eq!(node.command.as_deref(), Some("brew install node"));
    assert_eq!(node.url.as_deref(), Some("https://nodejs.org/"));

    let git = remediation_for_host(RuntimeId::Git, HostFamily::Macos, true);
    assert_eq!(git.kind, "brew");
    assert_eq!(git.command.as_deref(), Some("brew install git"));
    assert_eq!(git.url.as_deref(), Some("https://git-scm.com/downloads"));
}

#[test]
fn windows_without_winget_does_not_suggest_winget_install() {
    let rem = remediation_for_host(RuntimeId::NodeJs, HostFamily::Windows, false);
    assert_ne!(rem.kind, "winget");
    assert_ne!(rem.kind, "brew");
    assert!(rem.command.is_none());
    assert_eq!(rem.url.as_deref(), Some("https://nodejs.org/"));
    assert!(rem.text.as_deref().unwrap_or_default().contains("winget"));
}

#[test]
fn brew_channel_missing_remediations_include_homebrew_and_official_pages() {
    let remediations = remediations_when_installer_missing("brew", RuntimeId::NodeJs);
    assert!(remediations
        .iter()
        .any(|r| r.url.as_deref() == Some(HOMEBREW_URL)));
    assert!(remediations
        .iter()
        .any(|r| r.url.as_deref() == Some("https://nodejs.org/")));
    assert!(remediations
        .iter()
        .any(|r| r.command.as_deref() == Some(HOMEBREW_INSTALL_COMMAND)));
    assert!(remediations
        .iter()
        .all(|r| r.kind != "brew" && r.kind != "winget"));
    assert!(remediations
        .iter()
        .all(|r| r.command.as_deref() != Some("brew install node")));
}

fn official_url(id: RuntimeId) -> &'static str {
    match id {
        RuntimeId::Git => "https://git-scm.com/downloads",
        _ => "https://nodejs.org/",
    }
}
