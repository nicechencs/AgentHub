//! External OpenAI Responses request parser.
//!
//! `"developer"` is stored as [`MessageRole::Developer`]. This module does not
//! rewrite that role to system; Kimi and Codex policy do that at render.

use std::collections::HashSet;

use serde_json::{json, Map, Value};

use crate::bridge::types::{
    BridgeContent, BridgeMessage, BridgeRequest, BridgeTool, MessageRole, ProtocolError,
    ProtocolResult, ToolChoice,
};

/// Parse the subset of `POST /v1/responses` that this bridge can faithfully represent.
///
/// Unsupported multimodal inputs fail closed. Non-function tool types are dropped.
pub fn parse_responses_request(value: &Value) -> ProtocolResult<BridgeRequest> {
    let object = value
        .as_object()
        .ok_or_else(|| ProtocolError::invalid_request("The request body must be a JSON object."))?;
    // Responses `reasoning` is not a Chat field; map effort only for xAI.
    let model = required_string(object, "model", "A non-empty model is required.")?;
    let input = match object.get("input") {
        Some(Value::String(text)) => vec![BridgeMessage {
            role: MessageRole::User,
            name: None,
            content: vec![BridgeContent::Text { text: text.clone() }],
        }],
        Some(Value::Array(items)) => parse_input_items(items)?,
        Some(_) => {
            return Err(ProtocolError::invalid_request(
                "`input` must be a string or an array of input messages.",
            ));
        }
        None => return Err(ProtocolError::invalid_request("`input` is required.")),
    };
    let instructions = optional_string(object, "instructions")?;
    let tools = parse_tools(object.get("tools"))?;
    let tool_choice = parse_tool_choice(object.get("tool_choice"))?;
    let stream = match object.get("stream") {
        Some(Value::Bool(stream)) => *stream,
        Some(_) => {
            return Err(ProtocolError::invalid_request(
                "`stream` must be a boolean.",
            ))
        }
        None => false,
    };

    let known = known_request_fields();
    let mut passthrough = object
        .iter()
        .filter(|(key, _)| !known.contains(key.as_str()))
        .map(|(key, item)| (key.clone(), item.clone()))
        .collect::<Map<String, Value>>();
    if let Some(effort) = grok_reasoning_effort(object.get("reasoning")) {
        passthrough.insert("reasoning_effort".to_owned(), Value::String(effort));
    }

    Ok(BridgeRequest {
        model,
        instructions,
        input,
        tools,
        tool_choice,
        stream,
        passthrough,
    })
}

fn parse_input_items(items: &[Value]) -> ProtocolResult<Vec<BridgeMessage>> {
    let mut messages = Vec::with_capacity(items.len());
    let mut pending_function_calls = Vec::new();
    for item in items {
        let object = item.as_object().ok_or_else(|| {
            ProtocolError::invalid_request("Every item in `input` must be an object.")
        })?;
        match object.get("type").and_then(Value::as_str) {
            Some("function_call") => {
                // A Responses turn with parallel functions is represented as several
                // adjacent output items. Chat Completions requires those calls to be
                // carried by *one* assistant message before the individual tool
                // result messages. Keep the turn boundary at the next non-call item.
                pending_function_calls.push(parse_function_call_content(object)?);
            }
            Some("message") | None => {
                flush_function_calls(&mut messages, &mut pending_function_calls);
                messages.push(parse_message_item(object)?);
            }
            Some("function_call_output") => {
                flush_function_calls(&mut messages, &mut pending_function_calls);
                messages.push(parse_function_call_output_item(object)?);
            }
            Some(kind) if is_image_input(kind) => {
                flush_function_calls(&mut messages, &mut pending_function_calls);
                return Err(ProtocolError::unsupported(
                    "unsupported_image_input",
                    "Image input is not supported by this bridge.",
                ));
            }
            Some("item_reference") => {
                flush_function_calls(&mut messages, &mut pending_function_calls);
                return Err(ProtocolError::unsupported(
                    "unsupported_input",
                    "Referenced response items are not supported by this bridge.",
                ));
            }
            Some(_) => {
                flush_function_calls(&mut messages, &mut pending_function_calls);
                return Err(ProtocolError::unsupported(
                    "unsupported_input",
                    "This input item type is not supported by this bridge.",
                ));
            }
        }
    }
    flush_function_calls(&mut messages, &mut pending_function_calls);
    Ok(messages)
}

fn flush_function_calls(messages: &mut Vec<BridgeMessage>, pending: &mut Vec<BridgeContent>) {
    if pending.is_empty() {
        return;
    }
    messages.push(BridgeMessage {
        role: MessageRole::Assistant,
        name: None,
        content: std::mem::take(pending),
    });
}

fn parse_message_item(object: &Map<String, Value>) -> ProtocolResult<BridgeMessage> {
    let role = match required_string(object, "role", "Each message requires a role.")?.as_str() {
        "system" => MessageRole::System,
        "developer" => MessageRole::Developer,
        "user" => MessageRole::User,
        "assistant" => MessageRole::Assistant,
        "tool" => {
            return Err(ProtocolError::invalid_request(
                "Tool results must use a function_call_output input item.",
            ));
        }
        _ => {
            return Err(ProtocolError::unsupported(
                "unsupported_role",
                "This message role is not supported by this bridge.",
            ));
        }
    };
    let name = optional_string(object, "name")?;
    let content = match object.get("content") {
        Some(Value::String(text)) => vec![BridgeContent::Text { text: text.clone() }],
        Some(Value::Array(parts)) => parse_content_parts(parts)?,
        Some(_) => {
            return Err(ProtocolError::invalid_request(
                "Message content must be a string or an array.",
            ));
        }
        None => Vec::new(),
    };
    Ok(BridgeMessage {
        role,
        name,
        content,
    })
}

fn parse_content_parts(parts: &[Value]) -> ProtocolResult<Vec<BridgeContent>> {
    let mut content = Vec::with_capacity(parts.len());
    for part in parts {
        let object = part.as_object().ok_or_else(|| {
            ProtocolError::invalid_request("Each message content part must be an object.")
        })?;
        let kind = required_string(object, "type", "Each content part requires a type.")?;
        match kind.as_str() {
            "input_text" | "output_text" | "text" | "refusal" => {
                content.push(BridgeContent::Text {
                    text: required_string(object, "text", "Text content requires a text value.")?,
                });
            }
            kind if is_image_input(kind) => {
                return Err(ProtocolError::unsupported(
                    "unsupported_image_input",
                    "Image input is not supported by this bridge.",
                ));
            }
            "input_file" | "file" => {
                return Err(ProtocolError::unsupported(
                    "unsupported_input",
                    "File input is not supported by this bridge.",
                ));
            }
            _ => {
                return Err(ProtocolError::unsupported(
                    "unsupported_input_content",
                    "This input content type is not supported by this bridge.",
                ));
            }
        }
    }
    Ok(content)
}

fn parse_function_call_content(object: &Map<String, Value>) -> ProtocolResult<BridgeContent> {
    Ok(BridgeContent::ToolCall {
        id: required_string(object, "call_id", "Function calls require a call_id.")?,
        name: required_string(object, "name", "Function calls require a name.")?,
        arguments: required_string(
            object,
            "arguments",
            "Function calls require string arguments.",
        )?,
        index: None,
    })
}

fn parse_function_call_output_item(object: &Map<String, Value>) -> ProtocolResult<BridgeMessage> {
    let output = parse_function_call_output(object.get("output").ok_or_else(|| {
        ProtocolError::invalid_request("Function call output requires an output value.")
    })?)?;
    Ok(BridgeMessage {
        role: MessageRole::Tool,
        name: None,
        content: vec![BridgeContent::ToolResult {
            call_id: required_string(
                object,
                "call_id",
                "Function call output requires a call_id.",
            )?,
            output,
        }],
    })
}

/// Convert only text-shaped structured tool output.  Serializing arbitrary JSON would make
/// the upstream model receive a textual representation of data it was not asked to see.
fn parse_function_call_output(output: &Value) -> ProtocolResult<String> {
    match output {
        Value::String(output) => Ok(output.clone()),
        Value::Array(parts) => {
            let mut text = String::new();
            for part in parts {
                let object = part.as_object().ok_or_else(|| {
                    ProtocolError::invalid_request(
                        "Each function call output content part must be an object.",
                    )
                })?;
                let kind = required_string(
                    object,
                    "type",
                    "Each function call output content part requires a type.",
                )?;
                match kind.as_str() {
                    "input_text" | "output_text" | "text" => {
                        let value =
                            object.get("text").and_then(Value::as_str).ok_or_else(|| {
                                ProtocolError::invalid_request(
                                    "Text function call output requires a text value.",
                                )
                            })?;
                        text.push_str(value);
                    }
                    kind if is_image_input(kind) => {
                        return Err(ProtocolError::unsupported(
                            "unsupported_function_output_content",
                            "Image function call output is not supported by this bridge.",
                        ));
                    }
                    "input_file" | "file" => {
                        return Err(ProtocolError::unsupported(
                            "unsupported_function_output_content",
                            "File function call output is not supported by this bridge.",
                        ));
                    }
                    "input_audio" | "audio" => {
                        return Err(ProtocolError::unsupported(
                            "unsupported_function_output_content",
                            "Audio function call output is not supported by this bridge.",
                        ));
                    }
                    _ => {
                        return Err(ProtocolError::unsupported(
                            "unsupported_function_output_content",
                            "This function call output content type is not supported by this bridge.",
                        ));
                    }
                }
            }
            Ok(text)
        }
        _ => Err(ProtocolError::invalid_request(
            "Function call output must be a string or an array of text content.",
        )),
    }
}

fn parse_tools(value: Option<&Value>) -> ProtocolResult<Vec<BridgeTool>> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let items = value
        .as_array()
        .ok_or_else(|| ProtocolError::invalid_request("`tools` must be an array."))?;
    let mut tools = Vec::with_capacity(items.len());
    let mut dropped = HashSet::new();
    for item in items {
        let object = item
            .as_object()
            .ok_or_else(|| ProtocolError::invalid_request("Every tool must be an object."))?;
        let kind = required_string(object, "type", "Every tool requires a type.")?;
        if kind != "function" {
            // Non-function tool types are dropped.
            dropped.insert(kind);
            continue;
        }
        let parameters = object
            .get("parameters")
            .cloned()
            .unwrap_or_else(|| json!({ "type": "object", "properties": {} }));
        if !parameters.is_object() {
            return Err(ProtocolError::invalid_request(
                "Function tool parameters must be a JSON object.",
            ));
        }
        let strict = match object.get("strict") {
            Some(Value::Bool(strict)) => Some(*strict),
            Some(_) => {
                return Err(ProtocolError::invalid_request(
                    "Tool strict must be a boolean.",
                ))
            }
            None => None,
        };
        tools.push(BridgeTool {
            name: required_string(object, "name", "Function tools require a name.")?,
            description: optional_string(object, "description")?,
            parameters,
            strict,
        });
    }
    if !dropped.is_empty() {
        tracing::warn!(
            target: "core.adapter",
            dropped = ?dropped,
            "dropping hosted Responses tool types that Chat Completions cannot take",
        );
    }
    Ok(tools)
}

fn parse_tool_choice(value: Option<&Value>) -> ProtocolResult<Option<ToolChoice>> {
    let Some(value) = value else { return Ok(None) };
    match value {
        Value::String(choice) => match choice.as_str() {
            "auto" => Ok(Some(ToolChoice::Auto)),
            "none" => Ok(Some(ToolChoice::None)),
            "required" => Ok(Some(ToolChoice::Required)),
            _ => Err(ProtocolError::invalid_request("`tool_choice` is invalid.")),
        },
        Value::Object(object) => {
            if object.get("type").and_then(Value::as_str) != Some("function") {
                // Hosted tool_choice objects become None.
                return Ok(None);
            }
            let name = object
                .get("name")
                .and_then(Value::as_str)
                .or_else(|| {
                    object
                        .get("function")
                        .and_then(Value::as_object)
                        .and_then(|function| function.get("name"))
                        .and_then(Value::as_str)
                })
                .filter(|name| !name.is_empty())
                .ok_or_else(|| {
                    ProtocolError::invalid_request("Function tool choice requires a name.")
                })?;
            Ok(Some(ToolChoice::Function {
                name: name.to_owned(),
            }))
        }
        _ => Err(ProtocolError::invalid_request(
            "`tool_choice` must be a string or a function choice object.",
        )),
    }
}

fn required_string(
    object: &Map<String, Value>,
    key: &str,
    message: &'static str,
) -> ProtocolResult<String> {
    object
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| ProtocolError::invalid_request(message))
}

fn optional_string(object: &Map<String, Value>, key: &str) -> ProtocolResult<Option<String>> {
    match object.get(key) {
        Some(Value::String(value)) => Ok(Some(value.clone())),
        Some(Value::Null) | None => Ok(None),
        Some(_) => Err(ProtocolError::invalid_request(
            "This optional request field must be a string.",
        )),
    }
}

fn is_image_input(kind: &str) -> bool {
    matches!(kind, "input_image" | "image" | "image_url")
}

fn known_request_fields() -> HashSet<&'static str> {
    [
        "model",
        "input",
        "instructions",
        "tools",
        "tool_choice",
        "stream",
        "reasoning",
    ]
    .into_iter()
    .collect()
}

/// Responses `reasoning` is not a Chat field; map effort only for xAI.
pub(super) fn grok_reasoning_effort(value: Option<&Value>) -> Option<String> {
    let effort = match value? {
        Value::Object(object) => object.get("effort").and_then(Value::as_str)?,
        Value::String(effort) => effort.as_str(),
        _ => return None,
    };
    grok_effort_name(effort)
}

fn grok_effort_name(effort: &str) -> Option<String> {
    matches!(effort, "low" | "medium" | "high" | "xhigh" | "max").then(|| effort.to_owned())
}

/// Map Anthropic Messages top-level `thinking` to Grok `reasoning_effort`.
///
/// The original `thinking` object is never copied into passthrough.
pub(crate) fn grok_reasoning_effort_from_thinking(value: Option<&Value>) -> Option<String> {
    let object = value?.as_object()?;
    match object.get("type").and_then(Value::as_str)? {
        "disabled" => None,
        "enabled" | "adaptive" => {
            if let Some(effort) = object.get("effort").and_then(Value::as_str) {
                match effort {
                    "minimal" => return Some("low".to_owned()),
                    "low" | "medium" | "high" | "xhigh" | "max" => return Some(effort.to_owned()),
                    _ => {}
                }
            }
            Some(grok_effort_from_thinking_budget(
                object.get("budget_tokens"),
            ))
        }
        _ => None,
    }
}

fn grok_effort_from_thinking_budget(value: Option<&Value>) -> String {
    let budget = value.and_then(Value::as_f64).unwrap_or(0.0);
    if budget > 0.0 && budget <= 2048.0 {
        "low".to_owned()
    } else if budget > 10000.0 {
        "high".to_owned()
    } else {
        "medium".to_owned()
    }
}
