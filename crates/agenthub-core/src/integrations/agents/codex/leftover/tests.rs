use super::*;
use crate::models::{AgentId, Provider};

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
fn strip_drops_agenthub_bridge_grok_model_and_apikey_pref_but_keeps_mcp() {
    let mut doc = LEFTOVER.parse::<DocumentMut>().unwrap();
    assert!(toml_is_bridge_leftover(LEFTOVER));
    assert!(strip_bridge_leftovers_in_doc(&mut doc));
    let stored = doc.to_string();
    assert!(!stored.contains("model_provider"));
    assert!(!stored.contains("preferred_auth_method"));
    assert!(!stored.contains("agenthub_grok_bridge"));
    assert!(!stored.contains("127.0.0.1"));
    assert!(!stored.contains("grok-4"));
    assert!(!stored.contains("grok-4.5"));
    assert!(!stored.contains("model_reasoning_effort"));
    assert!(stored.contains("[mcp_servers.keep]"));
    assert!(stored.contains("disable_response_storage"));
    assert!(!toml_is_bridge_leftover(&stored));
}

#[test]
fn strip_drops_leftover_grok_model_even_when_bridge_slugs_are_empty() {
    let leftover = "model = \"grok-4.5\"\nmodel_reasoning_effort = \"high\"\ndisable_response_storage = true\n";
    let mut doc = leftover.parse::<DocumentMut>().unwrap();
    assert!(!toml_is_bridge_leftover(leftover));
    assert!(strip_bridge_leftovers_in_doc(&mut doc));
    let stored = doc.to_string();
    assert!(!stored.contains("grok-4"));
    assert!(!stored.contains("grok-4.5"));
    assert!(!stored.contains("127.0.0.1"));
    assert!(!stored.contains("model_reasoning_effort"));
    assert!(stored.contains("disable_response_storage"));
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
fn strip_keeps_official_gpt_model_and_reasoning() {
    let official = "model = \"gpt-5.1-codex\"\nmodel_reasoning_effort = \"high\"\ndisable_response_storage = true\n";
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
fn official_oauth_clears_openrouter_env_key_provider_without_inventing_a_model() {
    let leftover = r#"model_provider = "openrouter"
model = "stealth/ox-alpha"
review_model = "stealth/ox-alpha"
model_reasoning_effort = "xhigh"
preferred_auth_method = "apikey"

[model_providers.openrouter]
name = "OpenRouter"
base_url = "https://openrouter.ai/api/v1"
env_key = "OPENROUTER_API_KEY"
wire_api = "responses"
"#;
    let mut doc = leftover.parse::<DocumentMut>().unwrap();
    assert!(strip_env_key_provider_leftovers_in_doc(&mut doc));
    let stored = doc.to_string();
    assert!(!stored.contains("model_provider ="), "{stored}");
    assert!(!stored.contains("preferred_auth_method"), "{stored}");
    assert!(!stored.contains("stealth/ox-alpha"), "{stored}");
    assert!(!stored.contains("model_reasoning_effort"), "{stored}");
    assert!(stored.contains("[model_providers.openrouter]"), "{stored}");
    assert!(stored.contains("OPENROUTER_API_KEY"), "{stored}");
}

#[test]
fn official_oauth_keeps_custom_provider_without_env_key() {
    let custom = r#"model_provider = "custom"
model = "gpt-5.1-codex"

[model_providers.custom]
base_url = "https://relay.example.com/v1"
"#;
    let mut doc = custom.parse::<DocumentMut>().unwrap();
    assert!(!strip_env_key_provider_leftovers_in_doc(&mut doc));
    assert_eq!(doc.to_string(), custom);
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
fn strip_is_noop_for_official_gpt_apikey_pref_without_leftover_slug() {
    let official = "model = \"gpt-5.1-codex\"\npreferred_auth_method = \"apikey\"\n";
    let mut doc = official.parse::<DocumentMut>().unwrap();
    assert!(!toml_is_bridge_leftover(official));
    assert!(!strip_bridge_leftovers_in_doc(&mut doc));
    assert_eq!(doc.to_string(), official);
}

#[test]
fn strip_keeps_effort_when_model_key_is_missing() {
    let only_effort = "model_reasoning_effort = \"high\"\n";
    let mut doc = only_effort.parse::<DocumentMut>().unwrap();
    assert!(!toml_is_bridge_leftover(only_effort));
    assert!(!strip_bridge_leftovers_in_doc(&mut doc));
    assert_eq!(doc.to_string(), only_effort);
}

#[test]
fn strip_keeps_custom_provider_grok_model_and_effort() {
    let custom =
        "model_provider = \"custom\"\nmodel = \"grok-4\"\nmodel_reasoning_effort = \"high\"\n";
    let mut doc = custom.parse::<DocumentMut>().unwrap();
    assert!(!toml_is_bridge_leftover(custom));
    assert!(!strip_bridge_leftovers_in_doc(&mut doc));
    assert_eq!(doc.to_string(), custom);
}

#[test]
fn strip_drops_leftover_claude_model_and_effort() {
    let leftover = "model = \"claude-sonnet-4-20250514\"\nmodel_reasoning_effort = \"high\"\ndisable_response_storage = true\n";
    let mut doc = leftover.parse::<DocumentMut>().unwrap();
    assert!(strip_bridge_leftovers_in_doc(&mut doc));
    let stored = doc.to_string();
    assert!(!stored.contains("claude-"));
    assert!(!stored.contains("model_reasoning_effort"));
    assert!(stored.contains("disable_response_storage"));
}

#[test]
fn strip_drops_leftover_kimi_model() {
    let leftover = "model = \"kimi-k2.5\"\ndisable_response_storage = true\n";
    let mut doc = leftover.parse::<DocumentMut>().unwrap();
    assert!(strip_bridge_leftovers_in_doc(&mut doc));
    let stored = doc.to_string();
    assert!(!stored.contains("kimi-k2.5"));
    assert!(stored.contains("disable_response_storage"));
}

#[test]
fn strip_keeps_custom_provider_claude_model() {
    let custom = "model_provider = \"custom\"\nmodel = \"claude-sonnet-4-20250514\"\nmodel_reasoning_effort = \"high\"\n";
    let mut doc = custom.parse::<DocumentMut>().unwrap();
    assert!(!toml_is_bridge_leftover(custom));
    assert!(!strip_bridge_leftovers_in_doc(&mut doc));
    assert_eq!(doc.to_string(), custom);
}

#[test]
fn strip_clears_apikey_pref_when_only_provider_table_slug_remains() {
    let leftover = r#"preferred_auth_method = "apikey"

[model_providers.agenthub_grok_bridge]
base_url = "http://127.0.0.1:43121/v1"
"#;
    let mut doc = leftover.parse::<DocumentMut>().unwrap();
    assert!(toml_is_bridge_leftover(leftover));
    assert!(strip_bridge_leftovers_in_doc(&mut doc));
    let stored = doc.to_string();
    assert!(!stored.contains("preferred_auth_method"));
    assert!(!stored.contains("agenthub_grok_bridge"));
}

#[test]
fn strip_drops_bridge_table_but_keeps_grok_model_under_custom_provider() {
    let mixed = r#"model_provider = "custom"
model = "grok-4"
model_reasoning_effort = "high"

[model_providers.custom]
base_url = "https://relay.example.com/v1"

[model_providers.agenthub_grok_bridge]
base_url = "http://127.0.0.1:43121/v1"
"#;
    let mut doc = mixed.parse::<DocumentMut>().unwrap();
    assert!(toml_is_bridge_leftover(mixed));
    assert!(strip_bridge_leftovers_in_doc(&mut doc));
    let stored = doc.to_string();
    assert!(stored.contains("model_provider = \"custom\""));
    assert!(stored.contains("model = \"grok-4\""));
    assert!(stored.contains("model_reasoning_effort"));
    assert!(stored.contains("[model_providers.custom]"));
    assert!(!stored.contains("agenthub_grok_bridge"));
    assert!(!stored.contains("127.0.0.1"));
}

#[test]
fn active_openai_provider_is_not_leftover_when_dead_bridge_table_remains() {
    let mixed = r#"model_provider = "OpenAI"
model = "gpt-5.5"

[model_providers.OpenAI]
name = "OpenAI"
base_url = "https://mytokens.cc/v1"

[model_providers.agenthub_grok_bridge]
base_url = "http://127.0.0.1:43121/v1"
"#;
    assert!(toml_is_bridge_leftover(mixed));
    assert!(!toml_active_provider_is_bridge_leftover(mixed));
    let leftover_row = Provider {
        id: "codex-live-mixed".into(),
        agent_id: AgentId::Codex,
        name: "OpenAI".into(),
        settings_config: serde_json::json!({
            "format": "toml",
            "content": mixed,
        }),
        meta: serde_json::json!({ "source": "live" }),
        is_current: false,
        created_at: "t0".into(),
        updated_at: "t0".into(),
    };
    assert!(!provider_is_bridge_leftover(&leftover_row));
}

#[test]
fn live_import_hint_uses_provider_name_and_model() {
    let hint = crate::integrations::agents::codex::live_import_hint(&serde_json::json!({
        "format": "toml",
        "content": "model_provider = \"OpenAI\"\nmodel = \"gpt-5.5\"\n\n[model_providers.OpenAI]\nname = \"OpenAI\"\nbase_url = \"https://mytokens.cc/v1\"\n",
    }))
    .expect("hint");
    assert!(hint.label.contains("OpenAI"));
    assert!(hint.label.contains("gpt-5.5"));
    assert_eq!(hint.preset, "openai-compat");
}

#[test]
fn live_config_is_bridge_leftover_reads_codex_home() {
    let _lock = super::lock_codex_home();
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    let codex = home.join(".codex");
    std::fs::create_dir_all(&codex).unwrap();
    std::fs::write(codex.join("config.toml"), LEFTOVER).unwrap();
    let prev = std::env::var_os("CODEX_HOME");
    std::env::set_var("CODEX_HOME", &codex);
    let leftover = live_config_is_bridge_leftover();
    std::fs::write(codex.join("config.toml"), "model = \"gpt-5\"\n").unwrap();
    let clean = live_config_is_bridge_leftover();
    match prev {
        Some(value) => std::env::set_var("CODEX_HOME", value),
        None => std::env::remove_var("CODEX_HOME"),
    }
    assert!(leftover);
    assert!(!clean);
}

#[test]
fn live_import_hint_falls_back_to_host_when_provider_name_missing() {
    let hint = crate::integrations::agents::codex::live_import_hint(&serde_json::json!({
        "format": "toml",
        "content": "model_provider = \"custom\"\nmodel = \"gpt-5.5\"\n\n[model_providers.custom]\nbase_url = \"https://mytokens.cc/v1\"\n",
    }))
    .expect("hint");
    assert!(hint.label.contains("mytokens.cc"), "{}", hint.label);
    assert!(hint.label.contains("gpt-5.5"), "{}", hint.label);
    assert_eq!(hint.preset, "openai-compat");
}
