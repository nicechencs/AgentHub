//! Official Codex / Responses vendor policy.
//!
//! System and developer text fold into `instructions` and must not appear as
//! input items on the official ChatGPT Responses upstream.

use serde_json::{json, Map, Value};

use crate::bridge::types::{BridgeContent, BridgeMessage, BridgeRequest, MessageRole, ToolChoice};

/// Render a neutral [`BridgeRequest`] as an OpenAI Responses request body.
///
/// Used by the Codex subscription → Claude Code kernel when the approved upstream
/// transport is Responses. Unlike [`to_kimi_chat_request`], this keeps Responses
/// shapes (`input` items, `max_output_tokens`, function tools without Chat wrapping).
///
/// Official ChatGPT / Codex Responses rejects `role=system` input items (400
/// "System messages are not allowed"). Fold system and developer text into
/// `instructions` instead of emitting those roles.
pub fn to_responses_request(request: &BridgeRequest) -> Value {
    let mut body = Map::new();
    body.insert("model".to_owned(), Value::String(request.model.clone()));
    body.insert("stream".to_owned(), Value::Bool(request.stream));

    let mut instructions = request.instructions.clone();
    let mut input = Vec::new();
    for message in &request.input {
        match message.role {
            MessageRole::System | MessageRole::Developer => {
                fold_text_into_instructions(&mut instructions, &bridge_message_text(message));
            }
            _ => append_responses_input(&mut input, message),
        }
    }
    if let Some(instructions) = instructions {
        body.insert("instructions".to_owned(), Value::String(instructions));
    }
    body.insert("input".to_owned(), Value::Array(input));

    if !request.tools.is_empty() {
        body.insert(
            "tools".to_owned(),
            Value::Array(
                request
                    .tools
                    .iter()
                    .map(|tool| {
                        let mut object = Map::new();
                        object.insert("type".to_owned(), Value::String("function".to_owned()));
                        object.insert("name".to_owned(), Value::String(tool.name.clone()));
                        object.insert("parameters".to_owned(), tool.parameters.clone());
                        if let Some(description) = &tool.description {
                            object.insert(
                                "description".to_owned(),
                                Value::String(description.clone()),
                            );
                        }
                        if let Some(strict) = tool.strict {
                            object.insert("strict".to_owned(), Value::Bool(strict));
                        }
                        Value::Object(object)
                    })
                    .collect(),
            ),
        );
    }

    if let Some(tool_choice) = &request.tool_choice {
        body.insert(
            "tool_choice".to_owned(),
            render_responses_tool_choice(tool_choice),
        );
    }

    for key in [
        "temperature",
        "top_p",
        "presence_penalty",
        "frequency_penalty",
        "seed",
        "max_output_tokens",
        "metadata",
    ] {
        if let Some(value) = request.passthrough.get(key) {
            body.insert(key.to_owned(), value.clone());
        }
    }

    Value::Object(body)
}

/// Official ChatGPT / Codex Responses rejects leftover bridge / CN model ids
/// (400). Do not invent a ChatGPT model name to replace them — omit instead.
pub fn is_leftover_bridge_model(model: &str) -> bool {
    let model = model.trim();
    model.starts_with("grok-")
        || model.starts_with("claude-")
        || model.starts_with("kimi-")
        || model.starts_with("deepseek-")
        || (model.starts_with("agenthub_") && model.ends_with("_bridge"))
}

/// Write the Responses `model` for official Codex upstream.
///
/// Configured override wins when it is non-empty and not leftover. Incoming
/// leftovers are dropped rather than rewritten as `gpt-*`.
pub fn apply_official_codex_model(body: &mut Value, incoming: &str, configured: Option<&str>) {
    let configured = configured
        .map(str::trim)
        .filter(|value| !value.is_empty() && !is_leftover_bridge_model(value));
    let incoming = incoming.trim().to_owned();
    let incoming = if incoming.is_empty() || is_leftover_bridge_model(&incoming) {
        None
    } else {
        Some(incoming)
    };
    match (configured, incoming) {
        (Some(model), _) => {
            body["model"] = Value::String(model.to_owned());
        }
        (None, Some(model)) => {
            body["model"] = Value::String(model);
        }
        (None, None) => {
            if let Some(object) = body.as_object_mut() {
                object.remove("model");
            }
        }
    }
}

const OFFICIAL_CODEX_RESPONSE_KEYS: &[&str] = &[
    "model",
    "input",
    "stream",
    "store",
    "instructions",
    "tools",
    "tool_choice",
    // Kept until a live official 400; existing tests still forward them.
    "temperature",
    "top_p",
];

/// Prepare a request for the official ChatGPT / Codex Responses upstream.
///
/// The official endpoint requires storage to be disabled for this local
/// subscription route, **requires `stream: true`**, rejects `role=system`
/// input items, and 400s on unsupported request fields (`metadata`,
/// `max_output_tokens`, and other Chat Completions leftovers). Keep only
/// the allowlisted Responses keys so callers cannot accidentally forward
/// Claude/OpenAI extras while leaving the provider-neutral request
/// conversion unchanged.
///
/// Downstream `stream` stays the client's request. The host consumes the
/// official SSE and aggregates a complete JSON body when the client asked
/// for a non-stream response.
pub fn prepare_official_codex_request(
    body: &mut Value,
    incoming_model: &str,
    configured_model: Option<&str>,
) {
    apply_official_codex_model(body, incoming_model, configured_model);
    body["store"] = Value::Bool(false);
    body["stream"] = Value::Bool(true);
    fold_official_codex_system_items(body);
    if let Some(object) = body.as_object_mut() {
        object.retain(|key, _| OFFICIAL_CODEX_RESPONSE_KEYS.contains(&key.as_str()));
    }
}

/// Drop leftover `role=system` / `role=developer` input items.
///
/// Fold their text into `instructions` when that field is already present.
/// Otherwise prepend onto the first user message so a Claude-style inline
/// system prompt is not discarded. Official ChatGPT Responses 400s on
/// system input items; developer is folded here for the same Claude/Chat
/// conversion path.
pub(crate) fn fold_official_codex_system_items(body: &mut Value) {
    let Some(object) = body.as_object_mut() else {
        return;
    };
    let folded_text = {
        let Some(Value::Array(input)) = object.get_mut("input") else {
            return;
        };
        let mut folded = Vec::new();
        input.retain(|item| match item.get("role").and_then(Value::as_str) {
            Some("system") | Some("developer") => {
                let text = responses_item_text(item);
                if !text.is_empty() {
                    folded.push(text);
                }
                false
            }
            _ => true,
        });
        folded.join("\n")
    };
    if folded_text.is_empty() {
        return;
    }

    let existing = object
        .get("instructions")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    if !existing.is_empty() {
        object.insert(
            "instructions".to_owned(),
            Value::String(merge_instruction_text(&existing, &folded_text)),
        );
        return;
    }
    if let Some(Value::Array(input)) = object.get_mut("input") {
        if prepend_text_to_first_user_item(input, &folded_text) {
            return;
        }
    }
    object.insert("instructions".to_owned(), Value::String(folded_text));
}

fn responses_item_text(item: &Value) -> String {
    match item.get("content") {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(parts)) => parts
            .iter()
            .filter_map(|part| part.get("text").and_then(Value::as_str))
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join(""),
        _ => String::new(),
    }
}

fn prepend_text_to_first_user_item(input: &mut [Value], text: &str) -> bool {
    let Some(item) = input
        .iter_mut()
        .find(|item| item.get("role").and_then(Value::as_str) == Some("user"))
    else {
        return false;
    };
    match item.get_mut("content") {
        Some(Value::String(existing)) => {
            *existing = merge_instruction_text(text, existing);
            true
        }
        Some(Value::Array(parts)) => {
            if let Some(part) = parts
                .iter_mut()
                .find(|part| part.get("text").and_then(Value::as_str).is_some())
            {
                let existing = part
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_owned();
                part["text"] = Value::String(merge_instruction_text(text, &existing));
                return true;
            }
            parts.insert(0, json!({ "type": "input_text", "text": text }));
            true
        }
        _ => {
            item["content"] = json!([{ "type": "input_text", "text": text }]);
            true
        }
    }
}

fn fold_text_into_instructions(instructions: &mut Option<String>, text: &str) {
    if text.is_empty() {
        return;
    }
    match instructions {
        Some(existing) => {
            let merged = merge_instruction_text(existing, text);
            *existing = merged;
        }
        None => *instructions = Some(text.to_owned()),
    }
}

fn merge_instruction_text(first: &str, second: &str) -> String {
    match (first.is_empty(), second.is_empty()) {
        (true, _) => second.to_owned(),
        (_, true) => first.to_owned(),
        _ => format!("{first}\n{second}"),
    }
}

fn bridge_message_text(message: &BridgeMessage) -> String {
    message
        .content
        .iter()
        .filter_map(|content| match content {
            BridgeContent::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

fn append_responses_input(input: &mut Vec<Value>, message: &BridgeMessage) {
    let tool_results: Vec<_> = message
        .content
        .iter()
        .filter_map(|content| match content {
            BridgeContent::ToolResult { call_id, output } => Some((call_id, output)),
            _ => None,
        })
        .collect();
    if !tool_results.is_empty() {
        for (call_id, output) in tool_results {
            input.push(json!({
                "type": "function_call_output",
                "call_id": call_id,
                "output": output,
            }));
        }
        let has_text = message
            .content
            .iter()
            .any(|content| matches!(content, BridgeContent::Text { text } if !text.is_empty()));
        if !has_text {
            return;
        }
        // Mixed text + tool_result: keep function_call_output items, then fall through
        // so the existing role-message logic can emit the text.
    }

    let tool_calls: Vec<_> = message
        .content
        .iter()
        .filter_map(|content| match content {
            BridgeContent::ToolCall {
                id,
                name,
                arguments,
                ..
            } => Some(json!({
                "type": "function_call",
                "call_id": id,
                "name": name,
                "arguments": arguments,
            })),
            _ => None,
        })
        .collect();
    if !tool_calls.is_empty() {
        // Parallel function calls are adjacent Responses items, not one chat message.
        let text = message
            .content
            .iter()
            .filter_map(|content| match content {
                BridgeContent::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<String>();
        if !text.is_empty() {
            input.push(json!({
                "type": "message",
                "role": "assistant",
                "content": [{ "type": "output_text", "text": text }],
            }));
        }
        input.extend(tool_calls);
        return;
    }

    let text = message
        .content
        .iter()
        .filter_map(|content| match content {
            BridgeContent::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<String>();
    let role = match message.role {
        MessageRole::Developer => "developer",
        MessageRole::System => "system",
        MessageRole::User => "user",
        MessageRole::Assistant => "assistant",
        MessageRole::Tool => {
            // Tool-only messages without ToolResult content are invalid IR; drop safely.
            return;
        }
    };
    let content_type = if matches!(message.role, MessageRole::Assistant) {
        "output_text"
    } else {
        "input_text"
    };
    input.push(json!({
        "type": "message",
        "role": role,
        "content": [{ "type": content_type, "text": text }],
    }));
}

fn render_responses_tool_choice(choice: &ToolChoice) -> Value {
    match choice {
        ToolChoice::Auto => Value::String("auto".to_owned()),
        ToolChoice::None => Value::String("none".to_owned()),
        ToolChoice::Required => Value::String("required".to_owned()),
        ToolChoice::Function { name } => json!({
            "type": "function",
            "name": name,
        }),
    }
}
