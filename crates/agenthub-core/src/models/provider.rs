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
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn config_format_serde_lowercase() {
        let j = serde_json::to_string(&ConfigFormat::Json).unwrap();
        let t = serde_json::to_string(&ConfigFormat::Toml).unwrap();
        assert_eq!(j, "\"json\"");
        assert_eq!(t, "\"toml\"");
        assert_eq!(
            serde_json::from_str::<ConfigFormat>("\"json\"").unwrap(),
            ConfigFormat::Json
        );
        assert_eq!(
            serde_json::from_str::<ConfigFormat>("\"toml\"").unwrap(),
            ConfigFormat::Toml
        );
    }

    #[test]
    fn provider_preset_serde_camel_case() {
        let p = ProviderPreset {
            agent: AgentId::Claude,
            id: "anthropic".into(),
            label: "Anthropic 官方".into(),
            format: ConfigFormat::Json,
            template: "{}".into(),
        };
        let v = serde_json::to_value(&p).unwrap();
        assert_eq!(v["agent"], "claude");
        assert_eq!(v["id"], "anthropic");
        assert_eq!(v["label"], "Anthropic 官方");
        assert_eq!(v["format"], "json");
        assert_eq!(v["template"], "{}");
        assert!(v.get("template").is_some());
    }

    #[test]
    fn provider_serde_camel_case_and_fields() {
        let p = Provider {
            id: "p1".into(),
            agent_id: AgentId::Codex,
            name: "Corp Relay".into(),
            settings_config: json!({"base_url": "https://x", "api_key": "sk-secret"}),
            meta: json!({"preset": "openai-compatible"}),
            is_current: true,
            created_at: "2026-01-01 00:00:00".into(),
            updated_at: "2026-01-02 00:00:00".into(),
        };
        let v = serde_json::to_value(&p).unwrap();
        assert_eq!(v["id"], "p1");
        assert_eq!(v["agentId"], "codex");
        assert_eq!(v["name"], "Corp Relay");
        assert_eq!(v["settingsConfig"]["base_url"], "https://x");
        assert_eq!(v["settingsConfig"]["api_key"], "sk-secret");
        assert_eq!(v["meta"]["preset"], "openai-compatible");
        assert_eq!(v["isCurrent"], true);
        assert_eq!(v["createdAt"], "2026-01-01 00:00:00");
        assert_eq!(v["updatedAt"], "2026-01-02 00:00:00");
    }

    #[test]
    fn provider_input_serde_no_timestamps() {
        let input = ProviderInput {
            id: "p1".into(),
            agent_id: AgentId::Claude,
            name: "Relay".into(),
            settings_config: json!({"base_url": "https://x"}),
            meta: json!({}),
            is_current: false,
        };
        let v = serde_json::to_value(&input).unwrap();
        assert_eq!(v["id"], "p1");
        assert_eq!(v["agentId"], "claude");
        assert_eq!(v["name"], "Relay");
        assert_eq!(v["settingsConfig"]["base_url"], "https://x");
        assert_eq!(v["meta"], json!({}));
        assert_eq!(v["isCurrent"], false);
        assert!(v.get("createdAt").is_none());
        assert!(v.get("updatedAt").is_none());
        let back: ProviderInput = serde_json::from_value(v).unwrap();
        assert_eq!(back, input);
    }

    #[test]
    fn provider_redacted_masks_nested_secrets() {
        let p = Provider {
            id: "p1".into(),
            agent_id: AgentId::Grok,
            name: "xAI".into(),
            settings_config: json!({
                "api_key": "secret",
                "nested": { "TOKEN": "t", "base_url": "https://x" }
            }),
            meta: json!({"authorization": "Bearer x", "label": "ok"}),
            is_current: false,
            created_at: "t0".into(),
            updated_at: "t1".into(),
        };
        let r = p.redacted();
        assert_eq!(r.settings_config["api_key"], "***");
        assert_eq!(r.settings_config["nested"]["TOKEN"], "***");
        assert_eq!(r.settings_config["nested"]["base_url"], "https://x");
        assert_eq!(r.meta["authorization"], "***");
        assert_eq!(r.meta["label"], "ok");
        // Original unchanged.
        assert_eq!(p.settings_config["api_key"], "secret");
    }

    #[test]
    fn provider_redacted_masks_opaque_toml_body() {
        let p = Provider {
            id: "p-toml".into(),
            agent_id: AgentId::Grok,
            name: "xAI".into(),
            settings_config: json!({
                "format": "toml",
                "content": "model = 'grok'\napi_key = 'xai-secret'\n"
            }),
            meta: json!({}),
            is_current: true,
            created_at: "t0".into(),
            updated_at: "t1".into(),
        };

        let redacted = p.redacted();
        assert_eq!(redacted.settings_config["format"], "toml");
        assert_eq!(redacted.settings_config["content"], "***");
        assert_eq!(redacted.meta["secretTail"], "**cret");
        let hash = redacted.meta["secretHash"].as_str().expect("hash");
        assert_eq!(hash.len(), 64);
        assert!(!hash.contains("xai-secret"));
        assert!(!redacted.settings_config.to_string().contains("xai-secret"));
        assert!(p.settings_config["content"]
            .as_str()
            .unwrap()
            .contains("xai-secret"));
    }
}
