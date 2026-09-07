use super::*;
use std::collections::HashSet;

#[test]
fn registry_has_eight_presets_for_historical_agents() {
    // Pi/WorkBuddy support manual provider writes but do not ship built-in presets.
    assert_eq!(count(), 8);
    let all = list_all();
    assert_eq!(all.len(), 8);
    for agent in [
        AgentId::Claude,
        AgentId::Codex,
        AgentId::Kimi,
        AgentId::Grok,
    ] {
        assert_eq!(list_for(agent).len(), 2, "agent {agent}");
    }
    assert!(
        list_for(AgentId::Pi).is_empty(),
        "pi has no built-in presets (manual models.json providers are supported)"
    );
    assert!(
        list_for(AgentId::Cursor).is_empty(),
        "cursor has no provider presets (half-surface; write_config unsupported)"
    );
}

#[test]
fn registry_order_follows_agent_id_all() {
    let all = list_all();
    // Only agents that currently ship presets appear; order still follows AgentId::ALL.
    let mut expected_agents = Vec::new();
    for agent in AgentId::ALL {
        for _ in list_for(agent) {
            expected_agents.push(agent);
        }
    }
    let actual_agents: Vec<AgentId> = all.iter().map(|p| p.agent).collect();
    assert_eq!(actual_agents, expected_agents);
}

#[test]
fn registry_ids_match_frontend_mirror() {
    let all = list_all();
    let pairs: Vec<(&str, &str)> = all
        .iter()
        .map(|p| (p.agent.as_str(), p.id.as_str()))
        .collect();
    assert_eq!(
        pairs,
        vec![
            ("claude", "anthropic"),
            ("claude", "anthropic-compatible"),
            ("codex", "openai"),
            ("codex", "openai-compatible"),
            ("kimi", "moonshot"),
            ("kimi", "openai-compatible"),
            ("grok", "xai"),
            ("grok", "openai-compatible"),
        ]
    );
}

#[test]
fn filter_by_agent_returns_only_that_agent() {
    for agent in AgentId::ALL {
        let filtered = list(Some(agent));
        assert!(filtered.iter().all(|p| p.agent == agent));
        assert_eq!(filtered, list_for(agent));
        if matches!(
            agent,
            AgentId::Claude | AgentId::Codex | AgentId::Kimi | AgentId::Grok
        ) {
            assert_eq!(filtered.len(), 2);
        }
    }
}

#[test]
fn list_none_equals_list_all() {
    assert_eq!(list(None), list_all());
}

#[test]
fn formats_match_agent_conventions() {
    for p in list_for(AgentId::Claude) {
        assert_eq!(p.format, ConfigFormat::Json);
    }
    for agent in [AgentId::Codex, AgentId::Kimi, AgentId::Grok] {
        for p in list_for(agent) {
            assert_eq!(p.format, ConfigFormat::Toml, "{} {}", agent, p.id);
        }
    }
}

#[test]
fn templates_are_non_empty_and_ids_unique_per_agent() {
    for agent in AgentId::ALL {
        let presets = list_for(agent);
        let mut ids = HashSet::new();
        for p in &presets {
            assert!(!p.template.is_empty(), "empty template for {}", p.id);
            assert!(!p.label.is_empty());
            assert!(ids.insert(p.id.clone()), "duplicate id {}", p.id);
        }
    }
}

#[test]
fn claude_anthropic_template_is_json_env_object() {
    let p = list_for(AgentId::Claude)
        .into_iter()
        .find(|p| p.id == "anthropic")
        .expect("anthropic");
    let v: serde_json::Value = serde_json::from_str(&p.template).expect("valid json");
    assert!(v["env"].is_object());
}
