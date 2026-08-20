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
/// Top-level `thinking` configuration is ignored and dropped (Claude Code always sends
/// it). `thinking` / `redacted_thinking` content blocks in history still fail closed
/// rather than silently dropping information Claude Code would assume the model received.
/// Multimodal blocks and server tools also fail closed.
pub fn parse_messages_request(value: &Value) -> ProtocolResult<BridgeRequest> {
    let object = value
        .as_object()
        .ok_or_else(|| ProtocolError::invalid_request("The request body must be a JSON object."))?;

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
        reasoning_tokens: 0,
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
                    reasoning_tokens: 0,
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

/// Render a neutral [`BridgeRequest`] as an Anthropic Messages request body.
///
/// Used by the Anthropic API Key → Codex local_bridge: Codex speaks Responses
/// downstream; the host forwards this body to `POST /v1/messages` with
/// `x-api-key` + `anthropic-version` (headers stay in the host).
pub fn to_anthropic_messages_request(request: &BridgeRequest) -> Value {
    let mut system = request.instructions.clone();
    let mut messages = Vec::new();
    let mut pending_tool_results = Vec::new();

    for message in &request.input {
        match message.role {
            MessageRole::System | MessageRole::Developer => {
                flush_tool_results(&mut messages, &mut pending_tool_results);
                let text = collect_text(message);
                if text.is_empty() {
                    continue;
                }
                match &mut system {
                    Some(existing) => {
                        existing.push('\n');
                        existing.push_str(&text);
                    }
                    None => system = Some(text),
                }
            }
            MessageRole::Tool => {
                for part in &message.content {
                    if let BridgeContent::ToolResult { call_id, output } = part {
                        pending_tool_results.push(json!({
                            "type": "tool_result",
                            "tool_use_id": call_id,
                            "content": output,
                        }));
                    }
                }
            }
            MessageRole::User => {
                flush_tool_results(&mut messages, &mut pending_tool_results);
                messages.push(anthropic_user_message(message));
            }
            MessageRole::Assistant => {
                flush_tool_results(&mut messages, &mut pending_tool_results);
                messages.push(anthropic_assistant_message(message));
            }
        }
    }
    flush_tool_results(&mut messages, &mut pending_tool_results);

    let mut body = Map::new();
    body.insert("model".to_owned(), Value::String(request.model.clone()));
    body.insert("stream".to_owned(), Value::Bool(request.stream));
    body.insert(
        "max_tokens".to_owned(),
        request
            .passthrough
            .get("max_output_tokens")
            .cloned()
            .unwrap_or(json!(4096)),
    );
    if let Some(system) = system {
        body.insert("system".to_owned(), Value::String(system));
    }
    body.insert("messages".to_owned(), Value::Array(messages));

    if !request.tools.is_empty() {
        body.insert(
            "tools".to_owned(),
            Value::Array(
                request
                    .tools
                    .iter()
                    .map(|tool| {
                        let mut object = Map::new();
                        object.insert("name".to_owned(), Value::String(tool.name.clone()));
                        object.insert("input_schema".to_owned(), tool.parameters.clone());
                        if let Some(description) = &tool.description {
                            object.insert(
                                "description".to_owned(),
                                Value::String(description.clone()),
                            );
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
            render_anthropic_tool_choice(tool_choice),
        );
    }

    for key in [
        "temperature",
        "top_p",
        "top_k",
        "stop_sequences",
        "metadata",
    ] {
        if let Some(value) = request.passthrough.get(key) {
            body.insert(key.to_owned(), value.clone());
        }
    }

    Value::Object(body)
}

/// Convenience entry: Responses request → Anthropic Messages request.
pub fn translate_responses_to_anthropic_request(
    value: &Value,
) -> ProtocolResult<(BridgeRequest, Value)> {
    let request = crate::bridge::protocol::responses::parse_responses_request(value)?;
    let anthropic = to_anthropic_messages_request(&request);
    Ok((request, anthropic))
}

/// Convert a completed non-streaming Anthropic Messages object into [`IrEvent`]s.
pub fn anthropic_message_to_ir(value: &Value) -> ProtocolResult<Vec<IrEvent>> {
    if value.get("error").is_some() {
        return Ok(vec![IrEvent::Error {
            code: "upstream_error".to_owned(),
            message: "The upstream model provider returned an error.".to_owned(),
            retryable: false,
        }]);
    }

    let object = value.as_object().ok_or_else(|| {
        ProtocolError::invalid_request("Anthropic Messages body must be a JSON object.")
    })?;
    let id = object
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("msg_agenthub")
        .to_owned();
    let model = object
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_owned();

    let mut events = vec![IrEvent::MessageStart { id, model }];
    let content = object
        .get("content")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            ProtocolError::invalid_request("Anthropic Messages body requires a content array.")
        })?;

    for block in content {
        let block_object = block.as_object().ok_or_else(|| {
            ProtocolError::invalid_request("Each Anthropic content block must be an object.")
        })?;
        match block_object.get("type").and_then(Value::as_str) {
            Some("text") => {
                let text = block_object
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if !text.is_empty() {
                    events.push(IrEvent::TextDelta {
                        text: text.to_owned(),
                    });
                }
            }
            Some("tool_use") => {
                let call_id = required_string(block_object, "id", "tool_use requires an id.")?;
                let name = required_string(block_object, "name", "tool_use requires a name.")?;
                events.push(IrEvent::ToolCallStart {
                    id: call_id.clone(),
                    name,
                });
                if let Some(input) = block_object.get("input") {
                    let arguments = match input {
                        Value::String(raw) => raw.clone(),
                        other => serde_json::to_string(other).unwrap_or_else(|_| "{}".to_owned()),
                    };
                    if !arguments.is_empty() && arguments != "{}" {
                        events.push(IrEvent::ToolCallDelta {
                            id: call_id.clone(),
                            arguments_delta: arguments,
                        });
                    }
                }
                events.push(IrEvent::ToolCallEnd { id: call_id });
            }
            Some("thinking") | Some("redacted_thinking") => {
                return Err(ProtocolError::unsupported(
                    "unsupported_thinking",
                    "Thinking blocks are not supported by this bridge.",
                ));
            }
            Some("image") => {
                return Err(ProtocolError::unsupported(
                    "unsupported_image_input",
                    "Image input is not supported by this bridge.",
                ));
            }
            Some(_) => {
                return Err(ProtocolError::unsupported(
                    "unsupported_output_content",
                    "This Anthropic content type is not supported by this bridge.",
                ));
            }
            None => {
                return Err(ProtocolError::invalid_request(
                    "Each Anthropic content block requires a type.",
                ));
            }
        }
    }

    if let Some(usage) = object.get("usage").and_then(Usage::from_anthropic_usage) {
        events.push(IrEvent::Usage {
            input_tokens: usage.input_tokens,
            output_tokens: usage.output_tokens,
            cached_input_tokens: usage.cached_input_tokens,
        });
    }

    events.push(IrEvent::MessageEnd {
        stop_reason: StopReason::from_anthropic_stop_reason(
            object.get("stop_reason").and_then(Value::as_str),
        ),
    });
    Ok(events)
}

/// Stateful Anthropic Messages SSE → IR translator for the Anthropic upstream stream.
///
/// Feed each decoded `data:` JSON object to [`Self::push_event`]. Call [`Self::finish`]
/// if the upstream closes without `message_stop`.
#[derive(Debug, Default)]
pub struct AnthropicStreamToIr {
    started: bool,
    completed: bool,
    message_id: String,
    model: String,
    open_tool: Option<OpenAnthropicTool>,
    stop_reason: StopReason,
    usage: Option<Usage>,
    pending: Vec<IrEvent>,
}

#[derive(Debug, Clone)]
struct OpenAnthropicTool {
    id: String,
    ended: bool,
}

impl AnthropicStreamToIr {
    pub fn new() -> Self {
        Self {
            message_id: "msg_agenthub".to_owned(),
            model: "unknown".to_owned(),
            stop_reason: StopReason::Stop,
            ..Self::default()
        }
    }

    pub fn completed(&self) -> bool {
        self.completed
    }

    pub fn push_event(&mut self, value: &Value) -> ProtocolResult<Vec<IrEvent>> {
        if self.completed {
            return Ok(Vec::new());
        }
        let object = value.as_object().ok_or_else(|| {
            ProtocolError::invalid_request("Each Anthropic SSE event must be a JSON object.")
        })?;
        let event_type = object
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();

        match event_type {
            "message_start" => {
                if let Some(message) = object.get("message").and_then(Value::as_object) {
                    if let Some(id) = message.get("id").and_then(Value::as_str) {
                        self.message_id = id.to_owned();
                    }
                    if let Some(model) = message.get("model").and_then(Value::as_str) {
                        self.model = model.to_owned();
                    }
                    if let Some(usage) = message.get("usage").and_then(Usage::from_anthropic_usage)
                    {
                        self.usage = Some(usage);
                    }
                }
                self.ensure_message_start();
            }
            "content_block_start" => {
                self.ensure_message_start();
                if let Some(block) = object.get("content_block").and_then(Value::as_object) {
                    match block.get("type").and_then(Value::as_str) {
                        Some("text") => {}
                        Some("tool_use") => {
                            self.close_open_tool();
                            let call_id = block
                                .get("id")
                                .and_then(Value::as_str)
                                .filter(|value| !value.is_empty())
                                .unwrap_or("call_0");
                            let name = block
                                .get("name")
                                .and_then(Value::as_str)
                                .filter(|value| !value.is_empty())
                                .unwrap_or("tool");
                            self.open_tool = Some(OpenAnthropicTool {
                                id: call_id.to_owned(),
                                ended: false,
                            });
                            self.pending.push(IrEvent::ToolCallStart {
                                id: call_id.to_owned(),
                                name: name.to_owned(),
                            });
                            self.stop_reason = StopReason::ToolCalls;
                        }
                        Some("thinking") | Some("redacted_thinking") => {
                            return Err(ProtocolError::unsupported(
                                "unsupported_thinking",
                                "Thinking blocks are not supported by this bridge.",
                            ));
                        }
                        Some("image") => {
                            return Err(ProtocolError::unsupported(
                                "unsupported_image_input",
                                "Image input is not supported by this bridge.",
                            ));
                        }
                        Some(_) => {
                            return Err(ProtocolError::unsupported(
                                "unsupported_output_content",
                                "This Anthropic content type is not supported by this bridge.",
                            ));
                        }
                        None => {
                            return Err(ProtocolError::invalid_request(
                                "content_block_start requires a content_block type.",
                            ));
                        }
                    }
                }
            }
            "content_block_delta" => {
                self.ensure_message_start();
                if let Some(delta) = object.get("delta").and_then(Value::as_object) {
                    match delta.get("type").and_then(Value::as_str) {
                        Some("text_delta") => {
                            if let Some(text) = delta.get("text").and_then(Value::as_str) {
                                if !text.is_empty() {
                                    self.pending.push(IrEvent::TextDelta {
                                        text: text.to_owned(),
                                    });
                                }
                            }
                        }
                        Some("input_json_delta") => {
                            let Some(tool) = &self.open_tool else {
                                return Err(ProtocolError::invalid_request(
                                    "Tool call delta without an open tool block.",
                                ));
                            };
                            let partial = delta
                                .get("partial_json")
                                .and_then(Value::as_str)
                                .unwrap_or_default();
                            if !partial.is_empty() {
                                self.pending.push(IrEvent::ToolCallDelta {
                                    id: tool.id.clone(),
                                    arguments_delta: partial.to_owned(),
                                });
                            }
                        }
                        Some("thinking_delta") => {
                            return Err(ProtocolError::unsupported(
                                "unsupported_thinking",
                                "Thinking blocks are not supported by this bridge.",
                            ));
                        }
                        _ => {}
                    }
                }
            }
            "content_block_stop" => {
                self.ensure_message_start();
                self.close_open_tool();
            }
            "message_delta" => {
                self.ensure_message_start();
                if let Some(delta) = object.get("delta").and_then(Value::as_object) {
                    if let Some(reason) = delta.get("stop_reason").and_then(Value::as_str) {
                        self.stop_reason = StopReason::from_anthropic_stop_reason(Some(reason));
                    }
                }
                if let Some(usage) = object.get("usage").and_then(Usage::from_anthropic_usage) {
                    self.usage = Some(usage);
                }
            }
            "message_stop" => {
                self.ensure_message_start();
                self.close_open_tool();
                self.push_terminal();
            }
            "error" => {
                self.completed = true;
                self.pending.push(IrEvent::Error {
                    code: "upstream_error".to_owned(),
                    message: "The upstream model provider returned an error.".to_owned(),
                    retryable: false,
                });
            }
            "ping" => {}
            _ => {}
        }

        Ok(std::mem::take(&mut self.pending))
    }

    pub fn finish(&mut self) -> Vec<IrEvent> {
        if !self.completed {
            self.ensure_message_start();
            self.close_open_tool();
            self.push_terminal();
        }
        std::mem::take(&mut self.pending)
    }

    fn ensure_message_start(&mut self) {
        if !self.started {
            self.started = true;
            self.pending.push(IrEvent::MessageStart {
                id: self.message_id.clone(),
                model: self.model.clone(),
            });
        }
    }

    fn close_open_tool(&mut self) {
        if let Some(tool) = self.open_tool.take() {
            if !tool.ended {
                self.pending.push(IrEvent::ToolCallEnd { id: tool.id });
            }
        }
    }

    fn push_terminal(&mut self) {
        if self.completed {
            return;
        }
        self.completed = true;
        if let Some(usage) = self.usage.clone() {
            self.pending.push(IrEvent::Usage {
                input_tokens: usage.input_tokens,
                output_tokens: usage.output_tokens,
                cached_input_tokens: usage.cached_input_tokens,
            });
        }
        self.pending.push(IrEvent::MessageEnd {
            stop_reason: self.stop_reason.clone(),
        });
    }
}

fn flush_tool_results(messages: &mut Vec<Value>, pending: &mut Vec<Value>) {
    if pending.is_empty() {
        return;
    }
    messages.push(json!({
        "role": "user",
        "content": std::mem::take(pending),
    }));
}

fn collect_text(message: &BridgeMessage) -> String {
    message
        .content
        .iter()
        .filter_map(|content| match content {
            BridgeContent::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

fn anthropic_user_message(message: &BridgeMessage) -> Value {
    let mut content = Vec::new();
    for part in &message.content {
        match part {
            BridgeContent::Text { text } if !text.is_empty() => {
                content.push(json!({ "type": "text", "text": text }));
            }
            BridgeContent::ToolResult { call_id, output } => {
                content.push(json!({
                    "type": "tool_result",
                    "tool_use_id": call_id,
                    "content": output,
                }));
            }
            _ => {}
        }
    }
    if content.is_empty() {
        content.push(json!({ "type": "text", "text": "" }));
    }
    json!({ "role": "user", "content": content })
}

fn anthropic_assistant_message(message: &BridgeMessage) -> Value {
    let mut content = Vec::new();
    for part in &message.content {
        match part {
            BridgeContent::Text { text } if !text.is_empty() => {
                content.push(json!({ "type": "text", "text": text }));
            }
            BridgeContent::ToolCall {
                id,
                name,
                arguments,
                ..
            } => {
                let input = if arguments.trim().is_empty() {
                    json!({})
                } else {
                    serde_json::from_str(arguments).unwrap_or_else(|_| json!({ "raw": arguments }))
                };
                content.push(json!({
                    "type": "tool_use",
                    "id": id,
                    "name": name,
                    "input": input,
                }));
            }
            _ => {}
        }
    }
    json!({ "role": "assistant", "content": content })
}

fn render_anthropic_tool_choice(choice: &ToolChoice) -> Value {
    match choice {
        ToolChoice::Auto => json!({ "type": "auto" }),
        ToolChoice::None => json!({ "type": "none" }),
        ToolChoice::Required => json!({ "type": "any" }),
        ToolChoice::Function { name } => json!({ "type": "tool", "name": name }),
    }
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
