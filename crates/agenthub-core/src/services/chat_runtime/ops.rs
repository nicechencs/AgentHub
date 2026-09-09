//! B2 helpers: model/effort validation, Codex list parsing, turn input building.

use serde_json::{json, Value};

use crate::error::{AppError, Result};
use crate::models::AgentId;

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

/// Idle options should fill a catalog default when the stored model is missing
/// or is not in this login's list. Empty catalogs must not wipe a stored model.
pub(crate) fn settings_need_catalog_default(
    settings: &RuntimeTurnSettings,
    catalog: &[RuntimeModelOption],
) -> bool {
    if catalog.is_empty() {
        return false;
    }
    match trim_setting(&settings.model) {
        None => true,
        Some(id) => !catalog.iter().any(|item| item.id == id),
    }
}

/// When the user has not picked a model, use the first catalog row instead of
/// inheriting a Codex config.toml default that may not work with this login.
pub(crate) fn default_turn_settings(catalog: &[RuntimeModelOption]) -> Option<RuntimeTurnSettings> {
    let option = catalog.first()?;
    Some(RuntimeTurnSettings {
        model: Some(option.id.clone()),
        effort: resolved_default_effort(option),
    })
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
        return Err(AppError::InvalidArg("选择思考强度前需要先选择模型".into()));
    }
    let Some(model_id) = model else {
        return Ok(());
    };
    let option = catalog
        .iter()
        .find(|item| item.id == model_id)
        .ok_or_else(|| AppError::InvalidArg(format!("模型不可用: {model_id}")))?;
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
        return Err(AppError::InvalidArg("选择思考强度前需要先选择模型".into()));
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

    let option = catalog
        .iter()
        .find(|item| item.id == model_id)
        .ok_or_else(|| AppError::InvalidArg(format!("模型不可用: {model_id}")))?;

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
        .or_else(|| value.get("models"))
        .and_then(|v| v.as_array())
        .cloned()
        .or_else(|| value.as_array().cloned())
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

pub(crate) fn parse_grok_model_list(value: &Value) -> Vec<RuntimeModelOption> {
    let body = value.get("result").unwrap_or(value);
    let rows = body
        .get("availableModels")
        .or_else(|| body.get("models"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let mut out = Vec::new();
    for row in rows {
        let id = row
            .get("modelId")
            .or_else(|| row.get("id"))
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string);
        let Some(id) = id else { continue };
        let meta = row.get("_meta").unwrap_or(&row);
        let mut seen = std::collections::HashSet::new();
        let efforts = meta
            .get("reasoningEfforts")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|item| {
                        item.get("value")
                            .or_else(|| item.get("id"))
                            .and_then(|v| v.as_str())
                            .map(str::trim)
                            .filter(|s| !s.is_empty())
                            .map(str::to_string)
                    })
                    .filter(|s| seen.insert(s.clone()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let default_effort = meta
            .get("reasoningEfforts")
            .and_then(|v| v.as_array())
            .and_then(|arr| {
                arr.iter().find_map(|item| {
                    if item.get("default").and_then(|v| v.as_bool()) != Some(true) {
                        return None;
                    }
                    item.get("value")
                        .or_else(|| item.get("id"))
                        .and_then(|v| v.as_str())
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(str::to_string)
                })
            })
            .or_else(|| {
                meta.get("reasoningEffort")
                    .and_then(|v| v.as_str())
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
            });
        let option = RuntimeModelOption {
            id,
            efforts,
            default_effort,
        };
        let default_effort = resolved_default_effort(&option);
        out.push(RuntimeModelOption {
            id: option.id,
            efforts: option.efforts,
            default_effort,
        });
    }
    out
}

pub(crate) fn grok_model_rejects_thinking(model: &str) -> bool {
    let id = model.trim().to_ascii_lowercase();
    id == "grok-code-fast-1" || id.contains("grok-code-fast")
}

pub(crate) fn ensure_grok_catalog_efforts(
    models: Vec<RuntimeModelOption>,
) -> Vec<RuntimeModelOption> {
    const DEFAULT_EFFORTS: [&str; 3] = ["low", "high", "xhigh"];
    models
        .into_iter()
        .map(|mut option| {
            if grok_model_rejects_thinking(&option.id) {
                option.efforts.clear();
                option.default_effort = None;
                return option;
            }
            if option.efforts.is_empty() {
                option.efforts = DEFAULT_EFFORTS
                    .iter()
                    .map(|item| (*item).to_string())
                    .collect();
            }
            let default_ok = option
                .default_effort
                .as_deref()
                .is_some_and(|value| option.efforts.iter().any(|item| item == value));
            if !default_ok {
                option.default_effort = option
                    .efforts
                    .iter()
                    .find(|item| *item == "high")
                    .cloned()
                    .or_else(|| option.efforts.first().cloned());
            }
            option
        })
        .collect()
}

pub(crate) fn acp_session_prompt_params(session_id: &str, blocks: Vec<Value>) -> Value {
    json!({
        "sessionId": session_id,
        "prompt": blocks,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AcpSessionPlan {
    /// Same ACP process is still up: send another `session/prompt`.
    PromptExisting,
    /// Start `session/new` (and spawn if needed).
    New,
    /// Fresh process, try `session/load` then prompt.
    LoadThenPrompt,
    /// Kiro sessions cannot be reattached after the ACP process exits. Keep
    /// the durable id and ask the user to start a new conversation.
    Unavailable,
}

/// Kiro ACP `session/load` after the previous process exited hangs or kills the
/// new process. Reuse the live process; if it is gone, keep the session id and
/// ask the user to start a new conversation.
pub(crate) fn acp_session_plan(
    agent: AgentId,
    live_transport: bool,
    has_session_id: bool,
) -> AcpSessionPlan {
    if live_transport && has_session_id {
        return AcpSessionPlan::PromptExisting;
    }
    if has_session_id && agent == AgentId::Kiro {
        return AcpSessionPlan::Unavailable;
    }
    if has_session_id && agent != AgentId::Kiro {
        return AcpSessionPlan::LoadThenPrompt;
    }
    AcpSessionPlan::New
}

pub(crate) fn grok_prompt_blocks(prompt: &str, images: &[RuntimeLocalImage]) -> Result<Vec<Value>> {
    let mut blocks = vec![serde_json::json!({ "type": "text", "text": prompt })];
    for image in images {
        blocks.push(grok_image_block(&image.path)?);
    }
    Ok(blocks)
}

fn grok_image_mime(path: &str) -> Option<&'static str> {
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".png") {
        Some("image/png")
    } else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        Some("image/jpeg")
    } else if lower.ends_with(".gif") {
        Some("image/gif")
    } else if lower.ends_with(".webp") {
        Some("image/webp")
    } else if lower.ends_with(".bmp") {
        Some("image/bmp")
    } else {
        None
    }
}

fn grok_image_block(path: &str) -> Result<Value> {
    let path = path.trim();
    if path.is_empty() {
        return Err(AppError::InvalidArg("图片路径不能为空".into()));
    }
    let mime = grok_image_mime(path)
        .ok_or_else(|| AppError::InvalidArg(format!("不支持的图片类型: {path}")))?;
    let bytes = std::fs::read(path)
        .map_err(|err| AppError::InvalidArg(format!("无法读取图片: {path} ({err})")))?;
    use base64::Engine;
    Ok(serde_json::json!({
        "type": "image",
        "mimeType": mime,
        "data": base64::engine::general_purpose::STANDARD.encode(bytes),
    }))
}

/// Claude stream-json user message (text + optional base64 images).
pub(crate) fn claude_user_message(prompt: &str, images: &[RuntimeLocalImage]) -> Result<Value> {
    let mut content = vec![serde_json::json!({ "type": "text", "text": prompt })];
    for image in images {
        content.push(claude_image_block(&image.path)?);
    }
    Ok(serde_json::json!({
        "type": "user",
        "message": {
            "role": "user",
            "content": content
        }
    }))
}

fn claude_image_block(path: &str) -> Result<Value> {
    let path = path.trim();
    if path.is_empty() {
        return Err(AppError::InvalidArg("图片路径不能为空".into()));
    }
    let mime = grok_image_mime(path)
        .ok_or_else(|| AppError::InvalidArg(format!("不支持的图片类型: {path}")))?;
    let bytes = std::fs::read(path)
        .map_err(|err| AppError::InvalidArg(format!("无法读取图片: {path} ({err})")))?;
    use base64::Engine;
    Ok(serde_json::json!({
        "type": "image",
        "source": {
            "type": "base64",
            "media_type": mime,
            "data": base64::engine::general_purpose::STANDARD.encode(bytes),
        }
    }))
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
            let Some(name) = name.or_else(|| Some(id.clone())) else {
                continue;
            };
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
            if catalog
                .iter()
                .any(|item| item.kind == RuntimeExtensionKind::Skill)
            {
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
        let ok_ext = [".png", ".jpg", ".jpeg", ".gif", ".webp", ".bmp"]
            .iter()
            .any(|ext| lower.ends_with(ext));
        if !ok_ext {
            return Err(AppError::InvalidArg(format!("不支持的图片类型: {path}")));
        }
        let meta = std::fs::metadata(path)
            .map_err(|err| AppError::InvalidArg(format!("无法读取图片: {path} ({err})")))?;
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
mod tests;
