//! Adapter compatibility capability matrix (fail-closed).
//!
//! Dimensions: `source × credential × transport × target × protocol × version`.
//! Any combination without an explicit cell is [`AdapterRoute::Unsupported`] with
//! `can_apply = false`. This is separate from the per-agent feature matrix in
//! [`crate::models::capability`].
//!
//! Codex / ChatGPT official login → Claude is Messages local-bridge.
//! Codex / ChatGPT official login → Grok is Responses local-bridge; → Kimi / DSH
//! is Chat Completions local-bridge to Responses OAuth upstream. Grok subscription
//! → Claude / Codex is xAI Responses OAuth (cli-chat-proxy). Claude subscription
//! → Codex is Responses local-bridge to Anthropic Messages OAuth (preview; gates
//! closed). The App Server candidate remains closed.
//!
//! LocalBridge overlapping fields (rule_id / source / transport / target /
//! protocol / default_model) are declared once in [`local_bridge_edges`].

use super::{
    agent_bind_capability, speaks_intersect_accepts, AGENT_NO_WRITER_REASON,
    PROTOCOL_MISMATCH_REASON, SAME_PROTOCOL_NO_EDGE_REASON,
};
use crate::models::{
    AdapterGateKind, AdapterMaturity, AdapterRoute, AdapterSupport, AgentId, TicketSurface,
};

/// Shared public reason for the experimental Codex / ChatGPT subscription →
/// Claude Code Responses edge. Mock UI and core analyze must keep this string
/// in lockstep.
pub const CODEX_SUBSCRIPTION_TO_CLAUDE_REASON: &str =
    "Codex / ChatGPT 订阅会经本机路由接到 Claude Code.";

/// Shared public reason for the experimental Grok subscription → Claude Code edge.
pub const GROK_SUBSCRIPTION_TO_CLAUDE_REASON: &str = "Grok 登录会经本机路由接到 Claude Code。";

/// Shared public reason for Grok subscription to Codex local route.
pub const GROK_SUBSCRIPTION_TO_CODEX_REASON: &str = "Grok 登录会经本机路由接到 Codex。";

/// Codex / ChatGPT official login → Grok / Kimi / DSH local route.
pub const CODEX_SUBSCRIPTION_TO_GROK_REASON: &str = "Codex 官方登录会经本机路由接到 Grok。";
pub const CODEX_SUBSCRIPTION_TO_KIMI_REASON: &str = "Codex 官方登录会经本机路由接到 Kimi。";
pub const CODEX_SUBSCRIPTION_TO_DSH_REASON: &str =
    "Codex 官方登录会经本机路由接到 DeepSeek Harness。";
pub const CODEX_SUBSCRIPTION_TO_GROK_RULE_ID: &str = "codex-subscription-to-grok-v1";
pub const CODEX_SUBSCRIPTION_TO_KIMI_RULE_ID: &str = "codex-subscription-to-kimi-v1";
pub const CODEX_SUBSCRIPTION_TO_DSH_RULE_ID: &str = "codex-subscription-to-dsh-v1";
pub const CLAUDE_SUBSCRIPTION_TO_CODEX_RULE_ID: &str = "claude-subscription-to-codex-v1";

/// Closed reason: Grok login is not a supported upstream for Kimi.
pub const GROK_SUBSCRIPTION_TO_KIMI_REASON: &str =
    "Kimi 只认自己的官方 Key，接下不了这份 Grok 登录。";

/// Closed reason: Grok login is not a supported upstream for DSH.
pub const GROK_SUBSCRIPTION_TO_DSH_REASON: &str =
    "DSH 只认 DeepSeek 官方 Key，接下不了这份 Grok 登录。";

/// Claude subscription → Codex is ③-open; bind waits on fixtures.
pub const CLAUDE_SUBSCRIPTION_TO_CODEX_REASON: &str =
    "Claude 订阅接到 Codex 可以走本机转发，但规则还没做完，现在接不上。";

/// Closed fallback reason for Codex subscription shapes without the
/// `OauthAuthJson` Responses cell.
pub const CODEX_SUBSCRIPTION_TO_CLAUDE_CANDIDATE_REASON: &str = concat!(
    "Codex / ChatGPT 订阅现在还接不到 Claude Code。",
    "不会改配置，也不会开本机转发。",
    "这不表示现有连接坏了。",
    "可改用 Claude 自己的官方登录，或改用已支持的 API Key。",
);

pub const SUBSCRIPTION_PI_APPLY_LIMITS: &[&str] = &[
    "会把官方登录写进 Pi 认的位置；预览和日志不显示完整令牌。",
    "写进去之后由 Pi 自己续期；AgentHub 不会再刷一次。原来的工具和 Pi 一起续期可能互相踢下线。",
    "接上后会把自动生成的配置设成 Pi 当前在用的连接。",
];

pub const CLAUDE_SUBSCRIPTION_TO_PI_REASON: &str = "把这份 Claude 订阅写进 Pi 认的 Claude 登录。";
pub const CODEX_SUBSCRIPTION_TO_PI_REASON: &str =
    "把这份 Codex / ChatGPT 订阅写进 Pi 认的 Codex 登录。";
pub const GROK_SUBSCRIPTION_TO_PI_REASON: &str = "把这份 Grok 订阅写进 Pi 认的 Grok 登录。";

/// Product / origin that owns the selected Connection credentials.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AdapterSourceProduct {
    /// Kimi Code membership (`meta.preset = kimi-code-membership` **or** settings contain `api.kimi.com/coding`).
    KimiCodeMembership,
    /// Explicit Anthropic API Key (provider preset or account.extra.provider).
    AnthropicApi,
    /// Explicit OpenAI API Key (preset / extra.provider / official host).
    OpenaiApi,
    /// Explicit xAI API Key (preset / extra.provider / official host).
    XaiApi,
    /// GLM Coding Plan (Claude native_endpoint is experimental and writable).
    GlmCodingPlan,
    /// DeepSeek API (Claude native_endpoint is experimental and writable).
    DeepseekApi,
    /// Codex / ChatGPT subscription account (`auth_json` OAuth shape).
    CodexChatGptSubscription,
    /// Claude subscription OAuth account.
    ClaudeSubscription,
    /// Grok / xAI subscription OAuth account.
    XaiGrokSubscription,
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
    /// Anthropic API Key → Codex: local bridge with Messages upstream.
    LocalBridgeAnthropicMessages,
    /// Future Codex → Claude candidate: App Server transport (gate closed).
    CodexAppServer,
    /// Future Codex → Claude candidate: approved Responses + OAuth (gate closed).
    CodexResponsesOauth,
    /// Grok / xAI subscription OAuth: Responses upstream (CLI chat proxy).
    XaiResponsesOauth,
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
    /// DSH home-level Cordis patch + credentials reference (not a wire protocol).
    DshProviderConfig,
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
    /// RFC §7: polling stays fail-closed until an edge is evidenced.
    pub multi_account: bool,
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
                "现在还写不上去；不会改配置，也不会开本机转发。",
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
        AdapterUpstreamTransport::CodexAppServer
            | AdapterUpstreamTransport::CodexResponsesOauth
            | AdapterUpstreamTransport::XaiResponsesOauth
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

    // Recorded experimental candidate with can_apply=false (non-subscription) is
    // still a visible preview of its recorded route.
    if !cell.can_apply && cell.support == AdapterSupport::Experimental {
        return (cell.route, cell.support, AdapterGateKind::PreviewOnly);
    }

    let gate_kind = if can_apply {
        AdapterGateKind::None
    } else {
        AdapterGateKind::PreviewOnly
    };
    (cell.route, cell.support, gate_kind)
}

pub const VERIFIED_AT: &str = "2026-08-12";
pub const MATRIX_VERSION: &str = "1";

const KIMI_CLAUDE_LIMITS: &[&str] = &[
    "将写入 Claude 的 base URL 与凭据引用标记；不会在预览中传输明文 Key。",
    "应用后会切换当前 Claude Connection；请确认无其他进行中的配置写入。",
];

pub(super) const KIMI_CODEX_LIMITS: &[&str] = &[
    "将在本机地址启动本机转发，并切换 Codex 到该本地端点。",
    "AgentHub 需保持在托盘运行；退出前会尝试排空监听。",
    "本机转发仍是实验性覆盖；长流与工具调用可能受实现限制。",
    "固定端口被占用时会尝试重新分配端口并写回配置。",
];

const CODEX_NATIVE_API_LIMITS: &[&str] = &[
    "将把 Codex 配置为官方 Responses 端点；不会开本机转发。",
    "自动生成的配置只保存凭据引用；live 写入时才 materialize，回填前会 scrub 明文。",
    "当前未写入官方 ~/.codex/models.json；使用默认 model 与显式 Provider 配置。",
];

/// Official Codex / ChatGPT OAuth used on Codex itself.
pub const CODEX_SUBSCRIPTION_TO_CODEX_REASON: &str = "用这份官方登录接到 Codex。";
pub const CODEX_SUBSCRIPTION_TO_CODEX_RULE_ID: &str = "codex-subscription-to-codex-v1";

const CODEX_OFFICIAL_SELF_LIMITS: &[&str] = &[
    "会把这份官方登录写进 Codex；不会改到本机路由。",
    "应用后这份登录成为 Codex 当前登录。",
];

const KIMI_PI_LIMITS: &[&str] = &[
    "将写入 Pi models.json 的 kimi-for-coding 位置与凭据引用标记；不会在预览中传输明文 Key。",
    "接上后会把自动生成的配置设成 Pi 当前在用的连接；请确认无其他进行中的配置写入。",
];

const ANTHROPIC_PI_LIMITS: &[&str] = &[
    "将写入 Pi models.json 的 anthropic 位置与凭据引用标记；不会在预览中传输明文 Key。",
    "接上后会把自动生成的配置设成 Pi 当前在用的连接；请确认无其他进行中的配置写入。",
];

pub(super) const ANTHROPIC_CODEX_LIMITS: &[&str] = &[
    "将在本机地址启动本机转发，并切换 Codex 到该本地端点。",
    "AgentHub 需保持在托盘运行；退出前会尝试排空监听。",
    "本机转发仍是实验性覆盖：下游 Responses，上游 Anthropic Messages。",
    "固定端口被占用时会尝试重新分配端口并写回配置。",
];

const OPENAI_PI_LIMITS: &[&str] = &[
    "将写入 Pi models.json 的 openai 位置与凭据引用标记；不会在预览中传输明文 Key。",
    "接上后会把自动生成的配置设成 Pi 当前在用的连接；请确认无其他进行中的配置写入。",
];

pub(super) const OPENAI_CODEX_LIMITS: &[&str] = &[
    "将在本机地址启动本机转发，并切换 Codex 到该本地端点。",
    "AgentHub 需保持在托盘运行；退出前会尝试排空监听。",
    "本机转发仍是实验性覆盖：下游 Responses，上游 OpenAI Chat Completions。",
    "固定端口被占用时会尝试重新分配端口并写回配置。",
];

pub(super) const OPENAI_CLAUDE_LIMITS: &[&str] = &[
    "将在本机地址启动本机转发，并切换 Claude 到该本地端点。",
    "AgentHub 需保持在托盘运行；退出前会尝试排空监听。",
    "本机转发：下游 Messages，上游 OpenAI Chat Completions。",
    "固定端口被占用时会尝试重新分配端口并写回配置。",
];

pub(super) const OPENAI_GROK_BRIDGE_LIMITS: &[&str] = &[
    "将在本机地址启动本机转发，并切换 Grok 到该本地端点。",
    "AgentHub 需保持在托盘运行；退出前会尝试排空监听。",
    "本机转发：下游 Responses，上游 OpenAI Chat Completions。",
    "固定端口被占用时会尝试重新分配端口并写回配置。",
];

pub(super) const OPENAI_CHAT_BRIDGE_LIMITS: &[&str] = &[
    "将在本机地址启动本机转发，并切换目标 Agent 到该本地端点。",
    "AgentHub 需保持在托盘运行；退出前会尝试排空监听。",
    "本机转发：下游 Chat Completions，上游 OpenAI Chat Completions。",
    "固定端口被占用时会尝试重新分配端口并写回配置。",
];

const XAI_PI_LIMITS: &[&str] = &[
    "将写入 Pi models.json 的 xai 位置与凭据引用标记；不会在预览中传输明文 Key。",
    "接上后会把自动生成的配置设成 Pi 当前在用的连接；请确认无其他进行中的配置写入。",
];

const GLM_PI_LIMITS: &[&str] = &[
    "将写入 Pi models.json 的 glm-coding-plan 自定义位置（baseUrl、api、models）与凭据引用标记；不会在预览中传输明文 Key。",
    "自动生成的配置只保存凭据引用；live 写入时才 materialize，回填前会 scrub 明文。",
];

const DEEPSEEK_PI_LIMITS: &[&str] = &[
    "将写入 Pi models.json 的 deepseek 自定义位置（baseUrl、api、models）与凭据引用标记；不会在预览中传输明文 Key。",
    "自动生成的配置只保存凭据引用；live 写入时才 materialize，回填前会 scrub 明文。",
];

const GLM_CLAUDE_LIMITS: &[&str] = &[
    "将写入 Claude 的 GLM Coding Plan Anthropic 兼容 Base URL 与凭据引用标记；不会在预览中传输明文 Key。",
    "应用后会切换当前 Claude Connection；请确认无其他进行中的配置写入。",
    "实验性：官方 Anthropic 兼容入口；部分扩展字段可能被忽略或不支持。",
];

const DEEPSEEK_CLAUDE_LIMITS: &[&str] = &[
    "将写入 Claude 的 DeepSeek Anthropic 兼容 Base URL 与凭据引用标记；不会在预览中传输明文 Key。",
    "应用后会切换当前 Claude Connection；请确认无其他进行中的配置写入。",
    "实验性：官方 Anthropic 兼容入口；部分扩展字段可能被忽略或不支持。",
];

const DEEPSEEK_DSH_LIMITS: &[&str] = &[
    "将写入 DeepSeek Harness 的 home 级 provider 引用与凭据文件；不会把 API Key 写入 cordis.patch.yml。",
    "接上后会把自动生成的配置设成 DSH 当前在用的连接；请确认无其他进行中的配置写入。",
];

pub(super) const CODEX_CLAUDE_LIMITS: &[&str] = &[
    "会把 Claude 的 ANTHROPIC_BASE_URL / ANTHROPIC_AUTH_TOKEN 指向本机地址；上游 token 不进 Claude。",
    "实验性本机转发：Claude Messages → Codex Responses；AgentHub 需保持在托盘运行。",
    "Codex access token 过期后需重新同步 Codex 登录；Hub 本轮不自动 refresh。",
    "固定端口被占用时会尝试重新分配端口并写回配置。",
];

pub(super) const GROK_CLAUDE_LIMITS: &[&str] = &[
    "会把 Claude 的 ANTHROPIC_BASE_URL / ANTHROPIC_AUTH_TOKEN 指向本机地址；上游 xAI OAuth token 不进 Claude。",
    "实验性本机转发：Claude Messages → xAI Responses (cli-chat-proxy)；AgentHub 需保持在托盘运行。",
    "Grok access token 过期后需重新同步 Grok 登录；Hub 本轮不自动 refresh。",
    "固定端口被占用时会尝试重新分配端口并写回配置。",
];

pub(super) const GROK_CODEX_LIMITS: &[&str] = &[
    "会把 Codex 指到本机路由；上游 Grok 登录不会写入 Codex。",
    "AgentHub 需保持在托盘运行。",
    "Grok 登录过期后需重新同步；Hub 本轮不自动刷新。",
    "固定端口被占用时会尝试重新分配端口并写回配置。",
];

pub(super) const CODEX_CHAT_LIMITS: &[&str] = &[
    "会把目标 Agent 指到本机路由；上游 Codex 官方登录不会写入对方。",
    "AgentHub 需保持在托盘运行。",
    "Codex 登录过期后需重新同步；Hub 本轮不自动刷新。",
    "固定端口被占用时会尝试重新分配端口并写回配置。",
];

pub(super) const CLAUDE_CODEX_LIMITS: &[&str] = &[
    "会把 Codex 指到本机路由；上游 Claude 订阅 token 不会写入 Codex。",
    "实验性本机转发：下游 Responses，上游 Anthropic Messages OAuth。",
    "规则还没做完，现在接不上；thinking 无签名时降级关闭。",
    "Claude access token 过期后需重新同步登录；Hub 本轮不自动 refresh。",
];

const GROK_NATIVE_LIMITS: &[&str] = &[
    "将写入 Grok config.toml 的 OpenAI Chat Completions 模型位置；不会开本机转发。",
    "自动生成的配置只保存凭据引用；live 写入时才 materialize，回填前会 scrub 明文。",
    "仅接受官方 Kimi Code / OpenAI API 标记；Moonshot、自定义中转与仅 agent_id 不会自动升级。",
];

pub mod local_bridge_edges;
pub use local_bridge_edges::*;

/// Compile-time matrix. Order does not matter; lookup is by full key equality.
/// LocalBridge cells are `LocalBridgeEdge::to_cell()` — do not hand-write them.
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
        reason: "用这份 Kimi Code 会员接到 Claude，只改地址和模型。",
        limitations: KIMI_CLAUDE_LIMITS,
        rule_id: "kimi-membership-to-claude-v1",
        verified_at: VERIFIED_AT,
        gates: AdapterCapabilityGates::all_open(),
        multi_account: false,
    },
    AdapterCapabilityCell {
        key: AdapterCapabilityKey {
            source: AdapterSourceProduct::GlmCodingPlan,
            credential: AdapterCredentialClass::ApiKey,
            transport: AdapterUpstreamTransport::NativeHttp,
            target: AgentId::Codex,
            protocol: AdapterTargetProtocol::OpenAiResponses,
            version: MATRIX_VERSION,
        },
        route: AdapterRoute::NativeEndpoint,
        support: AdapterSupport::Experimental,
        can_apply: true,
        reason: "用这份 GLM 会员接到 Codex，只改地址和模型。",
        limitations: CODEX_NATIVE_API_LIMITS,
        rule_id: "glm-coding-plan-to-codex-v1",
        verified_at: "2026-08-15",
        gates: AdapterCapabilityGates::all_open(),
        multi_account: false,
    },
    AdapterCapabilityCell {
        key: AdapterCapabilityKey {
            source: AdapterSourceProduct::DeepseekApi,
            credential: AdapterCredentialClass::ApiKey,
            transport: AdapterUpstreamTransport::NativeHttp,
            target: AgentId::Codex,
            protocol: AdapterTargetProtocol::OpenAiResponses,
            version: MATRIX_VERSION,
        },
        route: AdapterRoute::NativeEndpoint,
        support: AdapterSupport::Experimental,
        can_apply: true,
        reason: "用这份 DeepSeek Key 接到 Codex，只改地址和模型。",
        limitations: CODEX_NATIVE_API_LIMITS,
        rule_id: "deepseek-api-to-codex-v1",
        verified_at: "2026-08-15",
        gates: AdapterCapabilityGates::all_open(),
        multi_account: false,
    },
    KIMI_CODEX_EDGE.to_cell(),
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
        can_apply: true,
        reason: "把这份 Kimi Code 会员写进 Pi 认的登录位置。",
        limitations: KIMI_PI_LIMITS,
        rule_id: "kimi-membership-to-pi-v1",
        verified_at: VERIFIED_AT,
        gates: AdapterCapabilityGates::all_open(),
        multi_account: false,
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
        can_apply: true,
        reason: "把这份 Anthropic API Key 写进 Pi 认的登录位置。",
        limitations: ANTHROPIC_PI_LIMITS,
        rule_id: "anthropic-api-to-pi-v1",
        verified_at: VERIFIED_AT,
        gates: AdapterCapabilityGates::all_open(),
        multi_account: false,
    },
    ANTHROPIC_CODEX_EDGE.to_cell(),
    AdapterCapabilityCell {
        key: AdapterCapabilityKey {
            source: AdapterSourceProduct::OpenaiApi,
            credential: AdapterCredentialClass::ApiKey,
            transport: AdapterUpstreamTransport::NativeHttp,
            target: AgentId::Pi,
            protocol: AdapterTargetProtocol::PiProviderConfig,
            version: MATRIX_VERSION,
        },
        route: AdapterRoute::ConfigSync,
        support: AdapterSupport::Stable,
        can_apply: true,
        reason: "把这份 OpenAI API Key 写进 Pi 认的登录位置。",
        limitations: OPENAI_PI_LIMITS,
        rule_id: "openai-api-to-pi-v1",
        verified_at: VERIFIED_AT,
        gates: AdapterCapabilityGates::all_open(),
        multi_account: false,
    },
    OPENAI_CODEX_EDGE.to_cell(),
    OPENAI_CLAUDE_EDGE.to_cell(),
    OPENAI_GROK_BRIDGE_EDGE.to_cell(),
    OPENAI_KIMI_BRIDGE_EDGE.to_cell(),
    OPENAI_DSH_BRIDGE_EDGE.to_cell(),
    AdapterCapabilityCell {
        key: AdapterCapabilityKey {
            source: AdapterSourceProduct::XaiApi,
            credential: AdapterCredentialClass::ApiKey,
            transport: AdapterUpstreamTransport::NativeHttp,
            target: AgentId::Pi,
            protocol: AdapterTargetProtocol::PiProviderConfig,
            version: MATRIX_VERSION,
        },
        route: AdapterRoute::ConfigSync,
        support: AdapterSupport::Stable,
        can_apply: true,
        reason: "把这份 xAI API Key 写进 Pi 认的登录位置。",
        limitations: XAI_PI_LIMITS,
        rule_id: "xai-api-to-pi-v1",
        verified_at: VERIFIED_AT,
        gates: AdapterCapabilityGates::all_open(),
        multi_account: false,
    },
    AdapterCapabilityCell {
        key: AdapterCapabilityKey {
            source: AdapterSourceProduct::GlmCodingPlan,
            credential: AdapterCredentialClass::ApiKey,
            transport: AdapterUpstreamTransport::NativeHttp,
            target: AgentId::Pi,
            protocol: AdapterTargetProtocol::PiProviderConfig,
            version: MATRIX_VERSION,
        },
        route: AdapterRoute::ConfigSync,
        support: AdapterSupport::Experimental,
        can_apply: true,
        reason: "把这份 GLM Coding Plan 写进 Pi 认的登录位置。",
        limitations: GLM_PI_LIMITS,
        rule_id: "glm-coding-plan-to-pi-v1",
        verified_at: "2026-08-15",
        gates: AdapterCapabilityGates::all_open(),
        multi_account: false,
    },
    AdapterCapabilityCell {
        key: AdapterCapabilityKey {
            source: AdapterSourceProduct::DeepseekApi,
            credential: AdapterCredentialClass::ApiKey,
            transport: AdapterUpstreamTransport::NativeHttp,
            target: AgentId::Pi,
            protocol: AdapterTargetProtocol::PiProviderConfig,
            version: MATRIX_VERSION,
        },
        route: AdapterRoute::ConfigSync,
        support: AdapterSupport::Experimental,
        can_apply: true,
        reason: "把这份 DeepSeek API 写进 Pi 认的登录位置。",
        limitations: DEEPSEEK_PI_LIMITS,
        rule_id: "deepseek-api-to-pi-v1",
        verified_at: "2026-08-15",
        gates: AdapterCapabilityGates::all_open(),
        multi_account: false,
    },
    AdapterCapabilityCell {
        key: AdapterCapabilityKey {
            source: AdapterSourceProduct::GlmCodingPlan,
            credential: AdapterCredentialClass::ApiKey,
            transport: AdapterUpstreamTransport::NativeHttp,
            target: AgentId::Claude,
            protocol: AdapterTargetProtocol::AnthropicMessages,
            version: MATRIX_VERSION,
        },
        route: AdapterRoute::NativeEndpoint,
        support: AdapterSupport::Experimental,
        can_apply: true,
        reason: "用这份 GLM Coding Plan 接到 Claude，只改地址和模型。",
        limitations: GLM_CLAUDE_LIMITS,
        rule_id: "glm-coding-plan-to-claude-v1",
        verified_at: VERIFIED_AT,
        gates: AdapterCapabilityGates::all_open(),
        multi_account: false,
    },
    AdapterCapabilityCell {
        key: AdapterCapabilityKey {
            source: AdapterSourceProduct::DeepseekApi,
            credential: AdapterCredentialClass::ApiKey,
            transport: AdapterUpstreamTransport::NativeHttp,
            target: AgentId::Claude,
            protocol: AdapterTargetProtocol::AnthropicMessages,
            version: MATRIX_VERSION,
        },
        route: AdapterRoute::NativeEndpoint,
        support: AdapterSupport::Experimental,
        can_apply: true,
        reason: "用这份 DeepSeek API 接到 Claude，只改地址和模型。",
        limitations: DEEPSEEK_CLAUDE_LIMITS,
        rule_id: "deepseek-api-to-claude-v1",
        verified_at: VERIFIED_AT,
        gates: AdapterCapabilityGates::all_open(),
        multi_account: false,
    },
    AdapterCapabilityCell {
        key: AdapterCapabilityKey {
            source: AdapterSourceProduct::DeepseekApi,
            credential: AdapterCredentialClass::ApiKey,
            transport: AdapterUpstreamTransport::NativeHttp,
            target: AgentId::Dsh,
            protocol: AdapterTargetProtocol::DshProviderConfig,
            version: MATRIX_VERSION,
        },
        route: AdapterRoute::ConfigSync,
        support: AdapterSupport::Stable,
        can_apply: true,
        reason: "把这份 DeepSeek Key 写进 DeepSeek Harness 认的登录位置。",
        limitations: DEEPSEEK_DSH_LIMITS,
        rule_id: "deepseek-api-to-dsh-v1",
        verified_at: VERIFIED_AT,
        gates: AdapterCapabilityGates::all_open(),
        multi_account: false,
    },
    AdapterCapabilityCell {
        key: AdapterCapabilityKey {
            source: AdapterSourceProduct::ClaudeSubscription,
            credential: AdapterCredentialClass::OauthOther,
            transport: AdapterUpstreamTransport::NativeHttp,
            target: AgentId::Pi,
            protocol: AdapterTargetProtocol::PiProviderConfig,
            version: MATRIX_VERSION,
        },
        route: AdapterRoute::ConfigSync,
        support: AdapterSupport::Experimental,
        can_apply: true,
        reason: CLAUDE_SUBSCRIPTION_TO_PI_REASON,
        limitations: SUBSCRIPTION_PI_APPLY_LIMITS,
        rule_id: "claude-subscription-to-pi-v1",
        verified_at: "2026-08-15",
        gates: AdapterCapabilityGates::all_open(),
        multi_account: false,
    },
    AdapterCapabilityCell {
        key: AdapterCapabilityKey {
            source: AdapterSourceProduct::CodexChatGptSubscription,
            credential: AdapterCredentialClass::OauthAuthJson,
            transport: AdapterUpstreamTransport::NativeHttp,
            target: AgentId::Pi,
            protocol: AdapterTargetProtocol::PiProviderConfig,
            version: MATRIX_VERSION,
        },
        route: AdapterRoute::ConfigSync,
        support: AdapterSupport::Experimental,
        can_apply: true,
        reason: CODEX_SUBSCRIPTION_TO_PI_REASON,
        limitations: SUBSCRIPTION_PI_APPLY_LIMITS,
        rule_id: "codex-subscription-to-pi-v1",
        verified_at: "2026-08-15",
        gates: AdapterCapabilityGates::all_open(),
        multi_account: false,
    },
    AdapterCapabilityCell {
        key: AdapterCapabilityKey {
            source: AdapterSourceProduct::CodexChatGptSubscription,
            credential: AdapterCredentialClass::OauthOther,
            transport: AdapterUpstreamTransport::NativeHttp,
            target: AgentId::Pi,
            protocol: AdapterTargetProtocol::PiProviderConfig,
            version: MATRIX_VERSION,
        },
        route: AdapterRoute::ConfigSync,
        support: AdapterSupport::Experimental,
        can_apply: true,
        reason: CODEX_SUBSCRIPTION_TO_PI_REASON,
        limitations: SUBSCRIPTION_PI_APPLY_LIMITS,
        rule_id: "codex-subscription-to-pi-v1",
        verified_at: "2026-08-15",
        gates: AdapterCapabilityGates::all_open(),
        multi_account: false,
    },
    AdapterCapabilityCell {
        key: AdapterCapabilityKey {
            source: AdapterSourceProduct::XaiGrokSubscription,
            credential: AdapterCredentialClass::OauthOther,
            transport: AdapterUpstreamTransport::NativeHttp,
            target: AgentId::Pi,
            protocol: AdapterTargetProtocol::PiProviderConfig,
            version: MATRIX_VERSION,
        },
        route: AdapterRoute::ConfigSync,
        support: AdapterSupport::Experimental,
        can_apply: true,
        reason: GROK_SUBSCRIPTION_TO_PI_REASON,
        limitations: SUBSCRIPTION_PI_APPLY_LIMITS,
        rule_id: "grok-subscription-to-pi-v1",
        verified_at: "2026-08-15",
        gates: AdapterCapabilityGates::all_open(),
        multi_account: false,
    },
    AdapterCapabilityCell {
        key: AdapterCapabilityKey {
            source: AdapterSourceProduct::KimiCodeMembership,
            credential: AdapterCredentialClass::ApiKey,
            transport: AdapterUpstreamTransport::NativeHttp,
            target: AgentId::Grok,
            protocol: AdapterTargetProtocol::OpenAiChatCompletions,
            version: MATRIX_VERSION,
        },
        route: AdapterRoute::NativeEndpoint,
        support: AdapterSupport::Experimental,
        can_apply: true,
        reason: "用这份 Kimi Code 会员接到 Grok，只改地址和模型。",
        limitations: GROK_NATIVE_LIMITS,
        rule_id: "kimi-membership-to-grok-v1",
        verified_at: "2026-08-15",
        gates: AdapterCapabilityGates::all_open(),
        multi_account: false,
    },
    AdapterCapabilityCell {
        key: AdapterCapabilityKey {
            source: AdapterSourceProduct::OpenaiApi,
            credential: AdapterCredentialClass::ApiKey,
            transport: AdapterUpstreamTransport::NativeHttp,
            target: AgentId::Grok,
            protocol: AdapterTargetProtocol::OpenAiChatCompletions,
            version: MATRIX_VERSION,
        },
        route: AdapterRoute::NativeEndpoint,
        support: AdapterSupport::Experimental,
        can_apply: true,
        reason: "用这份 OpenAI Key 接到 Grok，只改地址和模型。",
        limitations: GROK_NATIVE_LIMITS,
        rule_id: "openai-api-to-grok-v1",
        verified_at: "2026-08-15",
        gates: AdapterCapabilityGates::all_open(),
        multi_account: false,
    },
    GROK_CLAUDE_EDGE.to_cell(),
    GROK_CODEX_EDGE.to_cell(),
    // Codex OAuth Account → Claude Code App Server remains a closed candidate.
    CODEX_CLAUDE_APP_SERVER_EDGE.to_cell(),
    // Codex OAuth Account → Claude Code Responses is the experimental bridge
    // write surface. Only this OauthAuthJson cell is open.
    CODEX_CLAUDE_RESPONSES_EDGE.to_cell(),
    CODEX_CLAUDE_OAUTH_OTHER_EDGE.to_cell(),
    AdapterCapabilityCell {
        key: AdapterCapabilityKey {
            source: AdapterSourceProduct::CodexChatGptSubscription,
            credential: AdapterCredentialClass::OauthAuthJson,
            transport: AdapterUpstreamTransport::NativeHttp,
            target: AgentId::Codex,
            protocol: AdapterTargetProtocol::OpenAiResponses,
            version: MATRIX_VERSION,
        },
        route: AdapterRoute::NativeEndpoint,
        support: AdapterSupport::Stable,
        can_apply: true,
        reason: CODEX_SUBSCRIPTION_TO_CODEX_REASON,
        limitations: CODEX_OFFICIAL_SELF_LIMITS,
        rule_id: CODEX_SUBSCRIPTION_TO_CODEX_RULE_ID,
        verified_at: "2026-08-20",
        gates: AdapterCapabilityGates::all_open(),
        multi_account: false,
    },
    AdapterCapabilityCell {
        key: AdapterCapabilityKey {
            source: AdapterSourceProduct::CodexChatGptSubscription,
            credential: AdapterCredentialClass::OauthOther,
            transport: AdapterUpstreamTransport::NativeHttp,
            target: AgentId::Codex,
            protocol: AdapterTargetProtocol::OpenAiResponses,
            version: MATRIX_VERSION,
        },
        route: AdapterRoute::NativeEndpoint,
        support: AdapterSupport::Stable,
        can_apply: true,
        reason: CODEX_SUBSCRIPTION_TO_CODEX_REASON,
        limitations: CODEX_OFFICIAL_SELF_LIMITS,
        rule_id: CODEX_SUBSCRIPTION_TO_CODEX_RULE_ID,
        verified_at: "2026-08-20",
        gates: AdapterCapabilityGates::all_open(),
        multi_account: false,
    },
    CODEX_GROK_EDGE.to_cell(),
    CODEX_GROK_OAUTH_OTHER_EDGE.to_cell(),
    CODEX_KIMI_EDGE.to_cell(),
    CODEX_KIMI_OAUTH_OTHER_EDGE.to_cell(),
    CODEX_DSH_EDGE.to_cell(),
    CODEX_DSH_OAUTH_OTHER_EDGE.to_cell(),
    CLAUDE_CODEX_EDGE.to_cell(),
];

/// Resolve a cell by full key. Missing → [`None`] (caller must fail-closed).
pub fn lookup_adapter_capability(
    key: &AdapterCapabilityKey,
) -> Option<&'static AdapterCapabilityCell> {
    ADAPTER_CAPABILITY_MATRIX
        .iter()
        .find(|cell| cell.key == *key)
}

/// RFC §7: polling is fail-closed. Unknown rules do not open the gate.
pub fn local_bridge_multi_account(rule_id: &str) -> bool {
    ADAPTER_CAPABILITY_MATRIX
        .iter()
        .find(|cell| cell.rule_id == rule_id)
        .is_some_and(|cell| cell.multi_account)
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
    // Bind-entry table first: no writer → infeasible. Never opens can_apply.
    // Cursor must take this path; do not fall through to source-product copy.
    if !agent_bind_capability(target).writer {
        return AdapterCapabilityDecision::unsupported(AGENT_NO_WRITER_REASON);
    }

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
        if matches!(
            (source, target),
            (AdapterSourceProduct::XaiGrokSubscription, AgentId::Kimi)
        ) {
            return AdapterCapabilityDecision::unsupported(GROK_SUBSCRIPTION_TO_KIMI_REASON);
        }
        if matches!(
            (source, target),
            (AdapterSourceProduct::XaiGrokSubscription, AgentId::Dsh)
        ) {
            return AdapterCapabilityDecision::unsupported(GROK_SUBSCRIPTION_TO_DSH_REASON);
        }
        // Recorded gated candidate: keep subscription messaging if the cell is absent.
        if matches!(
            (source, target),
            (
                AdapterSourceProduct::CodexChatGptSubscription,
                AgentId::Claude
            )
        ) {
            return AdapterCapabilityDecision::unsupported_subscription_candidate(
                CODEX_SUBSCRIPTION_TO_CLAUDE_CANDIDATE_REASON,
            );
        }
        // 票.speaks ∩ agent.accepts — protocol graph, not a product whitelist.
        let speaks = TicketSurface::from_product(source).speaks();
        let reason = if speaks_intersect_accepts(speaks, agent_bind_capability(target).accepts) {
            SAME_PROTOCOL_NO_EDGE_REASON
        } else {
            PROTOCOL_MISMATCH_REASON
        };
        return AdapterCapabilityDecision::unsupported(reason);
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

/// Map a capability decision onto planner maturity.
///
/// Does not authorize writes. `can_apply` remains matrix-open ∩ plan `write_gate`.
pub fn adapter_maturity_from_decision(decision: &AdapterCapabilityDecision) -> AdapterMaturity {
    if decision.can_apply {
        return match decision.support {
            AdapterSupport::Stable => AdapterMaturity::Stable,
            AdapterSupport::Experimental => AdapterMaturity::Experimental,
            AdapterSupport::Unsupported => AdapterMaturity::None,
        };
    }

    // Recorded cell (rule id) or explain-only subscription / preview-only gate.
    if decision.rule_id.is_some()
        || matches!(
            decision.gate_kind,
            AdapterGateKind::SubscriptionCandidate | AdapterGateKind::PreviewOnly
        )
    {
        return AdapterMaturity::Preview;
    }

    AdapterMaturity::None
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
                    | AdapterUpstreamTransport::XaiResponsesOauth
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
mod tests;
