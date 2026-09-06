//! B2 helpers: model/effort validation, Codex list parsing, turn input building.

use serde_json::Value;

use crate::error::{AppError, Result};

use super::types::{
    RuntimeExtensionItem, RuntimeExtensionKind, RuntimeLocalImage, RuntimeModelOption,
    RuntimeSkillRef, RuntimeTurnSettings,
};

pub(crate) fn phase_freezes_settings(phase: super::types::RuntimePhase) -> bool {
    matches!(
        phase,
        super::types::RuntimePhase::Starting
            | super::types::RuntimePhase::Running
            | super::types::RuntimePhase::Waiting
            | super::types::RuntimePhase::Cancelling
    )
}

/// Idle / unknown phases may spawn a short-lived Codex process for model/skills lists.
/// Active turns must never fetch — only serve an already-warmed per-conversation cache.
pub(crate) fn may_fetch_catalog(phase: Option<super::types::RuntimePhase>) -> bool {
    !phase.is_some_and(phase_freezes_settings)
}

/// Default / first supported effort for a model/list row.
/// Ignores a defaultReasoningEffort that is not in supportedReasoningEfforts.
pub(crate) fn resolved_default_effort(option: &RuntimeModelOption) -> Option<String> {
    if let Some(default) = option
        .default_effort
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        if option.efforts.iter().any(|item| item == default) {
            return Some(default.to_string());
        }
    }
    option.efforts.first().cloned()
}

fn trim_setting(value: &Option<String>) -> Option<String> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// Soft repair for idle options(): replace an unsupported effort with the model
/// default / first supported value. Returns `None` when no write is needed.
pub(crate) fn reconcile_turn_settings(
    stored: &RuntimeTurnSettings,
    catalog: &[RuntimeModelOption],
) -> Option<RuntimeTurnSettings> {
    if catalog.is_empty() {
        return None;
    }
    let model = trim_setting(&stored.model);
    let effort = trim_setting(&stored.effort);
    let Some(model_id) = model else {
        if effort.is_some() || stored.model.is_some() || stored.effort.is_some() {
            return Some(RuntimeTurnSettings {
                model: None,
                effort: None,
            });
        }
        return None;
    };
    let Some(option) = catalog.iter().find(|item| item.id == model_id) else {
        return None;
    };
    let compatible = match effort.as_deref() {
        None => true,
        Some(_) if option.efforts.is_empty() => false,
        Some(value) => option.efforts.iter().any(|item| item == value),
    };
    if compatible {
        let normalized = RuntimeTurnSettings {
            model: Some(model_id),
            effort,
        };
        return if &normalized == stored {
            None
        } else {
            Some(normalized)
        };
    }
    Some(RuntimeTurnSettings {
        model: Some(model_id),
        effort: resolved_default_effort(option),
    })
}

/// Strict check used by `start`: reject unsupported (model, effort) pairs.
/// Omitted effort is allowed (Codex uses its own default). Empty catalog skips.
pub(crate) fn assert_settings_supported(
    settings: &RuntimeTurnSettings,
    catalog: &[RuntimeModelOption],
) -> Result<()> {
    if catalog.is_empty() {
        return Ok(());
    }
    let model = trim_setting(&settings.model);
    let effort = trim_setting(&settings.effort);
    if model.is_none() && effort.is_some() {
        return Err(AppError::InvalidArg(
            "选择思考强度前需要先选择模型".into(),
        ));
    }
    let Some(model_id) = model else {
        return Ok(());
    };
    let option = catalog.iter().find(|item| item.id == model_id).ok_or_else(|| {
        AppError::InvalidArg(format!("模型不可用: {model_id}"))
    })?;
    if let Some(value) = effort {
        if option.efforts.is_empty() {
            return Err(AppError::InvalidArg(format!(
                "模型 {model_id} 不支持思考强度"
            )));
        }
        if !option.efforts.iter().any(|item| item == &value) {
            return Err(AppError::InvalidArg(format!(
                "模型 {model_id} 不支持思考强度 {value}"
            )));
        }
    }
    Ok(())
}

/// Validate requested settings against a model/list catalog.
/// Empty catalog: only reject obviously empty model ids; effort may be set with model.
/// When effort is omitted, fill the model's default or first supported effort.
pub(crate) fn validate_turn_settings(
    requested: &RuntimeTurnSettings,
    catalog: &[RuntimeModelOption],
    prior: &RuntimeTurnSettings,
) -> Result<RuntimeTurnSettings> {
    let model = trim_setting(&requested.model);
    let effort = trim_setting(&requested.effort);

    if model.is_none() && effort.is_some() {
        return Err(AppError::InvalidArg(
            "选择思考强度前需要先选择模型".into(),
        ));
    }

    if catalog.is_empty() {
        return Ok(RuntimeTurnSettings { model, effort });
    }

    let Some(model_id) = model.clone() else {
        return Ok(RuntimeTurnSettings {
            model: None,
            effort: None,
        });
    };

    let option = catalog.iter().find(|item| item.id == model_id).ok_or_else(|| {
        AppError::InvalidArg(format!("模型不可用: {model_id}"))
    })?;

    let effort = match effort {
        None => resolved_default_effort(option),
        Some(value) => {
            if option.efforts.is_empty() {
                // Model advertises no efforts; reject non-empty effort.
                return Err(AppError::InvalidArg(format!(
                    "模型 {model_id} 不支持思考强度"
                )));
            }
            if !option.efforts.iter().any(|item| item == &value) {
                return Err(AppError::InvalidArg(format!(
                    "模型 {model_id} 不支持思考强度 {value}"
                )));
            }
            Some(value)
        }
    };

    let _ = prior;
    Ok(RuntimeTurnSettings {
        model: Some(model_id),
        effort,
    })
}

/// Extract one effort name from a model/list entry (string or object).
/// Objects with `available: false` / `supported: false` are skipped.
fn parse_effort_entry(item: &Value) -> Option<String> {
    if let Some(s) = item.as_str() {
        let trimmed = s.trim();
        return (!trimmed.is_empty()).then(|| trimmed.to_string());
    }
    let obj = item.as_object()?;
    // Prefer explicit unavailability markers when Codex encodes them.
    if obj.get("available").and_then(Value::as_bool) == Some(false)
        || obj.get("supported").and_then(Value::as_bool) == Some(false)
    {
        return None;
    }
    obj.get("reasoningEffort")
        .or_else(|| obj.get("effort"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

pub(crate) fn parse_model_list(value: &Value) -> Vec<RuntimeModelOption> {
    let rows = value
        .get("data")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let mut out = Vec::new();
    for row in rows {
        let id = row
            .get("id")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string);
        let Some(id) = id else { continue };
        let mut seen = std::collections::HashSet::new();
        let efforts = row
            .get("supportedReasoningEfforts")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(parse_effort_entry)
                    .filter(|s| seen.insert(s.clone()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let default_effort = row
            .get("defaultReasoningEffort")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string);
        let option = RuntimeModelOption {
            id,
            efforts,
            default_effort,
        };
        // Keep stored default only when it is actually supported; otherwise first effort.
        let default_effort = resolved_default_effort(&option);
        out.push(RuntimeModelOption {
            id: option.id,
            efforts: option.efforts,
            default_effort,
        });
    }
    out
}

/// Drop previously denied efforts from a catalog (Codex may over-report support).
pub(crate) fn apply_denied_efforts(
    models: &[RuntimeModelOption],
    denied: &std::collections::HashMap<String, std::collections::HashSet<String>>,
) -> Vec<RuntimeModelOption> {
    if denied.is_empty() {
        return models.to_vec();
    }
    models
        .iter()
        .map(|option| {
            let Some(blocked) = denied.get(&option.id) else {
                return option.clone();
            };
            if blocked.is_empty() {
                return option.clone();
            }
            let efforts: Vec<String> = option
                .efforts
                .iter()
                .filter(|effort| !blocked.contains(effort.as_str()))
                .cloned()
                .collect();
            let mut next = RuntimeModelOption {
                id: option.id.clone(),
                efforts,
                default_effort: option.default_effort.clone(),
            };
            next.default_effort = resolved_default_effort(&next);
            next
        })
        .collect()
}

/// Match the same upstream failure class that UI maps to thinkingUnsupported.
pub(crate) fn looks_like_thinking_unsupported(message: &str) -> bool {
    let hay = message.to_ascii_lowercase();
    hay.contains("reasoningeffort")
        || hay.contains("reasoning_effort")
        || hay.contains("does not support parameter")
        || hay.contains("不支持思考强度")
        || hay.contains("不支持当前思考设置")
}

pub(crate) fn parse_skills_list(value: &Value) -> Vec<RuntimeExtensionItem> {
    let mut out = Vec::new();
    let roots = value
        .get("data")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    for root in roots {
        // Directory result may nest skills; also accept flat skill objects.
        let skills = root
            .get("skills")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_else(|| {
                if root.get("name").is_some() || root.get("id").is_some() {
                    vec![root.clone()]
                } else {
                    Vec::new()
                }
            });
        for skill in skills {
            let name = skill
                .get("name")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string);
            let path = skill
                .get("path")
                .or_else(|| skill.get("skillPath"))
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string);
            let id = skill
                .get("id")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .or_else(|| path.clone())
                .or_else(|| name.clone());
            let Some(id) = id else { continue };
            let Some(name) = name.or_else(|| Some(id.clone())) else { continue };
            let enabled = skill
                .get("enabled")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            let loaded = skill
                .get("loaded")
                .or_else(|| skill.get("isLoaded"))
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let callable = path.is_some();
            out.push(RuntimeExtensionItem {
                id,
                name,
                kind: RuntimeExtensionKind::Skill,
                installed: true,
                enabled,
                loaded,
                callable,
                path,
            });
        }
    }
    out
}

pub(crate) fn parse_plugins_installed(value: &Value) -> Vec<RuntimeExtensionItem> {
    let mut out = Vec::new();
    let marketplaces = value
        .get("marketplaces")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    for market in marketplaces {
        let plugins = market
            .get("plugins")
            .or_else(|| market.get("installedPlugins"))
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        for plugin in plugins {
            let name = plugin
                .get("name")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string);
            let id = plugin
                .get("id")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .or_else(|| name.clone());
            let Some(id) = id else { continue };
            let name = name.unwrap_or_else(|| id.clone());
            let enabled = plugin
                .get("enabled")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let loaded = plugin
                .get("loaded")
                .or_else(|| plugin.get("isLoaded"))
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            out.push(RuntimeExtensionItem {
                id,
                name,
                kind: RuntimeExtensionKind::Plugin,
                installed: true,
                enabled,
                loaded,
                // Plugins are not directly callable via turn input in B2.
                callable: false,
                path: None,
            });
        }
    }
    out
}

pub(crate) fn build_turn_input(
    prompt: &str,
    images: &[RuntimeLocalImage],
    skills: &[RuntimeSkillRef],
) -> Result<Vec<Value>> {
    let mut input = vec![serde_json::json!({ "type": "text", "text": prompt })];
    for image in images {
        let path = image.path.trim();
        if path.is_empty() {
            return Err(AppError::InvalidArg("图片路径不能为空".into()));
        }
        // Reject pretending files are attachments via path-in-prompt; require localImage.
        input.push(serde_json::json!({ "type": "localImage", "path": path }));
    }
    for skill in skills {
        let name = skill.name.trim();
        let path = skill.path.trim();
        if name.is_empty() || path.is_empty() {
            return Err(AppError::InvalidArg(
                "Skill 需要稳定名称和路径，不能只按显示名调用".into(),
            ));
        }
        input.push(serde_json::json!({ "type": "skill", "name": name, "path": path }));
    }
    Ok(input)
}

pub(crate) fn validate_skill_refs(
    skills: &[RuntimeSkillRef],
    catalog: &[RuntimeExtensionItem],
) -> Result<()> {
    if skills.is_empty() {
        return Ok(());
    }
    for skill in skills {
        let name = skill.name.trim();
        let path = skill.path.trim();
        if name.is_empty() || path.is_empty() {
            return Err(AppError::InvalidArg(
                "Skill 需要稳定名称和路径，不能只按显示名调用".into(),
            ));
        }
        let matched = catalog.iter().any(|item| {
            item.kind == RuntimeExtensionKind::Skill
                && item.callable
                && item.name == name
                && item.path.as_deref() == Some(path)
        });
        if !matched {
            // When catalog is empty (Codex unavailable), still require name+path but cannot prove callability.
            if catalog.iter().any(|item| item.kind == RuntimeExtensionKind::Skill) {
                return Err(AppError::InvalidArg(format!(
                    "Skill 不可用于本轮调用: {name}"
                )));
            }
        }
    }
    Ok(())
}

pub(crate) fn validate_local_images(images: &[RuntimeLocalImage]) -> Result<()> {
    const MAX_IMAGES: usize = 8;
    if images.len() > MAX_IMAGES {
        return Err(AppError::InvalidArg(format!(
            "一次最多添加 {MAX_IMAGES} 张图片"
        )));
    }
    for image in images {
        let path = image.path.trim();
        if path.is_empty() {
            return Err(AppError::InvalidArg("图片路径不能为空".into()));
        }
        let lower = path.to_ascii_lowercase();
        let ok_ext = [".png", ".jpg", ".jpeg", ".gif", ".webp", ".bmp"].iter().any(|ext| lower.ends_with(ext));
        if !ok_ext {
            return Err(AppError::InvalidArg(format!(
                "不支持的图片类型: {path}"
            )));
        }
        let meta = std::fs::metadata(path).map_err(|err| {
            AppError::InvalidArg(format!("无法读取图片: {path} ({err})"))
        })?;
        if !meta.is_file() {
            return Err(AppError::InvalidArg(format!("不是图片文件: {path}")));
        }
        const MAX_BYTES: u64 = 10 * 1024 * 1024;
        if meta.len() > MAX_BYTES {
            return Err(AppError::InvalidArg(format!(
                "图片过大（最大 10MB）: {path}"
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn rejects_unknown_model_when_catalog_present() {
        let catalog = vec![RuntimeModelOption {
            id: "gpt-test".into(),
            efforts: vec!["low".into(), "high".into()],
            default_effort: Some("low".into()),
        }];
        let err = validate_turn_settings(
            &RuntimeTurnSettings {
                model: Some("nope".into()),
                effort: None,
            },
            &catalog,
            &RuntimeTurnSettings::default(),
        )
        .unwrap_err();
        assert!(err.to_string().contains("模型不可用"));
    }

    #[test]
    fn rejects_effort_not_in_model_list() {
        let catalog = vec![RuntimeModelOption {
            id: "gpt-test".into(),
            efforts: vec!["low".into()],
            default_effort: Some("low".into()),
        }];
        let err = validate_turn_settings(
            &RuntimeTurnSettings {
                model: Some("gpt-test".into()),
                effort: Some("ultra".into()),
            },
            &catalog,
            &RuntimeTurnSettings::default(),
        )
        .unwrap_err();
        assert!(err.to_string().contains("不支持思考强度"));
    }

    #[test]
    fn fills_default_effort_when_omitted() {
        let catalog = vec![RuntimeModelOption {
            id: "gpt-test".into(),
            efforts: vec!["low".into(), "high".into()],
            default_effort: Some("high".into()),
        }];
        let ok = validate_turn_settings(
            &RuntimeTurnSettings {
                model: Some("gpt-test".into()),
                effort: None,
            },
            &catalog,
            &RuntimeTurnSettings::default(),
        )
        .unwrap();
        assert_eq!(ok.effort.as_deref(), Some("high"));
    }

    #[test]
    fn fills_first_effort_when_default_missing_or_invalid() {
        let catalog = vec![RuntimeModelOption {
            id: "spark".into(),
            efforts: vec!["low".into(), "high".into()],
            default_effort: Some("medium".into()),
        }];
        let ok = validate_turn_settings(
            &RuntimeTurnSettings {
                model: Some("spark".into()),
                effort: None,
            },
            &catalog,
            &RuntimeTurnSettings::default(),
        )
        .unwrap();
        assert_eq!(ok.effort.as_deref(), Some("low"));
        assert_eq!(
            resolved_default_effort(&catalog[0]).as_deref(),
            Some("low")
        );
    }

    #[test]
    fn reconcile_resets_unsupported_effort_for_model() {
        let catalog = vec![RuntimeModelOption {
            id: "gpt-5.3-codex-spark".into(),
            efforts: vec!["low".into(), "high".into()],
            default_effort: Some("low".into()),
        }];
        let repaired = reconcile_turn_settings(
            &RuntimeTurnSettings {
                model: Some("gpt-5.3-codex-spark".into()),
                effort: Some("medium".into()),
            },
            &catalog,
        )
        .expect("should repair");
        assert_eq!(repaired.model.as_deref(), Some("gpt-5.3-codex-spark"));
        assert_eq!(repaired.effort.as_deref(), Some("low"));
        assert!(reconcile_turn_settings(&repaired, &catalog).is_none());
    }

    #[test]
    fn assert_settings_supported_rejects_bad_pair() {
        let catalog = vec![RuntimeModelOption {
            id: "gpt-5.3-codex-spark".into(),
            efforts: vec!["low".into()],
            default_effort: Some("low".into()),
        }];
        let err = assert_settings_supported(
            &RuntimeTurnSettings {
                model: Some("gpt-5.3-codex-spark".into()),
                effort: Some("medium".into()),
            },
            &catalog,
        )
        .unwrap_err();
        assert!(err.to_string().contains("不支持思考强度"));
        assert!(assert_settings_supported(
            &RuntimeTurnSettings {
                model: Some("gpt-5.3-codex-spark".into()),
                effort: None,
            },
            &catalog,
        )
        .is_ok());
    }

    #[test]
    fn parse_model_list_drops_unsupported_default_effort() {
        let value = json!({
            "data": [{
                "id": "spark",
                "supportedReasoningEfforts": ["low", "high"],
                "defaultReasoningEffort": "medium"
            }]
        });
        let models = parse_model_list(&value);
        assert_eq!(models[0].efforts, vec!["low", "high"]);
        assert_eq!(models[0].default_effort.as_deref(), Some("low"));
    }

    #[test]
    fn parses_model_list_efforts() {
        let value = json!({
            "data": [{
                "id": "m1",
                "supportedReasoningEfforts": [{"reasoningEffort": "low"}, "high"],
                "defaultReasoningEffort": "low"
            }]
        });
        let models = parse_model_list(&value);
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].efforts, vec!["low", "high"]);
        assert_eq!(models[0].default_effort.as_deref(), Some("low"));
    }

    #[test]
    fn parse_model_list_matches_live_codex_spark_object_shape() {
        // Codex 0.150+/0.153+ app-server list: objects with reasoningEffort + description.
        // Live catalog over-reports medium/xhigh for spark even though turn/start may reject some.
        let value = json!({
            "data": [{
                "id": "gpt-5.3-codex-spark",
                "displayName": "GPT-5.3-Codex-Spark",
                "supportedReasoningEfforts": [
                    {"reasoningEffort": "low", "description": "Fast responses with lighter reasoning"},
                    {"reasoningEffort": "medium", "description": "Balances speed and reasoning depth for everyday tasks"},
                    {"reasoningEffort": "high", "description": "Greater reasoning depth for complex problems"},
                    {"reasoningEffort": "xhigh", "description": "Extra high reasoning depth for complex problems"},
                    {"effort": "medium", "available": false, "description": "duplicate marked unavailable"}
                ],
                "defaultReasoningEffort": "high"
            }]
        });
        let models = parse_model_list(&value);
        assert_eq!(models.len(), 1);
        assert_eq!(
            models[0].efforts,
            vec!["low", "medium", "high", "xhigh"]
        );
        assert_eq!(models[0].default_effort.as_deref(), Some("high"));
    }

    #[test]
    fn apply_denied_efforts_filters_over_reported_catalog() {
        let models = vec![RuntimeModelOption {
            id: "gpt-5.3-codex-spark".into(),
            efforts: vec![
                "low".into(),
                "medium".into(),
                "high".into(),
                "xhigh".into(),
            ],
            default_effort: Some("high".into()),
        }];
        let mut denied = std::collections::HashMap::new();
        denied.insert(
            "gpt-5.3-codex-spark".into(),
            ["medium".into()].into_iter().collect(),
        );
        let filtered = apply_denied_efforts(&models, &denied);
        assert_eq!(filtered[0].efforts, vec!["low", "high", "xhigh"]);
        assert_eq!(filtered[0].default_effort.as_deref(), Some("high"));

        let repaired = reconcile_turn_settings(
            &RuntimeTurnSettings {
                model: Some("gpt-5.3-codex-spark".into()),
                effort: Some("medium".into()),
            },
            &filtered,
        )
        .expect("should repair after deny");
        assert_eq!(repaired.effort.as_deref(), Some("high"));
    }

    #[test]
    fn looks_like_thinking_unsupported_matches_localized_and_upstream() {
        assert!(looks_like_thinking_unsupported(
            "OpenAI API error: does not support parameter reasoningEffort"
        ));
        assert!(looks_like_thinking_unsupported("这个模型不支持当前思考设置。请点重试。"));
        assert!(!looks_like_thinking_unsupported("network timeout"));
    }

    #[test]
    fn build_input_uses_local_image_not_path_text() {
        let input = build_turn_input(
            "see this",
            &[RuntimeLocalImage { path: "/tmp/a.png".into() }],
            &[],
        )
        .unwrap();
        assert_eq!(input[0]["type"], "text");
        assert_eq!(input[1]["type"], "localImage");
        assert_eq!(input[1]["path"], "/tmp/a.png");
    }

    #[test]
    fn rejects_unknown_skill_when_catalog_has_skills() {
        let catalog = vec![RuntimeExtensionItem {
            id: "/skills/demo/SKILL.md".into(),
            name: "demo".into(),
            kind: RuntimeExtensionKind::Skill,
            installed: true,
            enabled: true,
            loaded: false,
            callable: true,
            path: Some("/skills/demo/SKILL.md".into()),
        }];
        let err = validate_skill_refs(
            &[RuntimeSkillRef {
                name: "demo".into(),
                path: "/skills/other/SKILL.md".into(),
            }],
            &catalog,
        )
        .unwrap_err();
        assert!(err.to_string().contains("不可用于本轮"));
    }

    #[test]
    fn may_fetch_catalog_only_when_idle() {
        use super::super::types::RuntimePhase;
        assert!(may_fetch_catalog(None));
        assert!(may_fetch_catalog(Some(RuntimePhase::Idle)));
        assert!(may_fetch_catalog(Some(RuntimePhase::Completed)));
        assert!(!may_fetch_catalog(Some(RuntimePhase::Starting)));
        assert!(!may_fetch_catalog(Some(RuntimePhase::Running)));
        assert!(!may_fetch_catalog(Some(RuntimePhase::Waiting)));
        assert!(!may_fetch_catalog(Some(RuntimePhase::Cancelling)));
    }
}
