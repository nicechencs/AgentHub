use std::fmt;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use super::account::{AccountPicker, BridgeMemberSpec, MemberHealth, PickedMember};
use super::route_index::EffectiveRouteIndex;
use crate::models::{AdapterSourceProduct, AgentId, RouteDownstreamDialect, RouteSchedulePolicy};

/// Opaque callback that may rotate the in-memory upstream bearer.
/// The host must not depend on AccountService types.
pub type UpstreamAuthReload = Arc<dyn Fn() -> Option<String> + Send + Sync>;

/// An already resolved upstream credential. It is intentionally supplied by the caller and
/// retained only in the live runtime process; do not persist or serialise it.
///
/// The inner cell is shared across spec/listener clones so a 401 retry can swap
/// the upstream bearer in place without restarting the loopback listener.
/// `revision` is the compare-and-swap token for singleflight reload: a stale
/// refresh must not overwrite a newer credential.
#[derive(Clone)]
pub struct ResolvedAuth {
    cell: Arc<Mutex<AuthCell>>,
}

struct AuthCell {
    token: String,
    revision: u64,
}

impl ResolvedAuth {
    pub fn bearer(token: impl Into<String>) -> Self {
        Self {
            cell: Arc::new(Mutex::new(AuthCell {
                token: token.into(),
                revision: 0,
            })),
        }
    }

    pub(crate) fn token(&self) -> String {
        self.cell
            .lock()
            .map(|guard| guard.token.clone())
            .unwrap_or_default()
    }

    pub(crate) fn revision(&self) -> u64 {
        self.cell.lock().map(|guard| guard.revision).unwrap_or(0)
    }

    /// Whether the cell currently holds a non-empty bearer. Does not expose the secret.
    pub fn has_token(&self) -> bool {
        self.cell
            .lock()
            .map(|guard| !guard.token.trim().is_empty())
            .unwrap_or(false)
    }

    pub(crate) fn replace_token(&self, token: impl Into<String>) {
        if let Ok(mut guard) = self.cell.lock() {
            guard.token = token.into();
            guard.revision = guard.revision.saturating_add(1);
        }
    }

    /// Write only if `expected` still matches. Returns false when a newer
    /// revision won or the token is unchanged.
    pub(crate) fn replace_token_at_revision(&self, expected: u64, token: &str) -> bool {
        let Ok(mut guard) = self.cell.lock() else {
            return false;
        };
        if guard.revision != expected || guard.token == token {
            return false;
        }
        guard.token = token.to_owned();
        guard.revision = expected.saturating_add(1);
        true
    }

    /// Adopt a shared-reload token. Same-cell waiters already hold it;
    /// other cells copy it only if `expected` still matches. A newer
    /// revision must not be overwritten.
    pub(crate) fn apply_reloaded_token(&self, expected: u64, token: &str) -> bool {
        let Ok(mut guard) = self.cell.lock() else {
            return false;
        };
        if guard.token == token {
            return true;
        }
        if guard.revision != expected {
            return false;
        }
        guard.token = token.to_owned();
        guard.revision = expected.saturating_add(1);
        true
    }
}

impl std::fmt::Debug for ResolvedAuth {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("ResolvedAuth(REDACTED)")
    }
}

/// Which upstream wire protocol the host should speak.
///
/// Selected from the adapter profile / route, never inferred from the
/// downstream Responses request. Downstream identity stays the local bearer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BridgeUpstreamProtocol {
    /// OpenAI-compatible Chat Completions upstream (Kimi Code membership, OpenAI API, and similar).
    /// Formerly `KimiChatCompletions`; this enum is not serde, so no persisted schema change.
    OpenAiChatCompletions,
    /// Anthropic API Key → Codex: Messages + `x-api-key` / `anthropic-version`.
    AnthropicMessages,
    /// Codex subscription OAuth: Responses upstream (ChatGPT). Local surface is
    /// per-target ([`BridgeLocalSurface`]).
    CodexResponsesOauth,
    /// Grok / xAI subscription OAuth: Responses upstream (CLI chat proxy).
    XaiResponsesOauth,
}

/// Responses wire dialect expected by a downstream client.
///
/// This is deliberately a runtime concern rather than a property inferred
/// from the request body or URL.  The local bearer selects an EdgeState and
/// the EdgeState carries this profile for the lifetime of the request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResponsesDialect {
    Codex,
    Grok,
}

impl ResponsesDialect {
    fn from_route_dialect(dialect: RouteDownstreamDialect) -> Option<Self> {
        match dialect {
            RouteDownstreamDialect::Codex => Some(Self::Codex),
            RouteDownstreamDialect::Grok => Some(Self::Grok),
            RouteDownstreamDialect::Claude
            | RouteDownstreamDialect::Kimi
            | RouteDownstreamDialect::Dsh
            | RouteDownstreamDialect::Generic => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::Grok => "grok",
        }
    }
}

/// Explicit downstream Responses client profile.
///
/// `None` on [`BridgeStartSpec`] means a generic/non-Responses host test and
/// preserves the existing identity-relay behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DownstreamResponsesProfile {
    pub dialect: ResponsesDialect,
}

impl DownstreamResponsesProfile {
    pub const fn new(dialect: ResponsesDialect) -> Self {
        Self { dialect }
    }

    /// Resolve the wire profile only when the local listener exposes the
    /// Responses surface. Messages and Chat Completions have their own
    /// response contracts and must never accidentally inherit a Responses
    /// dialect from a persisted route pool.
    pub fn from_surface_and_route_dialect(
        surface: BridgeLocalSurface,
        dialect: RouteDownstreamDialect,
    ) -> Option<Self> {
        if surface != BridgeLocalSurface::Responses {
            return None;
        }
        ResponsesDialect::from_route_dialect(dialect).map(Self::new)
    }
}

/// Local HTTP dialect this edge exposes. One edge, one surface.
///
/// Chosen from the *target* Agent, not sniffed from the upstream host.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BridgeLocalSurface {
    /// `POST /v1/responses` (Codex, Grok CLI `api_backend=responses`).
    Responses,
    /// `POST /v1/messages` (Claude Code).
    Messages,
    /// `POST /v1/chat/completions` (Kimi / DSH).
    ChatCompletions,
}

impl Default for BridgeUpstreamProtocol {
    fn default() -> Self {
        Self::OpenAiChatCompletions
    }
}

/// Upstream provider configuration. `source_id` is for status/audit
/// correlation only; the host neither resolves it nor touches AgentHub storage.
#[derive(Debug, Clone)]
pub struct BridgeUpstreamConfig {
    pub base_url: String,
    pub model: Option<String>,
    pub source_id: Option<String>,
    pub auth: ResolvedAuth,
    pub protocol: BridgeUpstreamProtocol,
    pub local_surface: BridgeLocalSurface,
}

/// Inputs required to start one independent local bridge instance.
#[derive(Clone)]
pub struct BridgeStartSpec {
    pub profile_id: String,
    /// A requested TCP port. `0` asks the OS for an available loopback port.
    pub port: u16,
    /// Bearer token accepted by the local HTTP endpoint. This value is never returned by status.
    pub local_token: String,
    pub upstream: BridgeUpstreamConfig,
    /// Credential-free model ids served by `GET /v1/models`. Synthesized from
    /// the adapter mapping table; never a secret.
    pub listed_models: Vec<String>,
    /// Optional owner-split follow/refresh. Identity is ignored when comparing live specs.
    pub reload_upstream_auth: Option<UpstreamAuthReload>,
    /// Ordered C1 members. Empty means synthesize a single lead from `upstream.auth`.
    pub members: Vec<BridgeMemberSpec>,
    /// RFC §7 matrix cell. Closed (default) keeps only the lead even if `members` is longer.
    pub multi_account: bool,
    /// Mapping identity for request-scoped model switch. Optional for host tests.
    pub mapping_source: Option<AdapterSourceProduct>,
    pub mapping_target: Option<AgentId>,
    /// Explicit dialect expected by the downstream Responses client. This is
    /// carried by the authenticated edge rather than inferred from mapping.
    pub downstream_responses_profile: Option<DownstreamResponsesProfile>,
    pub custom_openai: bool,
    /// Shared resolver snapshot. `None` keeps lead + `switch_edge_for_model`.
    pub route_index: Option<EffectiveRouteIndex>,
    /// `feature.codex_ingress_grok_upstream`. Fail-closed default.
    pub codex_ingress_grok_upstream: bool,
    /// `feature.grok_ingress_codex_upstream`. Fail-closed default.
    pub grok_ingress_codex_upstream: bool,
    /// Copied from the RoutePool. Default remains `priority_failover`.
    pub schedule_policy: RouteSchedulePolicy,
}

impl fmt::Debug for BridgeStartSpec {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BridgeStartSpec")
            .field("profile_id", &self.profile_id)
            .field("port", &self.port)
            .field("local_token", &"REDACTED")
            .field("upstream", &self.upstream)
            .field("listed_models", &self.listed_models)
            .field("reload_upstream_auth", &self.reload_upstream_auth.is_some())
            .field("members", &self.members)
            .field("multi_account", &self.multi_account)
            .field("mapping_source", &self.mapping_source)
            .field("mapping_target", &self.mapping_target)
            .field(
                "downstream_responses_profile",
                &self.downstream_responses_profile,
            )
            .field("custom_openai", &self.custom_openai)
            .field(
                "route_index",
                &self.route_index.as_ref().map(|index| index.generation),
            )
            .field(
                "codex_ingress_grok_upstream",
                &self.codex_ingress_grok_upstream,
            )
            .field(
                "grok_ingress_codex_upstream",
                &self.grok_ingress_codex_upstream,
            )
            .field("schedule_policy", &self.schedule_policy)
            .finish()
    }
}

impl BridgeStartSpec {
    pub fn new(
        profile_id: impl Into<String>,
        port: u16,
        local_token: impl Into<String>,
        upstream: BridgeUpstreamConfig,
    ) -> Self {
        Self {
            profile_id: profile_id.into(),
            port,
            local_token: local_token.into(),
            upstream,
            listed_models: Vec::new(),
            reload_upstream_auth: None,
            members: Vec::new(),
            multi_account: false,
            mapping_source: None,
            mapping_target: None,
            downstream_responses_profile: None,
            custom_openai: false,
            route_index: None,
            codex_ingress_grok_upstream: false,
            grok_ingress_codex_upstream: false,
            schedule_policy: RouteSchedulePolicy::PriorityFailover,
        }
    }

    pub fn with_members(mut self, members: Vec<BridgeMemberSpec>) -> Self {
        self.members = members;
        self
    }

    pub fn with_multi_account(mut self, multi_account: bool) -> Self {
        self.multi_account = multi_account;
        self
    }

    pub fn with_mapping(
        mut self,
        source: AdapterSourceProduct,
        target: AgentId,
        custom_openai: bool,
    ) -> Self {
        self.mapping_source = Some(source);
        self.mapping_target = Some(target);
        self.custom_openai = custom_openai;
        self
    }

    pub fn with_downstream_responses_profile(
        mut self,
        profile: Option<DownstreamResponsesProfile>,
    ) -> Self {
        self.downstream_responses_profile = profile;
        self
    }

    /// `route_index` skips the RFC §7 lead-only trim.
    pub fn account_picker(&self) -> AccountPicker {
        let members = if self.members.is_empty() {
            vec![self.lead_member()]
        } else {
            self.members.iter().map(PickedMember::from).collect()
        };
        let v2_pool = self.route_index.is_some() && members.len() > 1;
        if self.route_index.is_some() {
            AccountPicker::with_policy(
                members,
                self.multi_account || v2_pool,
                None,
                self.schedule_policy,
            )
        } else {
            AccountPicker::from_members(members, self.multi_account, None)
        }
    }

    fn lead_member(&self) -> PickedMember {
        let source_id = self.upstream.source_id.clone().unwrap_or_default();
        let ticket_id = if source_id.is_empty() {
            String::new()
        } else {
            format!("account:{source_id}")
        };
        PickedMember::new(
            ticket_id,
            "account",
            source_id,
            self.profile_id.clone(),
            self.upstream.auth.clone(),
            self.reload_upstream_auth.clone(),
            MemberHealth::Renewable,
        )
    }

    pub fn with_listed_models(mut self, listed_models: Vec<String>) -> Self {
        self.listed_models = listed_models;
        self
    }

    pub fn with_route_index(mut self, index: EffectiveRouteIndex) -> Self {
        self.route_index = Some(index);
        self
    }

    pub fn with_reload_upstream_auth(mut self, reload: Option<UpstreamAuthReload>) -> Self {
        self.reload_upstream_auth = reload;
        self
    }

    pub fn with_pair_adapter_flags(
        mut self,
        codex_ingress_grok_upstream: bool,
        grok_ingress_codex_upstream: bool,
    ) -> Self {
        self.codex_ingress_grok_upstream = codex_ingress_grok_upstream;
        self.grok_ingress_codex_upstream = grok_ingress_codex_upstream;
        self
    }

    pub fn with_schedule_policy(mut self, schedule_policy: RouteSchedulePolicy) -> Self {
        self.schedule_policy = schedule_policy;
        self
    }
}

/// Safe, credential-free runtime state exposed to callers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BridgeRuntimeStatus {
    pub profile_id: String,
    pub port: u16,
    pub running: bool,
    pub started_at: SystemTime,
    pub source_id: Option<String>,
    /// Listener lifecycle only. It deliberately does not infer that the upstream accepts a
    /// credential or is currently reachable.
    pub state: BridgeRuntimeState,
    pub upstream_status: BridgeUpstreamStatus,
}

/// A bridge listener's observable lifecycle. A stopped or failed listener is never represented
/// as a successful-but-false `running` flag alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BridgeRuntimeState {
    Starting,
    Running,
    Stopping,
    Stopped,
    Error,
    Degraded,
}

/// Last observed upstream outcome. Health and status reads never probe the provider; they
/// only report this stored value so a UI poll cannot create a billable request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BridgeUpstreamStatus {
    /// No successful or failed upstream outcome has been observed yet.
    Unknown,
    /// The last observed health or request outcome succeeded.
    Connected,
    /// The local listener is stopped; no live upstream session remains.
    Stopped,
    /// The listener is still up, but the last health/auth/upstream outcome failed.
    Degraded,
    /// A host/status read failed. A missing instance after a clean stop is `Stopped`.
    Unavailable,
}

impl BridgeUpstreamStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Connected => "connected",
            Self::Stopped => "stopped",
            Self::Degraded => "degraded",
            Self::Unavailable => "unavailable",
        }
    }
}
