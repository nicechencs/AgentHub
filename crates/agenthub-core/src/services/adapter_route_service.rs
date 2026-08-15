//! Read-only compatibility analysis for explicitly tagged connection records.
//!
//! Route presentation is sourced from the compile-time
//! [`crate::models::ADAPTER_CAPABILITY_MATRIX`]. Missing cells fail closed.
//!
//! `plan.can_apply` is **matrix open ∩ implemented apply whitelist** — the matrix
//! alone never authorizes writes.

use serde_json::Value;

use crate::error::{AppError, Result};
use crate::models::{
    decide_adapter_capability, AccountKind, AdapterAction, AdapterApplyPlan,
    AdapterCapabilityDecision, AdapterCredentialClass, AdapterEvidence, AdapterGateKind,
    AdapterPlanChange, AdapterRoute, AdapterRouteAnalysis, AdapterRouteRequest,
    AdapterServiceImpact, AdapterSourceKind, AdapterSourceProduct, AdapterSupport, AgentId,
};
use crate::services::adapter_route_constants::{
    is_deepseek_api_source, is_kimi_code_membership_source,
    settings_contain_anthropic_api_endpoint, settings_contain_deepseek_api_endpoint,
    ANTHROPIC_AUTH_TOKEN_ENV, DSH_DEEPSEEK_PROVIDER_SLOT, KIMI_CLAUDE_BASE_URL,
};
use crate::storage::{AccountRepo, Database, ProviderRepo};

/// Determines whether one saved connection has a supported preview route to an agent.
///
/// This service deliberately uses only explicit persisted fields. It does not inspect,
/// infer, return, or copy credentials and it never writes a config or starts a bridge.
pub struct AdapterRouteService {
    accounts: AccountRepo,
    providers: ProviderRepo,
}

impl AdapterRouteService {
    pub fn new(db: Database) -> Self {
        Self {
            accounts: AccountRepo::new(db.clone()),
            providers: ProviderRepo::new(db),
        }
    }

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
    /// Write permission is the intersection of:
    /// 1. capability matrix (`decision.can_apply`: cell flag + all gates), and
    /// 2. [`implemented_apply_whitelist`] for paths that actually have an apply service.
    ///
    /// Today that whitelist is provider-sourced Kimi → Claude (native),
    /// Kimi / Anthropic → Pi (config sync), and Kimi → Codex (local bridge).
    /// Matrix-open account sources stay preview-only.
    pub fn plan(&self, request: &AdapterRouteRequest) -> Result<AdapterApplyPlan> {
        let classified = self.classify(request)?;
        let analysis = analysis_from_decision(&classified.decision, &classified.source, request);
        let (service_impact, changes) = match analysis.route {
            AdapterRoute::NativeEndpoint if request.target_agent_id == AgentId::Claude => (
                AdapterServiceImpact::None,
                vec![
                    change("claude", "baseUrl", Some(KIMI_CLAUDE_BASE_URL), false),
                    change(
                        "claude",
                        "claudeAuthEnv",
                        Some(ANTHROPIC_AUTH_TOKEN_ENV),
                        false,
                    ),
                    change("claude", "apiKey", None, true),
                ],
            ),
            AdapterRoute::ConfigSync if request.target_agent_id == AgentId::Pi => {
                let provider = analysis
                    .actions
                    .iter()
                    .find(|action| action.kind == "set_config" && action.target == "Pi")
                    .and_then(|action| action.value.as_deref())
                    .unwrap_or("anthropic");
                (
                    AdapterServiceImpact::None,
                    vec![
                        change("pi", "provider", Some(provider), false),
                        change("pi", "apiKey", None, true),
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
            AdapterRoute::LocalBridge if request.target_agent_id == AgentId::Codex => (
                AdapterServiceImpact::RequiresLocalBridge,
                vec![
                    change("codex", "provider", Some("AgentHub Kimi 本地桥接"), false),
                    change(
                        "codex",
                        "baseUrl",
                        Some("http://127.0.0.1:<本机端口>/v1"),
                        false,
                    ),
                ],
            ),
            AdapterRoute::LocalBridge => (AdapterServiceImpact::RequiresLocalBridge, vec![]),
            AdapterRoute::Unsupported | AdapterRoute::ConfigSync | AdapterRoute::NativeEndpoint => {
                (AdapterServiceImpact::None, vec![])
            }
        };

        let can_apply =
            implemented_apply_whitelist(classified.decision.can_apply, request, &analysis);

        Ok(AdapterApplyPlan {
            analysis,
            target_agent_id: request.target_agent_id,
            can_apply,
            service_impact,
            changes,
        })
    }

    fn classify(&self, request: &AdapterRouteRequest) -> Result<ClassifiedRoute> {
        let source_id = request.source_id.trim();
        if source_id.is_empty() {
            return Err(AppError::InvalidArg(
                "adapter source id must not be empty".into(),
            ));
        }

        let identity = match request.source_kind {
            AdapterSourceKind::Provider => {
                let provider = self.providers.get_by_id(source_id)?.ok_or_else(|| {
                    AppError::NotFound(format!("provider not found: {source_id}"))
                })?;
                let preset = json_string(&provider.meta, "preset");
                // Membership is explicit preset *or* official Kimi coding endpoint in config.
                // Do not invent membership from agent_id alone (moonshot / custom stay closed).
                if is_kimi_code_membership_source(
                    provider.agent_id,
                    &provider.meta,
                    &provider.settings_config,
                ) {
                    SourceIdentity {
                        product: AdapterSourceProduct::KimiCodeMembership,
                        credential: AdapterCredentialClass::ApiKey,
                        label: RouteSourceLabel::KimiMembership,
                        reason_hint: None,
                    }
                } else if provider.agent_id == AgentId::Claude
                    && (preset == Some("anthropic")
                        || settings_contain_anthropic_api_endpoint(&provider.settings_config))
                {
                    SourceIdentity {
                        product: AdapterSourceProduct::AnthropicApi,
                        credential: AdapterCredentialClass::ApiKey,
                        label: RouteSourceLabel::AnthropicApiKey,
                        reason_hint: None,
                    }
                } else if is_deepseek_api_source(preset, &provider.settings_config) {
                    SourceIdentity {
                        product: AdapterSourceProduct::DeepSeekApi,
                        credential: AdapterCredentialClass::ApiKey,
                        label: RouteSourceLabel::DeepSeekApiKey,
                        reason_hint: None,
                    }
                } else if provider.agent_id == AgentId::Kimi {
                    SourceIdentity {
                        product: AdapterSourceProduct::Other,
                        credential: AdapterCredentialClass::ApiKey,
                        label: RouteSourceLabel::Other,
                        reason_hint: Some(KIMI_NON_MEMBERSHIP_REASON),
                    }
                } else {
                    SourceIdentity {
                        product: AdapterSourceProduct::Other,
                        credential: AdapterCredentialClass::Unknown,
                        label: RouteSourceLabel::Other,
                        reason_hint: None,
                    }
                }
            }
            AdapterSourceKind::Account => {
                let account = self
                    .accounts
                    .get_by_id(source_id)?
                    .ok_or_else(|| AppError::NotFound(format!("account not found: {source_id}")))?;
                let explicit_provider = json_string(&account.extra, "provider")
                    .or_else(|| json_string(&account.credentials, "provider"));
                let credential_format = json_string(&account.credentials, "format")
                    .or_else(|| json_string(&account.extra, "format"));

                if account.kind == AccountKind::ApiKey
                    && (explicit_provider
                        .is_some_and(|value| value.eq_ignore_ascii_case("anthropic"))
                        || settings_contain_anthropic_api_endpoint(&account.credentials)
                        || settings_contain_anthropic_api_endpoint(&account.extra))
                {
                    SourceIdentity {
                        product: AdapterSourceProduct::AnthropicApi,
                        credential: AdapterCredentialClass::ApiKey,
                        label: RouteSourceLabel::AnthropicApiKey,
                        reason_hint: None,
                    }
                } else if account.kind == AccountKind::ApiKey
                    && (explicit_provider
                        .is_some_and(|value| value.eq_ignore_ascii_case("deepseek"))
                        || settings_contain_deepseek_api_endpoint(&account.credentials)
                        || settings_contain_deepseek_api_endpoint(&account.extra))
                {
                    SourceIdentity {
                        product: AdapterSourceProduct::DeepSeekApi,
                        credential: AdapterCredentialClass::ApiKey,
                        label: RouteSourceLabel::DeepSeekApiKey,
                        reason_hint: None,
                    }
                } else if account.agent_id == AgentId::Codex
                    && account.kind == AccountKind::Oauth
                    && is_codex_auth_json(credential_format, &account.credentials)
                {
                    // Explicit Codex / ChatGPT subscription (`format=auth_json` or tokens blob).
                    // Matrix cell exists and stays fully gated closed.
                    SourceIdentity {
                        product: AdapterSourceProduct::CodexChatGptSubscription,
                        credential: AdapterCredentialClass::OauthAuthJson,
                        label: RouteSourceLabel::CodexSubscription,
                        reason_hint: None,
                    }
                } else if account.agent_id == AgentId::Codex && account.kind == AccountKind::Oauth {
                    // Codex OAuth without auth_json shape: same product messaging, but do not
                    // pretend the closed auth_json matrix cell matched. Fail closed via empty
                    // candidate → subscription_candidate unsupported surface for Claude.
                    SourceIdentity {
                        product: AdapterSourceProduct::CodexChatGptSubscription,
                        credential: AdapterCredentialClass::OauthOther,
                        label: RouteSourceLabel::CodexSubscription,
                        reason_hint: None,
                    }
                } else {
                    SourceIdentity {
                        product: AdapterSourceProduct::Other,
                        credential: match account.kind {
                            AccountKind::ApiKey => AdapterCredentialClass::ApiKey,
                            AccountKind::Oauth => AdapterCredentialClass::OauthOther,
                        },
                        label: RouteSourceLabel::Other,
                        reason_hint: None,
                    }
                }
            }
        };

        let mut decision = decide_adapter_capability(
            identity.product,
            identity.credential,
            request.target_agent_id,
        )
        .public_surface();
        // Replace the generic Other reason with an actionable product hint when we have one.
        if matches!(identity.product, AdapterSourceProduct::Other) {
            if let Some(hint) = identity.reason_hint {
                decision = AdapterCapabilityDecision::unsupported(hint).public_surface();
            }
        }

        Ok(ClassifiedRoute {
            source: identity.label,
            decision,
        })
    }
}

struct ClassifiedRoute {
    source: RouteSourceLabel,
    decision: AdapterCapabilityDecision,
}

struct SourceIdentity {
    product: AdapterSourceProduct,
    credential: AdapterCredentialClass,
    label: RouteSourceLabel,
    /// Optional replace for the generic Other unsupported reason (never secrets).
    reason_hint: Option<&'static str>,
}

#[derive(Debug, Clone, Copy)]
enum RouteSourceLabel {
    KimiMembership,
    AnthropicApiKey,
    DeepSeekApiKey,
    CodexSubscription,
    Other,
}

/// Kimi agent pool row that is not membership (open platform / custom compatible).
const KIMI_NON_MEMBERSHIP_REASON: &str = concat!(
    "当前 Kimi 连接不是「Kimi Code 会员」来源。",
    "跨 Agent 适配仅支持会员：Connections 中选择 preset「Kimi Code 会员」，",
    "或配置端点包含 api.kimi.com/coding。",
    "开放平台（moonshot）与任意兼容 API 不会自动升级。",
    "当前不支持不等于连接失效。",
);

fn is_codex_auth_json(format: Option<&str>, credentials: &Value) -> bool {
    if format.is_some_and(|value| value.eq_ignore_ascii_case("auth_json")) {
        return true;
    }
    // Codex on-disk auth.json often nests tokens without a separate format tag.
    // Require the nested `tokens` object — do not treat bare access_token / API key as auth_json.
    credentials
        .get("tokens")
        .and_then(Value::as_object)
        .is_some_and(|tokens| {
            tokens.contains_key("access_token") || tokens.contains_key("refresh_token")
        })
}

/// Paths that have a real apply/bridge implementation today.
///
/// Matrix `can_apply` is necessary but not sufficient: account sources and
/// not-yet-implemented routes stay closed even if a future matrix cell opens.
fn implemented_apply_whitelist(
    matrix_can_apply: bool,
    request: &AdapterRouteRequest,
    analysis: &AdapterRouteAnalysis,
) -> bool {
    if !matrix_can_apply {
        return false;
    }
    if request.source_kind != AdapterSourceKind::Provider {
        return false;
    }
    matches!(
        (analysis.route, analysis.support, request.target_agent_id),
        (
            AdapterRoute::NativeEndpoint,
            AdapterSupport::Stable,
            AgentId::Claude
        )         | (
            AdapterRoute::ConfigSync,
            AdapterSupport::Stable,
            AgentId::Pi
        ) | (
            AdapterRoute::ConfigSync,
            AdapterSupport::Stable,
            AgentId::Dsh
        ) | (
            AdapterRoute::LocalBridge,
            AdapterSupport::Experimental,
            AgentId::Codex
        )
    )
}

fn json_string<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value
        .get(key)?
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

const VERIFIED_AT: &str = "2026-08-12";

fn analysis_from_decision(
    decision: &AdapterCapabilityDecision,
    source: &RouteSourceLabel,
    request: &AdapterRouteRequest,
) -> AdapterRouteAnalysis {
    let actions =
        if decision.route == AdapterRoute::Unsupported || !decision_actions_allowed(decision) {
            vec![]
        } else {
            actions_for(source, request.target_agent_id, decision)
        };

    let evidence = evidence_for(source, request.target_agent_id, decision);
    let limitations = if decision.limitations.is_empty() {
        vec![
            "当前不支持此组合；不会改动来源连接、本机服务或配置。".into(),
            "plan.canApply=false：无 Apply、启动 Bridge 或强制继续入口。".into(),
        ]
    } else {
        decision
            .limitations
            .iter()
            .map(|item| (*item).to_owned())
            .collect()
    };

    let gate_kind = if matches!(source, RouteSourceLabel::CodexSubscription)
        && request.target_agent_id == AgentId::Claude
    {
        AdapterGateKind::SubscriptionCandidate
    } else {
        decision.gate_kind
    };

    AdapterRouteAnalysis {
        route: decision.route,
        support: decision.support,
        reason: decision.reason.into(),
        actions,
        limitations,
        evidence,
        rule_id: decision.rule_id.map(str::to_owned),
        gate_kind,
    }
}

fn decision_actions_allowed(decision: &AdapterCapabilityDecision) -> bool {
    !matches!(decision.route, AdapterRoute::Unsupported)
}

fn actions_for(
    source: &RouteSourceLabel,
    target: AgentId,
    decision: &AdapterCapabilityDecision,
) -> Vec<AdapterAction> {
    match (source, target, decision.route) {
        (RouteSourceLabel::KimiMembership, AgentId::Claude, AdapterRoute::NativeEndpoint) => {
            vec![
                action(
                    "set_config",
                    "Claude Code",
                    "设置 Kimi Code 官方 Anthropic-compatible Base URL。",
                    Some("https://api.kimi.com/coding/"),
                    false,
                ),
                action(
                    "set_env",
                    "Claude Code",
                    "使用 Claude Code 的认证环境变量名。",
                    Some("ANTHROPIC_AUTH_TOKEN"),
                    false,
                ),
                action(
                    "reference_connection_secret",
                    "Claude Code",
                    "从已选 Connection 引用 API Key；不会读取或显示它。",
                    None,
                    true,
                ),
            ]
        }
        (RouteSourceLabel::KimiMembership, AgentId::Codex, AdapterRoute::LocalBridge) => {
            vec![action(
                "requires_local_bridge",
                "Codex",
                "Codex Responses 与 Kimi Chat Completions 需要本地双向协议转换。",
                None,
                false,
            )]
        }
        (RouteSourceLabel::KimiMembership, AgentId::Pi, AdapterRoute::ConfigSync) => vec![
            action(
                "set_config",
                "Pi",
                "选择 Pi 的 Kimi For Coding provider。",
                Some("kimi-for-coding"),
                false,
            ),
            action(
                "reference_connection_secret",
                "Pi",
                "从已选 Connection 引用 API Key；不会读取或显示它。",
                None,
                true,
            ),
        ],
        (RouteSourceLabel::AnthropicApiKey, AgentId::Pi, AdapterRoute::ConfigSync) => vec![
            action(
                "set_config",
                "Pi",
                "选择 Pi 的 Anthropic provider。",
                Some("anthropic"),
                false,
            ),
            action(
                "reference_connection_secret",
                "Pi",
                "从已选 Connection 引用 API Key；不会读取或显示它。",
                None,
                true,
            ),
        ],
        (RouteSourceLabel::DeepSeekApiKey, AgentId::Dsh, AdapterRoute::ConfigSync) => vec![
            action(
                "set_config",
                "DeepSeek Harness",
                "选择 DSH 的官方 DeepSeek provider。",
                Some(DSH_DEEPSEEK_PROVIDER_SLOT),
                false,
            ),
            action(
                "reference_connection_secret",
                "DeepSeek Harness",
                "从已选 Connection 引用 API Key；不会读取或显示它。",
                None,
                true,
            ),
        ],
        _ => vec![],
    }
}

fn evidence_for(
    source: &RouteSourceLabel,
    target: AgentId,
    _decision: &AdapterCapabilityDecision,
) -> Vec<AdapterEvidence> {
    match (source, target) {
        (RouteSourceLabel::KimiMembership, AgentId::Claude) => vec![kimi_claude_evidence()],
        (RouteSourceLabel::KimiMembership, AgentId::Codex) => vec![kimi_codex_evidence()],
        (RouteSourceLabel::KimiMembership, AgentId::Pi) => vec![kimi_pi_evidence()],
        (RouteSourceLabel::KimiMembership, _) => vec![kimi_pi_evidence()],
        (RouteSourceLabel::AnthropicApiKey, _) => vec![anthropic_pi_evidence()],
        (RouteSourceLabel::DeepSeekApiKey, _) => vec![deepseek_dsh_evidence()],
        (RouteSourceLabel::CodexSubscription, _) => vec![adapter_compatibility_evidence()],
        (RouteSourceLabel::Other, _) => vec![adapter_compatibility_evidence()],
    }
}

fn action(
    kind: &str,
    target: &str,
    description: &str,
    value: Option<&str>,
    secret: bool,
) -> AdapterAction {
    debug_assert!(!secret || value.is_none());
    AdapterAction {
        kind: kind.into(),
        target: target.into(),
        description: description.into(),
        value: value.map(str::to_owned),
        secret,
    }
}

fn change(target: &str, field: &str, value: Option<&str>, secret: bool) -> AdapterPlanChange {
    debug_assert!(!secret || value.is_none());
    AdapterPlanChange {
        target: target.into(),
        field: field.into(),
        value: value.map(str::to_owned),
        secret,
    }
}

fn kimi_claude_evidence() -> AdapterEvidence {
    AdapterEvidence {
        label: "Kimi Code: Claude Code integration".into(),
        url: "https://www.kimi.com/code/docs/en/third-party-tools/claude-code.html".into(),
        verified_at: VERIFIED_AT.into(),
    }
}

fn kimi_codex_evidence() -> AdapterEvidence {
    AdapterEvidence {
        label: "Kimi Code: Codex local routing".into(),
        url: "https://www.kimi.com/code/docs/third-party-tools/codex.html".into(),
        verified_at: VERIFIED_AT.into(),
    }
}

fn kimi_pi_evidence() -> AdapterEvidence {
    AdapterEvidence {
        label: "Kimi Code CLI provider configuration".into(),
        url: "https://www.kimi.com/code/docs/en/kimi-code-cli/configuration/providers.html".into(),
        verified_at: VERIFIED_AT.into(),
    }
}

fn anthropic_pi_evidence() -> AdapterEvidence {
    AdapterEvidence {
        label: "Pi custom provider and model configuration".into(),
        url: "https://github.com/badlogic/pi-mono/blob/main/packages/coding-agent/docs/models.md"
            .into(),
        verified_at: VERIFIED_AT.into(),
    }
}

fn deepseek_dsh_evidence() -> AdapterEvidence {
    AdapterEvidence {
        label: "DeepSeek Harness LLM / credentials".into(),
        url: "https://deepseek-harness.github.io/deepseek-harness/en/reference/subsystems/credentials"
            .into(),
        verified_at: VERIFIED_AT.into(),
    }
}

fn adapter_compatibility_evidence() -> AdapterEvidence {
    AdapterEvidence {
        label: "AgentHub：厂商、API 与 OAuth 适配规则".into(),
        url:
            "https://github.com/nicechencs/AgentHub/blob/release/docs/provider-api-oauth-adaptation.md"
                .into(),
        verified_at: VERIFIED_AT.into(),
    }
}

#[cfg(test)]
mod tests;
