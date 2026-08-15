use super::*;
use agenthub_core::AgentHub;

#[test]
fn install_rejects_unknown_runtime() {
    let dir = tempfile::tempdir().unwrap();
    let hub = AgentHub::open(Some(dir.path())).unwrap();
    let err = install(&hub, "python", "", OutputFormat::Quiet).unwrap_err();
    assert_eq!(err.code(), "invalid_arg");
    assert!(err.to_string().contains("nodejs"));
}

#[test]
fn list_quiet_succeeds() {
    let dir = tempfile::tempdir().unwrap();
    let hub = AgentHub::open(Some(dir.path())).unwrap();
    list(&hub, OutputFormat::Quiet).unwrap();
}

#[test]
fn powershell_install_is_unsupported() {
    let dir = tempfile::tempdir().unwrap();
    let hub = AgentHub::open(Some(dir.path())).unwrap();
    let err = install(&hub, "powershell", "", OutputFormat::Quiet).unwrap_err();
    assert_eq!(err.code(), "unsupported");
}
