//! Claude Code settings.json projector.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::{Map, Value};

use crate::error::{AppError, Result};
use crate::models::AgentId;
use crate::platform::AgentKey;

use crate::platform::config::sources::util::{
    field, finish_apply, get_str_map, json_object_or_empty, plan_from_maps, redact_secrets,
    secret_unchanged, string_val, validate_known_fields, write_bytes,
};
use crate::platform::config::AgentConfigProjector;
use crate::platform::config::{
    AgentConfigSchema, ConfigValidationResult, ConfigValueType, NativeConfigFormat, SECRET_REDACTED,
};
use crate::platform::config::{ConfigApplyResult, ConfigChangePlan, NormalizedConfigDocument};

const SCHEMA_VERSION: u32 = 1;
const REL_PATH: &str = "settings.json";

const ROLE_ENV: &[(&str, &str)] = &[
    ("modelOpus", "ANTHROPIC_DEFAULT_OPUS_MODEL"),
    ("modelSonnet", "ANTHROPIC_DEFAULT_SONNET_MODEL"),
    ("modelHaiku", "ANTHROPIC_DEFAULT_HAIKU_MODEL"),
    ("modelFable", "ANTHROPIC_DEFAULT_FABLE_MODEL"),
    ("modelSubagent", "CLAUDE_CODE_SUBAGENT_MODEL"),
];

pub struct ClaudeConfigProjector;

impl ClaudeConfigProjector {
    fn schema_inner() -> AgentConfigSchema {
        AgentConfigSchema {
            agent_key: AgentKey::from_agent_id(AgentId::Claude),
            schema_version: SCHEMA_VERSION,
            native_format: NativeConfigFormat::Json,
            relative_path: REL_PATH.into(),
            fields: vec![
                field(
                    "baseUrl",
                    "Base URL",
                    ConfigValueType::String,
                    false,
                    false,
                    Some("ANTHROPIC_BASE_URL"),
                ),
                field(
                    "apiKey",
                    "API Key / Auth Token",
                    ConfigValueType::Secret,
                    true,
                    false,
                    Some("ANTHROPIC_AUTH_TOKEN or ANTHROPIC_API_KEY"),
                ),
                field(
                    "claudeAuthEnv",
                    "Auth env name",
                    ConfigValueType::Enum {
                        options: vec!["ANTHROPIC_AUTH_TOKEN".into(), "ANTHROPIC_API_KEY".into()],
                    },
                    false,
                    false,
                    None,
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
                    "modelOpus",
                    "Opus model",
                    ConfigValueType::String,
                    false,
                    false,
                    None,
                ),
                field(
                    "modelSonnet",
                    "Sonnet model",
                    ConfigValueType::String,
                    false,
                    false,
                    None,
                ),
                field(
                    "modelHaiku",
                    "Haiku model",
                    ConfigValueType::String,
                    false,
                    false,
                    None,
                ),
                field(
                    "modelFable",
                    "Fable model",
                    ConfigValueType::String,
                    false,
                    false,
                    None,
                ),
                field(
                    "modelSubagent",
                    "Subagent model",
                    ConfigValueType::String,
                    false,
                    false,
                    None,
                ),
            ],
        }
    }

    fn config_path(home: &Path) -> std::path::PathBuf {
        home.join(REL_PATH)
    }

    fn extract(root: &Map<String, Value>) -> BTreeMap<String, Value> {
        let env = root
            .get("env")
            .and_then(|v| v.as_object())
            .cloned()
            .unwrap_or_default();
        let token = env
            .get("ANTHROPIC_AUTH_TOKEN")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let api_key = env
            .get("ANTHROPIC_API_KEY")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let auth_env = if !token.is_empty() {
            "ANTHROPIC_AUTH_TOKEN"
        } else if !api_key.is_empty() {
            "ANTHROPIC_API_KEY"
        } else {
            "ANTHROPIC_AUTH_TOKEN"
        };
        let raw_key = if !token.is_empty() { token } else { api_key };
        let model = root
            .get("model")
            .and_then(|v| v.as_str())
            .or_else(|| env.get("ANTHROPIC_MODEL").and_then(|v| v.as_str()))
            .unwrap_or("");

        let mut values = BTreeMap::new();
        values.insert(
            "baseUrl".into(),
            string_val(env.get("ANTHROPIC_BASE_URL").and_then(|v| v.as_str())),
        );
        values.insert("apiKey".into(), string_val(Some(raw_key)));
        values.insert("claudeAuthEnv".into(), string_val(Some(auth_env)));
        values.insert("model".into(), string_val(Some(model)));
        for (field_key, env_key) in ROLE_ENV {
            values.insert(
                (*field_key).into(),
                string_val(env.get(*env_key).and_then(|v| v.as_str())),
            );
        }
        values
    }

    fn known_keys() -> std::collections::HashSet<&'static str> {
        [
            "baseUrl",
            "apiKey",
            "claudeAuthEnv",
            "model",
            "modelOpus",
            "modelSonnet",
            "modelHaiku",
            "modelFable",
            "modelSubagent",
        ]
        .into_iter()
        .collect()
    }

    /// Merge desired known fields into a root object; preserve other keys.
    fn merge_into_root(
        mut root: Map<String, Value>,
        current: &BTreeMap<String, Value>,
        desired: &BTreeMap<String, Value>,
    ) -> Result<Map<String, Value>> {
        let mut env = root
            .get("env")
            .and_then(|v| v.as_object())
            .cloned()
            .unwrap_or_default();

        let auth_env = get_str_map(desired, "claudeAuthEnv")
            .or_else(|| get_str_map(current, "claudeAuthEnv"))
            .unwrap_or_else(|| "ANTHROPIC_AUTH_TOKEN".into());
        if auth_env != "ANTHROPIC_AUTH_TOKEN" && auth_env != "ANTHROPIC_API_KEY" {
            return Err(AppError::InvalidArg(
                "claudeAuthEnv must be ANTHROPIC_AUTH_TOKEN or ANTHROPIC_API_KEY".into(),
            ));
        }
        let other = if auth_env == "ANTHROPIC_AUTH_TOKEN" {
            "ANTHROPIC_API_KEY"
        } else {
            "ANTHROPIC_AUTH_TOKEN"
        };
        env.remove(other);

        if let Some(base) = get_str_map(desired, "baseUrl") {
            let t = base.trim();
            if t.is_empty() {
                env.remove("ANTHROPIC_BASE_URL");
            } else {
                env.insert("ANTHROPIC_BASE_URL".into(), Value::String(t.to_string()));
            }
        }

        let desired_key = get_str_map(desired, "apiKey");
        if !secret_unchanged(desired_key.as_deref()) {
            let key = desired_key.unwrap();
            env.insert(auth_env.clone(), Value::String(key));
        } else {
            // Keep whichever secret currently exists; ensure selected env name.
            let existing = current
                .get("apiKey")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty() && *s != SECRET_REDACTED)
                .map(|s| s.to_string())
                .or_else(|| {
                    env.get(&auth_env)
                        .and_then(|v| v.as_str())
                        .filter(|s| !s.is_empty() && *s != SECRET_REDACTED)
                        .map(|s| s.to_string())
                })
                .or_else(|| {
                    env.get(other)
                        .and_then(|v| v.as_str())
                        .filter(|s| !s.is_empty() && *s != SECRET_REDACTED)
                        .map(|s| s.to_string())
                });
            if let Some(k) = existing {
                env.insert(auth_env.clone(), Value::String(k));
            }
        }
        // Ensure auth env key name is selected
        if env.contains_key("ANTHROPIC_AUTH_TOKEN") || env.contains_key("ANTHROPIC_API_KEY") {
            // move secret to selected name if needed
            if auth_env == "ANTHROPIC_AUTH_TOKEN" {
                if let Some(v) = env.remove("ANTHROPIC_API_KEY") {
                    if !env.contains_key("ANTHROPIC_AUTH_TOKEN") {
                        env.insert("ANTHROPIC_AUTH_TOKEN".into(), v);
                    }
                }
            } else if let Some(v) = env.remove("ANTHROPIC_AUTH_TOKEN") {
                if !env.contains_key("ANTHROPIC_API_KEY") {
                    env.insert("ANTHROPIC_API_KEY".into(), v);
                }
            }
        }

        if let Some(model) = get_str_map(desired, "model") {
            let t = model.trim();
            if t.is_empty() {
                root.remove("model");
                env.remove("ANTHROPIC_MODEL");
            } else {
                root.insert("model".into(), Value::String(t.to_string()));
                env.insert("ANTHROPIC_MODEL".into(), Value::String(t.to_string()));
            }
        }

        for (field_key, env_key) in ROLE_ENV {
            if let Some(v) = get_str_map(desired, field_key) {
                let t = v.trim();
                if t.is_empty() {
                    env.remove(*env_key);
                } else {
                    env.insert((*env_key).into(), Value::String(t.to_string()));
                }
            }
        }

        root.insert("env".into(), Value::Object(env));
        let _ = Self::known_keys();
        Ok(root)
    }
}

impl AgentConfigProjector for ClaudeConfigProjector {
    fn agent_key(&self) -> AgentKey {
        AgentKey::parse("claude").expect("builtin config projector key must be valid")
    }

    fn schema(&self) -> AgentConfigSchema {
        Self::schema_inner()
    }

    fn read_normalized(&self, agent_home: &Path) -> Result<NormalizedConfigDocument> {
        let path = Self::config_path(agent_home);
        let (root, missing) = if path.exists() {
            let text = std::fs::read_to_string(&path)?;
            if text.trim().is_empty() {
                (Map::new(), false)
            } else {
                let v: Value = serde_json::from_str(&text).map_err(|e| {
                    AppError::InvalidArg(format!("invalid Claude settings.json: {e}"))
                })?;
                if !v.is_object() {
                    return Err(AppError::InvalidArg(
                        "Claude settings.json must be a JSON object".into(),
                    ));
                }
                (json_object_or_empty(&v), false)
            }
        } else {
            (Map::new(), true)
        };
        let values = Self::extract(&root);
        let schema = self.schema();
        Ok(NormalizedConfigDocument {
            agent_key: AgentKey::from_agent_id(AgentId::Claude),
            schema_version: SCHEMA_VERSION,
            values: redact_secrets(values, &schema),
            unknown_native: Value::Object(root),
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
            AgentKey::from_agent_id(AgentId::Claude),
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
        // Re-read unredacted root for merge
        let root = if path.exists() {
            let text = std::fs::read_to_string(&path)?;
            if text.trim().is_empty() {
                Map::new()
            } else {
                let v: Value = serde_json::from_str(&text).map_err(|e| {
                    AppError::InvalidArg(format!("invalid Claude settings.json: {e}"))
                })?;
                json_object_or_empty(&v)
            }
        } else {
            Map::new()
        };
        // Unredacted extract for secret merge
        let current_values = Self::extract(&root);
        let merged = Self::merge_into_root(root, &current_values, desired)?;
        let mut bytes = serde_json::to_vec_pretty(&Value::Object(merged))?;
        bytes.push(b'\n');
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        write_bytes(&path, &bytes)?;

        finish_apply(
            AgentKey::from_agent_id(AgentId::Claude),
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
        let root = base_raw.map(json_object_or_empty).unwrap_or_default();
        let current_values = Self::extract(&root);
        let merged = Self::merge_into_root(root, &current_values, desired)?;
        Ok(Value::Object(merged))
    }
}

pub fn register(ctx: &mut crate::integrations::IntegrationContext<'_>) {
    ctx.config
        .register(std::sync::Arc::new(ClaudeConfigProjector))
        .expect("unique built-in config projector");
}
