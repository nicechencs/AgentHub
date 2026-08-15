//! Anthropic Messages request parsing and SSE encoding against the neutral IR.
//!
//! This module is pure protocol translation: no HTTP, credentials, or runtime state.

use std::collections::HashSet;

use serde_json::{json, Map, Value};

use crate::bridge::types::{
    BridgeContent, BridgeMessage, BridgeRequest, BridgeTool, IrEvent, MessageRole, ProtocolError,
    ProtocolResult, StopReason, ToolChoice, Usage,
};

/// Parse the subset of `POST /v1/messages` that the Codex→Claude kernel can represent.
///
/// Multimodal blocks, server tools, and thinking blocks fail closed rather than silently
/// dropping information Claude Code would assume the model received.
pub fn parse_messages_request(value: &Value) -> ProtocolResult<BridgeRequest> {
    let object = value
        .as_object()
        .ok_or_else(|| ProtocolError::invalid_request("The request body must be a JSON object."))?;

    if object.contains_key("thinking") {
        return Err(ProtocolError::unsupported(
            "unsupported_thinking",
            "Thinking configuration is not supported by this bridge.",
        ));
    }

    let model = required_string(object, "model", "A non-empty model is required.")?;
    let max_tokens = object.get("max_tokens").ok_or_else(|| {
        ProtocolError::invalid_request("`max_tokens` is required for Anthropic Messages.")
    })?;
    if !max_tokens.is_number() {
        return Err(ProtocolError::invalid_request(
            "`max_tokens` must be a number.",
        ));
    }

    let stream = match object.get("stream") {
        Some(Value::Bool(stream)) => *stream,
        Some(_) => {
            return Err(ProtocolError::invalid_request(
                "`stream` must be a boolean.",
            ))
        }
        None => false,
    };

    let instructions = parse_system(object.get("system"))?;
    let input = parse_messages(
        object
            .get("messages")
            .ok_or_else(|| ProtocolError::invalid_request("`messages` is required."))?,
    )?;
    let tools = parse_tools(object.get("tools"))?;
    let tool_choice = parse_tool_choice(object.get("tool_choice"))?;

    let known = known_request_fields();
    let mut passthrough = object
        .iter()
        .filter(|(key, _)| !known.contains(key.as_str()))
        .map(|(key, item)| (key.clone(), item.clone()))
        .collect::<Map<String, Value>>();
    passthrough.insert("max_output_tokens".to_owned(), max_tokens.clone());

    // Anthropic stop_sequences / metadata / temperature etc. stay in passthrough for a
    // deliberate future mapping policy rather than being silently applied upstream.
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

/// Encode IR events as Anthropic Messages SSE records (`event:` + `data:`).
pub fn encode_anthropic_sse(events: &[IrEvent]) -> ProtocolResult<Vec<String>> {
    let mut frames = Vec::new();
    let mut content_index: Option<usize> = None;
    let mut next_content_index: usize = 0;
    let mut open_block: Option<OpenBlock> = None;
    let mut usage = Usage {
        input_tokens: 0,
        output_tokens: 0,
        total_tokens: 0,
        cached_input_tokens: None,
    };
    let mut saw_message_start = false;
    let mut saw_message_end = false;

    for event in events {
        match event {
            IrEvent::MessageStart { id, model } => {
                if saw_message_start {
                    return Err(ProtocolError::invalid_request(
                        "Duplicate message start in IR event stream.",
                    ));
                }
                saw_message_start = true;
                frames.push(sse_frame(
                    "message_start",
                    json!({
                        "type": "message_start",
                        "message": {
                            "id": id,
                            "type": "message",
                            "role": "assistant",
                            "model": model,
                            "content": [],
                            "stop_reason": null,
                            "stop_sequence": null,
                            "usage": {
                                "input_tokens": 0,
                                "output_tokens": 0
                            }
                        }
                    }),
                ));
            }
            IrEvent::TextDelta { text } => {
                ensure_started(saw_message_start)?;
                if !matches!(open_block, Some(OpenBlock::Text)) {
                    close_open_block(&mut frames, &mut open_block, content_index)?;
                    content_index = Some(next_content_index);
                    next_content_index = next_content_index.saturating_add(1);
                    open_block = Some(OpenBlock::Text);
                    frames.push(sse_frame(
                        "content_block_start",
                        json!({
                            "type": "content_block_start",
                            "index": content_index.unwrap(),
                            "content_block": { "type": "text", "text": "" }
                        }),
                    ));
                }
                frames.push(sse_frame(
                    "content_block_delta",
                    json!({
                        "type": "content_block_delta",
                        "index": content_index.unwrap(),
                        "delta": { "type": "text_delta", "text": text }
                    }),
                ));
            }
            IrEvent::ToolCallStart { id, name } => {
                ensure_started(saw_message_start)?;
                close_open_block(&mut frames, &mut open_block, content_index)?;
                content_index = Some(next_content_index);
                next_content_index = next_content_index.saturating_add(1);
                open_block = Some(OpenBlock::Tool);
                frames.push(sse_frame(
                    "content_block_start",
                    json!({
                        "type": "content_block_start",
                        "index": content_index.unwrap(),
                        "content_block": {
                            "type": "tool_use",
                            "id": id,
                            "name": name,
                            "input": {}
                        }
                    }),
                ));
            }
            IrEvent::ToolCallDelta {
                id: _,
                arguments_delta,
            } => {
                ensure_started(saw_message_start)?;
                if !matches!(open_block, Some(OpenBlock::Tool)) {
                    return Err(ProtocolError::invalid_request(
                        "Tool call delta without an open tool block.",
                    ));
                }
                frames.push(sse_frame(
                    "content_block_delta",
                    json!({
                        "type": "content_block_delta",
                        "index": content_index.unwrap(),
                        "delta": {
                            "type": "input_json_delta",
                            "partial_json": arguments_delta
                        }
                    }),
                ));
            }
            IrEvent::ToolCallEnd { id: _ } => {
                ensure_started(saw_message_start)?;
                close_open_block(&mut frames, &mut open_block, content_index)?;
            }
            IrEvent::Usage {
                input_tokens,
                output_tokens,
                cached_input_tokens,
            } => {
                usage.input_tokens = *input_tokens;
                usage.output_tokens = *output_tokens;
                usage.total_tokens = input_tokens.saturating_add(*output_tokens);
                usage.cached_input_tokens = *cached_input_tokens;
            }
            IrEvent::MessageEnd { stop_reason } => {
                ensure_started(saw_message_start)?;
                if saw_message_end {
                    return Err(ProtocolError::invalid_request(
                        "Duplicate message end in IR event stream.",
                    ));
                }
                saw_message_end = true;
                close_open_block(&mut frames, &mut open_block, content_index)?;
                let mut delta = Map::new();
                delta.insert(
                    "stop_reason".to_owned(),
                    Value::String(stop_reason.to_anthropic_stop_reason().to_owned()),
                );
                delta.insert("stop_sequence".to_owned(), Value::Null);
                let mut message_delta = Map::new();
                message_delta.insert("type".to_owned(), Value::String("message_delta".to_owned()));
                message_delta.insert("delta".to_owned(), Value::Object(delta));
                message_delta.insert("usage".to_owned(), usage.to_anthropic_usage_json());
                frames.push(sse_frame("message_delta", Value::Object(message_delta)));
                frames.push(sse_frame("message_stop", json!({ "type": "message_stop" })));
            }
            IrEvent::Error {
                code,
                message,
                retryable: _,
            } => {
                frames.push(sse_frame(
                    "error",
                    json!({
                        "type": "error",
                        "error": {
                            "type": code,
                            "message": message
                        }
                    }),
                ));
            }
        }
    }

    Ok(frames)
}

/// Build a non-streaming Anthropic Messages response object from IR events.
pub fn encode_anthropic_message(events: &[IrEvent]) -> ProtocolResult<Value> {
    let mut message_id = String::from("msg_agenthub");
    let mut model = String::from("unknown");
    let mut content = Vec::new();
    let mut current_text: Option<String> = None;
    let mut current_tool: Option<(String, String, String)> = None;
    let mut usage = Usage::default();
    let mut stop_reason = StopReason::Stop;
    let mut saw_end = false;

    for event in events {
        match event {
            IrEvent::MessageStart { id, model: m } => {
                message_id = id.clone();
                model = m.clone();
            }
            IrEvent::TextDelta { text } => {
                if let Some((_, _, _)) = &current_tool {
                    return Err(ProtocolError::invalid_request(
                        "Text delta while a tool call is open.",
                    ));
                }
                match &mut current_text {
                    Some(buffer) => buffer.push_str(text),
                    None => current_text = Some(text.clone()),
                }
            }
            IrEvent::ToolCallStart { id, name } => {
                if let Some(text) = current_text.take() {
                    content.push(json!({ "type": "text", "text": text }));
                }
                if current_tool.is_some() {
                    return Err(ProtocolError::invalid_request(
                        "Nested tool calls are not supported in one content slot.",
                    ));
                }
                current_tool = Some((id.clone(), name.clone(), String::new()));
            }
            IrEvent::ToolCallDelta {
                id: _,
                arguments_delta,
            } => {
                let Some((_, _, args)) = &mut current_tool else {
                    return Err(ProtocolError::invalid_request(
                        "Tool call delta without a tool call start.",
                    ));
                };
                args.push_str(arguments_delta);
            }
            IrEvent::ToolCallEnd { id: _ } => {
                let Some((id, name, args)) = current_tool.take() else {
                    return Err(ProtocolError::invalid_request(
                        "Tool call end without a tool call start.",
                    ));
                };
                let input = if args.trim().is_empty() {
                    json!({})
                } else {
                    serde_json::from_str(&args).unwrap_or_else(|_| json!({ "raw": args }))
                };
                content.push(json!({
                    "type": "tool_use",
                    "id": id,
                    "name": name,
                    "input": input
                }));
            }
            IrEvent::Usage {
                input_tokens,
                output_tokens,
                cached_input_tokens,
            } => {
                usage = Usage {
                    input_tokens: *input_tokens,
                    output_tokens: *output_tokens,
                    total_tokens: input_tokens.saturating_add(*output_tokens),
                    cached_input_tokens: *cached_input_tokens,
                };
            }
            IrEvent::MessageEnd {
                stop_reason: reason,
            } => {
                saw_end = true;
                stop_reason = reason.clone();
            }
            IrEvent::Error { code, message, .. } => {
                return Err(ProtocolError::unsupported(
                    "upstream_error",
                    format!("{code}: {message}"),
                ));
            }
        }
    }

    if let Some(text) = current_text.take() {
        content.push(json!({ "type": "text", "text": text }));
    }
    if current_tool.is_some() {
        return Err(ProtocolError::invalid_request(
            "Unterminated tool call in IR event stream.",
        ));
    }
    if !saw_end {
        return Err(ProtocolError::invalid_request(
            "IR event stream is missing MessageEnd.",
        ));
    }

    Ok(json!({
        "id": message_id,
        "type": "message",
        "role": "assistant",
        "model": model,
        "content": content,
        "stop_reason": stop_reason.to_anthropic_stop_reason(),
        "stop_sequence": null,
        "usage": usage.to_anthropic_usage_json(),
    }))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OpenBlock {
    Text,
    Tool,
}

fn ensure_started(saw_message_start: bool) -> ProtocolResult<()> {
    if saw_message_start {
        Ok(())
    } else {
        Err(ProtocolError::invalid_request(
            "IR content events require MessageStart first.",
        ))
    }
}

fn close_open_block(
    frames: &mut Vec<String>,
    open_block: &mut Option<OpenBlock>,
    content_index: Option<usize>,
) -> ProtocolResult<()> {
    if open_block.take().is_some() {
        let index = content_index.ok_or_else(|| {
            ProtocolError::invalid_request("Open content block is missing an index.")
        })?;
        frames.push(sse_frame(
            "content_block_stop",
            json!({
                "type": "content_block_stop",
                "index": index
            }),
        ));
    }
    Ok(())
}

fn sse_frame(event: &str, data: Value) -> String {
    format!(
        "event: {event}\ndata: {}\n\n",
        serde_json::to_string(&data).unwrap_or_else(|_| "{}".to_owned())
    )
}

fn parse_system(value: Option<&Value>) -> ProtocolResult<Option<String>> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) => Ok(Some(text.clone())),
        Some(Value::Array(parts)) => {
            let mut text = String::new();
            for part in parts {
                let object = part.as_object().ok_or_else(|| {
                    ProtocolError::invalid_request("Each system content block must be an object.")
                })?;
                match object.get("type").and_then(Value::as_str) {
                    Some("text") => {
                        text.push_str(&required_string(
                            object,
                            "text",
                            "System text blocks require a text value.",
                        )?);
                    }
                    Some(_) => {
                        return Err(ProtocolError::unsupported(
                            "unsupported_system_content",
                            "This system content type is not supported by this bridge.",
                        ));
                    }
                    None => {
                        return Err(ProtocolError::invalid_request(
                            "Each system content block requires a type.",
                        ));
                    }
                }
            }
            if text.is_empty() {
                Ok(None)
            } else {
                Ok(Some(text))
            }
        }
        Some(_) => Err(ProtocolError::invalid_request(
            "`system` must be a string or an array of text blocks.",
        )),
    }
}

fn parse_messages(value: &Value) -> ProtocolResult<Vec<BridgeMessage>> {
    let items = value
        .as_array()
        .ok_or_else(|| ProtocolError::invalid_request("`messages` must be an array."))?;
    let mut messages = Vec::with_capacity(items.len());
    for item in items {
        let object = item.as_object().ok_or_else(|| {
            ProtocolError::invalid_request("Every messages item must be an object.")
        })?;
        let role = match required_string(object, "role", "Each message requires a role.")?.as_str()
        {
            "user" => MessageRole::User,
            "assistant" => MessageRole::Assistant,
            "system" => MessageRole::System,
            other => {
                return Err(ProtocolError::unsupported(
                    "unsupported_role",
                    format!("Message role `{other}` is not supported by this bridge."),
                ));
            }
        };
        let content = parse_message_content(object.get("content"), &role)?;
        // Tool results are represented as user messages with tool_result blocks in Messages.
        // When a user message contains only tool results, expose them as Tool-role IR messages
        // so Responses history can emit function_call_output items.
        if role == MessageRole::User
            && !content.is_empty()
            && content
                .iter()
                .all(|part| matches!(part, BridgeContent::ToolResult { .. }))
        {
            for part in content {
                if let BridgeContent::ToolResult { call_id, output } = part {
                    messages.push(BridgeMessage {
                        role: MessageRole::Tool,
                        name: None,
                        content: vec![BridgeContent::ToolResult { call_id, output }],
                    });
                }
            }
            continue;
        }
        messages.push(BridgeMessage {
            role,
            name: None,
            content,
        });
    }
    Ok(messages)
}

fn parse_message_content(
    value: Option<&Value>,
    role: &MessageRole,
) -> ProtocolResult<Vec<BridgeContent>> {
    match value {
        Some(Value::String(text)) => Ok(vec![BridgeContent::Text { text: text.clone() }]),
        Some(Value::Array(parts)) => {
            let mut content = Vec::with_capacity(parts.len());
            for part in parts {
                let object = part.as_object().ok_or_else(|| {
                    ProtocolError::invalid_request("Each content block must be an object.")
                })?;
                let kind = required_string(object, "type", "Each content block requires a type.")?;
                match kind.as_str() {
                    "text" => {
                        content.push(BridgeContent::Text {
                            text: required_string(
                                object,
                                "text",
                                "Text content requires a text value.",
                            )?,
                        });
                    }
                    "tool_use" => {
                        if *role != MessageRole::Assistant {
                            return Err(ProtocolError::invalid_request(
                                "tool_use blocks are only valid on assistant messages.",
                            ));
                        }
                        let input = object.get("input").cloned().unwrap_or_else(|| json!({}));
                        let arguments = match input {
                            Value::String(raw) => raw,
                            other => serde_json::to_string(&other).map_err(|_| {
                                ProtocolError::invalid_request(
                                    "tool_use input could not be serialised.",
                                )
                            })?,
                        };
                        content.push(BridgeContent::ToolCall {
                            id: required_string(object, "id", "tool_use requires an id.")?,
                            name: required_string(object, "name", "tool_use requires a name.")?,
                            arguments,
                            index: None,
                        });
                    }
                    "tool_result" => {
                        if *role != MessageRole::User {
                            return Err(ProtocolError::invalid_request(
                                "tool_result blocks are only valid on user messages.",
                            ));
                        }
                        let output = match object.get("content") {
                            Some(Value::String(text)) => text.clone(),
                            Some(Value::Array(parts)) => flatten_tool_result_parts(parts)?,
                            Some(Value::Null) | None => String::new(),
                            Some(_) => {
                                return Err(ProtocolError::invalid_request(
                                    "tool_result content must be a string or text blocks.",
                                ));
                            }
                        };
                        content.push(BridgeContent::ToolResult {
                            call_id: required_string(
                                object,
                                "tool_use_id",
                                "tool_result requires tool_use_id.",
                            )?,
                            output,
                        });
                    }
                    "image" => {
                        return Err(ProtocolError::unsupported(
                            "unsupported_image_input",
                            "Image input is not supported by this bridge.",
                        ));
                    }
                    "thinking" | "redacted_thinking" => {
                        return Err(ProtocolError::unsupported(
                            "unsupported_thinking",
                            "Thinking blocks are not supported by this bridge.",
                        ));
                    }
                    _ => {
                        return Err(ProtocolError::unsupported(
                            "unsupported_input_content",
                            "This content block type is not supported by this bridge.",
                        ));
                    }
                }
            }
            Ok(content)
        }
        Some(_) => Err(ProtocolError::invalid_request(
            "Message content must be a string or an array of content blocks.",
        )),
        None => Ok(Vec::new()),
    }
}

fn flatten_tool_result_parts(parts: &[Value]) -> ProtocolResult<String> {
    let mut text = String::new();
    for part in parts {
        let object = part.as_object().ok_or_else(|| {
            ProtocolError::invalid_request("Each tool_result content part must be an object.")
        })?;
        match object.get("type").and_then(Value::as_str) {
            Some("text") => {
                text.push_str(&required_string(
                    object,
                    "text",
                    "tool_result text parts require a text value.",
                )?);
            }
            Some("image") => {
                return Err(ProtocolError::unsupported(
                    "unsupported_function_output_content",
                    "Image tool_result content is not supported by this bridge.",
                ));
            }
            Some(_) => {
                return Err(ProtocolError::unsupported(
                    "unsupported_function_output_content",
                    "This tool_result content type is not supported by this bridge.",
                ));
            }
            None => {
                return Err(ProtocolError::invalid_request(
                    "Each tool_result content part requires a type.",
                ));
            }
        }
    }
    Ok(text)
}

fn parse_tools(value: Option<&Value>) -> ProtocolResult<Vec<BridgeTool>> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let items = value
        .as_array()
        .ok_or_else(|| ProtocolError::invalid_request("`tools` must be an array."))?;
    let mut tools = Vec::with_capacity(items.len());
    for item in items {
        let object = item
            .as_object()
            .ok_or_else(|| ProtocolError::invalid_request("Every tool must be an object."))?;
        // Anthropic tools are bare objects with name/description/input_schema (no type:function).
        if let Some(kind) = object.get("type").and_then(Value::as_str) {
            if kind != "custom" && kind != "function" {
                let code = match kind {
                    "web_search_20250305" | "web_search" => "unsupported_web_search",
                    "computer_20241022" | "computer" => "unsupported_computer_use",
                    _ => "unsupported_tool",
                };
                return Err(ProtocolError::unsupported(
                    code,
                    "This hosted tool type is not supported by this bridge.",
                ));
            }
        }
        let parameters = object
            .get("input_schema")
            .or_else(|| object.get("parameters"))
            .cloned()
            .unwrap_or_else(|| json!({ "type": "object", "properties": {} }));
        if !parameters.is_object() {
            return Err(ProtocolError::invalid_request(
                "Tool input_schema must be a JSON object.",
            ));
        }
        tools.push(BridgeTool {
            name: required_string(object, "name", "Tools require a name.")?,
            description: optional_string(object, "description")?,
            parameters,
            strict: None,
        });
    }
    Ok(tools)
}

fn parse_tool_choice(value: Option<&Value>) -> ProtocolResult<Option<ToolChoice>> {
    let Some(value) = value else {
        return Ok(None);
    };
    let object = value.as_object().ok_or_else(|| {
        ProtocolError::invalid_request("`tool_choice` must be an object for Anthropic Messages.")
    })?;
    match object.get("type").and_then(Value::as_str) {
        Some("auto") => Ok(Some(ToolChoice::Auto)),
        Some("any") => Ok(Some(ToolChoice::Required)),
        Some("none") => Ok(Some(ToolChoice::None)),
        Some("tool") => Ok(Some(ToolChoice::Function {
            name: required_string(object, "name", "tool_choice.tool requires a name.")?,
        })),
        Some(_) => Err(ProtocolError::unsupported(
            "unsupported_tool_choice",
            "This tool_choice type is not supported by this bridge.",
        )),
        None => Err(ProtocolError::invalid_request(
            "`tool_choice` requires a type.",
        )),
    }
}

fn known_request_fields() -> HashSet<&'static str> {
    // Only fields fully consumed into BridgeRequest fields are excluded from
    // passthrough. temperature / top_p / metadata / stop_sequences / top_k stay
    // available for a deliberate mapping policy (to_responses_request only
    // forwards a documented subset).
    HashSet::from([
        "model",
        "messages",
        "system",
        "tools",
        "tool_choice",
        "max_tokens",
        "stream",
        "thinking",
    ])
}

fn required_string(
    object: &Map<String, Value>,
    key: &str,
    message: &str,
) -> ProtocolResult<String> {
    object
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| ProtocolError::invalid_request(message.to_owned()))
}

fn optional_string(object: &Map<String, Value>, key: &str) -> ProtocolResult<Option<String>> {
    match object.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                Ok(None)
            } else {
                Ok(Some(trimmed.to_owned()))
            }
        }
        Some(_) => Err(ProtocolError::invalid_request(format!(
            "`{key}` must be a string."
        ))),
    }
}
