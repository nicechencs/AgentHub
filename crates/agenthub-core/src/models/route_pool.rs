//! RoutePool / RouteMember domain types for the unified loopback pool (P1).
//!
//! The Hub token lives on the pool, not on each ticket. Upstream credentials
//! stay on account/provider rows; members only store authorization references.

use std::fmt;

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use serde::{Deserialize, Serialize};

use super::{AdapterSourceKind, AgentId};
use crate::error::{AppError, Result};

#[cfg(test)]
mod tests;

/// Settings key. Absent / anything other than an explicit on-value is fail-closed.
pub const FEATURE_ROUTE_POOL_V2: &str = "feature.route_pool_v2";

/// Shared resolver + `/models` index. Off keeps lead + `switch_edge_for_model`.
/// One flag controls both dispatch and `GET /models`.
pub const FEATURE_ROUTE_INDEX_V2: &str = "feature.route_index_v2";

/// Codex client `/v1/responses` → Grok upstream pair adapter. Independent of
/// the reverse direction. Off keeps today's Experimental passthrough.
pub const FEATURE_CODEX_INGRESS_GROK_UPSTREAM: &str = "feature.codex_ingress_grok_upstream";

/// Grok client `/v1/responses` → Codex upstream pair adapter. Independent of
/// the reverse direction. Off keeps today's Experimental passthrough.
pub const FEATURE_GROK_INGRESS_CODEX_UPSTREAM: &str = "feature.grok_ingress_codex_upstream";

pub fn feature_flag_enabled(raw: Option<&str>) -> bool {
    matches!(
        raw.map(|value| value.trim().to_ascii_lowercase())
            .as_deref(),
        Some("1" | "true" | "on" | "yes")
    )
}

/// Downstream HTTP surface served by a pool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RouteDownstreamSurface {
    Responses,
    Messages,
    ChatCompletions,
}

impl RouteDownstreamSurface {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Responses => "responses",
            Self::Messages => "messages",
            Self::ChatCompletions => "chat_completions",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "responses" => Some(Self::Responses),
            "messages" => Some(Self::Messages),
            "chat_completions" | "chat-completions" => Some(Self::ChatCompletions),
            _ => None,
        }
    }

    pub fn for_agent(agent: AgentId) -> Option<Self> {
        match agent {
            AgentId::Claude => Some(Self::Messages),
            AgentId::Codex | AgentId::Grok => Some(Self::Responses),
            AgentId::Kimi | AgentId::Dsh => Some(Self::ChatCompletions),
            AgentId::Pi | AgentId::WorkBuddy | AgentId::Cursor => None,
        }
    }
}

/// Ingress dialect. Independent of [`RouteDownstreamSurface`] so Codex and Grok
/// can share `/v1/responses` without guessing Provider from the path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RouteDownstreamDialect {
    Claude,
    Codex,
    Grok,
    Kimi,
    Dsh,
    Generic,
}

impl RouteDownstreamDialect {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Grok => "grok",
            Self::Kimi => "kimi",
            Self::Dsh => "dsh",
            Self::Generic => "generic",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "claude" => Some(Self::Claude),
            "codex" => Some(Self::Codex),
            "grok" => Some(Self::Grok),
            "kimi" => Some(Self::Kimi),
            "dsh" => Some(Self::Dsh),
            "generic" => Some(Self::Generic),
            _ => None,
        }
    }

    pub fn for_agent(agent: AgentId) -> Self {
        match agent {
            AgentId::Claude => Self::Claude,
            AgentId::Codex => Self::Codex,
            AgentId::Grok => Self::Grok,
            AgentId::Kimi => Self::Kimi,
            AgentId::Dsh => Self::Dsh,
            AgentId::Pi | AgentId::WorkBuddy | AgentId::Cursor => Self::Generic,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RouteSchedulePolicy {
    PriorityFailover,
    RoundRobin,
}

impl RouteSchedulePolicy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PriorityFailover => "priority_failover",
            Self::RoundRobin => "round_robin",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "priority_failover" => Some(Self::PriorityFailover),
            "round_robin" => Some(Self::RoundRobin),
            _ => None,
        }
    }
}

impl Default for RouteSchedulePolicy {
    fn default() -> Self {
        Self::PriorityFailover
    }
}

/// Stable public entry for one Agent / surface. Members may change; the Hub
/// token and (after the first v2 write) the loopback port must not.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutePool {
    pub id: String,
    pub target_agent_id: AgentId,
    pub downstream_surface: RouteDownstreamSurface,
    pub downstream_dialect: RouteDownstreamDialect,
    /// Loopback bearer. Never an upstream credential. Skipped on serialize so
    /// GUI/log JSON cannot leak it.
    #[serde(skip_serializing)]
    pub hub_token: String,
    pub schedule_policy: RouteSchedulePolicy,
    pub is_default: bool,
    /// Explicit switch onto the unified gateway. Unenrolled pools keep any
    /// historical `profile.local_port` until this becomes true.
    pub v2_enrolled: bool,
    pub policy_revision: i64,
    pub auto_start: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gateway_port: Option<u16>,
    pub created_at: String,
    pub updated_at: String,
}

impl fmt::Debug for RoutePool {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RoutePool")
            .field("id", &self.id)
            .field("target_agent_id", &self.target_agent_id)
            .field("downstream_surface", &self.downstream_surface)
            .field("downstream_dialect", &self.downstream_dialect)
            .field("hub_token", &"REDACTED")
            .field("schedule_policy", &self.schedule_policy)
            .field("is_default", &self.is_default)
            .field("v2_enrolled", &self.v2_enrolled)
            .field("policy_revision", &self.policy_revision)
            .field("auto_start", &self.auto_start)
            .field("gateway_port", &self.gateway_port)
            .field("created_at", &self.created_at)
            .field("updated_at", &self.updated_at)
            .finish()
    }
}

/// Authorization reference inside a pool. Does not copy the upstream token.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteMember {
    pub id: String,
    pub route_pool_id: String,
    pub source_kind: AdapterSourceKind,
    pub source_id: String,
    pub enabled: bool,
    pub priority: i64,
    pub position: i64,
    pub created_at: String,
    pub updated_at: String,
}

impl RouteMember {
    pub fn fingerprint(&self) -> String {
        authorization_fingerprint(self.source_kind, &self.source_id)
    }
}

pub fn authorization_fingerprint(kind: AdapterSourceKind, source_id: &str) -> String {
    format!("{}:{source_id}", kind.as_str())
}

pub fn generate_hub_token() -> Result<String> {
    let mut bytes = [0u8; 32];
    getrandom::getrandom(&mut bytes).map_err(|error| {
        AppError::message("route_pool.hub_token", format!("random failed: {error}"))
    })?;
    Ok(format!("ahb_{}", URL_SAFE_NO_PAD.encode(bytes)))
}

/// Pick the unique default pool among candidates for one Agent / surface.
///
/// Active binding wins; otherwise the stable id order is used. Never guesses
/// from created_at or live runtime state.
pub fn choose_default_pool_id<'a>(
    pool_ids: impl IntoIterator<Item = &'a str>,
    active_binding_profile_id: Option<&str>,
) -> Option<String> {
    let mut ids: Vec<&str> = pool_ids.into_iter().collect();
    if ids.is_empty() {
        return None;
    }
    ids.sort_unstable();
    if let Some(active) = active_binding_profile_id {
        if ids.iter().any(|id| *id == active) {
            return Some(active.to_owned());
        }
    }
    ids.first().map(|id| (*id).to_owned())
}
