//! Grok Build config.toml projector.
//!
//! Current Grok Build stores providers under `[models]` and
//! `[model."<alias>"]`. The legacy top-level shape is still read and migrated
//! so existing installations remain usable.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::{json, Value};
use toml_edit::{DocumentMut, Item};

use crate::error::{AppError, Result};
use crate::models::AgentId;
use crate::platform::AgentKey;
use crate::utils::atomic::atomic_write;
use crate::utils::grok_toml::{
    active_model_alias, ensure_grok_model_shape, EnsureGrokModelShapeOptions,
};

use crate::platform::config::sources::util::{
    field, finish_apply, get_str_map, invalid_toml, plan_from_maps, redact_secrets,
    secret_unchanged, string_val, validate_known_fields,
};
use crate::platform::config::AgentConfigProjector;
use crate::platform::config::{
    AgentConfigSchema, ConfigValidationResult, ConfigValueType, NativeConfigFormat,
};
use crate::platform::config::{ConfigApplyResult, ConfigChangePlan, NormalizedConfigDocument};

const SCHEMA_VERSION: u32 = 2;
const REL_PATH: &str = "config.toml";

pub struct GrokConfigProjector;

impl GrokConfigProjector {
    fn schema_inner() -> AgentConfigSchema {
        AgentConfigSchema {
            agent_key: AgentKey::from_agent_id(AgentId::Grok),
            schema_version: SCHEMA_VERSION,
            native_format: NativeConfigFormat::Toml,
            relative_path: REL_PATH.into(),
            fields: vec![
                field(
                    "model",
                    "Model",
                    ConfigValueType::String,
                    false,
                    false,
                    None,
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
                    Some("api_key in [model.<alias>]"),
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

    fn active_entry<'a>(doc: &'a DocumentMut, alias: &str) -> Option<&'a toml_edit::Table> {
        doc.get("model")
            .and_then(Item::as_table)
            .and_then(|models| models.get(alias))
            .and_then(Item::as_table)
    }

    fn ensure_shape(doc: &mut DocumentMut, alias: &str) -> Result<()> {
        // Projector preserves legacy api_key during migration so non-secret
        // applies do not drop credentials. Unknown root keys (e.g. env_key) stay.
        ensure_grok_model_shape(
            doc,
            alias,
            EnsureGrokModelShapeOptions {
                migrate_legacy_api_key: true,
                strip_root_env_key: false,
            },
        )?;
        Ok(())
    }

    fn extract(doc: &DocumentMut) -> BTreeMap<String, Value> {
        let alias = active_model_alias(doc);
        let entry = Self::active_entry(doc, &alias);
        let mut values = BTreeMap::new();
        values.insert(
            "model".into(),
            string_val(
                entry
                    .and_then(|table| table.get("model"))
                    .and_then(Item::as_str)
                    .or_else(|| doc.get("model").and_then(Item::as_str)),
            ),
        );
        values.insert(
            "baseUrl".into(),
            string_val(
                entry
                    .and_then(|table| table.get("base_url"))
                    .and_then(Item::as_str)
                    .or_else(|| doc.get("base_url").and_then(Item::as_str)),
            ),
        );
        values.insert(
            "apiKey".into(),
            string_val(
                entry
                    .and_then(|table| table.get("api_key"))
                    .and_then(Item::as_str)
                    .or_else(|| doc.get("api_key").and_then(Item::as_str)),
            ),
        );
        values
    }

    fn merge(mut doc: DocumentMut, desired: &BTreeMap<String, Value>) -> Result<DocumentMut> {
        let alias = active_model_alias(&doc);
        Self::ensure_shape(&mut doc, &alias)?;
        let entry = doc["model"]
            .as_table_mut()
            .and_then(|models| models.get_mut(&alias))
            .and_then(Item::as_table_mut)
            .ok_or_else(|| AppError::InvalidArg(format!("Grok model.{alias} must be a table")))?;
        if let Some(model) = get_str_map(desired, "model") {
            let t = model.trim();
            if t.is_empty() {
                entry.remove("model");
            } else {
                entry["model"] = toml_edit::value(t);
            }
        }
        if let Some(base) = get_str_map(desired, "baseUrl") {
            let t = base.trim();
            if t.is_empty() {
                entry.remove("base_url");
            } else {
                entry["base_url"] = toml_edit::value(t);
            }
        }
        if let Some(key) = get_str_map(desired, "apiKey") {
            if !secret_unchanged(Some(&key)) {
                entry["api_key"] = toml_edit::value(key.trim());
            }
        }
        Ok(doc)
    }
}

impl AgentConfigProjector for GrokConfigProjector {
    fn agent_key(&self) -> AgentKey {
        AgentKey::parse("grok").expect("builtin config projector key must be valid")
    }

    fn schema(&self) -> AgentConfigSchema {
        Self::schema_inner()
    }

    fn read_normalized(&self, agent_home: &Path) -> Result<NormalizedConfigDocument> {
        let path = Self::path(agent_home);
        let (doc, missing) = Self::read_toml(&path)?;
        let values = Self::extract(&doc);
        let schema = self.schema();
        // Scrub api_key from unknown_native content for API safety.
        let mut safe_doc = doc.clone();
        if safe_doc.get("api_key").and_then(|v| v.as_str()).is_some() {
            safe_doc["api_key"] = toml_edit::value(crate::platform::config::SECRET_REDACTED);
        }
        if let Some(models) = safe_doc.get_mut("model").and_then(Item::as_table_mut) {
            for (_, item) in models.iter_mut() {
                if let Some(entry) = item.as_table_mut() {
                    if entry.get("api_key").and_then(Item::as_str).is_some() {
                        entry["api_key"] =
                            toml_edit::value(crate::platform::config::SECRET_REDACTED);
                    }
                }
            }
        }
        let content = if missing {
            String::new()
        } else {
            safe_doc.to_string()
        };
        Ok(NormalizedConfigDocument {
            agent_key: AgentKey::from_agent_id(AgentId::Grok),
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
            AgentKey::from_agent_id(AgentId::Grok),
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
        let merged = Self::merge(doc, desired)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        atomic_write(&path, merged.to_string().as_bytes())?;
        finish_apply(
            AgentKey::from_agent_id(AgentId::Grok),
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
        let merged = Self::merge(doc, desired)?;
        Ok(json!({ "format": "toml", "content": merged.to_string() }))
    }
}

pub fn register(ctx: &mut crate::integrations::IntegrationContext<'_>) {
    ctx.config
        .register(std::sync::Arc::new(GrokConfigProjector))
        .expect("unique built-in config projector");
}
