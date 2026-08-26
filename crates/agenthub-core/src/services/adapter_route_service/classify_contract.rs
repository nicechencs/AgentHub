//! Lockstep: core classify vs shared `source-classify-contract.json`.
//! Does not change production classify results.

use super::*;
use crate::models::{
    Account, AccountKind, AdapterSourceProduct, AgentId, Provider, TicketSurface,
};
use crate::services::adapter_route_constants::{
    ANTHROPIC_API_ENDPOINT_NEEDLE, DEEPSEEK_API_ENDPOINT_NEEDLE, DEEPSEEK_API_PRESET,
    GLM_CODING_ANTHROPIC_NEEDLE, GLM_CODING_CHAT_NEEDLE, GLM_CODING_PLAN_PRESET,
    GLM_CODING_RESPONSES_NEEDLE, KIMI_CODING_ENDPOINT_NEEDLE, KIMI_MEMBERSHIP_PRESET,
    OPENAI_API_ENDPOINT_NEEDLE, OPENAI_API_PRESET, OPENROUTER_API_ENDPOINT_NEEDLE,
    XAI_API_ENDPOINT_NEEDLE, XAI_API_PRESET,
};
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeSet;
use std::path::PathBuf;

const CONTRACT_WATCH: &str =
    include_str!("../../../../../src/lib/backend/contracts/source-classify-contract.json");

fn contract_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../src/lib/backend/contracts/source-classify-contract.json")
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClassifyContract {
    products: Vec<String>,
    needles: ClassifyNeedles,
    presets: ClassifyPresets,
    cases: Vec<ClassifyCase>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClassifyPresets {
    kimi_membership: String,
    anthropic: String,
    openai: String,
    xai: String,
    glm_coding_plan: String,
    deepseek_api: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClassifyNeedles {
    kimi_coding: String,
    anthropic_api: String,
    openai_api: String,
    openrouter: String,
    xai_api: String,
    glm_anthropic: String,
    glm_chat: String,
    glm_responses: String,
    deepseek_api: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClassifyCase {
    id: String,
    kind: String,
    agent_id: String,
    #[serde(default)]
    account_kind: Option<String>,
    #[serde(default)]
    extra: Option<Value>,
    #[serde(default)]
    credentials: Option<Value>,
    #[serde(default)]
    preset: Option<String>,
    #[serde(default)]
    settings: Option<Value>,
    expect_product: String,
}

fn load_contract() -> ClassifyContract {
    let path = contract_path();
    let text = std::fs::read_to_string(&path).unwrap_or_else(|err| {
        panic!("read source-classify-contract.json from {}: {err}", path.display())
    });
    let _ = CONTRACT_WATCH;
    serde_json::from_str(&text).expect("source-classify-contract.json")
}

fn product_id(product: AdapterSourceProduct) -> &'static str {
    match product {
        AdapterSourceProduct::KimiCodeMembership => "kimi-code-membership",
        AdapterSourceProduct::AnthropicApi => "anthropic-api",
        AdapterSourceProduct::OpenaiApi => "openai-api",
        AdapterSourceProduct::XaiApi => "xai-api",
        AdapterSourceProduct::GlmCodingPlan => "glm-coding-plan",
        AdapterSourceProduct::DeepseekApi => "deepseek-api",
        AdapterSourceProduct::CodexChatGptSubscription => "codex-chatgpt-subscription",
        AdapterSourceProduct::ClaudeSubscription => "claude-subscription",
        AdapterSourceProduct::XaiGrokSubscription => "grok-xai-subscription",
        AdapterSourceProduct::Other => "other",
    }
}

fn all_products() -> [AdapterSourceProduct; 10] {
    [
        AdapterSourceProduct::KimiCodeMembership,
        AdapterSourceProduct::AnthropicApi,
        AdapterSourceProduct::OpenaiApi,
        AdapterSourceProduct::XaiApi,
        AdapterSourceProduct::GlmCodingPlan,
        AdapterSourceProduct::DeepseekApi,
        AdapterSourceProduct::CodexChatGptSubscription,
        AdapterSourceProduct::ClaudeSubscription,
        AdapterSourceProduct::XaiGrokSubscription,
        AdapterSourceProduct::Other,
    ]
}

#[test]
fn shared_classify_fixture_covers_every_source_product() {
    let contract = load_contract();
    let from_enum: BTreeSet<&str> = all_products().into_iter().map(product_id).collect();
    let from_file: BTreeSet<&str> = contract.products.iter().map(String::as_str).collect();
    assert_eq!(from_enum, from_file);
    for product in all_products() {
        assert_eq!(
            TicketSurface::from_product(product).as_str(),
            match product {
                AdapterSourceProduct::Other => "unknown",
                other => product_id(other),
            }
        );
    }
}

#[test]
fn shared_classify_needles_match_core_constants() {
    let needles = load_contract().needles;
    assert_eq!(needles.kimi_coding, KIMI_CODING_ENDPOINT_NEEDLE);
    assert_eq!(needles.anthropic_api, ANTHROPIC_API_ENDPOINT_NEEDLE);
    assert_eq!(needles.openai_api, OPENAI_API_ENDPOINT_NEEDLE);
    assert_eq!(needles.openrouter, OPENROUTER_API_ENDPOINT_NEEDLE);
    assert_eq!(needles.xai_api, XAI_API_ENDPOINT_NEEDLE);
    assert_eq!(needles.glm_anthropic, GLM_CODING_ANTHROPIC_NEEDLE);
    assert_eq!(needles.glm_chat, GLM_CODING_CHAT_NEEDLE);
    assert_eq!(needles.glm_responses, GLM_CODING_RESPONSES_NEEDLE);
    assert_eq!(needles.deepseek_api, DEEPSEEK_API_ENDPOINT_NEEDLE);
    let presets = load_contract().presets;
    assert_eq!(presets.kimi_membership, KIMI_MEMBERSHIP_PRESET);
    assert_eq!(presets.anthropic, "anthropic");
    assert_eq!(presets.openai, OPENAI_API_PRESET);
    assert_eq!(presets.xai, XAI_API_PRESET);
    assert_eq!(presets.glm_coding_plan, GLM_CODING_PLAN_PRESET);
    assert_eq!(presets.deepseek_api, DEEPSEEK_API_PRESET);
}

#[test]
fn shared_classify_cases_match_production() {
    let contract = load_contract();
    let mut seen = BTreeSet::new();
    for case in &contract.cases {
        let got = match case.kind.as_str() {
            "account" => {
                let account = Account {
                    id: case.id.clone(),
                    agent_id: AgentId::parse_required(&case.agent_id).unwrap(),
                    kind: AccountKind::parse(case.account_kind.as_deref().unwrap_or("apikey"))
                        .expect("accountKind"),
                    label: case.id.clone(),
                    credentials: case.credentials.clone().unwrap_or_else(|| serde_json::json!({})),
                    extra: case.extra.clone().unwrap_or_else(|| serde_json::json!({})),
                    status: "active".into(),
                    is_current: false,
                    created_at: "now".into(),
                    updated_at: "now".into(),
                };
                AdapterRouteService::classify_account_source_product(&account)
            }
            "provider" => {
                let provider = Provider {
                    id: case.id.clone(),
                    agent_id: AgentId::parse_required(&case.agent_id).unwrap(),
                    name: case.id.clone(),
                    settings_config: case.settings.clone().unwrap_or_else(|| serde_json::json!({})),
                    meta: serde_json::json!({
                        "preset": case.preset.as_deref().unwrap_or("custom")
                    }),
                    is_current: false,
                    created_at: "now".into(),
                    updated_at: "now".into(),
                };
                AdapterRouteService::classify_provider_source_product(&provider)
            }
            other => panic!("unknown case kind {other}"),
        };
        assert_eq!(product_id(got), case.expect_product.as_str(), "{}", case.id);
        seen.insert(case.expect_product.as_str());
    }
    for product in &contract.products {
        assert!(
            seen.contains(product.as_str()),
            "no classify case for product {product}"
        );
    }
}
