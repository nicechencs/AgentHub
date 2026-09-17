//! P2-1: production integrations + test-only open-key agent.

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use crate::integrations::agents::demo_agent;
use crate::integrations::{production_integrations, ProductionIntegrations};
use crate::models::AgentId;
use crate::platform::AgentKey;

#[test]
fn production_register_integrations_covers_all_agents_without_demo() {
    let prod = production_integrations();
    let expected: Vec<_> = AgentId::ALL
        .iter()
        .copied()
        .map(AgentKey::from_agent_id)
        .collect();

    assert_eq!(prod.install.supported_agent_keys(), expected);
    assert_eq!(prod.detectors.supported_agent_keys(), expected);
    for agent in AgentId::ALL {
        assert!(prod.paths.contains(agent), "{}", agent.as_str());
    }
    for agent in AgentId::ALL {
        assert!(prod.projects.contains(agent), "{}", agent.as_str());
        assert!(
            prod.usage.contains(agent) || matches!(agent, AgentId::Cursor | AgentId::Kiro),
            "{}",
            agent.as_str()
        );
    }

    let demo = demo_agent::key();
    assert!(!prod.install.contains_key(&demo));
    assert!(!prod.detectors.contains_key(&demo));
    assert!(!prod.config.contains_key(&demo));
    assert!(!prod.usage.contains_key(&demo));
    assert!(!prod.stream.contains_key(&demo));
    assert!(!prod.projects.contains_key(&demo));
    assert!(!prod.skills.contains_key(&demo));
    assert!(prod.session_titles.get(&demo).is_none());
}

#[test]
fn session_title_sources_cover_only_agents_that_write_their_own_title() {
    let prod = production_integrations();
    let titled = [AgentId::Codex, AgentId::Grok, AgentId::Kiro, AgentId::Dsh];
    for agent in titled {
        assert!(
            prod.session_titles.get_agent_id(agent).is_some(),
            "{}",
            agent.as_str()
        );
    }
    // Claude keeps no session title of its own, so it must not get a source
    // (and neither may any other agent that never writes one).
    for agent in AgentId::ALL {
        if titled.contains(&agent) {
            continue;
        }
        assert!(
            prod.session_titles.get_agent_id(agent).is_none(),
            "{}",
            agent.as_str()
        );
    }
}

#[test]
fn ninth_test_only_agent_is_one_directory_plus_one_register() {
    let mut bundle = ProductionIntegrations::empty();
    demo_agent::register(&mut bundle.as_context(), Arc::new(AtomicBool::new(false)));

    let key = demo_agent::key();
    assert!(bundle.detectors.contains_key(&key));
    assert_eq!(
        bundle.install.get(&key).unwrap().npm_package(),
        Some("@agenthub/demo-agent")
    );
    assert!(bundle.config.contains_key(&key));
    assert_eq!(bundle.install.supported_agent_keys(), vec![key.clone()]);

    // Sparse: demo-agent does not grow unused ports or AgentId.
    assert!(bundle.usage.get(&key).is_none());
    assert!(bundle.stream.get(&key).is_none());
    assert!(bundle.projects.get(&key).is_none());
    assert!(bundle.session_titles.get(&key).is_none());
    assert!(AgentId::ALL.iter().all(|id| id.as_str() != demo_agent::KEY));
}

#[test]
fn kimi_omits_skills_target_like_before() {
    let prod = production_integrations();
    assert!(!prod
        .skills
        .contains_key(&AgentKey::from_agent_id(AgentId::Kimi)));
    assert!(prod
        .skills
        .contains_key(&AgentKey::from_agent_id(AgentId::Claude)));
    assert!(prod
        .skills
        .contains_key(&AgentKey::from_agent_id(AgentId::Cursor)));
    assert!(prod
        .skills
        .contains_key(&AgentKey::from_agent_id(AgentId::Zcode)));
}
