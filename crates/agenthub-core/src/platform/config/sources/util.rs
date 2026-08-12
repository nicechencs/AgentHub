//! Shared helpers for config projectors.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use crate::error::{AppError, Result};
use crate::platform::AgentKey;
use crate::utils::atomic::atomic_write;

use super::super::document::{
    ConfigApplyResult, ConfigChangePlan, FieldChange, NormalizedConfigDocument,
};
use super::super::schema::{
    AgentConfigSchema, ConfigFieldSchema, ConfigValidationIssue, ConfigValidationResult,
    ConfigValueType, SECRET_REDACTED,
};

pub(super) fn field(
    key: &str,
    label: &str,
    value_type: ConfigValueType,
    secret: bool,
    required: bool,
    help: Option<&str>,
) -> ConfigFieldSchema {
    ConfigFieldSchema {
        key: key.to_string(),
        label: label.to_string(),
        value_type,
        required,
        secret,
        default: None,
        validation: None,
        help: help.map(|s| s.to_string()),
        capability: None,
    }
}

pub(super) fn string_val(v: Option<&str>) -> Value {
    Value::String(v.unwrap_or("").to_string())
}

pub(super) fn get_str_map(values: &BTreeMap<String, Value>, key: &str) -> Option<String> {
    values
        .get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

/// Secret is "unchanged" when omitted, empty, or redaction marker.
pub(super) fn secret_unchanged(desired: Option<&str>) -> bool {
    match desired {
        None => true,
        Some(s) => s.is_empty() || s == SECRET_REDACTED,
    }
}

pub(super) fn redact_secrets(
    mut values: BTreeMap<String, Value>,
    schema: &AgentConfigSchema,
) -> BTreeMap<String, Value> {
    for f in &schema.fields {
        if !f.secret {
            continue;
        }
        if let Some(v) = values.get_mut(&f.key) {
            if let Some(s) = v.as_str() {
                if !s.is_empty() && s != SECRET_REDACTED {
                    *v = Value::String(SECRET_REDACTED.to_string());
                }
            }
        }
    }
    values
}

pub(super) fn validate_known_fields(
    schema: &AgentConfigSchema,
    values: &BTreeMap<String, Value>,
) -> ConfigValidationResult {
    let mut issues = Vec::new();
    for f in &schema.fields {
        let Some(v) = values.get(&f.key) else {
            if f.required {
                issues.push(ConfigValidationIssue {
                    field_key: f.key.clone(),
                    code: "required".into(),
                    message: format!("{} is required", f.label),
                });
            }
            continue;
        };
        match &f.value_type {
            ConfigValueType::String | ConfigValueType::Secret => {
                if !v.is_string() && !v.is_null() {
                    issues.push(ConfigValidationIssue {
                        field_key: f.key.clone(),
                        code: "type".into(),
                        message: format!("{} must be a string", f.label),
                    });
                }
            }
            ConfigValueType::Number => {
                if !v.is_number() && !v.is_null() {
                    issues.push(ConfigValidationIssue {
                        field_key: f.key.clone(),
                        code: "type".into(),
                        message: format!("{} must be a number", f.label),
                    });
                }
            }
            ConfigValueType::Boolean => {
                if !v.is_boolean() && !v.is_null() {
                    issues.push(ConfigValidationIssue {
                        field_key: f.key.clone(),
                        code: "type".into(),
                        message: format!("{} must be a boolean", f.label),
                    });
                }
            }
            ConfigValueType::Enum { options } => {
                if let Some(s) = v.as_str() {
                    if !s.is_empty() && !options.iter().any(|o| o == s) {
                        issues.push(ConfigValidationIssue {
                            field_key: f.key.clone(),
                            code: "enum".into(),
                            message: format!("{} is not a valid option", f.label),
                        });
                    }
                } else if !v.is_null() {
                    issues.push(ConfigValidationIssue {
                        field_key: f.key.clone(),
                        code: "type".into(),
                        message: format!("{} must be a string enum", f.label),
                    });
                }
            }
        }
    }
    // Reject unknown keys at validation (strict for submit path).
    let known: std::collections::HashSet<&str> =
        schema.fields.iter().map(|f| f.key.as_str()).collect();
    for k in values.keys() {
        if !known.contains(k.as_str()) {
            issues.push(ConfigValidationIssue {
                field_key: k.clone(),
                code: "unknown_field".into(),
                message: format!("unknown field: {k}"),
            });
        }
    }
    if issues.is_empty() {
        ConfigValidationResult::success()
    } else {
        ConfigValidationResult::failure(issues)
    }
}

pub(super) fn plan_from_maps(
    agent_key: AgentKey,
    schema_version: u32,
    target_path: PathBuf,
    schema: &AgentConfigSchema,
    current: &BTreeMap<String, Value>,
    desired: &BTreeMap<String, Value>,
) -> ConfigChangePlan {
    let mut field_changes = Vec::new();
    for f in &schema.fields {
        let from = current.get(&f.key).cloned();
        let to = desired.get(&f.key).cloned();
        if f.secret {
            let from_s = from.as_ref().and_then(|v| v.as_str());
            let to_s = to.as_ref().and_then(|v| v.as_str());
            if secret_unchanged(to_s) {
                continue;
            }
            // Treat any non-redacted desired secret as a change (do not compare plaintext).
            if to_s.map(|s| !s.is_empty()).unwrap_or(false) {
                field_changes.push(FieldChange {
                    field_key: f.key.clone(),
                    from: from_s
                        .filter(|s| !s.is_empty())
                        .map(|_| Value::String(SECRET_REDACTED.to_string())),
                    to: Some(Value::String(SECRET_REDACTED.to_string())),
                    secret: true,
                });
            }
            continue;
        }
        let from_norm = normalize_cmp(&from);
        let to_norm = normalize_cmp(&to);
        if from_norm != to_norm {
            field_changes.push(FieldChange {
                field_key: f.key.clone(),
                from,
                to,
                secret: false,
            });
        }
    }
    ConfigChangePlan {
        agent_key,
        schema_version,
        target_path,
        field_changes,
    }
}

fn normalize_cmp(v: &Option<Value>) -> Option<String> {
    match v {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) => {
            let t = s.trim();
            if t.is_empty() {
                None
            } else {
                Some(t.to_string())
            }
        }
        Some(other) => Some(other.to_string()),
    }
}

pub(super) fn write_bytes(path: &Path, bytes: &[u8]) -> Result<()> {
    atomic_write(path, bytes)
}

pub(super) fn json_object_or_empty(v: &Value) -> Map<String, Value> {
    v.as_object().cloned().unwrap_or_default()
}

pub(super) fn invalid_toml(path: &Path, e: impl std::fmt::Display) -> AppError {
    AppError::InvalidArg(format!("invalid TOML at {}: {e}", path.display()))
}

pub(super) fn finish_apply(
    agent_key: AgentKey,
    schema: &AgentConfigSchema,
    path: PathBuf,
    current_values: &BTreeMap<String, Value>,
    desired: &BTreeMap<String, Value>,
    read_after: impl FnOnce() -> Result<NormalizedConfigDocument>,
) -> Result<ConfigApplyResult> {
    let plan = plan_from_maps(
        agent_key,
        schema.schema_version,
        path,
        schema,
        current_values,
        desired,
    );
    let mut document = read_after()?;
    document.values = redact_secrets(document.values, schema);
    Ok(ConfigApplyResult { document, plan })
}
