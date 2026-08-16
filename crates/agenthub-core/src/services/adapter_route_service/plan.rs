use serde_json::Value;

use crate::error::{AppError, Result};
use crate::models::{
    adapter_maturity_from_decision, decide_adapter_capability, AccountKind, AdapterAction,
    AdapterApplyPlan, AdapterCapabilityDecision, AdapterCredentialClass, AdapterEvidence,
    AdapterPlanChange, AdapterReusePath, AdapterRoute, AdapterRouteAnalysis, AdapterRouteRequest,
    AdapterServiceImpact, AdapterSourceKind, AdapterSourceProduct, AdapterSupport, AgentId,
};
use crate::services::adapter_route_constants::{
    claude_native_base_url, is_deepseek_api_marker, is_glm_coding_plan_marker,
    is_kimi_code_membership_account, is_kimi_code_membership_source, is_openai_api_marker,
    is_xai_api_marker, settings_contain_anthropic_api_endpoint, ANTHROPIC_AUTH_TOKEN_ENV,
    DEEPSEEK_CLAUDE_BASE_URL, DEEPSEEK_CLAUDE_RULE_ID, DEEPSEEK_CODEX_BASE_URL,
    DEEPSEEK_CODEX_RULE_ID, DEEPSEEK_PI_PROVIDER_SLOT, DEEPSEEK_PI_RULE_ID,
    DSH_DEEPSEEK_PROVIDER_SLOT, GLM_CLAUDE_BASE_URL, GLM_CLAUDE_RULE_ID, GLM_CODEX_BASE_URL,
    GLM_CODEX_RULE_ID, GLM_PI_PROVIDER_SLOT, GLM_PI_RULE_ID, KIMI_CLAUDE_BASE_URL,
    KIMI_CLAUDE_RULE_ID, KIMI_GROK_BASE_URL, KIMI_GROK_DEFAULT_MODEL,
    OPENAI_GROK_BASE_URL, OPENAI_GROK_DEFAULT_MODEL,
};
use crate::storage::{AccountRepo, Database, ProviderRepo};

use super::actions::*;
use super::{AdapterRouteService, ClassifiedRoute};

impl AdapterRouteService {

    pub fn analyze(&self, request: &AdapterRouteRequest) -> Result<AdapterRouteAnalysis> {
        let classified = self.classify(request)?;
        Ok(analysis_from_decision(
            &classified.decision,
            &classified.source,
            request,
        ))
    }

    /// Build a safe representation of an eventual configuration change.
    ///
    /// This is the only public place that computes `can_apply`, maturity, route,
    /// and the planner reason. Write permission is matrix-open ∩ [`write_gate`].
    /// `can_apply` means bind would succeed now for this ticket `source_kind`.
    pub fn plan(&self, request: &AdapterRouteRequest) -> Result<AdapterApplyPlan> {
        let classified = self.classify(request)?;
        let analysis = analysis_from_decision(&classified.decision, &classified.source, request);
        let (service_impact, changes) = match analysis.route {
            AdapterRoute::NativeEndpoint if request.target_agent_id == AgentId::Claude => {
                let base_url = claude_native_base_url(analysis.rule_id.as_deref().unwrap_or(""))
                    .unwrap_or(KIMI_CLAUDE_BASE_URL);
                (
                    AdapterServiceImpact::None,
                    vec![
                        change("claude", "baseUrl", Some(base_url), false),
                        change(
                            "claude",
                            "claudeAuthEnv",
                            Some(ANTHROPIC_AUTH_TOKEN_ENV),
                            false,
                        ),
                        change("claude", "apiKey", None, true),
                    ],
                )
            }
            AdapterRoute::ConfigSync if request.target_agent_id == AgentId::Pi => {
                let provider = analysis
                    .actions
                    .iter()
                    .find(|action| action.kind == "set_config" && action.target == "Pi")
                    .and_then(|action| action.value.as_deref())
                    .unwrap_or("anthropic");
                let secret_field = if matches!(
                    analysis.rule_id.as_deref(),
                    Some(
                        "claude-subscription-to-pi-v1"
                            | "codex-subscription-to-pi-v1"
                            | "grok-subscription-to-pi-v1"
                    )
                ) {
                    "auth"
                } else {
                    "apiKey"
                };
                (
                    AdapterServiceImpact::None,
                    vec![
                        change("pi", "provider", Some(provider), false),
                        change("pi", secret_field, None, true),
                    ],
                )
            }
            AdapterRoute::ConfigSync if request.target_agent_id == AgentId::Dsh => (
                AdapterServiceImpact::None,
                vec![
                    change("dsh", "provider", Some(DSH_DEEPSEEK_PROVIDER_SLOT), false),
                    change("dsh", "apiKeyEnv", Some("DEEPSEEK_API_KEY"), false),
                    change("dsh", "apiKey", None, true),
                ],
            ),
            AdapterRoute::LocalBridge if request.target_agent_id == AgentId::Codex => {
                let provider = if analysis.rule_id.as_deref() == Some("anthropic-api-to-codex-v1") {
                    "AgentHub Anthropic 本地桥接"
                } else {
                    "AgentHub Kimi 本地桥接"
                };
                (
                    AdapterServiceImpact::RequiresLocalBridge,
                    vec![
                        change("codex", "provider", Some(provider), false),
                        change(
                            "codex",
                            "baseUrl",
                            Some("http://127.0.0.1:<本机端口>/v1"),
                            false,
                        ),
                    ],
                )
            }
            AdapterRoute::NativeEndpoint if request.target_agent_id == AgentId::Codex => {
                let (provider, base_url) = if analysis.rule_id.as_deref() == Some(GLM_CODEX_RULE_ID)
                {
                    ("GLM Coding Plan", GLM_CODEX_BASE_URL)
                } else {
                    ("DeepSeek API", DEEPSEEK_CODEX_BASE_URL)
                };
                (
                    AdapterServiceImpact::None,
                    vec![
                        change("codex", "provider", Some(provider), false),
                        change("codex", "baseUrl", Some(base_url), false),
                        change("codex", "wireApi", Some("responses"), false),
                    ],
                )
            }
            AdapterRoute::NativeEndpoint if request.target_agent_id == AgentId::Grok => {
                let (base_url, model) =
                    if analysis.rule_id.as_deref() == Some("kimi-membership-to-grok-v1") {
                        (KIMI_GROK_BASE_URL, KIMI_GROK_DEFAULT_MODEL)
                    } else {
                        (OPENAI_GROK_BASE_URL, OPENAI_GROK_DEFAULT_MODEL)
                    };
                (
                    AdapterServiceImpact::None,
                    vec![
                        change("grok", "baseUrl", Some(base_url), false),
                        change("grok", "model", Some(model), false),
                        change("grok", "apiBackend", Some("chat_completions"), false),
                        change("grok", "apiKey", None, true),
                    ],
                )
            }
            AdapterRoute::LocalBridge if request.target_agent_id == AgentId::Claude => (
                AdapterServiceImpact::RequiresLocalBridge,
                vec![
                    change(
                        "claude",
                        "ANTHROPIC_BASE_URL",
                        Some("http://127.0.0.1:<本机端口>"),
                        false,
                    ),
                    change("claude", "ANTHROPIC_AUTH_TOKEN", None, true),
                ],
            ),
            AdapterRoute::LocalBridge => (AdapterServiceImpact::RequiresLocalBridge, vec![]),
            AdapterRoute::Unsupported | AdapterRoute::ConfigSync | AdapterRoute::NativeEndpoint => {
                (AdapterServiceImpact::None, vec![])
            }
        };

        let can_apply = write_gate(
            &self.accounts,
            classified.decision.can_apply,
            request,
            &analysis,
        );
        let maturity = adapter_maturity_from_decision(&classified.decision);
        let reason = analysis.reason.clone();
        let reuse_path = reuse_path_for(classified.decision.route, classified.credential);

        Ok(AdapterApplyPlan {
            analysis,
            target_agent_id: request.target_agent_id,
            can_apply,
            maturity,
            reuse_path,
            reason,
            service_impact,
            changes,
        })
    }
}
