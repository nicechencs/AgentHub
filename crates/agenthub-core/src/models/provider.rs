//! Provider-related pure data structures (serde). No business logic.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::AgentId;
use super::BackupRecord;
use crate::utils::redact::{api_key_tail, redact_json};

/// Live config file format for a provider template.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConfigFormat {
    Json,
    Toml,
}

impl ConfigFormat {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::Toml => "toml",
        }
    }
}

impl std::fmt::Display for ConfigFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Built-in (L3) provider preset template, shared by CLI and future GUI.
///
/// `agent` is included so flat list / JSON outputs stay self-describing when
/// multiple agents are returned together.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderPreset {
    pub agent: AgentId,
    pub id: String,
    pub label: String,
    pub format: ConfigFormat,
    /// Full template body (JSON or TOML text). Placeholder keys are intentional.
    pub template: String,
}

/// Persisted L1 provider row (API config pool in `providers` table).
///
/// `settings_config` / `meta` stay opaque JSON — field shapes differ per agent
/// and preset. Use [`Provider::redacted`] before serializing to users.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Provider {
    pub id: String,
    pub agent_id: AgentId,
    pub name: String,
    pub settings_config: Value,
    pub meta: Value,
    pub is_current: bool,
    pub created_at: String,
    pub updated_at: String,
}

/// Write-side input for create / update / upsert.
///
/// Timestamps are owned by core and intentionally absent here.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderInput {
    pub id: String,
    pub agent_id: AgentId,
    pub name: String,
    pub settings_config: Value,
    pub meta: Value,
    pub is_current: bool,
}

/// Outcome of applying a persisted provider to an agent's live config.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSwitchResult {
    /// The provider selected after backfill and the transactional DB switch.
    pub provider: Provider,
    /// Snapshot created before the live write. `None` means no live files
    /// existed yet, so there was nothing to snapshot.
    pub backup: Option<BackupRecord>,
    /// Existing current provider updated from the live config before switch.
    pub backfilled_provider_id: Option<String>,
}

fn json_str<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

fn json_bool(value: &Value, key: &str) -> Option<bool> {
    value.get(key).and_then(Value::as_bool)
}

impl Provider {
    /// Deep-copy with likely secret keys in JSON blobs redacted.
    pub fn redacted(&self) -> Self {
        let mut meta = redact_json(&self.meta);
        if let Some(tail) = api_key_tail(&self.settings_config) {
            if let Value::Object(map) = &mut meta {
                map.insert("secretTail".into(), json!(tail));
            } else {
                meta = json!({ "secretTail": tail });
            }
        } else if meta
            .get("secretTail")
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or("")
            .is_empty()
        {
            if let Some(tail) = crate::utils::redact::secret_tail_from_masked_preview(&self.name) {
                if let Value::Object(map) = &mut meta {
                    map.insert("secretTail".into(), json!(tail));
                } else {
                    meta = json!({ "secretTail": tail });
                }
            }
        }
        if let Some(hash) = crate::utils::redact::api_key_secret_hash(&self.settings_config) {
            if let Value::Object(map) = &mut meta {
                map.insert("secretHash".into(), json!(hash));
            } else {
                meta = json!({ "secretHash": hash });
            }
        }
        Self {
            id: self.id.clone(),
            agent_id: self.agent_id,
            name: self.name.clone(),
            settings_config: redact_json(&self.settings_config),
            meta,
            is_current: self.is_current,
            created_at: self.created_at.clone(),
            updated_at: self.updated_at.clone(),
        }
    }

    /// `meta.official`. Unknown keys stay on `meta`.
    pub fn official(&self) -> Option<bool> {
        json_bool(&self.meta, "official")
    }

    pub fn preset(&self) -> Option<&str> {
        json_str(&self.meta, "preset")
    }

    pub fn source(&self) -> Option<&str> {
        json_str(&self.meta, "source")
    }

    pub fn generated_by(&self) -> Option<&str> {
        json_str(&self.meta, "generatedBy")
    }

    pub fn meta_provider(&self) -> Option<&str> {
        json_str(&self.meta, "provider")
    }

    pub fn adapter_rule_id(&self) -> Option<&str> {
        json_str(&self.meta, "adapterRuleId")
    }

    pub fn settings_format(&self) -> Option<&str> {
        json_str(&self.settings_config, "format")
    }
}

impl ProviderSwitchResult {
    /// Redact the selected provider for CLI/GUI serialization.
    pub fn redacted(&self) -> Self {
        Self {
            provider: self.provider.redacted(),
            backup: self.backup.clone(),
            backfilled_provider_id: self.backfilled_provider_id.clone(),
        }
    }
}

#[cfg(test)]
mod tests;
