//! Pure catalog DTOs (no I/O, no registry).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::models::{CapabilityStateDto, RuntimeId};

use super::AgentKey;

/// One install channel exposed by the agent catalog (display / gate metadata).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallChannelDescriptor {
    pub id: String,
    pub label: String,
    /// Human-facing install command or setup URL (from install catalog).
    pub command: String,
    pub requires: Vec<RuntimeId>,
}

/// Read-only agent descriptor for directory / capability gates.
///
/// Pure data: no DB connection, no process execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentDescriptor {
    pub key: AgentKey,
    pub display_name: String,
    /// Integration package version (bumped when ports/schema change).
    pub integration_version: u32,
    /// Capability id (`camelCase` as_str) → state for wire format.
    pub capabilities: BTreeMap<String, CapabilityStateDto>,
    pub install_channels: Vec<InstallChannelDescriptor>,
    /// Present when the agent exposes a versioned config schema; optional in P01.
    pub config_schema_version: Option<u32>,
}
