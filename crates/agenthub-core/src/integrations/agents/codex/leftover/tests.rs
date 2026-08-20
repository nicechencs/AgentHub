use super::*;

const LEFTOVER: &str = r#"model_provider = "agenthub_grok_bridge"
model = "grok-4"
model_reasoning_effort = "high"
disable_response_storage = true
preferred_auth_method = "apikey"

[model_providers.agenthub_grok_bridge]
name = "AgentHub Grok Route"
base_url = "http://127.0.0.1:43121/v1"
wire_api = "responses"

[mcp_servers.keep]
command = "keep"
"#;

#[test]
fn strip_drops_agenthub_bridge_and_apikey_pref_but_keeps_mcp() {
    let mut doc = LEFTOVER.parse::<DocumentMut>().unwrap();
    assert!(toml_is_bridge_leftover(LEFTOVER));
    assert!(strip_bridge_leftovers_in_doc(&mut doc));
    let stored = doc.to_string();
    assert!(!stored.contains("model_provider"));
    assert!(!stored.contains("preferred_auth_method"));
    assert!(!stored.contains("agenthub_grok_bridge"));
    assert!(!stored.contains("127.0.0.1"));
    assert!(stored.contains("[mcp_servers.keep]"));
    assert!(stored.contains("model = \"grok-4\""));
    assert!(!toml_is_bridge_leftover(&stored));
}

#[test]
fn strip_is_noop_for_clean_official_toml() {
    let official = "model = \"gpt-5.1-codex\"\n";
    let mut doc = official.parse::<DocumentMut>().unwrap();
    assert!(!toml_is_bridge_leftover(official));
    assert!(!strip_bridge_leftovers_in_doc(&mut doc));
    assert_eq!(doc.to_string(), official);
}

#[test]
fn strip_keeps_user_custom_remote_provider() {
    let custom = r#"model_provider = "custom"

[model_providers.custom]
base_url = "https://relay.example.com/v1"
"#;
    let mut doc = custom.parse::<DocumentMut>().unwrap();
    assert!(!toml_is_bridge_leftover(custom));
    assert!(!strip_bridge_leftovers_in_doc(&mut doc));
    assert!(doc.to_string().contains("[model_providers.custom]"));
}

#[test]
fn backup_is_bridge_leftover_reads_config_toml() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("config.toml"), LEFTOVER).unwrap();
    let leftover = BackupRecord {
        id: "bk-leftover".into(),
        agent_id: Some(AgentId::Codex),
        kind: crate::models::BackupKind::AutoSwitch,
        path: dir.path().display().to_string(),
        files: vec!["config.toml".into(), "auth.json".into()],
        size: 1,
        note: None,
        created_at: "now".into(),
    };
    assert!(backup_is_bridge_leftover(&leftover));

    std::fs::write(dir.path().join("config.toml"), "model = \"gpt-5\"\n").unwrap();
    assert!(!backup_is_bridge_leftover(&leftover));
}

#[test]
fn slug_detects_agenthub_bridge_only() {
    assert!(is_agenthub_bridge_slug("agenthub_grok_bridge"));
    assert!(is_agenthub_bridge_slug("agenthub_kimi_bridge"));
    assert!(!is_agenthub_bridge_slug("custom"));
    assert!(!is_agenthub_bridge_slug("agenthub_notes"));
}

#[test]
fn live_config_is_bridge_leftover_reads_codex_home() {
    let _lock = super::lock_codex_home();
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    let codex = home.join(".codex");
    std::fs::create_dir_all(&codex).unwrap();
    std::fs::write(codex.join("config.toml"), LEFTOVER).unwrap();
    let prev = std::env::var_os("HOME");
    std::env::set_var("HOME", &home);
    let leftover = live_config_is_bridge_leftover();
    std::fs::write(codex.join("config.toml"), "model = \"gpt-5\"\n").unwrap();
    let clean = live_config_is_bridge_leftover();
    match prev {
        Some(value) => std::env::set_var("HOME", value),
        None => std::env::remove_var("HOME"),
    }
    assert!(leftover);
    assert!(!clean);
}
