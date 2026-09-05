//! DeepSeek Harness home-level `cordis.patch.yml` projector.
//!
//! Native file is YAML; projected values are JSON. Secrets live in
//! `.credentials.yaml` and are never written into the patch.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::{json, Map, Value};

use crate::adapters::dsh::{
    read_credential_value, read_llm_fields, write_credential_value, write_llm_fields,
    CREDENTIALS_FILE, DEFAULT_API_KEY_ENV, DEFAULT_BASE_URL, DEFAULT_MODEL, DEFAULT_PROVIDER,
    HOME_PATCH_FILE,
};
use crate::error::{AppError, Result};
use crate::models::AgentId;
use crate::platform::AgentKey;
use crate::utils::atomic::with_restored_files;

use crate::platform::config::sources::util::{
    field, finish_apply, get_str_map, plan_from_maps, redact_secrets, secret_unchanged, string_val,
    validate_known_fields,
};
use crate::platform::config::AgentConfigProjector;
use crate::platform::config::{
    AgentConfigSchema, ConfigValidationResult, ConfigValueType, NativeConfigFormat, SECRET_REDACTED,
};
use crate::platform::config::{ConfigApplyResult, ConfigChangePlan, NormalizedConfigDocument};

const SCHEMA_VERSION: u32 = 1;

pub struct DshConfigProjector;

impl DshConfigProjector {
    fn schema_inner() -> AgentConfigSchema {
        AgentConfigSchema {
            agent_key: AgentKey::from_agent_id(AgentId::Dsh),
            schema_version: SCHEMA_VERSION,
            native_format: NativeConfigFormat::Json,
            relative_path: HOME_PATCH_FILE.into(),
            fields: vec![
                field(
                    "provider",
                    "Provider",
                    ConfigValueType::String,
                    false,
                    false,
                    Some("Official DeepSeek slot is deepseek-official"),
                ),
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
                    Some("Official API host, not written as a secret"),
                ),
                field(
                    "thinking",
                    "Thinking",
                    ConfigValueType::Enum {
                        options: vec!["enabled".into(), "disabled".into()],
                    },
                    false,
                    false,
                    None,
                ),
                field(
                    "reasoningEffort",
                    "Reasoning effort",
                    ConfigValueType::Enum {
                        options: vec!["off".into(), "low".into(), "high".into(), "max".into()],
                    },
                    false,
                    false,
                    None,
                ),
                field(
                    "maxTokens",
                    "Max tokens",
                    ConfigValueType::Number,
                    false,
                    false,
                    None,
                ),
                field(
                    "apiKeyEnv",
                    "API key env name",
                    ConfigValueType::String,
                    false,
                    false,
                    Some("Reference name only; value stays in credentials"),
                ),
                field(
                    "apiKey",
                    "API Key",
                    ConfigValueType::Secret,
                    true,
                    false,
                    Some("Stored in .credentials.yaml, never in cordis.patch.yml"),
                ),
            ],
        }
    }

    fn extract(home: &Path) -> Result<(BTreeMap<String, Value>, bool)> {
        let patch = home.join(HOME_PATCH_FILE);
        let creds = home.join(CREDENTIALS_FILE);
        let missing = !patch.exists() && !creds.exists();
        let fields = read_llm_fields(&patch)?;
        let key = read_credential_value(&creds, &fields.api_key_env)?.unwrap_or_default();
        let mut values = BTreeMap::new();
        values.insert("provider".into(), string_val(Some(DEFAULT_PROVIDER)));
        values.insert("model".into(), string_val(Some(&fields.model)));
        values.insert("baseUrl".into(), string_val(Some(&fields.base_url)));
        values.insert("thinking".into(), string_val(Some(&fields.thinking)));
        values.insert(
            "reasoningEffort".into(),
            string_val(Some(&fields.reasoning_effort)),
        );
        values.insert(
            "maxTokens".into(),
            fields.max_tokens.map(|n| json!(n)).unwrap_or(Value::Null),
        );
        values.insert("apiKeyEnv".into(), string_val(Some(&fields.api_key_env)));
        values.insert("apiKey".into(), string_val(Some(&key)));
        Ok((values, missing))
    }

    fn merge_fields(
        home: &Path,
        current: &BTreeMap<String, Value>,
        desired: &BTreeMap<String, Value>,
    ) -> Result<()> {
        let patch = home.join(HOME_PATCH_FILE);
        let mut fields = read_llm_fields(&patch)?;
        if let Some(v) = get_str_map(desired, "model") {
            if !v.trim().is_empty() {
                fields.model = v.trim().to_string();
            }
        }
        if let Some(v) = get_str_map(desired, "baseUrl") {
            fields.base_url = v.trim().to_string();
        }
        if let Some(v) = get_str_map(desired, "thinking") {
            if !v.trim().is_empty() {
                fields.thinking = v.trim().to_string();
            }
        }
        if let Some(v) = get_str_map(desired, "reasoningEffort") {
            if !v.trim().is_empty() {
                fields.reasoning_effort = v.trim().to_string();
            }
        }
        if let Some(v) = get_str_map(desired, "apiKeyEnv") {
            if !v.trim().is_empty() {
                fields.api_key_env = v.trim().to_string();
            }
        }
        if let Some(n) = desired.get("maxTokens").and_then(Value::as_u64) {
            fields.max_tokens = Some(n);
        }
        let creds = home.join(CREDENTIALS_FILE);
        with_restored_files(&[&patch, &creds], || {
            write_llm_fields(&patch, &fields)?;
            let desired_key = get_str_map(desired, "apiKey");
            if !secret_unchanged(desired_key.as_deref()) {
                write_credential_value(&creds, &fields.api_key_env, desired_key.unwrap().trim())?;
            } else if let Some(existing) = current
                .get("apiKey")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty() && *s != SECRET_REDACTED)
            {
                write_credential_value(&creds, &fields.api_key_env, existing)?;
            }
            Ok(())
        })
    }
}

impl AgentConfigProjector for DshConfigProjector {
    fn agent_key(&self) -> AgentKey {
        AgentKey::parse("dsh").expect("builtin config projector key must be valid")
    }

    fn schema(&self) -> AgentConfigSchema {
        Self::schema_inner()
    }

    fn read_normalized(&self, agent_home: &Path) -> Result<NormalizedConfigDocument> {
        let (values, missing) = Self::extract(agent_home)?;
        let schema = self.schema();
        Ok(NormalizedConfigDocument {
            agent_key: AgentKey::from_agent_id(AgentId::Dsh),
            schema_version: SCHEMA_VERSION,
            values: redact_secrets(values, &schema),
            unknown_native: json!({
                "format": "yaml",
                "path": HOME_PATCH_FILE,
            }),
            path: Some(agent_home.join(HOME_PATCH_FILE)),
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
            .unwrap_or_else(|| Path::new(HOME_PATCH_FILE).to_path_buf());
        Ok(plan_from_maps(
            AgentKey::from_agent_id(AgentId::Dsh),
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
        let (current_values, _) = Self::extract(agent_home)?;
        Self::merge_fields(agent_home, &current_values, desired)?;
        finish_apply(
            AgentKey::from_agent_id(AgentId::Dsh),
            &schema,
            agent_home.join(HOME_PATCH_FILE),
            &current_values,
            desired,
            || self.read_normalized(agent_home),
        )
    }

    fn materialize_settings_config(
        &self,
        _base_raw: Option<&Value>,
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
        let mut out = Map::new();
        out.insert("provider".into(), json!(DEFAULT_PROVIDER));
        out.insert(
            "model".into(),
            json!(get_str_map(desired, "model").unwrap_or_else(|| DEFAULT_MODEL.into())),
        );
        out.insert(
            "baseURL".into(),
            json!(get_str_map(desired, "baseUrl").unwrap_or_else(|| DEFAULT_BASE_URL.into())),
        );
        if let Some(v) = get_str_map(desired, "thinking") {
            out.insert("thinking".into(), json!(v));
        }
        if let Some(v) = get_str_map(desired, "reasoningEffort") {
            out.insert("reasoningEffort".into(), json!(v));
        }
        if let Some(n) = desired.get("maxTokens").and_then(Value::as_u64) {
            out.insert("maxTokens".into(), json!(n));
        }
        out.insert(
            "apiKeyEnv".into(),
            json!(get_str_map(desired, "apiKeyEnv").unwrap_or_else(|| DEFAULT_API_KEY_ENV.into())),
        );
        if let Some(key) = get_str_map(desired, "apiKey") {
            if !secret_unchanged(Some(&key)) {
                out.insert("api_key".into(), json!(key));
            }
        }
        Ok(Value::Object(out))
    }
}

pub fn register(ctx: &mut crate::integrations::IntegrationContext<'_>) {
    ctx.config
        .register(std::sync::Arc::new(DshConfigProjector))
        .expect("unique built-in config projector");
}
