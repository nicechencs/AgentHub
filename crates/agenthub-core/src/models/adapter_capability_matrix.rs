//! Adapter compatibility capability matrix (fail-closed).
//!
//! Dimensions: `source × credential × transport × target × protocol × version`.
//! Any combination without an explicit cell is [`AdapterRoute::Unsupported`] with
//! `can_apply = false`. This is separate from the per-agent feature matrix in
//! [`crate::models::capability`].
//!
//! Codex / ChatGPT subscription OAuth → Claude Code is recorded as an
//! experimental *candidate* with every gate closed, so analyze/plan stay
//! unsupported and Apply/Start/Bridge remain forbidden.

use super::{AdapterGateKind, AdapterRoute, AdapterSupport, AgentId};

/// Shared public reason for Codex / ChatGPT subscription → Claude Code (closed).
/// Mock UI and core analyze must keep this string in lockstep.
pub const CODEX_SUBSCRIPTION_TO_CLAUDE_REASON: &str = concat!(
    "Codex / ChatGPT 订阅 → Claude Code：当前不支持。",
    "尚未通过上游授权、条款与协议兼容性门禁，plan.canApply=false。",
    "不会创建适配、启动 Bridge，也不会把订阅凭据写入 Claude。",
    "这只表示没有可执行规则，不代表连接失效。",
    "替代路径：在 Claude 使用自身官方登录，或改用已支持的 API Key 来源。",
);

/// Product / origin that owns the selected Connection credentials.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AdapterSourceProduct {
    /// Kimi Code membership provider (`meta.preset = kimi-code-membership`).
    KimiCodeMembership,
    /// Explicit Anthropic API Key (provider preset or account.extra.provider).
    AnthropicApi,
    /// Codex / ChatGPT subscription account (`auth_json` OAuth shape).
    CodexChatGptSubscription,
    /// Anything else; never upgraded by name guessing.
    Other,
}

/// How the source authenticates. API Key and OAuth are never interchangeable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AdapterCredentialClass {
    ApiKey,
    /// Codex subscription on-disk auth blob (`credentials.format = auth_json`).
    OauthAuthJson,
    /// Other OAuth shapes (PKCE tokens, managed OAuth, etc.).
    OauthOther,
    Unknown,
}

/// Upstream transport candidate for a matrix cell.
///
/// For direct routes this is the vendor HTTP surface. For subscription bridge
/// candidates it is the still-gated transport under evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AdapterUpstreamTransport {
    /// Vendor-native HTTP endpoint (no local listener).
    NativeHttp,
    /// Existing Kimi path: local bridge with Chat Completions upstream.
    LocalBridgeChatCompletions,
    /// Future Codex → Claude candidate: App Server transport (gate closed).
    CodexAppServer,
    /// Future Codex → Claude candidate: approved Responses + OAuth (gate closed).
    CodexResponsesOauth,
    /// No transport selected / not applicable.
    None,
}

/// Wire protocol the *target* client will speak on the adapted path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AdapterTargetProtocol {
    AnthropicMessages,
    OpenAiChatCompletions,
    OpenAiResponses,
    /// Pi-native provider config slot (not a wire protocol).
    PiProviderConfig,
}

/// Closed lookup key for one matrix cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AdapterCapabilityKey {
    pub source: AdapterSourceProduct,
    pub credential: AdapterCredentialClass,
    pub transport: AdapterUpstreamTransport,
    pub target: AgentId,
    pub protocol: AdapterTargetProtocol,
    /// Rule / matrix generation. Bump when gates or route semantics change.
    pub version: &'static str,
}

/// Independent subscription / experimental gates. All must be true to open apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdapterCapabilityGates {
    pub official_contract: bool,
    pub terms_reviewed: bool,
    pub endpoint_stable: bool,
    pub auth_refresh: bool,
    pub protocol_conversion: bool,
    pub isolation_verified: bool,
    pub e2e_verified: bool,
}

impl AdapterCapabilityGates {
    pub const fn all_closed() -> Self {
        Self {
            official_contract: false,
            terms_reviewed: false,
            endpoint_stable: false,
            auth_refresh: false,
            protocol_conversion: false,
            isolation_verified: false,
            e2e_verified: false,
        }
    }

    pub const fn all_open() -> Self {
        Self {
            official_contract: true,
            terms_reviewed: true,
            endpoint_stable: true,
            auth_refresh: true,
            protocol_conversion: true,
            isolation_verified: true,
            e2e_verified: true,
        }
    }

    /// Subscription experimental rules require every gate before `can_apply`.
    pub const fn all_passed(self) -> bool {
        self.official_contract
            && self.terms_reviewed
            && self.endpoint_stable
            && self.auth_refresh
            && self.protocol_conversion
            && self.isolation_verified
            && self.e2e_verified
    }
}

/// One explicit matrix cell. Missing keys are never invented at runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdapterCapabilityCell {
    pub key: AdapterCapabilityKey,
    pub route: AdapterRoute,
    pub support: AdapterSupport,
    /// Write / bridge execution flag. Must stay false for gated candidates.
    pub can_apply: bool,
    pub reason: &'static str,
    pub limitations: &'static [&'static str],
    pub rule_id: &'static str,
    pub verified_at: &'static str,
    pub gates: AdapterCapabilityGates,
}

/// Safe decision returned to analyze / plan. Secrets never appear here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterCapabilityDecision {
    pub route: AdapterRoute,
    pub support: AdapterSupport,
    pub can_apply: bool,
    pub reason: &'static str,
    pub limitations: &'static [&'static str],
    pub rule_id: Option<&'static str>,
    pub rule_version: Option<&'static str>,
    pub verified_at: Option<&'static str>,
    pub transport: AdapterUpstreamTransport,
    pub protocol: Option<AdapterTargetProtocol>,
    pub gates: Option<AdapterCapabilityGates>,
    /// UI presentation class derived from the cell / fallback path.
    pub gate_kind: AdapterGateKind,
}

impl AdapterCapabilityDecision {
    pub fn unsupported(reason: &'static str) -> Self {
        Self {
            route: AdapterRoute::Unsupported,
            support: AdapterSupport::Unsupported,
            can_apply: false,
            reason,
            limitations: &[
                "当前不支持此组合；不会改动来源连接、本机服务或配置。",
                "plan.canApply=false：无 Apply、启动 Bridge 或强制继续入口。",
            ],
            rule_id: None,
            rule_version: None,
            verified_at: None,
            transport: AdapterUpstreamTransport::None,
            protocol: None,
            gates: None,
            gate_kind: AdapterGateKind::Unsupported,
        }
    }

    pub fn unsupported_subscription_candidate(reason: &'static str) -> Self {
        let mut decision = Self::unsupported(reason);
        decision.limitations = CODEX_CLAUDE_LIMITS;
        decision.gate_kind = AdapterGateKind::SubscriptionCandidate;
        decision.transport = AdapterUpstreamTransport::CodexAppServer;
        decision.gates = Some(AdapterCapabilityGates::all_closed());
        decision
    }

    pub fn from_cell(cell: &AdapterCapabilityCell) -> Self {
        // Fail-closed: experimental subscription cells cannot open apply while any gate is closed.
        let can_apply = cell.can_apply && cell.gates.all_passed();
        let (route, support, gate_kind) = surface_from_cell(cell, can_apply);
        Self {
            route,
            support,
            can_apply,
            reason: cell.reason,
            limitations: cell.limitations,
            rule_id: Some(cell.rule_id),
            rule_version: Some(cell.key.version),
            verified_at: Some(cell.verified_at),
            transport: cell.key.transport,
            protocol: Some(cell.key.protocol),
            gates: Some(cell.gates),
            gate_kind,
        }
    }
}

/// Collapse cell flags into the public route/support/gate presentation.
fn surface_from_cell(
    cell: &AdapterCapabilityCell,
    can_apply: bool,
) -> (AdapterRoute, AdapterSupport, AdapterGateKind) {
    let subscription_transport = matches!(
        cell.key.transport,
        AdapterUpstreamTransport::CodexAppServer | AdapterUpstreamTransport::CodexResponsesOauth
    );

    // Closed experimental subscription candidates always surface as unsupported.
    if subscription_transport && !can_apply {
        return (
            AdapterRoute::Unsupported,
            AdapterSupport::Unsupported,
            AdapterGateKind::SubscriptionCandidate,
        );
    }

    if cell.support == AdapterSupport::Unsupported {
        return (
            AdapterRoute::Unsupported,
            AdapterSupport::Unsupported,
            AdapterGateKind::Unsupported,
        );
    }

    // Cell claims apply but gates still closed → fail closed as unsupported.
    if cell.can_apply && !cell.gates.all_passed() {
        return (
            AdapterRoute::Unsupported,
            AdapterSupport::Unsupported,
            if subscription_transport {
                AdapterGateKind::SubscriptionCandidate
            } else {
                AdapterGateKind::Unsupported
            },
        );
    }

    // Recorded experimental candidate with can_apply=false (non-subscription) → unsupported.
    if !cell.can_apply && cell.support == AdapterSupport::Experimental {
        return (
            AdapterRoute::Unsupported,
            AdapterSupport::Unsupported,
            AdapterGateKind::Unsupported,
        );
    }

    let gate_kind = if can_apply {
        AdapterGateKind::None
    } else {
        AdapterGateKind::PreviewOnly
    };
    (cell.route, cell.support, gate_kind)
}

const VERIFIED_AT: &str = "2026-08-12";
const MATRIX_VERSION: &str = "1";

const KIMI_CLAUDE_LIMITS: &[&str] = &[
    "将写入 Claude 的 base URL 与凭据引用标记；不会在预览中传输明文 Key。",
    "应用后会切换当前 Claude Connection；请确认无其他进行中的配置写入。",
];

const KIMI_CODEX_LIMITS: &[&str] = &[
    "将在本机 loopback 启动协议桥接，并切换 Codex 到该本地端点。",
    "AgentHub 需保持在托盘运行；退出前会尝试排空监听。",
    "桥接为实验性协议覆盖；长流与工具调用可能受实现限制。",
    "固定端口被占用时会尝试重新分配端口并写回配置。",
];

const KIMI_PI_LIMITS: &[&str] = &["Phase 0 仅预览；不会同步配置或传输凭据。"];

const ANTHROPIC_PI_LIMITS: &[&str] = &["Phase 0 仅预览；不会同步配置或传输凭据。"];

const CODEX_CLAUDE_LIMITS: &[&str] = &[
    "当前不支持此组合；尚未通过上游授权、条款与协议兼容性门禁。",
    "plan.canApply=false：不会创建 adapter profile、启动 Bridge 或写入 Claude 配置。",
    "不会把 ChatGPT / Codex OAuth token 导出或写入目标客户端。",
    "替代路径：在 Claude 使用自身官方登录，或改用已支持的 API Key 来源（例如 Kimi Code 会员 → Claude）。",
];

/// Compile-time matrix. Order does not matter; lookup is by full key equality.
pub const ADAPTER_CAPABILITY_MATRIX: &[AdapterCapabilityCell] = &[
    AdapterCapabilityCell {
        key: AdapterCapabilityKey {
            source: AdapterSourceProduct::KimiCodeMembership,
            credential: AdapterCredentialClass::ApiKey,
            transport: AdapterUpstreamTransport::NativeHttp,
            target: AgentId::Claude,
            protocol: AdapterTargetProtocol::AnthropicMessages,
            version: MATRIX_VERSION,
        },
        route: AdapterRoute::NativeEndpoint,
        support: AdapterSupport::Stable,
        can_apply: true,
        reason: "Kimi Code 会员可预览为 Claude 的原生 Anthropic Messages 端点。",
        limitations: KIMI_CLAUDE_LIMITS,
        rule_id: "kimi-membership-to-claude-v1",
        verified_at: VERIFIED_AT,
        gates: AdapterCapabilityGates::all_open(),
    },
    AdapterCapabilityCell {
        key: AdapterCapabilityKey {
            source: AdapterSourceProduct::KimiCodeMembership,
            credential: AdapterCredentialClass::ApiKey,
            transport: AdapterUpstreamTransport::LocalBridgeChatCompletions,
            target: AgentId::Codex,
            protocol: AdapterTargetProtocol::OpenAiResponses,
            version: MATRIX_VERSION,
        },
        route: AdapterRoute::LocalBridge,
        support: AdapterSupport::Experimental,
        can_apply: true,
        reason: "Kimi Code 会员到 Codex 需要本地协议桥接。",
        limitations: KIMI_CODEX_LIMITS,
        rule_id: "kimi-membership-to-codex-v1",
        verified_at: VERIFIED_AT,
        gates: AdapterCapabilityGates::all_open(),
    },
    AdapterCapabilityCell {
        key: AdapterCapabilityKey {
            source: AdapterSourceProduct::KimiCodeMembership,
            credential: AdapterCredentialClass::ApiKey,
            transport: AdapterUpstreamTransport::NativeHttp,
            target: AgentId::Pi,
            protocol: AdapterTargetProtocol::PiProviderConfig,
            version: MATRIX_VERSION,
        },
        route: AdapterRoute::ConfigSync,
        support: AdapterSupport::Stable,
        can_apply: false,
        reason: "Kimi Code 会员可预览为 Pi 的配置同步。",
        limitations: KIMI_PI_LIMITS,
        rule_id: "kimi-membership-to-pi-v1",
        verified_at: VERIFIED_AT,
        gates: AdapterCapabilityGates::all_open(),
    },
    AdapterCapabilityCell {
        key: AdapterCapabilityKey {
            source: AdapterSourceProduct::AnthropicApi,
            credential: AdapterCredentialClass::ApiKey,
            transport: AdapterUpstreamTransport::NativeHttp,
            target: AgentId::Pi,
            protocol: AdapterTargetProtocol::PiProviderConfig,
            version: MATRIX_VERSION,
        },
        route: AdapterRoute::ConfigSync,
        support: AdapterSupport::Stable,
        can_apply: false,
        reason: "显式 Anthropic API Key 可预览为 Pi 的配置同步。",
        limitations: ANTHROPIC_PI_LIMITS,
        rule_id: "anthropic-api-to-pi-v1",
        verified_at: VERIFIED_AT,
        gates: AdapterCapabilityGates::all_open(),
    },
    // Codex OAuth Account → Claude Code: recorded candidate, every gate closed.
    // Decision surface remains unsupported / can_apply=false (both transports).
    AdapterCapabilityCell {
        key: AdapterCapabilityKey {
            source: AdapterSourceProduct::CodexChatGptSubscription,
            credential: AdapterCredentialClass::OauthAuthJson,
            transport: AdapterUpstreamTransport::CodexAppServer,
            target: AgentId::Claude,
            protocol: AdapterTargetProtocol::AnthropicMessages,
            version: "0",
        },
        route: AdapterRoute::LocalBridge,
        support: AdapterSupport::Experimental,
        can_apply: false,
        reason: CODEX_SUBSCRIPTION_TO_CLAUDE_REASON,
        limitations: CODEX_CLAUDE_LIMITS,
        rule_id: "codex-subscription-to-claude-app-server-v0",
        verified_at: VERIFIED_AT,
        gates: AdapterCapabilityGates::all_closed(),
    },
    AdapterCapabilityCell {
        key: AdapterCapabilityKey {
            source: AdapterSourceProduct::CodexChatGptSubscription,
            credential: AdapterCredentialClass::OauthAuthJson,
            transport: AdapterUpstreamTransport::CodexResponsesOauth,
            target: AgentId::Claude,
            protocol: AdapterTargetProtocol::AnthropicMessages,
            version: "0",
        },
        route: AdapterRoute::LocalBridge,
        support: AdapterSupport::Experimental,
        can_apply: false,
        reason: CODEX_SUBSCRIPTION_TO_CLAUDE_REASON,
        limitations: CODEX_CLAUDE_LIMITS,
        rule_id: "codex-subscription-to-claude-responses-v0",
        verified_at: VERIFIED_AT,
        gates: AdapterCapabilityGates::all_closed(),
    },
];

/// Resolve a cell by full key. Missing → [`None`] (caller must fail-closed).
pub fn lookup_adapter_capability(
    key: &AdapterCapabilityKey,
) -> Option<&'static AdapterCapabilityCell> {
    ADAPTER_CAPABILITY_MATRIX
        .iter()
        .find(|cell| cell.key == *key)
}

/// Evaluate the primary route for a classified source/target pair.
///
/// When multiple transport candidates exist (e.g. Codex → Claude), the first
/// matching source/credential/target row is used for the *decision surface*.
/// Apply remains closed unless that cell has `can_apply` and all gates open.
pub fn decide_adapter_capability(
    source: AdapterSourceProduct,
    credential: AdapterCredentialClass,
    target: AgentId,
) -> AdapterCapabilityDecision {
    if matches!(source, AdapterSourceProduct::Other)
        || matches!(credential, AdapterCredentialClass::Unknown)
    {
        return AdapterCapabilityDecision::unsupported(
            "AgentHub 暂未提供此来源到所选目标的适配规则。当前不支持不等于连接失效。",
        );
    }

    let candidates: Vec<_> = ADAPTER_CAPABILITY_MATRIX
        .iter()
        .filter(|cell| {
            cell.key.source == source
                && cell.key.credential == credential
                && cell.key.target == target
        })
        .collect();

    if candidates.is_empty() {
        return match (source, target) {
            (AdapterSourceProduct::KimiCodeMembership, _) => AdapterCapabilityDecision::unsupported(
                "Kimi Code 会员当前仅支持预览到 Claude、Codex 或 Pi。",
            ),
            (AdapterSourceProduct::AnthropicApi, _) => AdapterCapabilityDecision::unsupported(
                "Anthropic API Key 当前仅支持预览到 Pi。",
            ),
            (AdapterSourceProduct::CodexChatGptSubscription, AgentId::Claude) => {
                AdapterCapabilityDecision::unsupported_subscription_candidate(
                    CODEX_SUBSCRIPTION_TO_CLAUDE_REASON,
                )
            }
            (AdapterSourceProduct::CodexChatGptSubscription, _) => {
                AdapterCapabilityDecision::unsupported(
                    "AgentHub 暂未提供从 Codex 账户到所选目标的适配规则。当前不支持不等于连接失效。",
                )
            }
            (AdapterSourceProduct::Other, _) => AdapterCapabilityDecision::unsupported(
                "AgentHub 暂未提供此来源到所选目标的适配规则。当前不支持不等于连接失效。",
            ),
        };
    }

    // Prefer an open, applicable cell; otherwise keep the first recorded candidate
    // (still fail-closed via gates / can_apply).
    if let Some(open) = candidates
        .iter()
        .find(|cell| cell.can_apply && cell.gates.all_passed())
    {
        return AdapterCapabilityDecision::from_cell(open);
    }

    AdapterCapabilityDecision::from_cell(candidates[0])
}

/// Normalize gated experimental candidates to the public unsupported surface.
impl AdapterCapabilityDecision {
    /// Public analyze/plan surface: gated candidates always appear as unsupported.
    pub fn public_surface(mut self) -> Self {
        if !self.can_apply
            && matches!(
                self.transport,
                AdapterUpstreamTransport::CodexAppServer
                    | AdapterUpstreamTransport::CodexResponsesOauth
            )
        {
            self.route = AdapterRoute::Unsupported;
            self.support = AdapterSupport::Unsupported;
            self.gate_kind = AdapterGateKind::SubscriptionCandidate;
        }
        // Preview-only stable rules keep their route but never set can_apply.
        if !self.can_apply && self.route == AdapterRoute::Unsupported {
            self.support = AdapterSupport::Unsupported;
            if self.gate_kind == AdapterGateKind::None {
                self.gate_kind = AdapterGateKind::Unsupported;
            }
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_key_is_fail_closed() {
        let key = AdapterCapabilityKey {
            source: AdapterSourceProduct::AnthropicApi,
            credential: AdapterCredentialClass::ApiKey,
            transport: AdapterUpstreamTransport::NativeHttp,
            target: AgentId::Codex,
            protocol: AdapterTargetProtocol::OpenAiResponses,
            version: MATRIX_VERSION,
        };
        assert!(lookup_adapter_capability(&key).is_none());
        let decision = decide_adapter_capability(
            AdapterSourceProduct::AnthropicApi,
            AdapterCredentialClass::ApiKey,
            AgentId::Codex,
        )
        .public_surface();
        assert_eq!(decision.route, AdapterRoute::Unsupported);
        assert!(!decision.can_apply);
    }

    #[test]
    fn kimi_claude_and_codex_cells_are_applicable() {
        let claude = decide_adapter_capability(
            AdapterSourceProduct::KimiCodeMembership,
            AdapterCredentialClass::ApiKey,
            AgentId::Claude,
        );
        assert_eq!(claude.route, AdapterRoute::NativeEndpoint);
        assert!(claude.can_apply);
        assert_eq!(claude.rule_id, Some("kimi-membership-to-claude-v1"));

        let codex = decide_adapter_capability(
            AdapterSourceProduct::KimiCodeMembership,
            AdapterCredentialClass::ApiKey,
            AgentId::Codex,
        );
        assert_eq!(codex.route, AdapterRoute::LocalBridge);
        assert!(codex.can_apply);
        assert_eq!(codex.support, AdapterSupport::Experimental);
    }

    #[test]
    fn codex_oauth_to_claude_is_unsupported_and_cannot_apply() {
        let decision = decide_adapter_capability(
            AdapterSourceProduct::CodexChatGptSubscription,
            AdapterCredentialClass::OauthAuthJson,
            AgentId::Claude,
        )
        .public_surface();
        assert_eq!(decision.route, AdapterRoute::Unsupported);
        assert_eq!(decision.support, AdapterSupport::Unsupported);
        assert!(!decision.can_apply);
        assert_eq!(decision.reason, CODEX_SUBSCRIPTION_TO_CLAUDE_REASON);
        assert_eq!(decision.gate_kind, AdapterGateKind::SubscriptionCandidate);
        assert!(decision.reason.contains("当前不支持"));
        assert!(decision.reason.contains("门禁"));
        let gates = decision.gates.expect("candidate retains gate record");
        assert!(!gates.all_passed());

        // Both transport candidates exist in the matrix and stay closed.
        for transport in [
            AdapterUpstreamTransport::CodexAppServer,
            AdapterUpstreamTransport::CodexResponsesOauth,
        ] {
            let cell = lookup_adapter_capability(&AdapterCapabilityKey {
                source: AdapterSourceProduct::CodexChatGptSubscription,
                credential: AdapterCredentialClass::OauthAuthJson,
                transport,
                target: AgentId::Claude,
                protocol: AdapterTargetProtocol::AnthropicMessages,
                version: "0",
            })
            .expect("candidate cell");
            assert!(!cell.can_apply);
            assert!(!cell.gates.all_passed());
            assert!(!AdapterCapabilityDecision::from_cell(cell).can_apply);
        }
    }

    #[test]
    fn every_matrix_cell_has_reason_and_version() {
        for cell in ADAPTER_CAPABILITY_MATRIX {
            assert!(!cell.reason.is_empty());
            assert!(!cell.key.version.is_empty());
            assert!(!cell.rule_id.is_empty());
            assert!(!cell.verified_at.is_empty());
            if cell.can_apply {
                assert!(
                    cell.gates.all_passed(),
                    "{} claims can_apply with closed gates",
                    cell.rule_id
                );
            }
        }
    }

    #[test]
    fn preview_only_pi_rules_do_not_open_apply() {
        let kimi_pi = decide_adapter_capability(
            AdapterSourceProduct::KimiCodeMembership,
            AdapterCredentialClass::ApiKey,
            AgentId::Pi,
        );
        assert_eq!(kimi_pi.route, AdapterRoute::ConfigSync);
        assert!(!kimi_pi.can_apply);

        let anthropic_pi = decide_adapter_capability(
            AdapterSourceProduct::AnthropicApi,
            AdapterCredentialClass::ApiKey,
            AgentId::Pi,
        );
        assert_eq!(anthropic_pi.route, AdapterRoute::ConfigSync);
        assert!(!anthropic_pi.can_apply);
    }
}
