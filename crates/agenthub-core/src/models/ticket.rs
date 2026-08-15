//! Read-only Ticket / Binding wallet DTOs (connection-binding-model §6 step 1).
//!
//! Aggregation lives in [`crate::services::TicketReadService`]. These types are
//! pure serde wire shapes — no business logic.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{AdapterSourceKind, AdapterSourceProduct, AgentId};

/// `plan_ticket` rejects generated projection providers (not tickets).
pub const PROJECTION_NOT_A_TICKET: &str = "投影不是票 / 禁止二次投影";

/// Wallet list payload for `list_ticket_wallet`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TicketWallet {
    pub tickets: Vec<Ticket>,
    pub bindings: Vec<TicketBinding>,
}

/// One authorization ticket aggregated from an account or provider row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Ticket {
    /// Stable id: `provider:<row-id>` or `account:<row-id>`.
    pub id: String,
    pub source_kind: AdapterSourceKind,
    pub source_id: String,
    pub agent_id: AgentId,
    pub label: String,
    pub surface: TicketSurface,
    pub credential_class: TicketCredentialClass,
    pub speaks: Vec<TicketProtocol>,
    /// Audit-only: agent that owned the underlying row when imported.
    pub imported_from: Option<AgentId>,
}

/// Outcome of reading a persisted `extra.surface` / `meta.surface` field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PersistedTicketSurface {
    /// No `surface` key — classify and best-effort write back.
    Missing,
    /// Known wire value for this version.
    Known(TicketSurface),
    /// Key present but this version does not recognize it.
    /// Display as [`TicketSurface::Unknown`]; do not overwrite the stored value.
    Unrecognized,
}

/// Product surface recognized by classify (or `unknown`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TicketSurface {
    KimiCodeMembership,
    AnthropicApi,
    CodexChatgptSubscription,
    Unknown,
}

impl TicketSurface {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::KimiCodeMembership => "kimi-code-membership",
            Self::AnthropicApi => "anthropic-api",
            Self::CodexChatgptSubscription => "codex-chatgpt-subscription",
            Self::Unknown => "unknown",
        }
    }

    pub fn from_product(product: AdapterSourceProduct) -> Self {
        match product {
            AdapterSourceProduct::KimiCodeMembership => Self::KimiCodeMembership,
            AdapterSourceProduct::AnthropicApi => Self::AnthropicApi,
            AdapterSourceProduct::CodexChatGptSubscription => Self::CodexChatgptSubscription,
            AdapterSourceProduct::Other => Self::Unknown,
        }
    }

    /// Parse a persisted `extra.surface` / `meta.surface` wire value.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim() {
            "kimi-code-membership" => Some(Self::KimiCodeMembership),
            "anthropic-api" => Some(Self::AnthropicApi),
            "codex-chatgpt-subscription" => Some(Self::CodexChatgptSubscription),
            "unknown" => Some(Self::Unknown),
            _ => None,
        }
    }

    /// Read a persisted `surface` field from account extra or provider meta.
    ///
    /// Distinguishes a missing key (classify + write back) from a key this
    /// version does not recognize (display as [`TicketSurface::Unknown`], do
    /// not overwrite).
    pub fn from_persisted_json(blob: &Value) -> PersistedTicketSurface {
        let Some(raw) = blob.get("surface") else {
            return PersistedTicketSurface::Missing;
        };
        match raw.as_str().and_then(Self::parse) {
            Some(surface) => PersistedTicketSurface::Known(surface),
            None => PersistedTicketSurface::Unrecognized,
        }
    }

    pub fn speaks(self) -> &'static [TicketProtocol] {
        match self {
            Self::KimiCodeMembership => &[
                TicketProtocol::AnthropicMessages,
                TicketProtocol::OpenaiChat,
            ],
            Self::AnthropicApi => &[TicketProtocol::AnthropicMessages],
            Self::CodexChatgptSubscription => &[TicketProtocol::OpenaiResponses],
            Self::Unknown => &[],
        }
    }
}

/// Credential family exposed on the wallet (not AdapterCredentialClass).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TicketCredentialClass {
    ApiKey,
    Oauth,
    Unknown,
}

impl TicketCredentialClass {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ApiKey => "api_key",
            Self::Oauth => "oauth",
            Self::Unknown => "unknown",
        }
    }
}

/// Upstream protocols a ticket can speak.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TicketProtocol {
    AnthropicMessages,
    OpenaiChat,
    OpenaiResponses,
}

impl TicketProtocol {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AnthropicMessages => "anthropic-messages",
            Self::OpenaiChat => "openai-chat",
            Self::OpenaiResponses => "openai-responses",
        }
    }
}

/// Binding route names on the wallet wire (collapsed from AdapterRoute).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TicketBindingRoute {
    Native,
    Reshape,
    Bridge,
}

impl TicketBindingRoute {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Native => "native",
            Self::Reshape => "reshape",
            Self::Bridge => "bridge",
        }
    }
}

/// One ticket↔agent usage row (active or inactive).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TicketBinding {
    pub ticket_id: String,
    pub agent_id: AgentId,
    pub route: TicketBindingRoute,
    pub active: bool,
    pub profile_id: Option<String>,
    /// Present only when `route == bridge`; otherwise `null`.
    pub bridge: Option<TicketBridgeRuntime>,
}

/// Best-effort local bridge runtime snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TicketBridgeRuntime {
    pub port: Option<u16>,
    pub running: bool,
}

/// Input for `plan_ticket` / [`crate::services::TicketReadService::plan`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TicketPlanRequest {
    pub ticket_id: String,
    pub target_agent_id: AgentId,
}

/// Input for `bind_ticket` / [`crate::services::TicketBindService::bind`].
pub type TicketBindRequest = TicketPlanRequest;

/// Input for `unbind_ticket` / [`crate::services::TicketBindService::unbind`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TicketUnbindRequest {
    pub ticket_id: String,
    pub agent_id: AgentId,
}

/// Write `surface` into an extra/meta JSON object (creates an object if needed).
pub fn attach_persisted_surface(blob: &mut Value, surface: TicketSurface) {
    let encoded = Value::String(surface.as_str().to_owned());
    if let Some(obj) = blob.as_object_mut() {
        obj.insert("surface".into(), encoded);
        return;
    }
    *blob = serde_json::json!({ "surface": surface.as_str() });
}

/// Format a stable ticket id from table origin + row id.
pub fn ticket_id(kind: AdapterSourceKind, source_id: &str) -> String {
    format!("{}:{source_id}", kind.as_str())
}

/// Parse `provider:<id>` / `account:<id>` into kind + row id.
pub fn parse_ticket_id(ticket_id: &str) -> Result<(AdapterSourceKind, String), String> {
    let trimmed = ticket_id.trim();
    let (prefix, rest) = trimmed.split_once(':').ok_or_else(|| {
        format!("invalid ticket id '{ticket_id}', expected: account:<id>|provider:<id>")
    })?;
    let kind = AdapterSourceKind::parse(prefix).ok_or_else(|| {
        format!("invalid ticket id prefix '{prefix}', expected: account|provider")
    })?;
    let source_id = rest.trim();
    if source_id.is_empty() {
        return Err(format!(
            "invalid ticket id '{ticket_id}': source id must not be empty"
        ));
    }
    Ok((kind, source_id.to_owned()))
}
