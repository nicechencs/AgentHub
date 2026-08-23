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
    KIMI_CLAUDE_RULE_ID, KIMI_GROK_BASE_URL, KIMI_GROK_DEFAULT_MODEL, OPENAI_GROK_BASE_URL,
    OPENAI_GROK_DEFAULT_MODEL,
};
use crate::storage::{AccountRepo, Database, ProviderRepo};

use super::AdapterRouteService;

pub(super) fn reuse_path_for(
    route: AdapterRoute,
    credential: AdapterCredentialClass,
) -> AdapterReusePath {
    match route {
        AdapterRoute::Unsupported => AdapterReusePath::None,
        AdapterRoute::LocalBridge => AdapterReusePath::LocalBridge,
        AdapterRoute::NativeEndpoint | AdapterRoute::ConfigSync => match credential {
            AdapterCredentialClass::ApiKey => AdapterReusePath::ApiEndpoint,
            AdapterCredentialClass::OauthAuthJson | AdapterCredentialClass::OauthOther => {
                AdapterReusePath::NativeSubscription
            }
            AdapterCredentialClass::Unknown => AdapterReusePath::None,
        },
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) enum RouteSourceLabel {
    KimiMembership,
    AnthropicApiKey,
    OpenaiApiKey,
    XaiApiKey,
    GlmCodingPlan,
    DeepseekApi,
    CodexSubscription,
    ClaudeSubscription,
    XaiGrokSubscription,
    Other,
}

/// Kimi agent pool row that is not membership (open platform / custom compatible).
pub(super) const KIMI_NON_MEMBERSHIP_REASON: &str = concat!(
    "当前 Kimi 连接不是「Kimi Code 会员」来源。",
    "跨 Agent 适配仅支持会员：连接页中选择 preset「Kimi Code 会员」，",
    "或配置端点包含 api.kimi.com/coding。",
    "开放平台（moonshot）与任意兼容 API 不会自动升级。",
    "当前不支持不等于连接失效。",
);

pub(super) fn is_codex_auth_json(format: Option<&str>, credentials: &Value) -> bool {
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

/// Private write gate for `plan()`. Not a public third source of truth.
///
/// Matrix `can_apply` is necessary but not sufficient. A write is open only
/// when a bind implementation exists for this `(rule, source_kind, target)`
/// and the secret resolver can take that ticket's `source_kind`.
pub(super) fn write_gate(
    accounts: &AccountRepo,
    matrix_can_apply: bool,
    request: &AdapterRouteRequest,
    analysis: &AdapterRouteAnalysis,
) -> bool {
    matrix_can_apply
        && bind_implementation_open(request, analysis)
        && subscription_account_secret_open(accounts, request, analysis)
}

pub(super) fn subscription_account_secret_open(
    accounts: &AccountRepo,
    request: &AdapterRouteRequest,
    analysis: &AdapterRouteAnalysis,
) -> bool {
    if request.source_kind != AdapterSourceKind::Account
        || !matches!(
            analysis.rule_id.as_deref(),
            Some(
                "claude-subscription-to-pi-v1"
                    | "codex-subscription-to-pi-v1"
                    | "grok-subscription-to-pi-v1"
                    | "codex-subscription-to-claude-responses-v1"
                    | "grok-subscription-to-claude-v1"
                    | "grok-subscription-to-codex-v1"
                    | "codex-subscription-to-codex-v1"
                    | "codex-subscription-to-grok-v1"
                    | "codex-subscription-to-kimi-v1"
                    | "codex-subscription-to-dsh-v1"
            )
        )
    {
        return true;
    }
    let Ok(Some(account)) = accounts.get_by_id(&request.source_id) else {
        return false;
    };
    [
        "/access_token",
        "/tokens/access_token",
        "/body/tokens/access_token",
    ]
    .iter()
    .filter_map(|pointer| account.credentials.pointer(pointer))
    .any(|value| value.as_str().is_some_and(|token| !token.trim().is_empty()))
}

/// Bind implementations opened in this step. API secrets resolve from either
/// a Provider or an Account row (`credentials.api_key`).
pub(crate) fn bind_implementation_open(
    request: &AdapterRouteRequest,
    analysis: &AdapterRouteAnalysis,
) -> bool {
    match (
        analysis.rule_id.as_deref(),
        request.source_kind,
        request.target_agent_id,
        analysis.route,
        analysis.support,
    ) {
        (
            Some(KIMI_CLAUDE_RULE_ID),
            AdapterSourceKind::Provider | AdapterSourceKind::Account,
            AgentId::Claude,
            AdapterRoute::NativeEndpoint,
            AdapterSupport::Stable,
        )
        | (
            Some(GLM_CLAUDE_RULE_ID) | Some(DEEPSEEK_CLAUDE_RULE_ID),
            AdapterSourceKind::Provider | AdapterSourceKind::Account,
            AgentId::Claude,
            AdapterRoute::NativeEndpoint,
            AdapterSupport::Experimental,
        )
        | (
            Some("kimi-membership-to-pi-v1"),
            AdapterSourceKind::Provider | AdapterSourceKind::Account,
            AgentId::Pi,
            AdapterRoute::ConfigSync,
            AdapterSupport::Stable,
        )
        | (
            Some("kimi-membership-to-codex-v1")
            | Some("anthropic-api-to-codex-v1")
            | Some("openai-api-to-codex-v1"),
            AdapterSourceKind::Provider | AdapterSourceKind::Account,
            AgentId::Codex,
            AdapterRoute::LocalBridge,
            AdapterSupport::Experimental,
        )
        | (
            Some("anthropic-api-to-pi-v1") | Some("openai-api-to-pi-v1") | Some("xai-api-to-pi-v1"),
            AdapterSourceKind::Provider | AdapterSourceKind::Account,
            AgentId::Pi,
            AdapterRoute::ConfigSync,
            AdapterSupport::Stable,
        )
        | (
            Some(GLM_PI_RULE_ID) | Some(DEEPSEEK_PI_RULE_ID),
            AdapterSourceKind::Provider | AdapterSourceKind::Account,
            AgentId::Pi,
            AdapterRoute::ConfigSync,
            AdapterSupport::Experimental,
        )
        | (
            Some("claude-subscription-to-pi-v1")
            | Some("codex-subscription-to-pi-v1")
            | Some("grok-subscription-to-pi-v1"),
            AdapterSourceKind::Account,
            AgentId::Pi,
            AdapterRoute::ConfigSync,
            AdapterSupport::Experimental,
        )
        | (
            Some(GLM_CODEX_RULE_ID) | Some(DEEPSEEK_CODEX_RULE_ID),
            AdapterSourceKind::Provider | AdapterSourceKind::Account,
            AgentId::Codex,
            AdapterRoute::NativeEndpoint,
            AdapterSupport::Experimental,
        )
        | (
            Some("codex-subscription-to-claude-responses-v1"),
            AdapterSourceKind::Account,
            AgentId::Claude,
            AdapterRoute::LocalBridge,
            AdapterSupport::Experimental,
        )
        | (
            Some("grok-subscription-to-claude-v1"),
            AdapterSourceKind::Account,
            AgentId::Claude,
            AdapterRoute::LocalBridge,
            AdapterSupport::Experimental,
        )
        | (
            Some("grok-subscription-to-codex-v1"),
            AdapterSourceKind::Account,
            AgentId::Codex,
            AdapterRoute::LocalBridge,
            AdapterSupport::Experimental,
        )
        | (
            Some("codex-subscription-to-codex-v1"),
            AdapterSourceKind::Account,
            AgentId::Codex,
            AdapterRoute::NativeEndpoint,
            AdapterSupport::Stable,
        )
        | (
            Some("codex-subscription-to-grok-v1"),
            AdapterSourceKind::Account,
            AgentId::Grok,
            AdapterRoute::LocalBridge,
            AdapterSupport::Experimental,
        )
        | (
            Some("codex-subscription-to-kimi-v1"),
            AdapterSourceKind::Account,
            AgentId::Kimi,
            AdapterRoute::LocalBridge,
            AdapterSupport::Experimental,
        )
        | (
            Some("codex-subscription-to-dsh-v1"),
            AdapterSourceKind::Account,
            AgentId::Dsh,
            AdapterRoute::LocalBridge,
            AdapterSupport::Experimental,
        )
        | (
            Some("kimi-membership-to-grok-v1") | Some("openai-api-to-grok-v1"),
            AdapterSourceKind::Provider | AdapterSourceKind::Account,
            AgentId::Grok,
            AdapterRoute::NativeEndpoint,
            AdapterSupport::Experimental,
        )
        | (
            Some("deepseek-api-to-dsh-v1"),
            AdapterSourceKind::Provider,
            AgentId::Dsh,
            AdapterRoute::ConfigSync,
            AdapterSupport::Stable,
        ) => true,
        _ => false,
    }
}

pub(super) fn json_string<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value
        .get(key)?
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

pub(super) const VERIFIED_AT: &str = "2026-08-12";

pub(super) fn analysis_from_decision(
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
            "现在还写不上去；不会改配置，也不会开本机转发。".into(),
        ]
    } else {
        decision
            .limitations
            .iter()
            .map(|item| (*item).to_owned())
            .collect()
    };

    AdapterRouteAnalysis {
        route: decision.route,
        support: decision.support,
        reason: decision.reason.into(),
        actions,
        limitations,
        evidence,
        rule_id: decision.rule_id.map(str::to_owned),
        gate_kind: decision.gate_kind,
    }
}

pub(super) fn decision_actions_allowed(decision: &AdapterCapabilityDecision) -> bool {
    !matches!(decision.route, AdapterRoute::Unsupported)
}

pub(super) fn actions_for(
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
                    Some(KIMI_CLAUDE_BASE_URL),
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
        (RouteSourceLabel::KimiMembership, AgentId::Grok, AdapterRoute::NativeEndpoint) => vec![
            action(
                "set_config",
                "Grok",
                "写入 Grok 的 Kimi Code 官方 OpenAI Chat Completions 配置。",
                Some(KIMI_GROK_BASE_URL),
                false,
            ),
            action(
                "set_config",
                "Grok",
                "设置 Grok 模型与 Chat Completions backend。",
                Some("model=kimi-k2.5; api_backend=chat_completions"),
                false,
            ),
            action(
                "reference_connection_secret",
                "Grok",
                "从已选 Connection 引用 API Key；不会读取或显示它。",
                None,
                true,
            ),
        ],
        (RouteSourceLabel::OpenaiApiKey, AgentId::Grok, AdapterRoute::NativeEndpoint) => vec![
            action(
                "set_config",
                "Grok",
                "写入 Grok 的 OpenAI 官方 Chat Completions 配置。",
                Some(OPENAI_GROK_BASE_URL),
                false,
            ),
            action(
                "set_config",
                "Grok",
                "设置 Grok 模型与 Chat Completions backend。",
                Some("model=gpt-4o; api_backend=chat_completions"),
                false,
            ),
            action(
                "reference_connection_secret",
                "Grok",
                "从已选 Connection 引用 API Key；不会读取或显示它。",
                None,
                true,
            ),
        ],
        (RouteSourceLabel::KimiMembership, AgentId::Codex, AdapterRoute::LocalBridge) => {
            vec![action(
                "requires_local_bridge",
                "Codex",
                "Codex 和 Kimi 说的话对不上，需要本机转发。",
                None,
                false,
            )]
        }
        (RouteSourceLabel::AnthropicApiKey, AgentId::Codex, AdapterRoute::LocalBridge) => {
            vec![action(
                "requires_local_bridge",
                "Codex",
                "Codex 和 Anthropic 说的话对不上，需要本机转发。",
                None,
                false,
            )]
        }
        (RouteSourceLabel::OpenaiApiKey, AgentId::Codex, AdapterRoute::LocalBridge) => {
            vec![action(
                "requires_local_bridge",
                "Codex",
                "Codex 和 OpenAI 说的话对不上，需要本机转发。",
                None,
                false,
            )]
        }
        (RouteSourceLabel::GlmCodingPlan, AgentId::Codex, AdapterRoute::NativeEndpoint) => {
            vec![
                action(
                    "set_config",
                    "Codex",
                    "设置 GLM Coding Plan 官方 Responses Base URL；不会启动本机路由。",
                    Some(GLM_CODEX_BASE_URL),
                    false,
                ),
                action(
                    "set_config",
                    "Codex",
                    "使用 Codex Responses wire_api 与默认模型 glm-5.3。",
                    Some("wire_api=responses; model=glm-5.3"),
                    false,
                ),
                action(
                    "reference_connection_secret",
                    "Codex",
                    "从已选 Connection 引用 API Key；不会读取或显示它。",
                    None,
                    true,
                ),
            ]
        }
        (RouteSourceLabel::DeepseekApi, AgentId::Codex, AdapterRoute::NativeEndpoint) => {
            vec![
                action(
                    "set_config",
                    "Codex",
                    "设置 DeepSeek 官方 Responses Base URL；不会启动本机路由。",
                    Some(DEEPSEEK_CODEX_BASE_URL),
                    false,
                ),
                action(
                    "set_config",
                    "Codex",
                    "使用 Codex Responses wire_api 与默认模型 deepseek-v4-flash。",
                    Some("wire_api=responses; model=deepseek-v4-flash"),
                    false,
                ),
                action(
                    "reference_connection_secret",
                    "Codex",
                    "从已选 Connection 引用 API Key；不会读取或显示它。",
                    None,
                    true,
                ),
            ]
        }
        (RouteSourceLabel::CodexSubscription, AgentId::Claude, AdapterRoute::LocalBridge) => vec![
            action(
                "requires_local_bridge",
                "Claude Code",
                "Claude 和 Codex 说的话对不上，需要本机转发。",
                None,
                false,
            ),
            action(
                "set_env",
                "Claude Code",
                "写入 Claude Code 的本机地址 Base URL 与本机 bearer；不会写入上游 OAuth token。",
                Some("ANTHROPIC_BASE_URL / ANTHROPIC_AUTH_TOKEN"),
                false,
            ),
        ],
        (RouteSourceLabel::XaiGrokSubscription, AgentId::Claude, AdapterRoute::LocalBridge) => {
            vec![
            action(
                "requires_local_bridge",
                "Claude Code",
                "Claude 和 Grok 说的话对不上，需要本机转发。",
                None,
                false,
            ),
            action(
                "set_env",
                "Claude Code",
                "写入 Claude Code 的本机地址 Base URL 与本机 bearer；不会写入上游 OAuth token。",
                Some("ANTHROPIC_BASE_URL / ANTHROPIC_AUTH_TOKEN"),
                false,
            ),
        ]
        }
        (RouteSourceLabel::XaiGrokSubscription, AgentId::Codex, AdapterRoute::LocalBridge) => {
            vec![
                action(
                    "requires_local_bridge",
                    "Codex",
                    "会把 Codex 指到本机路由；上游 Grok 登录不会写入 Codex。",
                    None,
                    false,
                ),
                action(
                    "set_config",
                    "Codex",
                    "写入 Codex 的本机路由端点。",
                    Some("AgentHub Grok 本机路由"),
                    false,
                ),
            ]
        }
        (RouteSourceLabel::CodexSubscription, AgentId::Grok, AdapterRoute::LocalBridge) => vec![
            action(
                "requires_local_bridge",
                "Grok",
                "会把 Grok 指到本机路由；上游 Codex 官方登录不会写入 Grok。",
                None,
                false,
            ),
            action(
                "set_config",
                "Grok",
                "写入 Grok 的本机路由端点。",
                Some("http://127.0.0.1:<本机端口>/v1"),
                false,
            ),
        ],
        (RouteSourceLabel::CodexSubscription, AgentId::Kimi, AdapterRoute::LocalBridge) => vec![
            action(
                "requires_local_bridge",
                "Kimi",
                "会把 Kimi 指到本机路由；上游 Codex 官方登录不会写入 Kimi。",
                None,
                false,
            ),
            action(
                "set_config",
                "Kimi",
                "写入 Kimi 的本机路由端点。",
                Some("http://127.0.0.1:<本机端口>/v1"),
                false,
            ),
        ],
        (RouteSourceLabel::CodexSubscription, AgentId::Dsh, AdapterRoute::LocalBridge) => vec![
            action(
                "requires_local_bridge",
                "DeepSeek Harness",
                "会把 DeepSeek Harness 指到本机路由；上游 Codex 官方登录不会写入 DSH。",
                None,
                false,
            ),
            action(
                "set_config",
                "DeepSeek Harness",
                "写入 DSH 的本机路由端点。",
                Some("http://127.0.0.1:<本机端口>"),
                false,
            ),
        ],
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
        (RouteSourceLabel::OpenaiApiKey, AgentId::Pi, AdapterRoute::ConfigSync) => vec![
            action(
                "set_config",
                "Pi",
                "选择 Pi 的 OpenAI provider。",
                Some("openai"),
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
        (RouteSourceLabel::XaiApiKey, AgentId::Pi, AdapterRoute::ConfigSync) => vec![
            action(
                "set_config",
                "Pi",
                "选择 Pi 的 xAI provider。",
                Some("xai"),
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
        (RouteSourceLabel::GlmCodingPlan, AgentId::Pi, AdapterRoute::ConfigSync) => vec![
            action(
                "set_config",
                "Pi",
                "写入 Pi 的 GLM Coding Plan 自定义 provider 位置。",
                Some(GLM_PI_PROVIDER_SLOT),
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
        (RouteSourceLabel::DeepseekApi, AgentId::Pi, AdapterRoute::ConfigSync) => vec![
            action(
                "set_config",
                "Pi",
                "写入 Pi 的 DeepSeek 自定义 provider 位置。",
                Some(DEEPSEEK_PI_PROVIDER_SLOT),
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
        (RouteSourceLabel::ClaudeSubscription, AgentId::Pi, AdapterRoute::ConfigSync) => vec![
            action(
                "set_config",
                "Pi",
                "选择 Pi 的 anthropic 登录位置。",
                Some("anthropic"),
                false,
            ),
            action(
                "reference_connection_secret",
                "Pi",
                "从已选 Connection 引用授权（OAuth）；不会读取或显示 token。",
                None,
                true,
            ),
        ],
        (RouteSourceLabel::CodexSubscription, AgentId::Pi, AdapterRoute::ConfigSync) => vec![
            action(
                "set_config",
                "Pi",
                "选择 Pi 的 openai-codex 登录位置。",
                Some("openai-codex"),
                false,
            ),
            action(
                "reference_connection_secret",
                "Pi",
                "从已选 Connection 引用授权（OAuth）；不会读取或显示 token。",
                None,
                true,
            ),
        ],
        (RouteSourceLabel::XaiGrokSubscription, AgentId::Pi, AdapterRoute::ConfigSync) => vec![
            action(
                "set_config",
                "Pi",
                "选择 Pi 的 xai 登录位置。",
                Some("xai"),
                false,
            ),
            action(
                "reference_connection_secret",
                "Pi",
                "从已选 Connection 引用授权（OAuth）；不会读取或显示 token。",
                None,
                true,
            ),
        ],
        (RouteSourceLabel::GlmCodingPlan, AgentId::Claude, AdapterRoute::NativeEndpoint) => {
            vec![
                action(
                    "set_config",
                    "Claude Code",
                    "设置 GLM Coding Plan 官方 Anthropic-compatible Base URL。",
                    Some(GLM_CLAUDE_BASE_URL),
                    false,
                ),
                action(
                    "set_env",
                    "Claude Code",
                    "使用 Claude Code 的认证环境变量名。",
                    Some(ANTHROPIC_AUTH_TOKEN_ENV),
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
        (RouteSourceLabel::DeepseekApi, AgentId::Claude, AdapterRoute::NativeEndpoint) => {
            vec![
                action(
                    "set_config",
                    "Claude Code",
                    "设置 DeepSeek 官方 Anthropic-compatible Base URL。",
                    Some(DEEPSEEK_CLAUDE_BASE_URL),
                    false,
                ),
                action(
                    "set_env",
                    "Claude Code",
                    "使用 Claude Code 的认证环境变量名。",
                    Some(ANTHROPIC_AUTH_TOKEN_ENV),
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
        (RouteSourceLabel::DeepseekApi, AgentId::Dsh, AdapterRoute::ConfigSync) => vec![
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

pub(super) fn evidence_for(
    source: &RouteSourceLabel,
    target: AgentId,
    _decision: &AdapterCapabilityDecision,
) -> Vec<AdapterEvidence> {
    match (source, target) {
        (RouteSourceLabel::KimiMembership, AgentId::Claude) => vec![kimi_claude_evidence()],
        (RouteSourceLabel::KimiMembership, AgentId::Grok) => vec![kimi_pi_evidence()],
        (RouteSourceLabel::KimiMembership, AgentId::Codex) => vec![kimi_codex_evidence()],
        (RouteSourceLabel::KimiMembership, AgentId::Pi) => vec![kimi_pi_evidence()],
        (RouteSourceLabel::KimiMembership, _) => vec![kimi_pi_evidence()],
        (RouteSourceLabel::AnthropicApiKey, AgentId::Codex) => vec![anthropic_codex_evidence()],
        (RouteSourceLabel::AnthropicApiKey, _) => vec![anthropic_pi_evidence()],
        (RouteSourceLabel::OpenaiApiKey, AgentId::Codex) => vec![openai_codex_evidence()],
        (RouteSourceLabel::OpenaiApiKey, AgentId::Grok) => {
            vec![adapter_compatibility_evidence()]
        }
        (RouteSourceLabel::XaiGrokSubscription, AgentId::Claude | AgentId::Codex) => {
            vec![adapter_compatibility_evidence()]
        }
        (RouteSourceLabel::CodexSubscription, AgentId::Grok | AgentId::Kimi | AgentId::Dsh) => {
            vec![adapter_compatibility_evidence()]
        }
        (RouteSourceLabel::OpenaiApiKey | RouteSourceLabel::XaiApiKey, _) => {
            vec![anthropic_pi_evidence()]
        }
        (RouteSourceLabel::GlmCodingPlan, AgentId::Claude) => vec![glm_claude_evidence()],
        (RouteSourceLabel::GlmCodingPlan, AgentId::Codex) => vec![glm_codex_evidence()],
        (RouteSourceLabel::GlmCodingPlan, AgentId::Pi) => vec![pi_api_evidence()],
        (RouteSourceLabel::DeepseekApi, AgentId::Claude) => vec![deepseek_claude_evidence()],
        (RouteSourceLabel::DeepseekApi, AgentId::Codex) => vec![deepseek_codex_evidence()],
        (RouteSourceLabel::DeepseekApi, AgentId::Pi) => vec![pi_api_evidence()],
        (RouteSourceLabel::DeepseekApi, AgentId::Dsh) => vec![deepseek_dsh_evidence()],
        (
            RouteSourceLabel::ClaudeSubscription
            | RouteSourceLabel::CodexSubscription
            | RouteSourceLabel::XaiGrokSubscription,
            AgentId::Pi,
        ) => vec![anthropic_pi_evidence()],
        (
            RouteSourceLabel::GlmCodingPlan
            | RouteSourceLabel::DeepseekApi
            | RouteSourceLabel::CodexSubscription
            | RouteSourceLabel::ClaudeSubscription
            | RouteSourceLabel::XaiGrokSubscription
            | RouteSourceLabel::Other,
            _,
        ) => vec![adapter_compatibility_evidence()],
    }
}

pub(super) fn action(
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

pub(super) fn change(
    target: &str,
    field: &str,
    value: Option<&str>,
    secret: bool,
) -> AdapterPlanChange {
    debug_assert!(!secret || value.is_none());
    AdapterPlanChange {
        target: target.into(),
        field: field.into(),
        value: value.map(str::to_owned),
        secret,
    }
}

pub(super) fn kimi_claude_evidence() -> AdapterEvidence {
    AdapterEvidence {
        label: "Kimi Code: Claude Code integration".into(),
        url: "https://www.kimi.com/code/docs/en/third-party-tools/claude-code.html".into(),
        verified_at: VERIFIED_AT.into(),
    }
}

pub(super) fn kimi_codex_evidence() -> AdapterEvidence {
    AdapterEvidence {
        label: "Kimi Code: Codex local routing".into(),
        url: "https://www.kimi.com/code/docs/third-party-tools/codex.html".into(),
        verified_at: VERIFIED_AT.into(),
    }
}

pub(super) fn kimi_pi_evidence() -> AdapterEvidence {
    AdapterEvidence {
        label: "Kimi Code CLI provider configuration".into(),
        url: "https://www.kimi.com/code/docs/en/kimi-code-cli/configuration/providers.html".into(),
        verified_at: VERIFIED_AT.into(),
    }
}

pub(super) fn anthropic_pi_evidence() -> AdapterEvidence {
    AdapterEvidence {
        label: "Pi custom provider and model configuration".into(),
        url: "https://github.com/badlogic/pi-mono/blob/main/packages/coding-agent/docs/models.md"
            .into(),
        verified_at: VERIFIED_AT.into(),
    }
}

pub(super) fn pi_api_evidence() -> AdapterEvidence {
    AdapterEvidence {
        label: "Pi custom provider and model configuration".into(),
        url: "https://github.com/badlogic/pi-mono/blob/main/packages/coding-agent/docs/models.md"
            .into(),
        verified_at: "2026-08-15".into(),
    }
}

pub(super) fn anthropic_codex_evidence() -> AdapterEvidence {
    AdapterEvidence {
        label: "Anthropic Messages API".into(),
        url: "https://docs.anthropic.com/en/api/messages".into(),
        verified_at: VERIFIED_AT.into(),
    }
}

pub(super) fn openai_codex_evidence() -> AdapterEvidence {
    AdapterEvidence {
        label: "OpenAI Chat Completions API".into(),
        url: "https://platform.openai.com/docs/api-reference/chat".into(),
        verified_at: "2026-08-21".into(),
    }
}

pub(super) fn glm_claude_evidence() -> AdapterEvidence {
    AdapterEvidence {
        label: "GLM Coding Plan 接入工具与双协议端点".into(),
        url: "https://docs.bigmodel.cn/cn/coding-plan/tool/others".into(),
        verified_at: VERIFIED_AT.into(),
    }
}

pub(super) fn deepseek_claude_evidence() -> AdapterEvidence {
    AdapterEvidence {
        label: "DeepSeek 接入 Claude Code".into(),
        url: "https://api-docs.deepseek.com/quick_start/agent_integrations/claude_code/".into(),
        verified_at: VERIFIED_AT.into(),
    }
}

pub(super) fn glm_codex_evidence() -> AdapterEvidence {
    AdapterEvidence {
        label: "GLM Coding Plan Codex Responses integration".into(),
        url: "https://docs.bigmodel.cn/cn/coding-plan/tool/codex".into(),
        verified_at: "2026-08-15".into(),
    }
}

pub(super) fn deepseek_codex_evidence() -> AdapterEvidence {
    AdapterEvidence {
        label: "DeepSeek API Codex Responses integration".into(),
        url: "https://api-docs.deepseek.com/quick_start/agent_integrations/codex/".into(),
        verified_at: "2026-08-15".into(),
    }
}

pub(super) fn deepseek_dsh_evidence() -> AdapterEvidence {
    AdapterEvidence {
        label: "DeepSeek Harness LLM / credentials".into(),
        url: "https://deepseek-harness.github.io/deepseek-harness/en/reference/subsystems/credentials"
            .into(),
        verified_at: VERIFIED_AT.into(),
    }
}

pub(super) fn adapter_compatibility_evidence() -> AdapterEvidence {
    AdapterEvidence {
        label: "AgentHub：厂商、API 与 OAuth 适配规则".into(),
        url:
            "https://github.com/nicechencs/AgentHub/blob/release/docs/provider-api-oauth-adaptation.md"
                .into(),
        verified_at: VERIFIED_AT.into(),
    }
}
