//! Credential-free local-bridge listener status for control surfaces.
//!
//! Core [`crate::bridge::BridgeRuntimeStatus`] has no serde implementation, so
//! hosts expose this deliberate DTO rather than a structural dump of runtime
//! internals. No bearer / upstream secret fields are included.

use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::bridge::host::{InboundRequestRecord, InboundRequestStats, RouteRequestTrace};
use crate::bridge::{BridgeRuntimeState, BridgeRuntimeStatus, BridgeUpstreamStatus};
use crate::models::AdapterProfile;

/// Observable loopback listener state (GUI / CLI / future sidecar client).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdapterBridgeStatus {
    pub profile_id: String,
    pub port: Option<u16>,
    pub running: bool,
    pub state: String,
    pub upstream_status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_connection_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at_unix_ms: Option<u128>,
    /// Newest first. Empty when no tool has connected since this process started.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub recent_inbound: Vec<InboundRequestRecord>,
    /// Newest first. Per-request route traces for monitoring (credential-free).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub recent_route_traces: Vec<RouteRequestTrace>,
    /// Authenticated inbound requests since this process started (not ring-capped).
    #[serde(default, skip_serializing_if = "is_zero_u64")]
    pub total_request_count: u64,
    /// Failed authenticated inbound requests since this process started.
    #[serde(default, skip_serializing_if = "is_zero_u64")]
    pub failed_request_count: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_request_at_unix_ms: Option<u128>,
    /// Loopback bearer the listener accepts (`ahb_…`). Shown so the user can copy
    /// the token that actually authenticates; never the unused pool hub token.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local_token: Option<String>,
}

fn is_zero_u64(value: &u64) -> bool {
    *value == 0
}

impl AdapterBridgeStatus {
    pub fn stopped(profile: &AdapterProfile) -> Self {
        Self {
            profile_id: profile.id.clone(),
            port: profile.local_port,
            running: false,
            state: "stopped".into(),
            upstream_status: "stopped".into(),
            source_connection_id: Some(profile.source_id.clone()),
            started_at_unix_ms: None,
            recent_inbound: Vec::new(),
            recent_route_traces: Vec::new(),
            total_request_count: 0,
            failed_request_count: 0,
            last_request_at_unix_ms: None,
            local_token: None,
        }
    }

    pub fn from_runtime(status: BridgeRuntimeStatus) -> Self {
        Self {
            profile_id: status.profile_id,
            port: Some(status.port),
            running: status.running,
            state: runtime_state_name(status.state).into(),
            upstream_status: upstream_status_name(status.upstream_status).into(),
            source_connection_id: status.source_connection_id,
            started_at_unix_ms: system_time_millis(status.started_at),
            recent_inbound: Vec::new(),
            recent_route_traces: Vec::new(),
            total_request_count: 0,
            failed_request_count: 0,
            last_request_at_unix_ms: None,
            local_token: None,
        }
    }

}

/// Shared local-entry (relay) status for the board switch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalEntryStatus {
    pub running: bool,
    pub port: Option<u16>,
    pub statuses: Vec<AdapterBridgeStatus>,
    /// Failed local-auth attempts with no bound profile (newest first).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub recent_unauthenticated_traces: Vec<RouteRequestTrace>,
    /// True while restore or start_local_entry is bringing listeners back.
    /// Not set for stop_local_entry. GUI shows a non-blocking restart banner.
    pub restarting: bool,
}

impl AdapterBridgeStatus {
    pub fn with_local_token(mut self, local_token: Option<String>) -> Self {
        let token = local_token.and_then(|value| {
            let trimmed = value.trim().to_owned();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        });
        self.local_token = token;
        self
    }

    pub fn with_recent_inbound(mut self, recent_inbound: Vec<InboundRequestRecord>) -> Self {
        self.recent_inbound = recent_inbound;
        self
    }

    pub fn with_recent_route_traces(mut self, recent_route_traces: Vec<RouteRequestTrace>) -> Self {
        self.recent_route_traces = recent_route_traces;
        self
    }

    pub fn with_inbound_stats(mut self, stats: InboundRequestStats) -> Self {
        self.total_request_count = stats.total_request_count;
        self.failed_request_count = stats.failed_request_count;
        self.last_request_at_unix_ms = stats.last_request_at_unix_ms;
        self
    }
}

fn runtime_state_name(state: BridgeRuntimeState) -> &'static str {
    match state {
        BridgeRuntimeState::Starting => "starting",
        BridgeRuntimeState::Running => "running",
        BridgeRuntimeState::Stopping => "stopping",
        BridgeRuntimeState::Stopped => "stopped",
        BridgeRuntimeState::Error => "error",
        BridgeRuntimeState::Degraded => "degraded",
    }
}

fn upstream_status_name(status: BridgeUpstreamStatus) -> &'static str {
    status.as_str()
}

fn system_time_millis(value: SystemTime) -> Option<u128> {
    value
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|value| value.as_millis())
}
