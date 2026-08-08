//! Platform configuration field schema (agent-agnostic DTO).

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::platform::AgentKey;

/// Wire marker for secret fields that should not be echoed or re-submitted.
pub const SECRET_REDACTED: &str = "***";

/// Supported field value kinds for generic form rendering (P09).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum ConfigValueType {
    String,
    Number,
    Boolean,
    Secret,
    Enum { options: Vec<String> },
}

/// Optional simple validation constraints.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct FieldValidation {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_length: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_length: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
}

/// One configurable field in the platform schema.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigFieldSchema {
    pub key: String,
    pub label: String,
    pub value_type: ConfigValueType,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub secret: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validation: Option<FieldValidation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub help: Option<String>,
    /// Capability gate for UI visibility (optional).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capability: Option<String>,
}

/// Native on-disk format for this agent's primary config document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NativeConfigFormat {
    Json,
    Toml,
}

/// Full schema for one agent (stable keys + version).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentConfigSchema {
    pub agent_key: AgentKey,
    pub schema_version: u32,
    pub native_format: NativeConfigFormat,
    /// Relative path under agent home (e.g. `settings.json`, `config.toml`).
    pub relative_path: String,
    pub fields: Vec<ConfigFieldSchema>,
}

/// Field-level validation issue.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigValidationIssue {
    pub field_key: String,
    pub code: String,
    pub message: String,
}

/// Result of validate (may succeed with empty issues).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigValidationResult {
    pub ok: bool,
    pub issues: Vec<ConfigValidationIssue>,
}

impl ConfigValidationResult {
    pub fn success() -> Self {
        Self {
            ok: true,
            issues: Vec::new(),
        }
    }

    pub fn failure(issues: Vec<ConfigValidationIssue>) -> Self {
        Self { ok: false, issues }
    }
}
