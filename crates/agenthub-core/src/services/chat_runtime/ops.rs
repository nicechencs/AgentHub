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

/// Validate requested settings against a model/list catalog.
/// Empty catalog: only reject obviously empty model ids; effort may be set with model.
pub(crate) fn validate_turn_settings(
    requested: &RuntimeTurnSettings,
    catalog: &[RuntimeModelOption],
    prior: &RuntimeTurnSettings,
) -> Result<RuntimeTurnSettings> {
    let model = requested
        .model
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let effort = requested
        .effort
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);

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
        None => option.default_effort.clone(),
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
        let efforts = row
            .get("supportedReasoningEfforts")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|item| {
                        if let Some(s) = item.as_str() {
                            return Some(s.trim().to_string());
                        }
                        item.get("reasoningEffort")
                            .or_else(|| item.get("effort"))
                            .and_then(|v| v.as_str())
                            .map(|s| s.trim().to_string())
                    })
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let default_effort = row
            .get("defaultReasoningEffort")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string);
        out.push(RuntimeModelOption {
            id,
            efforts,
            default_effort,
        });
    }
    out
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
