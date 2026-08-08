//! Normalized config document and apply plan DTOs.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::platform::AgentKey;

/// Normalized view of an agent config (known fields + preserved unknown native).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NormalizedConfigDocument {
    pub agent_key: AgentKey,
    pub schema_version: u32,
    /// Known field values (secrets redacted when served via API).
    pub values: BTreeMap<String, Value>,
    /// Opaque native remainder / full tree for round-trip (never log secrets).
    pub unknown_native: Value,
    /// Absolute path of the primary config file when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    /// True when the primary file was missing (defaults applied for known fields).
    #[serde(default)]
    pub missing: bool,
}

/// Planned change set before apply (for UI preview / auditing).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigChangePlan {
    pub agent_key: AgentKey,
    pub schema_version: u32,
    pub target_path: PathBuf,
    pub field_changes: Vec<FieldChange>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldChange {
    pub field_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<Value>,
    /// Secret fields never include plaintext in from/to.
    #[serde(default)]
    pub secret: bool,
}

/// Successful apply outcome (re-read normalized document).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigApplyResult {
    pub document: NormalizedConfigDocument,
    pub plan: ConfigChangePlan,
}
