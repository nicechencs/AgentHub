//! Codex config.toml + auth.json projector.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::{json, Value};
use toml_edit::DocumentMut;

use crate::error::{AppError, Result};
use crate::models::AgentId;
use crate::platform::AgentKey;
use crate::utils::atomic::atomic_write;

use crate::platform::config::sources::util::{
    field, finish_apply, get_str_map, invalid_toml, plan_from_maps, redact_secrets,
    secret_unchanged, string_val, validate_known_fields,
};
use crate::platform::config::AgentConfigProjector;
use crate::platform::config::{
    AgentConfigSchema, ConfigValidationResult, ConfigValueType, NativeConfigFormat,
};
use crate::platform::config::{ConfigApplyResult, ConfigChangePlan, NormalizedConfigDocument};

const SCHEMA_VERSION: u32 = 1;
const REL_PATH: &str = "config.toml";

pub struct CodexConfigProjector;

impl CodexConfigProjector {
    fn schema_inner() -> AgentConfigSchema {
        AgentConfigSchema {
            agent_key: AgentKey::from_agent_id(AgentId::Codex),
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
                    Some("model_providers.<slug>.base_url"),
                ),
                field(
                    "apiKey",
                    "OpenAI API Key",
                    ConfigValueType::Secret,
                    true,
                    false,
                    Some("auth.json OPENAI_API_KEY (not stored in config.toml)"),
                ),
                field(
                    "reasoningEffort",
                    "Reasoning effort",
                    ConfigValueType::String,
                    false,
                    false,
                    Some("model_reasoning_effort"),
                ),
                field(
                    "wireApi",
                    "Wire API",
                    ConfigValueType::String,
                    false,
                    false,
                    Some("model_providers.<slug>.wire_api"),
                ),
                field(
                    "providerSlug",
                    "Provider slug",
                    ConfigValueType::String,
                    false,
                    false,
                    Some("model_provider / [model_providers.slug]"),
                ),
            ],
        }
    }

    fn config_path(home: &Path) -> std::path::PathBuf {
        home.join(REL_PATH)
    }

    fn auth_path(home: &Path) -> std::path::PathBuf {
        home.join("auth.json")
    }

    fn read_toml(path: &Path) -> Result<(DocumentMut, bool)> {
        if !path.exists() {
            return Ok((DocumentMut::new(), true));
        }
        let text = std::fs::read_to_string(path)?;
        if text.trim().is_empty() {
            return Ok((DocumentMut::new(), false));
        }
        let doc = text
            .parse::<DocumentMut>()
            .map_err(|e| invalid_toml(path, e))?;
        Ok((doc, false))
    }

    fn doc_str(doc: &DocumentMut, key: &str) -> String {
        doc.get(key)
            .and_then(|i| i.as_str())
            .unwrap_or("")
            .to_string()
    }

    fn provider_table_str(doc: &DocumentMut, slug: &str, key: &str) -> String {
        doc.get("model_providers")
            .and_then(|p| p.as_table())
            .and_then(|t| t.get(slug))
            .and_then(|item| item.get(key))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string()
    }

    fn first_provider_slug(doc: &DocumentMut) -> String {
        if let Some(providers) = doc.get("model_providers").and_then(|i| i.as_table()) {
            if let Some((name, _)) = providers.iter().next() {
                return name.to_string();
            }
        }
        "custom".into()
    }

    fn extract(doc: &DocumentMut, api_key: Option<&str>) -> BTreeMap<String, Value> {
        let top_slug = Self::doc_str(doc, "model_provider");
        let slug = if top_slug.is_empty() {
            Self::first_provider_slug(doc)
        } else {
            top_slug
        };
        let mut values = BTreeMap::new();
        values.insert(
            "model".into(),
            string_val(Some(&Self::doc_str(doc, "model"))),
        );
        values.insert(
            "baseUrl".into(),
            string_val(Some(&Self::provider_table_str(doc, &slug, "base_url"))),
        );
        values.insert("apiKey".into(), string_val(api_key));
        values.insert(
            "reasoningEffort".into(),
            string_val(Some(&Self::doc_str(doc, "model_reasoning_effort"))),
        );
        let wire = Self::provider_table_str(doc, &slug, "wire_api");
        values.insert(
            "wireApi".into(),
            string_val(Some(if wire.is_empty() { "responses" } else { &wire })),
        );
        values.insert("providerSlug".into(), string_val(Some(&slug)));
        values
    }

    fn read_api_key(auth_path: &Path) -> Result<Option<String>> {
        if !auth_path.exists() {
            return Ok(None);
        }
        let text = std::fs::read_to_string(auth_path)?;
        if text.trim().is_empty() {
            return Ok(None);
        }
        let v: Value = serde_json::from_str(&text)
            .map_err(|e| AppError::InvalidArg(format!("invalid Codex auth.json: {e}")))?;
        Ok(v.get("OPENAI_API_KEY")
            .and_then(|x| x.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string()))
    }

    fn ensure_provider<'a>(doc: &'a mut DocumentMut, slug: &str) -> Result<()> {
        if doc.get("model_providers").is_none() {
            doc["model_providers"] = toml_edit::table();
        }
        let providers = doc["model_providers"]
            .as_table_mut()
            .ok_or_else(|| AppError::InvalidArg("Codex model_providers must be a table".into()))?;
        if providers.get(slug).is_none() {
            providers.insert(slug, toml_edit::table());
        }
        Ok(())
    }

    fn merge_toml(
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
                doc.remove("model");
            } else {
                doc["model"] = toml_edit::value(t);
            }
        }
        doc["model_provider"] = toml_edit::value(slug.as_str());

        if let Some(effort) = get_str_map(desired, "reasoningEffort") {
            let t = effort.trim();
            if t.is_empty() {
                doc.remove("model_reasoning_effort");
            } else {
                doc["model_reasoning_effort"] = toml_edit::value(t);
            }
        }

        Self::ensure_provider(&mut doc, &slug)?;
        {
            let providers = doc["model_providers"].as_table_mut().unwrap();
            let entry = providers.get_mut(slug.as_str()).unwrap();
            if entry
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .is_empty()
            {
                entry["name"] = toml_edit::value(slug.as_str());
            }
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
            if let Some(wire) = get_str_map(desired, "wireApi") {
                let t = wire.trim();
                if t.is_empty() {
                    if entry
                        .get("wire_api")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .is_empty()
                    {
                        entry["wire_api"] = toml_edit::value("responses");
                    }
                } else {
                    entry["wire_api"] = toml_edit::value(t);
                }
            } else if entry
                .get("wire_api")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .is_empty()
            {
                entry["wire_api"] = toml_edit::value("responses");
            }
        }
        Ok(doc)
    }

    fn write_auth(path: &Path, api_key: &str) -> Result<()> {
        super::write_api_key_auth(path, api_key)
    }
}

impl AgentConfigProjector for CodexConfigProjector {
    fn agent_key(&self) -> AgentKey {
        AgentKey::parse("codex").expect("builtin config projector key must be valid")
    }

    fn schema(&self) -> AgentConfigSchema {
        Self::schema_inner()
    }

    fn read_normalized(&self, agent_home: &Path) -> Result<NormalizedConfigDocument> {
        let path = Self::config_path(agent_home);
        let (doc, missing) = Self::read_toml(&path)?;
        let api_key = Self::read_api_key(&Self::auth_path(agent_home))?;
        let values = Self::extract(&doc, api_key.as_deref());
        let schema = self.schema();
        let unknown = if path.exists() {
            let text = std::fs::read_to_string(&path).unwrap_or_default();
            json!({ "format": "toml", "content": text })
        } else {
            json!({ "format": "toml", "content": "" })
        };
        Ok(NormalizedConfigDocument {
            agent_key: AgentKey::from_agent_id(AgentId::Codex),
            schema_version: SCHEMA_VERSION,
            values: redact_secrets(values, &schema),
            unknown_native: unknown,
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
            AgentKey::from_agent_id(AgentId::Codex),
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

        let path = Self::config_path(agent_home);
        let (doc, _) = Self::read_toml(&path)?;
        let api_key = Self::read_api_key(&Self::auth_path(agent_home))?;
        let current_values = Self::extract(&doc, api_key.as_deref());
        let merged = Self::merge_toml(doc, &current_values, desired)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        atomic_write(&path, merged.to_string().as_bytes())?;

        if let Some(key) = get_str_map(desired, "apiKey") {
            if !secret_unchanged(Some(&key)) {
                Self::write_auth(&Self::auth_path(agent_home), key.trim())?;
            }
        }

        finish_apply(
            AgentKey::from_agent_id(AgentId::Codex),
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
        let api_key = base_raw
            .and_then(|v| v.get("auth"))
            .and_then(|a| a.get("OPENAI_API_KEY"))
            .and_then(|v| v.as_str());
        let current_values = Self::extract(&doc, api_key);
        let merged = Self::merge_toml(doc, &current_values, desired)?;
        let mut out = json!({
            "format": "toml",
            "content": merged.to_string(),
        });
        if let Some(key) = get_str_map(desired, "apiKey") {
            if !secret_unchanged(Some(&key)) {
                out.as_object_mut()
                    .unwrap()
                    .insert("auth".into(), json!({ "OPENAI_API_KEY": key.trim() }));
            } else if let Some(prev) = api_key.filter(|s| !s.is_empty()) {
                out.as_object_mut()
                    .unwrap()
                    .insert("auth".into(), json!({ "OPENAI_API_KEY": prev }));
            }
        } else if let Some(prev) = api_key.filter(|s| !s.is_empty()) {
            out.as_object_mut()
                .unwrap()
                .insert("auth".into(), json!({ "OPENAI_API_KEY": prev }));
        }
        Ok(out)
    }
}

pub fn register(ctx: &mut crate::integrations::IntegrationContext<'_>) {
    ctx.config
        .register(std::sync::Arc::new(CodexConfigProjector))
        .expect("unique built-in config projector");
}
