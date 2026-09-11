//! Shared ACP `session/update` decoding (Grok JSON-RPC and Kiro stream-json).

use serde_json::Value;

use crate::models::ProcessStep;

/// Decode one ACP session-update payload (`params`, `data`, or the update object).
pub(crate) fn decode_session_update(v: &Value) -> Vec<ProcessStep> {
    let update = v.get("update").or_else(|| v.get("data")).unwrap_or(v);
    let uty = update
        .get("sessionUpdate")
        .or_else(|| update.get("session_update"))
        .or_else(|| update.get("type"))
        .and_then(|t| t.as_str())
        .unwrap_or("");

    match uty {
        "agent_message_chunk" | "message" | "agent_message" => {
            let text = acp_content_text(update);
            if text.is_empty() {
                vec![]
            } else {
                vec![ProcessStep::Text { text }]
            }
        }
        "agent_thought_chunk" | "thought" => {
            let text = acp_content_text(update);
            if text.is_empty() {
                vec![]
            } else {
                vec![ProcessStep::Thinking { text, done: false }]
            }
        }
        "tool_call" | "tool_call_update" => {
            let is_update = uty.ends_with("update");
            let raw_status = update.get("status").and_then(|s| s.as_str()).unwrap_or("");
            vec![ProcessStep::Tool {
                id: first_str(update, &["toolCallId", "tool_call_id", "id"]),
                name: acp_tool_name(update),
                input: update
                    .get("rawInput")
                    .or_else(|| update.get("raw_input"))
                    .or_else(|| update.get("input"))
                    .cloned(),
                status: map_tool_status(raw_status, is_update),
                result: extract_tool_result(update),
            }]
        }
        "retry_state" => {
            let attempt = update.get("attempt").and_then(|n| n.as_u64()).unwrap_or(0);
            let max = update
                .get("max_retries")
                .or_else(|| update.get("maxRetries"))
                .and_then(|n| n.as_u64())
                .unwrap_or(0);
            let reason = update
                .get("reason")
                .and_then(|s| s.as_str())
                .unwrap_or("")
                .trim();
            let detail = if reason.is_empty() {
                format!("retry {attempt}/{max}")
            } else {
                format!("retry {attempt}/{max}: {reason}")
            };
            vec![ProcessStep::Status {
                phase: "running".into(),
                detail: Some(detail),
            }]
        }
        // Live chrome on the snapshot — never a process-timeline row.
        "plan" => vec![],
        // Catalog only — never a process-timeline row.
        "available_commands" | "available_commands_update" => vec![],
        "usage" | "token_usage" | "tokenUsage" | "tokens_used" | "turn_completed"
        | "turn_usage" | "response_completed" => usage_steps(update),
        "context_usage" => context_usage_steps(update),
        "auto_compact_started"
        | "auto_compact_completed"
        | "auto_compact"
        | "context_compact"
        | "compaction"
        | "config_option_update" => vec![], // Catalog only — never a process-timeline row.
        "error" => {
            let message = update
                .get("message")
                .or_else(|| update.get("error"))
                .and_then(|s| s.as_str())
                .unwrap_or("error")
                .to_string();
            vec![ProcessStep::Error { message }]
        }
        // Recognized envelope, unknown kind — do not fall back to raw JSON.
        _ => vec![],
    }
}

/// Agent-declared slash command. Not a `ProcessStep` (must not enter the timeline).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AcpAvailableCommand {
    pub name: String,
    pub description: String,
    pub hint: Option<String>,
}

/// Returns `Some` when this payload is an available-commands update, even if the list is empty.
pub(crate) fn extract_available_commands(v: &Value) -> Option<Vec<AcpAvailableCommand>> {
    let update = v.get("update").or_else(|| v.get("data")).unwrap_or(v);
    let uty = update
        .get("sessionUpdate")
        .or_else(|| update.get("session_update"))
        .or_else(|| update.get("type"))
        .and_then(|t| t.as_str())
        .unwrap_or("");
    if uty != "available_commands" && uty != "available_commands_update" {
        return None;
    }
    let list = update
        .get("availableCommands")
        .or_else(|| update.get("available_commands"))
        .and_then(|value| value.as_array());
    Some(
        list.map(|items| items.iter().filter_map(parse_available_command).collect())
            .unwrap_or_default(),
    )
}

/// Model/effort lists from ACP config options. Not a `ProcessStep`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct AcpConfigCatalog {
    pub models: Vec<String>,
    pub current_model: Option<String>,
    pub efforts: Vec<String>,
    pub current_effort: Option<String>,
}

/// `Some` for `config_option_update` (even if empty) or a payload that already has `configOptions`.
pub(crate) fn extract_config_catalog(v: &Value) -> Option<AcpConfigCatalog> {
    let update = v.get("update").or_else(|| v.get("data")).unwrap_or(v);
    let uty = update
        .get("sessionUpdate")
        .or_else(|| update.get("session_update"))
        .or_else(|| update.get("type"))
        .and_then(|t| t.as_str())
        .unwrap_or("");
    let list = update
        .get("configOptions")
        .or_else(|| update.get("config_options"))
        .or_else(|| v.get("configOptions"))
        .or_else(|| v.get("config_options"))
        .and_then(|value| value.as_array());
    let is_update = uty == "config_option_update";
    if !is_update && list.is_none() {
        return None;
    }
    Some(config_catalog_from_options(list.map(|items| items.as_slice()).unwrap_or(&[])))
}

fn config_catalog_from_options(items: &[Value]) -> AcpConfigCatalog {
    let mut catalog = AcpConfigCatalog::default();
    for item in items {
        let id = first_str(item, &["id", "category"]).unwrap_or_default();
        let category = first_str(item, &["category"]).unwrap_or_default();
        let kind = compact_tool_token(&id);
        let category_kind = compact_tool_token(&category);
        let values = select_option_values(item);
        let current = config_current_value(item);
        if is_model_config(&kind, &category_kind) {
            if !values.is_empty() {
                catalog.models = values;
            }
            if current.is_some() {
                catalog.current_model = current;
            }
        } else if is_effort_config(&kind, &category_kind) {
            if !values.is_empty() {
                catalog.efforts = values;
            }
            if current.is_some() {
                catalog.current_effort = current;
            }
        }
    }
    catalog
}

fn is_model_config(id: &str, category: &str) -> bool {
    matches!(id, "model" | "models" | "modelid") || matches!(category, "model" | "models")
}

fn is_effort_config(id: &str, category: &str) -> bool {
    matches!(
        id,
        "effort" | "thinking" | "reasoning" | "reasoningeffort" | "think"
    ) || matches!(category, "effort" | "thinking" | "reasoning")
}

fn select_option_values(item: &Value) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    item.get("options")
        .and_then(|value| value.as_array())
        .into_iter()
        .flatten()
        .filter_map(|row| {
            first_str(row, &["value", "id", "name"]).filter(|s| seen.insert(s.clone()))
        })
        .collect()
}

fn config_current_value(item: &Value) -> Option<String> {
    first_str(item, &["currentValue", "current_value", "selectedValue", "selected_value"])
}

/// One ACP plan row. Not a `ProcessStep`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AcpPlanEntry {
    pub content: String,
    pub status: Option<String>,
    pub priority: Option<String>,
}

/// `Some` for `sessionUpdate: plan`, even when the entry list is empty.
pub(crate) fn extract_plan(v: &Value) -> Option<Vec<AcpPlanEntry>> {
    let update = v.get("update").or_else(|| v.get("data")).unwrap_or(v);
    let uty = update
        .get("sessionUpdate")
        .or_else(|| update.get("session_update"))
        .or_else(|| update.get("type"))
        .and_then(|t| t.as_str())
        .unwrap_or("");
    if uty != "plan" {
        return None;
    }
    if let Some(items) = update
        .get("entries")
        .or_else(|| update.get("plan"))
        .and_then(|value| value.as_array())
    {
        return Some(items.iter().filter_map(parse_plan_entry).collect());
    }
    let body = first_str(update, &["planContent", "plan_content"])
        .or_else(|| {
            update
                .get("content")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
        });
    Some(body.into_iter().map(|content| AcpPlanEntry {
        content,
        status: None,
        priority: None,
    }).collect())
}

fn parse_plan_entry(value: &Value) -> Option<AcpPlanEntry> {
    let content = first_str(value, &["content", "text", "title"])?;
    Some(AcpPlanEntry {
        content,
        status: first_str(value, &["status"]),
        priority: first_str(value, &["priority"]),
    })
}

fn parse_available_command(value: &Value) -> Option<AcpAvailableCommand> {
    let name = first_str(value, &["name", "command"])?;
    let description = first_str(value, &["description"]).unwrap_or_default();
    let hint = value
        .pointer("/input/hint")
        .and_then(|h| h.as_str())
        .or_else(|| value.get("hint").and_then(|h| h.as_str()))
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    Some(AcpAvailableCommand {
        name,
        description,
        hint,
    })
}

fn acp_tool_name(update: &Value) -> String {
    let title = first_str(update, &["title", "name"]);
    let kind = first_str(update, &["kind"]);
    if let Some(mapped) = kind.as_deref().and_then(map_acp_tool_kind) {
        if title
            .as_deref()
            .is_some_and(|name| map_acp_tool_kind(name).is_some())
        {
            return title.unwrap();
        }
        return mapped.to_string();
    }
    title.or(kind).unwrap_or_else(|| "tool".into())
}

fn map_acp_tool_kind(kind: &str) -> Option<&'static str> {
    match compact_tool_token(kind).as_str() {
        "read" | "search" | "fetch" | "grep" | "view" => Some("read"),
        "edit" | "write" | "delete" | "move" | "patch" | "create" => Some("edit"),
        "execute" | "exec" | "command" | "terminal" | "bash" | "shell" => Some("execute"),
        _ => None,
    }
}

fn compact_tool_token(value: &str) -> String {
    value
        .trim()
        .to_ascii_lowercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect()
}

pub(crate) fn acp_content_text(v: &Value) -> String {
    if let Some(t) = v
        .pointer("/content/text")
        .or_else(|| v.get("text"))
        .or_else(|| v.get("delta"))
        .or_else(|| v.pointer("/content/delta"))
        .and_then(|t| t.as_str())
    {
        if !t.is_empty() {
            return t.to_string();
        }
    }
    collect_text(v.get("content").unwrap_or(&Value::Null), 0)
}

pub(crate) fn extract_tool_result(v: &Value) -> Option<String> {
    if let Some(s) = v
        .get("data")
        .or_else(|| v.get("result"))
        .or_else(|| v.get("output"))
        .and_then(|o| match o {
            Value::String(s) => Some(s.clone()),
            other if !other.is_null() => Some(other.to_string()),
            _ => None,
        })
    {
        return Some(truncate(&s, 800));
    }
    let from_content = collect_text(v.get("content").unwrap_or(&Value::Null), 0);
    if from_content.is_empty() {
        None
    } else {
        Some(truncate(&from_content, 800))
    }
}

pub(crate) fn collect_text(value: &Value, depth: usize) -> String {
    if depth > 4 {
        return String::new();
    }
    match value {
        Value::String(s) => s.clone(),
        Value::Array(items) => {
            let mut out = String::new();
            for item in items {
                let piece = collect_text(item, depth + 1);
                if !piece.is_empty() {
                    out.push_str(&piece);
                }
            }
            out
        }
        Value::Object(map) => {
            if let Some(t) = map.get("text").and_then(|t| t.as_str()) {
                return t.to_string();
            }
            if let Some(c) = map.get("content") {
                return collect_text(c, depth + 1);
            }
            String::new()
        }
        _ => String::new(),
    }
}

fn usage_steps(update: &Value) -> Vec<ProcessStep> {
    let usage = update
        .get("usage")
        .or_else(|| update.get("tokenUsage"))
        .or_else(|| update.get("token_usage"))
        .filter(|u| u.is_object())
        .unwrap_or(update);
    ProcessStep::from_usage_object(usage)
        .map(|step| vec![step])
        .unwrap_or_default()
}

/// Window used/size from `context_usage`. Skips all-zero payloads (no fake 0).
fn context_usage_steps(update: &Value) -> Vec<ProcessStep> {
    let usage = update
        .get("usage")
        .filter(|value| value.is_object())
        .unwrap_or(update);
    let used = first_u64(
        usage,
        &[
            "used",
            "usedTokens",
            "used_tokens",
            "tokenCount",
            "token_count",
            "current",
            "totalTokens",
            "total_tokens",
            "total",
        ],
    );
    let window = first_u64(
        usage,
        &[
            "size",
            "maxTokens",
            "max_tokens",
            "contextWindow",
            "context_window",
            "window",
            "limit",
        ],
    );
    if used.unwrap_or(0) == 0 && window.unwrap_or(0) == 0 {
        return vec![];
    }
    vec![ProcessStep::Usage {
        scope: Some("context".into()),
        input: None,
        output: None,
        cache_read: None,
        cache_write: None,
        reasoning: None,
        total: used,
        context_window: window,
    }]
}

pub(crate) fn first_str(v: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|k| v.get(*k).and_then(|x| x.as_str()))
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn first_u64(v: &Value, keys: &[&str]) -> Option<u64> {
    for key in keys {
        let Some(value) = v.get(*key) else {
            continue;
        };
        if let Some(n) = value.as_u64() {
            return Some(n);
        }
        if let Some(n) = value.as_i64().and_then(|n| u64::try_from(n).ok()) {
            return Some(n);
        }
        if let Some(n) = value.as_f64().and_then(|n| (n >= 0.0).then_some(n as u64)) {
            return Some(n);
        }
    }
    None
}

fn map_tool_status(raw: &str, is_update: bool) -> String {
    match raw.to_ascii_lowercase().as_str() {
        "completed" | "complete" | "success" | "ok" | "done" => "end".into(),
        "failed" | "error" | "rejected" | "denied" | "cancelled" | "canceled" => "end".into(),
        "pending" | "in_progress" | "running" | "start" => {
            if is_update {
                "update".into()
            } else {
                "start".into()
            }
        }
        "" => {
            if is_update {
                "update".into()
            } else {
                "start".into()
            }
        }
        other => other.to_string(),
    }
}

fn truncate(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let head: String = s.chars().take(max_chars).collect();
        format!("{head}…")
    }
}
