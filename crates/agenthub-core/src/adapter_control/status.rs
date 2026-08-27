//! Credential-free local-bridge listener status for control surfaces.
//!
//! Core [`crate::bridge::BridgeRuntimeStatus`] has no serde implementation, so
//! hosts expose this deliberate DTO rather than a structural dump of runtime
//! internals. No bearer / upstream secret fields are included.

use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::bridge::host::InboundRequestRecord;
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
        }
    }

    pub fn with_recent_inbound(mut self, recent_inbound: Vec<InboundRequestRecord>) -> Self {
        self.recent_inbound = recent_inbound;
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
