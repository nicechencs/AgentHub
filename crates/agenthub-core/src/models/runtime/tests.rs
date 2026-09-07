use super::*;

#[test]
fn runtime_id_parse_aliases() {
    assert_eq!(RuntimeId::parse("nodejs"), Some(RuntimeId::NodeJs));
    assert_eq!(RuntimeId::parse("node"), Some(RuntimeId::NodeJs));
    assert_eq!(RuntimeId::parse("Node"), Some(RuntimeId::NodeJs));
    assert_eq!(RuntimeId::parse("npm"), Some(RuntimeId::Npm));
    assert_eq!(RuntimeId::parse("  NPM  "), Some(RuntimeId::Npm));
    assert_eq!(RuntimeId::parse("powershell"), Some(RuntimeId::PowerShell));
    assert_eq!(RuntimeId::parse("pwsh"), Some(RuntimeId::PowerShell));
    assert_eq!(RuntimeId::parse("PowerShell"), Some(RuntimeId::PowerShell));
    assert_eq!(RuntimeId::parse("git"), Some(RuntimeId::Git));
    assert_eq!(RuntimeId::parse("  Git  "), Some(RuntimeId::Git));
}

#[test]
fn runtime_id_parse_rejects_invalid() {
    assert_eq!(RuntimeId::parse(""), None);
    assert_eq!(RuntimeId::parse("python"), None);
    assert_eq!(RuntimeId::parse("bash"), None);
    assert_eq!(RuntimeId::parse("node.js"), None);
    assert_eq!(RuntimeId::parse("github"), None);
}

#[test]
fn runtime_id_as_str_roundtrip() {
    for id in RuntimeId::ALL {
        let s = id.as_str();
        assert_eq!(RuntimeId::parse(s), Some(id));
    }
}
