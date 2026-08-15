use super::*;
use agenthub_core::error::AppError;
use serde_json::json;

fn sample_provider() -> Provider {
    Provider {
        id: "p1".into(),
        agent_id: AgentId::Claude,
        name: "Relay".into(),
        settings_config: json!({
            "api_key": "sk-secret",
            "base_url": "https://example.com",
            "nested": { "auth_token": "tok", "x": 1 }
        }),
        meta: json!({"TOKEN": "t", "note": "ok"}),
        is_current: true,
        created_at: "2026-01-01 00:00:00".into(),
        updated_at: "2026-01-02 00:00:00".into(),
    }
}

#[test]
fn parse_agent_filter_none() {
    assert_eq!(parse_agent_filter(None).unwrap(), None);
}

#[test]
fn parse_agent_filter_valid() {
    assert_eq!(
        parse_agent_filter(Some("claude")).unwrap(),
        Some(AgentId::Claude)
    );
    assert_eq!(
        parse_agent_filter(Some("GROK")).unwrap(),
        Some(AgentId::Grok)
    );
    assert_eq!(
        parse_agent_filter(Some("cursor")).unwrap(),
        Some(AgentId::Cursor)
    );
    assert_eq!(
        parse_agent_filter(Some("cursor-agent")).unwrap(),
        Some(AgentId::Cursor)
    );
}

#[test]
fn parse_agent_filter_invalid_is_invalid_arg() {
    let err = parse_agent_filter(Some("not-an-agent")).unwrap_err();
    assert_eq!(err.code(), "invalid_arg");
    match &err {
        AppError::InvalidArg(msg) => {
            assert!(msg.contains("not-an-agent"));
            assert!(msg.contains("claude"));
            assert!(msg.contains("cursor"));
        }
        other => panic!("expected InvalidArg, got {other:?}"),
    }
}

#[test]
fn select_presets_all_and_filtered() {
    let all = select_presets(None).unwrap();
    assert_eq!(all.len(), 8);

    let claude = select_presets(Some("claude")).unwrap();
    assert_eq!(claude.len(), 2);
    assert!(claude.iter().all(|p| p.agent == AgentId::Claude));
    assert!(claude.iter().all(|p| !p.template.is_empty()));
}

#[test]
fn select_presets_rejects_invalid_agent() {
    assert!(matches!(
        select_presets(Some("nope")),
        Err(AppError::InvalidArg(_))
    ));
}

#[test]
fn resolve_agent_filter_mirrors_parse() {
    assert_eq!(resolve_agent_filter(None).unwrap(), None);
    assert_eq!(
        resolve_agent_filter(Some("kimi")).unwrap(),
        Some(AgentId::Kimi)
    );
    assert!(matches!(
        resolve_agent_filter(Some("bad")),
        Err(AppError::InvalidArg(_))
    ));
}

#[test]
fn write_operations_require_agent() {
    assert_eq!(
        require_agent(None, "switch").unwrap_err().code(),
        "invalid_arg"
    );
    assert_eq!(
        require_agent(None, "import-live").unwrap_err().code(),
        "invalid_arg"
    );
    assert_eq!(
        require_agent(None, "undo").unwrap_err().code(),
        "invalid_arg"
    );
    assert_eq!(
        require_agent(None, "test-latency").unwrap_err().code(),
        "invalid_arg"
    );
    assert_eq!(
        require_agent(Some("codex"), "switch").unwrap(),
        AgentId::Codex
    );
}

#[test]
fn emit_list_and_show_quiet_is_ok() {
    let items = vec![sample_provider()];
    emit_provider_list(&items, OutputFormat::Quiet).unwrap();
    emit_provider_show(&items[0], OutputFormat::Quiet).unwrap();
}

#[test]
fn emit_json_redacts_secrets_and_is_valid() {
    let p = sample_provider();
    let r = p.redacted();
    let s = serde_json::to_string(&r).unwrap();
    let v: serde_json::Value = serde_json::from_str(&s).unwrap();
    assert_eq!(v["settingsConfig"]["api_key"], "***");
    assert_eq!(v["settingsConfig"]["base_url"], "https://example.com");
    assert_eq!(v["settingsConfig"]["nested"]["auth_token"], "***");
    assert_eq!(v["settingsConfig"]["nested"]["x"], 1);
    assert_eq!(v["meta"]["TOKEN"], "***");
    assert_eq!(v["meta"]["note"], "ok");
    assert_eq!(v["isCurrent"], true);
    assert_eq!(p.settings_config["api_key"], "sk-secret");
}

#[test]
fn emit_list_json_shape_via_redacted_vec() {
    let items = vec![sample_provider()];
    let redacted: Vec<Provider> = items.iter().map(Provider::redacted).collect();
    let v = serde_json::to_value(&redacted).unwrap();
    assert!(v.is_array());
    assert_eq!(v[0]["id"], "p1");
    assert_eq!(v[0]["agentId"], "claude");
    assert_eq!(v[0]["name"], "Relay");
    assert_eq!(v[0]["settingsConfig"]["api_key"], "***");
}

#[test]
fn switch_result_redacts_provider_secrets() {
    let result = ProviderSwitchResult {
        provider: sample_provider(),
        backup: None,
        backfilled_provider_id: Some("old-provider".into()),
    };
    emit_provider_switch(&result, OutputFormat::Quiet).unwrap();
    let value = serde_json::to_value(result.redacted()).unwrap();
    assert_eq!(value["provider"]["settingsConfig"]["api_key"], "***");
    assert_eq!(value["provider"]["meta"]["TOKEN"], "***");
    assert_eq!(value["backfilledProviderId"], "old-provider");
}

#[test]
fn switch_confirm_prompt_mentions_backfill_backup_and_process() {
    let dir = tempfile::tempdir().unwrap();
    let hub = AgentHub::open(Some(dir.path())).unwrap();
    let prompt = switch_confirm_prompt(&hub, AgentId::Claude, "relay");
    assert!(prompt.contains("backfill:"));
    assert!(prompt.contains("backup:"));
    assert!(prompt.contains("process:"));
    assert!(prompt.contains("claude"));
    assert!(prompt.contains("relay"));
}
