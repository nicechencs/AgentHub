use super::*;
use crate::bridge::{DownstreamResponsesProfile, ResponsesDialect};
use crate::models::{map_edge_model, AdapterModelMapResult, AdapterSourceProduct, AgentId};

fn lead_codex_official(profile_id: &str) -> ModelSwitchCandidate {
    ModelSwitchCandidate {
        profile_id: profile_id.into(),
        source: AdapterSourceProduct::CodexChatGptSubscription,
        target: AgentId::Claude,
        downstream_responses_profile: None,
        custom_openai_compat: false,
        same_surface: true,
        running: true,
        listed_models: vec!["gpt-5.4".into()],
    }
}

fn openrouter_claude(profile_id: &str, running: bool) -> ModelSwitchCandidate {
    ModelSwitchCandidate {
        profile_id: profile_id.into(),
        source: AdapterSourceProduct::OpenaiApi,
        target: AgentId::Claude,
        downstream_responses_profile: None,
        custom_openai_compat: true,
        same_surface: true,
        running,
        listed_models: vec!["gpt-4o".into()],
    }
}

fn cand(
    id: &str,
    source: AdapterSourceProduct,
    target: AgentId,
    custom: bool,
    running: bool,
) -> ModelSwitchCandidate {
    ModelSwitchCandidate {
        profile_id: id.into(),
        source,
        target,
        downstream_responses_profile: None,
        custom_openai_compat: custom,
        same_surface: true,
        running,
        listed_models: Vec::new(),
    }
}

#[test]
fn custom_openai_passthroughs_only_stealth_ox_alpha() {
    assert_eq!(
        map_edge_model(
            AdapterSourceProduct::OpenaiApi,
            AgentId::Codex,
            "stealth/ox-alpha",
            true,
        ),
        AdapterModelMapResult::Passthrough
    );
    assert_eq!(
        map_edge_model(
            AdapterSourceProduct::OpenaiApi,
            AgentId::Codex,
            "stealth/ox-alpha",
            false,
        ),
        AdapterModelMapResult::Missing
    );
    assert_eq!(
        map_edge_model(
            AdapterSourceProduct::OpenaiApi,
            AgentId::Claude,
            "stealth/ox-alpha",
            true,
        ),
        AdapterModelMapResult::Passthrough
    );
    assert_eq!(
        map_edge_model(
            AdapterSourceProduct::OpenaiApi,
            AgentId::Grok,
            "openai/gpt-4o",
            true,
        ),
        AdapterModelMapResult::Missing
    );
}

#[test]
fn mapped_lead_stays() {
    let lead = ModelSwitchCandidate {
        profile_id: "official-codex".into(),
        source: AdapterSourceProduct::OpenaiApi,
        target: AgentId::Codex,
        downstream_responses_profile: None,
        custom_openai_compat: false,
        same_surface: true,
        running: true,
        listed_models: vec!["gpt-4o".into()],
    };
    let alternate = openrouter_claude("or-claude", true);
    assert_eq!(
        decide_model_switch(&lead, "gpt-4o", &[alternate]),
        ModelSwitchDecision::Stay
    );
}

#[test]
fn missing_on_lead_switches_to_running_passthrough_alternate() {
    let lead = lead_codex_official("codex-claude");
    let alternate = openrouter_claude("or-claude", true);
    assert_eq!(
        decide_model_switch(&lead, "stealth/ox-alpha", std::slice::from_ref(&alternate)),
        ModelSwitchDecision::SwitchTo {
            profile_id: "or-claude".into(),
        }
    );
}

#[test]
fn missing_on_lead_fail_closed_when_alternate_not_running() {
    let lead = lead_codex_official("codex-claude");
    let alternate = openrouter_claude("or-claude", false);
    assert_eq!(
        decide_model_switch(&lead, "stealth/ox-alpha", &[alternate]),
        ModelSwitchDecision::Unavailable
    );
}

#[test]
fn missing_on_lead_fail_closed_when_no_alternate() {
    let lead = lead_codex_official("codex-claude");
    assert_eq!(
        decide_model_switch(&lead, "stealth/ox-alpha", &[]),
        ModelSwitchDecision::Unavailable
    );
}

#[test]
fn does_not_switch_across_target_or_surface() {
    let lead = lead_codex_official("codex-claude");
    let grok_alt = ModelSwitchCandidate {
        profile_id: "or-grok".into(),
        source: AdapterSourceProduct::OpenaiApi,
        target: AgentId::Grok,
        downstream_responses_profile: None,
        custom_openai_compat: true,
        same_surface: true,
        running: true,
        listed_models: vec![],
    };
    let wrong_surface = ModelSwitchCandidate {
        profile_id: "or-claude-chat".into(),
        source: AdapterSourceProduct::OpenaiApi,
        target: AgentId::Claude,
        downstream_responses_profile: None,
        custom_openai_compat: true,
        same_surface: false,
        running: true,
        listed_models: vec![],
    };
    assert_eq!(
        decide_model_switch(&lead, "stealth/ox-alpha", &[grok_alt, wrong_surface]),
        ModelSwitchDecision::Unavailable
    );
}

#[test]
fn listed_lead_model_stays_even_when_mapping_table_is_reserved() {
    let lead = lead_codex_official("codex-claude");
    assert_eq!(
        decide_model_switch(&lead, "gpt-5.4", &[]),
        ModelSwitchDecision::Stay
    );
}

#[test]
fn kimi_unknown_model_fail_closed_without_alternate() {
    let lead = ModelSwitchCandidate {
        profile_id: "kimi-codex".into(),
        source: AdapterSourceProduct::KimiCodeMembership,
        target: AgentId::Codex,
        downstream_responses_profile: None,
        custom_openai_compat: false,
        same_surface: true,
        running: true,
        listed_models: vec!["kimi-k2.5".into()],
    };
    assert_eq!(
        decide_model_switch(&lead, "unknown-model", &[]),
        ModelSwitchDecision::Unavailable
    );
    assert_eq!(
        decide_model_switch(&lead, "kimi-k2.5", &[]),
        ModelSwitchDecision::Stay
    );
}

#[test]
fn custom_empty_listed_follows_downstream_model() {
    let lead = ModelSwitchCandidate {
        profile_id: "or-claude".into(),
        source: AdapterSourceProduct::OpenaiApi,
        target: AgentId::Claude,
        downstream_responses_profile: None,
        custom_openai_compat: true,
        same_surface: true,
        running: true,
        listed_models: vec![],
    };
    assert_eq!(
        decide_model_switch(&lead, "anthropic/claude-sonnet-4", &[]),
        ModelSwitchDecision::Stay
    );
}

#[test]
fn custom_listed_models_accept_case_insensitive() {
    let lead = ModelSwitchCandidate {
        profile_id: "or-claude".into(),
        source: AdapterSourceProduct::OpenaiApi,
        target: AgentId::Claude,
        downstream_responses_profile: None,
        custom_openai_compat: true,
        same_surface: true,
        running: true,
        listed_models: vec!["anthropic/claude-sonnet-4".into(), "openai/gpt-4o".into()],
    };
    assert_eq!(
        decide_model_switch(&lead, "Anthropic/Claude-Sonnet-4", &[]),
        ModelSwitchDecision::Stay
    );
    assert_eq!(
        decide_model_switch(&lead, "not-listed", &[]),
        ModelSwitchDecision::Unavailable
    );
}

#[test]
fn model_switch_picks_running_openrouter_when_lead_misses() {
    let lead = cand(
        "official-claude",
        AdapterSourceProduct::XaiGrokSubscription,
        AgentId::Claude,
        false,
        true,
    );
    let alt = cand(
        "openrouter-claude",
        AdapterSourceProduct::OpenaiApi,
        AgentId::Claude,
        true,
        true,
    );
    assert_eq!(
        decide_model_switch(&lead, "stealth/ox-alpha", &[alt.clone()]),
        ModelSwitchDecision::SwitchTo {
            profile_id: "openrouter-claude".into()
        }
    );
    let stopped = cand(
        "openrouter-claude",
        AdapterSourceProduct::OpenaiApi,
        AgentId::Claude,
        true,
        false,
    );
    assert_eq!(
        decide_model_switch(&lead, "stealth/ox-alpha", &[stopped]),
        ModelSwitchDecision::Unavailable
    );
}

#[test]
fn model_switch_rejects_alternate_with_different_downstream_profile() {
    let lead = ModelSwitchCandidate {
        profile_id: "codex-lead".into(),
        source: AdapterSourceProduct::OpenaiApi,
        target: AgentId::Codex,
        downstream_responses_profile: Some(DownstreamResponsesProfile::new(
            ResponsesDialect::Codex,
        )),
        // Keep the lead genuinely unable to serve the probe model. A custom
        // lead with an empty listing intentionally accepts every model.
        custom_openai_compat: false,
        same_surface: true,
        running: true,
        listed_models: vec![],
    };
    let grok_profile = ModelSwitchCandidate {
        profile_id: "grok-alternate".into(),
        source: AdapterSourceProduct::OpenaiApi,
        target: AgentId::Codex,
        downstream_responses_profile: Some(DownstreamResponsesProfile::new(ResponsesDialect::Grok)),
        custom_openai_compat: true,
        same_surface: true,
        running: true,
        listed_models: vec![],
    };
    let same_profile = ModelSwitchCandidate {
        profile_id: "codex-alternate".into(),
        downstream_responses_profile: lead.downstream_responses_profile,
        ..grok_profile.clone()
    };
    assert_eq!(
        decide_model_switch(&lead, "stealth/ox-alpha", &[same_profile]),
        ModelSwitchDecision::SwitchTo {
            profile_id: "codex-alternate".into(),
        }
    );
    assert_eq!(
        decide_model_switch(&lead, "stealth/ox-alpha", &[grok_profile]),
        ModelSwitchDecision::Unavailable
    );
}
