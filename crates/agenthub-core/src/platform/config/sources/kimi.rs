//! Kimi config.toml projector.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::{json, Value};
use toml_edit::DocumentMut;

use crate::error::{AppError, Result};
use crate::models::AgentId;
use crate::platform::AgentKey;
use crate::utils::atomic::atomic_write;

use super::super::document::{ConfigApplyResult, ConfigChangePlan, NormalizedConfigDocument};
use super::super::projector::AgentConfigProjector;
use super::super::schema::{
    AgentConfigSchema, ConfigValidationResult, ConfigValueType, NativeConfigFormat, SECRET_REDACTED,
};
use super::util::{
    field, finish_apply, get_str_map, invalid_toml, plan_from_maps, redact_secrets,
    secret_unchanged, string_val, validate_known_fields,
};

const SCHEMA_VERSION: u32 = 1;
const REL_PATH: &str = "config.toml";

pub struct KimiConfigProjector;

impl KimiConfigProjector {
    fn schema_inner() -> AgentConfigSchema {
        AgentConfigSchema {
            agent_key: AgentKey::from_agent_id(AgentId::Kimi),
            schema_version: SCHEMA_VERSION,
            native_format: NativeConfigFormat::Toml,
            relative_path: REL_PATH.into(),
            fields: vec![
                field(
                    "model",
                    "Default model",
                    ConfigValueType::String,
                    false,
                    false,
                    Some("default_model"),
                ),
                field(
                    "baseUrl",
                    "Base URL",
                    ConfigValueType::String,
                    false,
                    false,
                    None,
                ),
                field(
                    "apiKey",
                    "API Key",
                    ConfigValueType::Secret,
                    true,
                    false,
                    Some("providers.<slug>.api_key"),
                ),
                field(
                    "providerSlug",
                    "Provider slug",
                    ConfigValueType::String,
                    false,
                    false,
                    Some("[providers.slug]"),
                ),
            ],
        }
    }

    fn path(home: &Path) -> std::path::PathBuf {
        home.join(REL_PATH)
    }

    fn read_toml(path: &Path) -> Result<(DocumentMut, bool)> {
        if !path.exists() {
            return Ok((DocumentMut::new(), true));
        }
        let text = std::fs::read_to_string(path)?;
        if text.trim().is_empty() {
            return Ok((DocumentMut::new(), false));
        }
        Ok((
            text.parse::<DocumentMut>()
                .map_err(|e| invalid_toml(path, e))?,
            false,
        ))
    }

    fn first_slug(doc: &DocumentMut) -> String {
        if let Some(providers) = doc.get("providers").and_then(|p| p.as_table()) {
            if let Some((name, _)) = providers.iter().next() {
                return name.to_string();
            }
        }
        doc.get("default_provider")
            .and_then(|v| v.as_str())
            .unwrap_or("custom")
            .to_string()
    }

    fn provider_get(doc: &DocumentMut, slug: &str, key: &str) -> String {
        doc.get("providers")
            .and_then(|p| p.as_table())
            .and_then(|t| t.get(slug))
            .and_then(|item| item.get(key))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string()
    }

    fn extract(doc: &DocumentMut) -> BTreeMap<String, Value> {
        let slug = Self::first_slug(doc);
        let api_key = {
            let k = Self::provider_get(doc, &slug, "api_key");
            if k.is_empty() {
                doc.get("api_key")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string()
            } else {
                k
            }
        };
        let base = {
            let b = Self::provider_get(doc, &slug, "base_url");
            if b.is_empty() {
                doc.get("base_url")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string()
            } else {
                b
            }
        };
        let mut values = BTreeMap::new();
        values.insert(
            "model".into(),
            string_val(
                doc.get("default_model")
                    .and_then(|v| v.as_str())
                    .or(Some("")),
            ),
        );
        values.insert("baseUrl".into(), string_val(Some(&base)));
        values.insert("apiKey".into(), string_val(Some(&api_key)));
        values.insert("providerSlug".into(), string_val(Some(&slug)));
        values
    }

    fn merge(
        mut doc: DocumentMut,
        current: &BTreeMap<String, Value>,
        desired: &BTreeMap<String, Value>,
    ) -> Result<DocumentMut> {
        let slug = get_str_map(desired, "providerSlug")
            .or_else(|| get_str_map(current, "providerSlug"))
            .unwrap_or_else(|| "custom".into());
        let slug = {
            let t = slug.trim();
            if t.is_empty() {
                "custom".to_string()
            } else {
                t.to_string()
            }
        };

        if let Some(model) = get_str_map(desired, "model") {
            let t = model.trim();
            if t.is_empty() {
                doc.remove("default_model");
            } else {
                doc["default_model"] = toml_edit::value(t);
            }
        }

        if doc.get("providers").is_none() {
            doc["providers"] = toml_edit::table();
        }
        let providers = doc["providers"]
            .as_table_mut()
            .ok_or_else(|| AppError::InvalidArg("Kimi providers must be a table".into()))?;
        if providers.get(slug.as_str()).is_none() {
            providers.insert(slug.as_str(), toml_edit::table());
        }
        let entry = providers.get_mut(slug.as_str()).unwrap();
        if let Some(base) = get_str_map(desired, "baseUrl") {
            let t = base.trim();
            if t.is_empty() {
                if let Some(t) = entry.as_table_mut() {
                    t.remove("base_url");
                }
            } else {
                entry["base_url"] = toml_edit::value(t);
            }
        }
        if let Some(key) = get_str_map(desired, "apiKey") {
            if !secret_unchanged(Some(&key)) {
                entry["api_key"] = toml_edit::value(key.trim());
            } else {
                // keep existing; if redacted in desired, leave native
                let _ = SECRET_REDACTED;
            }
        }
        Ok(doc)
    }
}

impl AgentConfigProjector for KimiConfigProjector {
    fn agent_key(&self) -> AgentKey {
        AgentKey::parse("kimi").expect("builtin config projector key must be valid")
    }

    fn schema(&self) -> AgentConfigSchema {
        Self::schema_inner()
    }

    fn read_normalized(&self, agent_home: &Path) -> Result<NormalizedConfigDocument> {
        let path = Self::path(agent_home);
        let (doc, missing) = Self::read_toml(&path)?;
        let values = Self::extract(&doc);
        let schema = self.schema();
        let content = if path.exists() {
            std::fs::read_to_string(&path).unwrap_or_default()
        } else {
            String::new()
        };
        Ok(NormalizedConfigDocument {
            agent_key: AgentKey::from_agent_id(AgentId::Kimi),
            schema_version: SCHEMA_VERSION,
            values: redact_secrets(values, &schema),
            unknown_native: json!({ "format": "toml", "content": content }),
            path: Some(path),
            missing,
        })
    }

    fn validate(&self, values: &BTreeMap<String, Value>) -> Result<ConfigValidationResult> {
        Ok(validate_known_fields(&self.schema(), values))
    }

    fn plan_apply(
        &self,
        current: &NormalizedConfigDocument,
        desired: &BTreeMap<String, Value>,
    ) -> Result<ConfigChangePlan> {
        let path = current
            .path
            .clone()
            .unwrap_or_else(|| Path::new(REL_PATH).to_path_buf());
        Ok(plan_from_maps(
            AgentKey::from_agent_id(AgentId::Kimi),
            SCHEMA_VERSION,
            path,
            &self.schema(),
            &current.values,
            desired,
        ))
    }

    fn apply(
        &self,
        agent_home: &Path,
        desired: &BTreeMap<String, Value>,
    ) -> Result<ConfigApplyResult> {
        let schema = self.schema();
        let validation = self.validate(desired)?;
        if !validation.ok {
            let msg = validation
                .issues
                .first()
                .map(|i| i.message.clone())
                .unwrap_or_else(|| "validation failed".into());
            return Err(AppError::InvalidArg(msg));
        }
        let path = Self::path(agent_home);
        let (doc, _) = Self::read_toml(&path)?;
        let current_values = Self::extract(&doc);
        let merged = Self::merge(doc, &current_values, desired)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        atomic_write(&path, merged.to_string().as_bytes())?;
        finish_apply(
            AgentKey::from_agent_id(AgentId::Kimi),
            &schema,
            path,
            &current_values,
            desired,
            || self.read_normalized(agent_home),
        )
    }

    fn materialize_settings_config(
        &self,
        base_raw: Option<&Value>,
        desired: &BTreeMap<String, Value>,
    ) -> Result<Value> {
        let validation = self.validate(desired)?;
        if !validation.ok {
            let msg = validation
                .issues
                .first()
                .map(|i| i.message.clone())
                .unwrap_or_else(|| "validation failed".into());
            return Err(AppError::InvalidArg(msg));
        }
        let content = base_raw
            .and_then(|v| v.get("content"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let doc = if content.trim().is_empty() {
            DocumentMut::new()
        } else {
            content
                .parse::<DocumentMut>()
                .map_err(|e| AppError::InvalidArg(format!("invalid base TOML: {e}")))?
        };
        let current_values = Self::extract(&doc);
        let merged = Self::merge(doc, &current_values, desired)?;
        Ok(json!({ "format": "toml", "content": merged.to_string() }))
    }
}
