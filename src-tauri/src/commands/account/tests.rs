use super::*;
use tempfile::tempdir;

#[test]
fn add_apikey_is_redacted_without_live_reconcile() {
    let dir = tempdir().unwrap();
    let hub = AgentHub::open(Some(dir.path())).unwrap();
    let created =
        add_api_key_account_inner(&hub, "grok", "xai-super-secret-key", Some("work"), None)
            .unwrap();
    assert_eq!(created.label, "work");
    let creds = serde_json::to_string(&created.credentials).unwrap();
    assert!(!creds.contains("xai-super-secret-key"));
    assert!(creds.contains("***"));

    // Do not call `list_accounts_inner` here: the production list path
    // intentionally reconciles live adapter homes, which would make this
    // GUI command test depend on the host's real credentials.  The command
    // boundary's redaction contract is exercised directly on the returned
    // DTO instead.
    let listed = serde_json::to_string(&created.redacted().credentials).unwrap();
    assert!(!listed.contains("xai-super-secret-key"));
    assert!(listed.contains("***"));
}
