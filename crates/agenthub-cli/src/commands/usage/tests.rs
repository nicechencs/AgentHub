use super::*;
use agenthub_core::AgentHub;

#[test]
fn parse_days_rejects_zero() {
    assert_eq!(parse_days(0).unwrap_err().code(), "invalid_arg");
    assert_eq!(parse_days(7).unwrap(), 7);
    assert_eq!(parse_days(30).unwrap(), 30);
}

#[test]
fn models_and_health_quiet_succeed() {
    let dir = tempfile::tempdir().unwrap();
    let hub = AgentHub::open(Some(dir.path())).unwrap();
    models(&hub, OutputFormat::Quiet, None).unwrap();
    health(&hub, OutputFormat::Quiet).unwrap();
    stats(&hub, 7, OutputFormat::Quiet, None, None).unwrap();
}
