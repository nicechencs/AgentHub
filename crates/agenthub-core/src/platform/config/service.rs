//! ConfigurationService — path policy, validation gate, projector orchestration.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::Value;

use crate::error::{AppError, Result};
use crate::logging::{self, targets};
use crate::models::AgentId;
use crate::platform::paths::resolve_agent_home;
use crate::platform::AgentKey;
use crate::utils::redact::redact_text;

use super::document::{ConfigApplyResult, ConfigChangePlan, NormalizedConfigDocument};
use super::projector::AgentConfigProjector;
use super::registry::{builtin_config_registry, ConfigProjectorRegistry};
use super::schema::{AgentConfigSchema, ConfigValidationResult};

/// Platform configuration façade (no agent-specific match arms).
#[derive(Clone)]
pub struct ConfigurationService {
    registry: Arc<ConfigProjectorRegistry>,
}

impl Default for ConfigurationService {
    fn default() -> Self {
        Self::new()
    }
}

impl ConfigurationService {
    pub fn new() -> Self {
        Self {
            registry: Arc::new(builtin_config_registry().clone()),
        }
    }

    pub fn with_registry(registry: ConfigProjectorRegistry) -> Self {
        Self {
            registry: Arc::new(registry),
        }
    }

    pub fn supports(&self, agent: AgentId) -> bool {
        self.supports_key(&AgentKey::from_agent_id(agent))
    }

    pub fn supports_key(&self, key: &AgentKey) -> bool {
        self.registry.contains_key(key)
    }

    pub fn supported_agents(&self) -> Vec<AgentId> {
        self.registry.supported_agents()
    }

    pub fn supported_agent_keys(&self) -> Vec<AgentKey> {
        self.registry.supported_agent_keys()
    }

    fn projector(&self, key: &AgentKey) -> Result<Arc<dyn AgentConfigProjector>> {
        self.registry.get(key).ok_or_else(|| {
            AppError::Unsupported(format!("no config projector for agent {}", key.as_str()))
        })
    }

    fn legacy_home(&self, agent: AgentId, home_override: Option<&Path>) -> Result<PathBuf> {
        if let Some(h) = home_override {
            return Ok(h.to_path_buf());
        }
        resolve_agent_home(agent)
    }

    pub fn schema(&self, agent: AgentId) -> Result<AgentConfigSchema> {
        self.schema_for_agent_key(&AgentKey::from_agent_id(agent))
    }

    pub fn schema_for_agent_key(&self, key: &AgentKey) -> Result<AgentConfigSchema> {
        Ok(self.projector(key)?.schema())
    }

    pub fn schema_for_key(&self, key: &str) -> Result<AgentConfigSchema> {
        let agent_key = AgentKey::parse(key)?;
        self.schema_for_agent_key(&agent_key)
    }

    pub fn read(&self, agent: AgentId) -> Result<NormalizedConfigDocument> {
        self.read_at(agent, None)
    }

    pub fn read_at(
        &self,
        agent: AgentId,
        home_override: Option<&Path>,
    ) -> Result<NormalizedConfigDocument> {
        let key = AgentKey::from_agent_id(agent);
        let home = self.legacy_home(agent, home_override)?;
        self.read_for_agent_key(&key, &home)
    }

    pub fn read_for_agent_key(
        &self,
        key: &AgentKey,
        agent_home: &Path,
    ) -> Result<NormalizedConfigDocument> {
        let mut doc = self.projector(key)?.read_normalized(agent_home)?;
        // Avoid leaking secrets via unknown_native for JSON agents.
        scrub_unknown_native(&mut doc);
        Ok(doc)
    }

    pub fn validate(
        &self,
        agent: AgentId,
        values: &BTreeMap<String, Value>,
    ) -> Result<ConfigValidationResult> {
        self.validate_for_agent_key(&AgentKey::from_agent_id(agent), values)
    }

    pub fn validate_for_agent_key(
        &self,
        key: &AgentKey,
        values: &BTreeMap<String, Value>,
    ) -> Result<ConfigValidationResult> {
        self.projector(key)?.validate(values)
    }

    pub fn validate_value(&self, agent: AgentId, values: Value) -> Result<ConfigValidationResult> {
        let map = value_to_map(values)?;
        self.validate(agent, &map)
    }

    pub fn plan_apply(
        &self,
        agent: AgentId,
        desired: &BTreeMap<String, Value>,
    ) -> Result<ConfigChangePlan> {
        self.plan_apply_at(agent, desired, None)
    }

    pub fn plan_apply_at(
        &self,
        agent: AgentId,
        desired: &BTreeMap<String, Value>,
        home_override: Option<&Path>,
    ) -> Result<ConfigChangePlan> {
        let key = AgentKey::from_agent_id(agent);
        let home = self.legacy_home(agent, home_override)?;
        self.plan_apply_for_agent_key(&key, desired, &home)
    }

    pub fn plan_apply_for_agent_key(
        &self,
        key: &AgentKey,
        desired: &BTreeMap<String, Value>,
        agent_home: &Path,
    ) -> Result<ConfigChangePlan> {
        let p = self.projector(key)?;
        let current = self.read_for_agent_key(key, agent_home)?;
        p.plan_apply(&current, desired)
    }

    pub fn apply(
        &self,
        agent: AgentId,
        desired: &BTreeMap<String, Value>,
    ) -> Result<ConfigApplyResult> {
        self.apply_at(agent, desired, None)
    }

    pub fn apply_at(
        &self,
        agent: AgentId,
        desired: &BTreeMap<String, Value>,
        home_override: Option<&Path>,
    ) -> Result<ConfigApplyResult> {
        let key = AgentKey::from_agent_id(agent);
        let home = self.legacy_home(agent, home_override)?;
        self.apply_for_agent_key(&key, desired, &home)
    }

    pub fn apply_for_agent_key(
        &self,
        key: &AgentKey,
        desired: &BTreeMap<String, Value>,
        agent_home: &Path,
    ) -> Result<ConfigApplyResult> {
        // Never log desired values (may contain secrets); only keys + counts.
        match self.projector(key).and_then(|p| p.apply(agent_home, desired)) {
            Ok(mut result) => {
                scrub_unknown_native(&mut result.document);
                let changed = result.plan.field_changes.len();
                let keys: String = desired.keys().cloned().collect::<Vec<_>>().join(",");
                let msg = redact_text(&format!(
                    "config apply ok; fields_in={} changed={} keys=[{keys}]",
                    desired.len(),
                    changed
                ));
                tracing::info!(
                    module = targets::SETTINGS,
                    op = "apply",
                    agent = key.as_str(),
                    fields_in = desired.len(),
                    fields_changed = changed,
                    "{msg}"
                );
                Ok(result)
            }
            Err(e) => {
                logging::log_app_error_agent(targets::SETTINGS, "apply", key.as_str(), &e);
                Err(e)
            }
        }
    }

    pub fn apply_value(&self, agent: AgentId, values: Value) -> Result<ConfigApplyResult> {
        let map = value_to_map(values)?;
        self.apply(agent, &map)
    }

    /// Build provider-pool `settings_config` from schema field values (no FS write).
    pub fn materialize_settings_config(
        &self,
        agent: AgentId,
        desired: &BTreeMap<String, Value>,
        base_raw: Option<&Value>,
    ) -> Result<Value> {
        self.materialize_settings_config_for_agent_key(
            &AgentKey::from_agent_id(agent),
            desired,
            base_raw,
        )
    }

    pub fn materialize_settings_config_for_agent_key(
        &self,
        key: &AgentKey,
        desired: &BTreeMap<String, Value>,
        base_raw: Option<&Value>,
    ) -> Result<Value> {
        match self
            .projector(key)
            .and_then(|p| p.materialize_settings_config(base_raw, desired))
        {
            Ok(value) => {
                tracing::debug!(
                    module = targets::SETTINGS,
                    op = "materialize",
                    agent = key.as_str(),
                    fields_in = desired.len(),
                    "config materialize ok"
                );
                Ok(value)
            }
            Err(e) => {
                logging::log_app_error_agent(targets::SETTINGS, "materialize", key.as_str(), &e);
                Err(e)
            }
        }
    }

    pub fn materialize_settings_config_value(
        &self,
        agent: AgentId,
        values: Value,
        base_raw: Option<Value>,
    ) -> Result<Value> {
        let map = value_to_map(values)?;
        self.materialize_settings_config(agent, &map, base_raw.as_ref())
    }
}

fn value_to_map(values: Value) -> Result<BTreeMap<String, Value>> {
    let obj = values
        .as_object()
        .ok_or_else(|| AppError::InvalidArg("config values must be a JSON object".into()))?;
    Ok(obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
}

fn scrub_unknown_native(doc: &mut NormalizedConfigDocument) {
    // JSON root: redact known secret env keys under env.
    if let Some(obj) = doc.unknown_native.as_object_mut() {
        if let Some(env) = obj.get_mut("env").and_then(|e| e.as_object_mut()) {
            for key in ["ANTHROPIC_AUTH_TOKEN", "ANTHROPIC_API_KEY"] {
                if env
                    .get(key)
                    .and_then(|v| v.as_str())
                    .map(|s| !s.is_empty())
                    .unwrap_or(false)
                {
                    env.insert(
                        key.into(),
                        Value::String(super::schema::SECRET_REDACTED.into()),
                    );
                }
            }
        }
        // TOML dual-shape: do not attempt full scrub; content may still hold keys
        // for TOML agents that embed api_key — Grok projector already scrubs.
        if obj.get("format").and_then(|v| v.as_str()) == Some("toml") {
            if let Some(content) = obj.get("content").and_then(|v| v.as_str()) {
                if content.contains("api_key") {
                    // Best-effort line scrub for inline secrets in returned content.
                    let scrubbed: String = content
                        .lines()
                        .map(|line| {
                            let trimmed = line.trim_start();
                            if trimmed.starts_with("api_key") && trimmed.contains('=') {
                                "api_key = \"***\""
                            } else {
                                line
                            }
                        })
                        .collect::<Vec<_>>()
                        .join("\n");
                    obj.insert("content".into(), Value::String(scrubbed));
                }
            }
        }
    }
}
