use super::{linux_family_from_os_release, os_release_field, remediation_for, LinuxFamily};
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
    assert_eq!(
        linux_family_from_os_release("ID=arch\n"),
        LinuxFamily::Arch
    );
    assert_eq!(
        linux_family_from_os_release("ID=alpine\n"),
        LinuxFamily::Other
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
    let command = rem.command.expect("linux node remediation must be copyable");
    assert!(
        command.contains("apt-get") || command.contains("dnf") || command.contains("pacman"),
        "expected a distro package command, got {command}"
    );
    assert!(command.contains("nodejs"));
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
    let command = rem.command.expect("linux git remediation must be copyable");
    assert!(command.contains("git"));
    assert!(!command.to_ascii_lowercase().contains("winget"));
    assert_eq!(rem.url.as_deref(), Some("https://git-scm.com/downloads"));
}
