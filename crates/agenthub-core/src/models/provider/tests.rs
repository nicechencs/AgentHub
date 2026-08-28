use super::*;
use serde_json::json;

#[test]
fn config_format_serde_lowercase() {
    let j = serde_json::to_string(&ConfigFormat::Json).unwrap();
    let t = serde_json::to_string(&ConfigFormat::Toml).unwrap();
    assert_eq!(j, "\"json\"");
    assert_eq!(t, "\"toml\"");
    assert_eq!(
        serde_json::from_str::<ConfigFormat>("\"json\"").unwrap(),
        ConfigFormat::Json
    );
    assert_eq!(
        serde_json::from_str::<ConfigFormat>("\"toml\"").unwrap(),
        ConfigFormat::Toml
    );
}

#[test]
fn provider_preset_serde_camel_case() {
    let p = ProviderPreset {
        agent: AgentId::Claude,
        id: "anthropic".into(),
        label: "Anthropic 官方".into(),
        format: ConfigFormat::Json,
        template: "{}".into(),
    };
    let v = serde_json::to_value(&p).unwrap();
    assert_eq!(v["agent"], "claude");
    assert_eq!(v["id"], "anthropic");
    assert_eq!(v["label"], "Anthropic 官方");
    assert_eq!(v["format"], "json");
    assert_eq!(v["template"], "{}");
    assert!(v.get("template").is_some());
}

#[test]
fn provider_serde_camel_case_and_fields() {
    let p = Provider {
        id: "p1".into(),
        agent_id: AgentId::Codex,
        name: "Corp Relay".into(),
        settings_config: json!({"base_url": "https://x", "api_key": "sk-secret"}),
        meta: json!({"preset": "openai-compatible"}),
        is_current: true,
        created_at: "2026-01-01 00:00:00".into(),
        updated_at: "2026-01-02 00:00:00".into(),
    };
    let v = serde_json::to_value(&p).unwrap();
    assert_eq!(v["id"], "p1");
    assert_eq!(v["agentId"], "codex");
    assert_eq!(v["name"], "Corp Relay");
    assert_eq!(v["settingsConfig"]["base_url"], "https://x");
    assert_eq!(v["settingsConfig"]["api_key"], "sk-secret");
    assert_eq!(v["meta"]["preset"], "openai-compatible");
    assert_eq!(v["isCurrent"], true);
    assert_eq!(v["createdAt"], "2026-01-01 00:00:00");
    assert_eq!(v["updatedAt"], "2026-01-02 00:00:00");
}

#[test]
fn provider_input_serde_no_timestamps() {
    let input = ProviderInput {
        id: "p1".into(),
        agent_id: AgentId::Claude,
        name: "Relay".into(),
        settings_config: json!({"base_url": "https://x"}),
        meta: json!({}),
        is_current: false,
    };
    let v = serde_json::to_value(&input).unwrap();
    assert_eq!(v["id"], "p1");
    assert_eq!(v["agentId"], "claude");
    assert_eq!(v["name"], "Relay");
    assert_eq!(v["settingsConfig"]["base_url"], "https://x");
    assert_eq!(v["meta"], json!({}));
    assert_eq!(v["isCurrent"], false);
    assert!(v.get("createdAt").is_none());
    assert!(v.get("updatedAt").is_none());
    let back: ProviderInput = serde_json::from_value(v).unwrap();
    assert_eq!(back, input);
}

#[test]
fn provider_redacted_masks_nested_secrets() {
    let p = Provider {
        id: "p1".into(),
        agent_id: AgentId::Grok,
        name: "xAI".into(),
        settings_config: json!({
            "api_key": "secret",
            "nested": { "TOKEN": "t", "base_url": "https://x" }
        }),
        meta: json!({"authorization": "Bearer x", "label": "ok"}),
        is_current: false,
        created_at: "t0".into(),
        updated_at: "t1".into(),
    };
    let r = p.redacted();
    assert_eq!(r.settings_config["api_key"], "***");
    assert_eq!(r.settings_config["nested"]["TOKEN"], "***");
    assert_eq!(r.settings_config["nested"]["base_url"], "https://x");
    assert_eq!(r.meta["authorization"], "***");
    assert_eq!(r.meta["label"], "ok");
    // Original unchanged.
    assert_eq!(p.settings_config["api_key"], "secret");
}

#[test]
fn provider_redacted_masks_opaque_toml_body() {
    let p = Provider {
        id: "p-toml".into(),
        agent_id: AgentId::Grok,
        name: "xAI".into(),
        settings_config: json!({
            "format": "toml",
            "content": "model = 'grok'\napi_key = 'xai-secret'\n"
        }),
        meta: json!({}),
        is_current: true,
        created_at: "t0".into(),
        updated_at: "t1".into(),
    };

    let redacted = p.redacted();
    assert_eq!(redacted.settings_config["format"], "toml");
    let content = redacted.settings_config["content"].as_str().expect("content");
    assert!(content.contains("model = 'grok'"), "{content}");
    assert!(content.contains("api_key = \"***\""), "{content}");
    assert!(!content.contains("xai-secret"), "{content}");
    assert_eq!(redacted.meta["secretTail"], "**cret");
    let hash = redacted.meta["secretHash"].as_str().expect("hash");
    assert_eq!(hash.len(), 64);
    assert!(!hash.contains("xai-secret"));
    assert!(!redacted.settings_config.to_string().contains("xai-secret"));
    assert!(p.settings_config["content"]
        .as_str()
        .unwrap()
        .contains("xai-secret"));
}

#[test]
fn provider_redacted_recovers_secret_tail_from_stored_name() {
    let p = Provider {
        id: "p-name".into(),
        agent_id: AgentId::Grok,
        name: "xai-••••6aa9 (API Key)".into(),
        settings_config: json!({
            "format": "toml",
            "content": "[model.\"grok\"]\napi_key = \"***\"\n"
        }),
        meta: json!({}),
        is_current: false,
        created_at: "t0".into(),
        updated_at: "t1".into(),
    };
    let redacted = p.redacted();
    assert_eq!(redacted.meta["secretTail"], "**6aa9");
}

#[test]
fn known_meta_keys_are_read_through_accessors() {
    let p = Provider {
        id: "p1".into(),
        agent_id: AgentId::Grok,
        name: "xAI".into(),
        settings_config: json!({"format": "toml", "content": "x"}),
        meta: json!({
            "official": true,
            "preset": "xai",
            "source": "live",
            "generatedBy": "adapter",
            "provider": "xai",
            "adapterRuleId": "rule-1",
            "keepUnknown": "ok",
        }),
        is_current: true,
        created_at: "t0".into(),
        updated_at: "t1".into(),
    };
    assert_eq!(p.official(), Some(true));
    assert_eq!(p.preset(), Some("xai"));
    assert_eq!(p.source(), Some("live"));
    assert_eq!(p.generated_by(), Some("adapter"));
    assert_eq!(p.meta_provider(), Some("xai"));
    assert_eq!(p.adapter_rule_id(), Some("rule-1"));
    assert_eq!(p.settings_format(), Some("toml"));
    assert_eq!(p.meta["keepUnknown"], "ok");
}

#[test]
fn missing_meta_keys_are_none() {
    let p = Provider {
        id: "p2".into(),
        agent_id: AgentId::Claude,
        name: "Relay".into(),
        settings_config: json!({"base_url": "https://x"}),
        meta: json!({}),
        is_current: false,
        created_at: "t0".into(),
        updated_at: "t1".into(),
    };
    assert_eq!(p.official(), None);
    assert_eq!(p.preset(), None);
    assert_eq!(p.source(), None);
    assert_eq!(p.generated_by(), None);
    assert_eq!(p.settings_format(), None);
}
