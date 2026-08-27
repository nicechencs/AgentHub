use super::*;
use agenthub_core::models::AgentId;
use serde_json::json;
use tempfile::tempdir;

fn hub_tmp() -> (tempfile::TempDir, AgentHub) {
    let dir = tempdir().unwrap();
    let hub = AgentHub::open(Some(dir.path())).unwrap();
    (dir, hub)
}

#[test]
fn parse_agent_accepts_catalog_and_rejects_unknown() {
    assert_eq!(parse_agent("claude").unwrap(), AgentId::Claude);
    assert_eq!(parse_agent("cursor").unwrap(), AgentId::Cursor);
    assert_eq!(parse_agent("cursor-agent").unwrap(), AgentId::Cursor);
    let err = parse_agent("not-an-agent").unwrap_err();
    assert!(err.contains("invalid agent id"), "{err}");
    assert!(err.contains("cursor"), "{err}");
}

#[test]
fn list_empty_is_redacted_vec() {
    let (_dir, hub) = hub_tmp();
    let items = list_providers_inner(&hub, Some("claude")).unwrap();
    assert!(items.is_empty());
}

#[test]
fn upsert_roundtrip_redacts_secrets_and_preserves_on_marker() {
    let (_dir, hub) = hub_tmp();
    let created = upsert_provider_inner(
        &hub,
        ProviderInput {
            id: "p-secret".into(),
            agent_id: AgentId::Claude,
            name: "Relay".into(),
            settings_config: json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://relay.example.com",
                    "ANTHROPIC_AUTH_TOKEN": "sk-live-secret"
                }
            }),
            meta: json!({ "preset": "anthropic-compatible", "token": "meta-secret" }),
            is_current: false,
        },
    )
    .unwrap();

    assert_eq!(
        created.settings_config["env"]["ANTHROPIC_AUTH_TOKEN"],
        "***"
    );
    assert_eq!(created.meta["token"], "***");
    assert_eq!(
        created.settings_config["env"]["ANTHROPIC_BASE_URL"],
        "https://relay.example.com"
    );

    // Client re-sends redacted payload (read → edit name → save).
    let updated = upsert_provider_inner(
        &hub,
        ProviderInput {
            id: "p-secret".into(),
            agent_id: AgentId::Claude,
            name: "Relay Renamed".into(),
            settings_config: created.settings_config.clone(),
            meta: created.meta.clone(),
            is_current: false,
        },
    )
    .unwrap();
    assert_eq!(updated.name, "Relay Renamed");
    assert_eq!(
        updated.settings_config["env"]["ANTHROPIC_AUTH_TOKEN"],
        "***"
    );

    // Stored secret must still be the original (unredacted load).
    let stored = hub.providers().get("p-secret", None).unwrap();
    assert_eq!(
        stored.settings_config["env"]["ANTHROPIC_AUTH_TOKEN"],
        "sk-live-secret"
    );
    assert_eq!(stored.meta["token"], "meta-secret");
}

#[test]
fn upsert_preserves_codex_auth_openai_api_key_on_marker() {
    let (_dir, hub) = hub_tmp();
    let created = upsert_provider_inner(
        &hub,
        ProviderInput {
            id: "p-codex-auth".into(),
            agent_id: AgentId::Codex,
            name: "Codex Relay".into(),
            settings_config: json!({
                "format": "toml",
                "content": "model = \"gpt-5\"\n",
                "auth": { "OPENAI_API_KEY": "sk-codex-live" }
            }),
            meta: json!({ "preset": "openai-compatible" }),
            is_current: false,
        },
    )
    .unwrap();
    assert_eq!(created.settings_config["auth"]["OPENAI_API_KEY"], "***");

    let updated = upsert_provider_inner(
        &hub,
        ProviderInput {
            id: "p-codex-auth".into(),
            agent_id: AgentId::Codex,
            name: "Codex Relay 2".into(),
            settings_config: json!({
                "format": "toml",
                "content": "***",
                "auth": { "OPENAI_API_KEY": "***" }
            }),
            meta: created.meta.clone(),
            is_current: false,
        },
    )
    .unwrap();
    assert_eq!(updated.name, "Codex Relay 2");

    let stored = hub.providers().get("p-codex-auth", None).unwrap();
    assert_eq!(
        stored.settings_config["auth"]["OPENAI_API_KEY"],
        "sk-codex-live"
    );
    assert!(stored.settings_config["content"]
        .as_str()
        .unwrap()
        .contains("gpt-5"));
}

#[test]
fn upsert_preserves_opaque_toml_when_content_is_marker() {
    let (_dir, hub) = hub_tmp();
    let created = upsert_provider_inner(
        &hub,
        ProviderInput {
            id: "p-toml".into(),
            agent_id: AgentId::Grok,
            name: "Grok Relay".into(),
            settings_config: json!({
                "format": "toml",
                "content": "model = \"grok\"\napi_key = \"xai-secret\"\n"
            }),
            meta: json!({ "preset": "xai" }),
            is_current: false,
        },
    )
    .unwrap();
    let listed = created.settings_config["content"].as_str().unwrap();
    assert!(listed.contains("model = \"grok\""), "{listed}");
    assert!(!listed.contains("xai-secret"), "{listed}");
    assert!(listed.contains("***"), "{listed}");

    let _ = upsert_provider_inner(
        &hub,
        ProviderInput {
            id: "p-toml".into(),
            agent_id: AgentId::Grok,
            name: "Grok Relay 2".into(),
            settings_config: json!({ "format": "toml", "content": "***" }),
            meta: json!({ "preset": "xai" }),
            is_current: false,
        },
    )
    .unwrap();

    let stored = hub.providers().get("p-toml", None).unwrap();
    assert!(stored.settings_config["content"]
        .as_str()
        .unwrap()
        .contains("xai-secret"));
}

#[test]
fn stored_api_key_for_remote_models_resolves_without_network() {
    let (_dir, hub) = hub_tmp();

    let missing = stored_api_key_for_remote_models(&hub, "no-such-provider").unwrap_err();
    assert!(
        missing.to_lowercase().contains("not found") || missing.contains("provider"),
        "{missing}"
    );
    assert!(!missing.contains("sk-"));

    upsert_provider_inner(
        &hub,
        ProviderInput {
            id: "p-no-key".into(),
            agent_id: AgentId::Claude,
            name: "No Key".into(),
            settings_config: json!({
                "env": { "ANTHROPIC_BASE_URL": "https://relay.example.com" }
            }),
            meta: json!({}),
            is_current: false,
        },
    )
    .unwrap();
    let no_key = stored_api_key_for_remote_models(&hub, "p-no-key").unwrap_err();
    assert!(no_key.to_lowercase().contains("missing"), "{no_key}");
    assert!(!no_key.contains("sk-"));

    let created = upsert_provider_inner(
        &hub,
        ProviderInput {
            id: "p-secret".into(),
            agent_id: AgentId::Claude,
            name: "Relay".into(),
            settings_config: json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://relay.example.com",
                    "ANTHROPIC_AUTH_TOKEN": "sk-live-secret"
                }
            }),
            meta: json!({}),
            is_current: false,
        },
    )
    .unwrap();
    assert_eq!(
        created.settings_config["env"]["ANTHROPIC_AUTH_TOKEN"],
        "***"
    );

    let stored = hub.providers().get("p-secret", None).unwrap();
    assert!(agenthub_core::utils::redact::api_key_secret(&stored.settings_config).is_some());
    let gui = get_provider_inner(&hub, "p-secret", None).unwrap();
    assert_eq!(gui.settings_config["env"]["ANTHROPIC_AUTH_TOKEN"], "***");
    assert!(stored_api_key_for_remote_models(&hub, "p-secret").is_ok());
}

#[test]
fn delete_and_invalid_agent_map_errors() {
    let (_dir, hub) = hub_tmp();
    let err = delete_provider_inner(&hub, "nope", "x").unwrap_err();
    assert!(err.contains("invalid agent"));

    upsert_provider_inner(
        &hub,
        ProviderInput {
            id: "p1".into(),
            agent_id: AgentId::Claude,
            name: "A".into(),
            settings_config: json!({}),
            meta: json!({}),
            is_current: false,
        },
    )
    .unwrap();
    delete_provider_inner(&hub, "claude", "p1").unwrap();
    let err = get_provider_inner(&hub, "p1", Some("claude")).unwrap_err();
    assert!(err.to_lowercase().contains("not found") || err.contains("provider"));
}

#[test]
fn switch_preview_is_read_only() {
    let (_dir, hub) = hub_tmp();
    upsert_provider_inner(
        &hub,
        ProviderInput {
            id: "cur".into(),
            agent_id: AgentId::Claude,
            name: "Current".into(),
            settings_config: json!({ "env": {} }),
            meta: json!({}),
            is_current: true,
        },
    )
    .unwrap();
    upsert_provider_inner(
        &hub,
        ProviderInput {
            id: "tgt".into(),
            agent_id: AgentId::Claude,
            name: "Target".into(),
            settings_config: json!({ "env": {} }),
            meta: json!({}),
            is_current: false,
        },
    )
    .unwrap();

    let preview = switch_provider_preview_inner(&hub, "claude", "tgt").unwrap();
    assert!(
        preview.backfill_summary.contains("回存") || preview.backfill_summary.contains("Current")
    );
    assert!(preview.backup_path.contains("live"));
    assert!(preview.backup_path.contains("claude"));

    // Still only one current; preview did not switch.
    let list = list_providers_inner(&hub, Some("claude")).unwrap();
    assert_eq!(list.iter().filter(|p| p.is_current).count(), 1);
    assert!(list.iter().find(|p| p.id == "cur").unwrap().is_current);
}

#[test]
fn list_presets_filtered() {
    let all = list_provider_presets(None).unwrap();
    assert_eq!(all.len(), 8);
    let claude = list_provider_presets(Some("claude".into())).unwrap();
    assert_eq!(claude.len(), 2);
    assert!(claude.iter().all(|p| p.agent == AgentId::Claude));
}

#[test]
fn merge_preserving_secrets_nested() {
    let old = json!({
        "env": { "ANTHROPIC_AUTH_TOKEN": "sk-real", "ANTHROPIC_BASE_URL": "https://a" },
        "note": "x"
    });
    let new = json!({
        "env": { "ANTHROPIC_AUTH_TOKEN": "***", "ANTHROPIC_BASE_URL": "https://b" },
        "note": "y"
    });
    let merged = merge_preserving_secrets(&old, &new);
    assert_eq!(merged["env"]["ANTHROPIC_AUTH_TOKEN"], "sk-real");
    assert_eq!(merged["env"]["ANTHROPIC_BASE_URL"], "https://b");
    assert_eq!(merged["note"], "y");
}
