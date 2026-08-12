//! Read-only compatibility analysis for explicitly tagged connection records.

use serde_json::Value;

use crate::error::{AppError, Result};
use crate::models::{
    AccountKind, AdapterAction, AdapterApplyPlan, AdapterEvidence, AdapterPlanChange, AdapterRoute,
    AdapterRouteAnalysis, AdapterRouteRequest, AdapterServiceImpact, AdapterSourceKind,
    AdapterSupport, AgentId,
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
        let source_id = request.source_id.trim();
        if source_id.is_empty() {
            return Err(AppError::InvalidArg(
                "adapter source id must not be empty".into(),
            ));
        }

        let source = match request.source_kind {
            AdapterSourceKind::Provider => {
                let provider = self.providers.get_by_id(source_id)?.ok_or_else(|| {
                    AppError::NotFound(format!("provider not found: {source_id}"))
                })?;
                if provider.agent_id == AgentId::Kimi
                    && json_string(&provider.meta, "preset") == Some("kimi-code-membership")
                {
                    RouteSource::KimiMembership
                } else if provider.agent_id == AgentId::Claude
                    && json_string(&provider.meta, "preset") == Some("anthropic")
                {
                    RouteSource::AnthropicApiKey
                } else {
                    RouteSource::Other(provider.agent_id)
                }
            }
            AdapterSourceKind::Account => {
                let account = self
                    .accounts
                    .get_by_id(source_id)?
                    .ok_or_else(|| AppError::NotFound(format!("account not found: {source_id}")))?;
                let explicit_provider = json_string(&account.extra, "provider")
                    .or_else(|| json_string(&account.credentials, "provider"));
                if account.kind == AccountKind::ApiKey
                    && explicit_provider
                        .is_some_and(|value| value.eq_ignore_ascii_case("anthropic"))
                {
                    RouteSource::AnthropicApiKey
                } else {
                    RouteSource::Other(account.agent_id)
                }
            }
        };

        Ok(match (source, request.target_agent_id) {
            (RouteSource::KimiMembership, AgentId::Claude) => stable(
                AdapterRoute::NativeEndpoint,
                "Kimi Code 会员可预览为 Claude 的原生 Anthropic Messages 端点。",
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
                ],
                vec![
                    "将写入 Claude 的 base URL 与凭据引用标记；不会在预览中传输明文 Key。",
                    "应用后会切换当前 Claude Connection；请确认无其他进行中的配置写入。",
                ],
                vec![kimi_claude_evidence()],
            ),
            (RouteSource::KimiMembership, AgentId::Codex) => experimental(
                AdapterRoute::LocalBridge,
                "Kimi Code 会员到 Codex 需要本地协议桥接。",
                vec![action(
                    "requires_local_bridge",
                    "Codex",
                    "Codex Responses 与 Kimi Chat Completions 需要本地双向协议转换。",
                    None,
                    false,
                )],
                vec![
                    "将在本机 loopback 启动协议桥接，并切换 Codex 到该本地端点。",
                    "AgentHub 需保持在托盘运行；退出前会尝试排空监听。",
                    "桥接为实验性协议覆盖；长流与工具调用可能受实现限制。",
                    "固定端口被占用时会尝试重新分配端口并写回配置。",
                ],
                vec![kimi_codex_evidence()],
            ),
            (RouteSource::KimiMembership, AgentId::Pi) => stable(
                AdapterRoute::ConfigSync,
                "Kimi Code 会员可预览为 Pi 的配置同步。",
                vec![
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
                vec!["Phase 0 仅预览；不会同步配置或传输凭据。"],
                vec![kimi_pi_evidence()],
            ),
            (RouteSource::AnthropicApiKey, AgentId::Pi) => stable(
                AdapterRoute::ConfigSync,
                "显式 Anthropic API Key 可预览为 Pi 的配置同步。",
                vec![
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
                vec!["Phase 0 仅预览；不会同步配置或传输凭据。"],
                vec![anthropic_pi_evidence()],
            ),
            (RouteSource::KimiMembership, _) => unsupported(
                "Kimi Code 会员当前仅支持预览到 Claude、Codex 或 Pi。",
                vec![kimi_pi_evidence()],
            ),
            (RouteSource::AnthropicApiKey, _) => unsupported(
                "Anthropic API Key 当前仅支持预览到 Pi。",
                vec![anthropic_pi_evidence()],
            ),
            (RouteSource::Other(AgentId::Codex), AgentId::Claude)
                if request.source_kind == AdapterSourceKind::Account =>
            {
                unsupported(
                    "AgentHub 暂未提供从 Codex 账户到 Claude Code 的适配规则。当前尚未完成上游授权、条款和协议兼容性验证，因此不能应用；这只表示没有可执行规则，不代表连接失效。",
                    vec![adapter_compatibility_evidence()],
                )
            }
            (RouteSource::Other(_), _) => unsupported(
                "AgentHub 暂未提供此来源到所选目标的适配规则。这不表示连接失效。",
                vec![adapter_compatibility_evidence()],
            ),
        })
    }

    /// Build a safe representation of an eventual configuration change.
    ///
    /// A direct Kimi -> Claude projection and the explicitly implemented
    /// Kimi -> Codex local bridge are actionable. Every other preview remains
    /// read-only even if it is compatible in principle.
    pub fn plan(&self, request: &AdapterRouteRequest) -> Result<AdapterApplyPlan> {
        let analysis = self.analyze(request)?;
        let (service_impact, changes) = match analysis.route {
            AdapterRoute::NativeEndpoint if request.target_agent_id == AgentId::Claude => (
                AdapterServiceImpact::None,
                vec![
                    change(
                        "claude",
                        "baseUrl",
                        Some("https://api.kimi.com/coding/"),
                        false,
                    ),
                    change(
                        "claude",
                        "claudeAuthEnv",
                        Some("ANTHROPIC_AUTH_TOKEN"),
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

        let can_apply = request.source_kind == AdapterSourceKind::Provider
            && matches!(
                (analysis.route, analysis.support, request.target_agent_id),
                (
                    AdapterRoute::NativeEndpoint,
                    AdapterSupport::Stable,
                    AgentId::Claude
                ) | (
                    AdapterRoute::LocalBridge,
                    AdapterSupport::Experimental,
                    AgentId::Codex
                )
            );
        Ok(AdapterApplyPlan {
            analysis,
            target_agent_id: request.target_agent_id,
            can_apply,
            service_impact,
            changes,
        })
    }
}

#[derive(Debug, Clone, Copy)]
enum RouteSource {
    KimiMembership,
    AnthropicApiKey,
    Other(AgentId),
}

fn json_string<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value
        .get(key)?
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

const VERIFIED_AT: &str = "2026-08-12";

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

fn stable(
    route: AdapterRoute,
    reason: &str,
    actions: Vec<AdapterAction>,
    limitations: Vec<&str>,
    evidence: Vec<AdapterEvidence>,
) -> AdapterRouteAnalysis {
    AdapterRouteAnalysis {
        route,
        support: AdapterSupport::Stable,
        reason: reason.into(),
        actions,
        limitations: limitations.into_iter().map(str::to_owned).collect(),
        evidence,
    }
}

fn experimental(
    route: AdapterRoute,
    reason: &str,
    actions: Vec<AdapterAction>,
    limitations: Vec<&str>,
    evidence: Vec<AdapterEvidence>,
) -> AdapterRouteAnalysis {
    AdapterRouteAnalysis {
        route,
        support: AdapterSupport::Experimental,
        reason: reason.into(),
        actions,
        limitations: limitations.into_iter().map(str::to_owned).collect(),
        evidence,
    }
}

fn unsupported(reason: &str, evidence: Vec<AdapterEvidence>) -> AdapterRouteAnalysis {
    AdapterRouteAnalysis {
        route: AdapterRoute::Unsupported,
        support: AdapterSupport::Unsupported,
        reason: reason.into(),
        actions: vec![],
        limitations: vec!["该组合暂未支持；不会改动来源连接、本机服务或配置。".into()],
        evidence,
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
