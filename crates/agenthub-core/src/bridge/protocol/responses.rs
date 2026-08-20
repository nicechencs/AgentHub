//! OpenAI Responses request parsing and Kimi Chat Completions request rendering.

use std::collections::HashSet;

use serde_json::{json, Map, Value};

use crate::bridge::types::{
    BridgeContent, BridgeEvent, BridgeMessage, BridgeRequest, BridgeTool, IrEvent, MessageRole,
    ProtocolError, ProtocolResult, StopReason, ToolChoice, Usage,
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

/// Convert a parsed OpenAI Responses request to the Kimi OpenAI-compatible Chat request.
pub fn to_kimi_chat_request(request: &BridgeRequest) -> Value {
    let mut messages = Vec::new();
    if let Some(instructions) = &request.instructions {
        messages.push(json!({ "role": "system", "content": instructions }));
    }

    for message in &request.input {
        append_kimi_messages(&mut messages, message);
    }

    let mut body = Map::new();
    body.insert("model".to_owned(), Value::String(request.model.clone()));
    body.insert("messages".to_owned(), Value::Array(messages));
    body.insert("stream".to_owned(), Value::Bool(request.stream));
    if request.stream {
        // Kimi follows the OpenAI-compatible opt-in for a final usage-only stream chunk.
        // Without it, a Responses client loses its token accounting for streamed requests.
        body.insert(
            "stream_options".to_owned(),
            json!({ "include_usage": true }),
        );
    }

    if !request.tools.is_empty() {
        body.insert(
            "tools".to_owned(),
            Value::Array(
                request
                    .tools
                    .iter()
                    .map(|tool| {
                        let mut function = Map::new();
                        function.insert("name".to_owned(), Value::String(tool.name.clone()));
                        function.insert("parameters".to_owned(), tool.parameters.clone());
                        if let Some(description) = &tool.description {
                            function.insert(
                                "description".to_owned(),
                                Value::String(description.clone()),
                            );
                        }
                        if let Some(strict) = tool.strict {
                            function.insert("strict".to_owned(), Value::Bool(strict));
                        }
                        json!({ "type": "function", "function": function })
                    })
                    .collect(),
            ),
        );
    }

    if let Some(tool_choice) = &request.tool_choice {
        body.insert("tool_choice".to_owned(), render_tool_choice(tool_choice));
    }

    // Forward only Chat Completions options with an equivalent Kimi meaning.  All other
    // unknown options remain in BridgeRequest::passthrough for a deliberate future policy.
    for key in [
        "temperature",
        "top_p",
        "presence_penalty",
        "frequency_penalty",
        "seed",
        "response_format",
        "n",
    ] {
        if let Some(value) = request.passthrough.get(key) {
            body.insert(key.to_owned(), value.clone());
        }
    }
    if let Some(max_output_tokens) = request.passthrough.get("max_output_tokens") {
        body.insert("max_tokens".to_owned(), max_output_tokens.clone());
    }

    Value::Object(body)
}

/// Convert a parsed OpenAI Responses request to xAI/Grok Chat Completions.
///
/// Same Chat Completions shape as [`to_kimi_chat_request`], plus `reasoning_effort`
/// when Codex sent a mappable Responses `reasoning.effort`.
pub fn to_grok_chat_request(request: &BridgeRequest) -> Value {
    let mut body = to_kimi_chat_request(request);
    if let Some(effort) = request.passthrough.get("reasoning_effort") {
        if let Some(object) = body.as_object_mut() {
            object.insert("reasoning_effort".to_owned(), effort.clone());
        }
    }
    body
}

/// Convenience entry point for HTTP handlers.
pub fn translate_responses_request(value: &Value) -> ProtocolResult<(BridgeRequest, Value)> {
    let request = parse_responses_request(value)?;
    let kimi_request = to_kimi_chat_request(&request);
    Ok((request, kimi_request))
}

/// Render a neutral [`BridgeRequest`] as an OpenAI Responses request body.
///
/// Used by the Codex subscription → Claude Code kernel when the approved upstream
/// transport is Responses. Unlike [`to_kimi_chat_request`], this keeps Responses
/// shapes (`input` items, `max_output_tokens`, function tools without Chat wrapping).
pub fn to_responses_request(request: &BridgeRequest) -> Value {
    let mut body = Map::new();
    body.insert("model".to_owned(), Value::String(request.model.clone()));
    body.insert("stream".to_owned(), Value::Bool(request.stream));
    if let Some(instructions) = &request.instructions {
        body.insert(
            "instructions".to_owned(),
            Value::String(instructions.clone()),
        );
    }

    let mut input = Vec::new();
    for message in &request.input {
        append_responses_input(&mut input, message);
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

/// Official ChatGPT / Codex Responses rejects leftover `grok-*` and `claude-*`
/// model ids (400). Do not invent a ChatGPT model name to replace them — omit
/// instead.
pub fn is_leftover_grok_model(model: &str) -> bool {
    let model = model.trim();
    model.starts_with("grok-") || model.starts_with("claude-")
}

/// Write the Responses `model` for official Codex upstream.
///
/// Configured override wins when it is non-empty and not leftover `grok-*` /
/// `claude-*`. Incoming leftovers are dropped rather than rewritten as `gpt-*`.
pub fn apply_official_codex_model(body: &mut Value, incoming: &str, configured: Option<&str>) {
    let configured = configured
        .map(str::trim)
        .filter(|value| !value.is_empty() && !is_leftover_grok_model(value));
    let incoming = incoming
        .trim()
        .to_owned();
    let incoming = if incoming.is_empty() || is_leftover_grok_model(&incoming) {
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

/// Build a non-streaming OpenAI Responses object from IR events.
///
/// Used by the Anthropic API Key → Codex path after Anthropic Messages
/// (JSON or reconstructed stream) has been reduced to IR.
pub fn encode_responses_from_ir(
    events: &[IrEvent],
    response_id: Option<&str>,
) -> ProtocolResult<Value> {
    let mut message_id = String::from("msg_agenthub");
    let mut model = String::from("unknown");
    let mut output = Vec::new();
    let mut current_text: Option<String> = None;
    let mut current_tool: Option<(String, String, String)> = None;
    let mut usage = None;
    let mut stop_reason = StopReason::Stop;
    let mut saw_end = false;
    let mut allocated_id = response_id.map(ToOwned::to_owned);

    for event in events {
        match event {
            IrEvent::MessageStart { id, model: m } => {
                message_id = id.clone();
                model = m.clone();
                if allocated_id.is_none() {
                    allocated_id = Some(id.strip_prefix("msg_").unwrap_or(id).to_owned());
                }
            }
            IrEvent::TextDelta { text } => {
                if current_tool.is_some() {
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
                    output.push(responses_message_item(&message_id, &text));
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
                output.push(responses_function_call_item(&id, &name, &args));
            }
            IrEvent::Usage {
                input_tokens,
                output_tokens,
                cached_input_tokens,
            } => {
                usage = Some(Usage {
                    input_tokens: *input_tokens,
                    output_tokens: *output_tokens,
                    total_tokens: input_tokens.saturating_add(*output_tokens),
                    cached_input_tokens: *cached_input_tokens,
                    reasoning_tokens: 0,
                });
            }
            IrEvent::MessageEnd {
                stop_reason: reason,
            } => {
                saw_end = true;
                stop_reason = reason.clone();
            }
            IrEvent::Error { .. } => {
                return Err(ProtocolError::upstream());
            }
        }
    }

    if let Some(text) = current_text.take() {
        output.push(responses_message_item(&message_id, &text));
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

    Ok(responses_object(
        allocated_id.as_deref().unwrap_or("resp_agenthub"),
        &model,
        0,
        output,
        stop_reason,
        usage,
        true,
    ))
}

/// Incremental IR → Responses SSE encoder for the Anthropic → Codex stream.
#[derive(Debug)]
pub struct IrToResponsesSse {
    response_id: String,
    model: String,
    started: bool,
    completed: bool,
    message: Option<ResponsesMessageState>,
    tool: Option<ResponsesToolState>,
    next_output_index: usize,
    next_sequence_number: u64,
    usage: Option<Usage>,
    stop_reason: StopReason,
}

#[derive(Debug, Clone)]
struct ResponsesMessageState {
    id: String,
    output_index: usize,
    text: String,
    content_added: bool,
}

#[derive(Debug, Clone)]
struct ResponsesToolState {
    output_index: usize,
    item_id: String,
    call_id: String,
    name: String,
    arguments: String,
    ended: bool,
}

impl IrToResponsesSse {
    pub fn new(response_id: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            response_id: response_id.into(),
            model: model.into(),
            started: false,
            completed: false,
            message: None,
            tool: None,
            next_output_index: 0,
            next_sequence_number: 0,
            usage: None,
            stop_reason: StopReason::Unknown,
        }
    }

    pub fn push_event(&mut self, event: &IrEvent) -> ProtocolResult<Vec<BridgeEvent>> {
        if self.completed {
            return Ok(Vec::new());
        }
        let mut events = Vec::new();
        match event {
            IrEvent::MessageStart { id, model } => {
                if let Some(stripped) = id.strip_prefix("msg_") {
                    if self.response_id == "resp_agenthub" || self.response_id.is_empty() {
                        self.response_id = stripped.to_owned();
                    }
                }
                if !model.is_empty() {
                    self.model = model.clone();
                }
                events.extend(self.ensure_started());
            }
            IrEvent::TextDelta { text } => {
                events.extend(self.ensure_started());
                if !text.is_empty() {
                    self.append_text_delta(text, &mut events);
                }
            }
            IrEvent::ToolCallStart { id, name } => {
                events.extend(self.ensure_started());
                self.close_open_tool(&mut events);
                let output_index = self.allocate_output_index();
                let item_id = format!("fc_{id}");
                self.tool = Some(ResponsesToolState {
                    output_index,
                    item_id: item_id.clone(),
                    call_id: id.clone(),
                    name: name.clone(),
                    arguments: String::new(),
                    ended: false,
                });
                self.stop_reason = StopReason::ToolCalls;
                events.push(BridgeEvent::OutputItemAdded {
                    output_index,
                    item: json!({
                        "id": item_id,
                        "type": "function_call",
                        "status": "in_progress",
                        "call_id": id,
                        "name": name,
                        "arguments": "",
                    }),
                    sequence_number: self.next_sequence_number(),
                });
            }
            IrEvent::ToolCallDelta {
                id,
                arguments_delta,
            } => {
                events.extend(self.ensure_started());
                if arguments_delta.is_empty() {
                    return Ok(events);
                }
                let delta = self.tool.as_mut().and_then(|tool| {
                    if tool.call_id == *id || tool.call_id.is_empty() {
                        tool.arguments.push_str(arguments_delta);
                        Some((
                            tool.output_index,
                            tool.item_id.clone(),
                            tool.call_id.clone(),
                        ))
                    } else {
                        None
                    }
                });
                if let Some((output_index, item_id, call_id)) = delta {
                    events.push(BridgeEvent::FunctionCallArgumentsDelta {
                        output_index,
                        item_id,
                        call_id,
                        delta: arguments_delta.clone(),
                        sequence_number: self.next_sequence_number(),
                    });
                }
            }
            IrEvent::ToolCallEnd { id } => {
                events.extend(self.ensure_started());
                let done = self.tool.as_mut().and_then(|tool| {
                    if tool.call_id == *id && !tool.ended {
                        tool.ended = true;
                        Some(tool.clone())
                    } else {
                        None
                    }
                });
                if let Some(tool) = done {
                    events.push(BridgeEvent::FunctionCallArgumentsDone {
                        output_index: tool.output_index,
                        item_id: tool.item_id.clone(),
                        call_id: tool.call_id.clone(),
                        arguments: tool.arguments.clone(),
                        sequence_number: self.next_sequence_number(),
                    });
                    events.push(BridgeEvent::OutputItemDone {
                        output_index: tool.output_index,
                        item: json!({
                            "id": tool.item_id,
                            "type": "function_call",
                            "status": "completed",
                            "call_id": tool.call_id,
                            "name": tool.name,
                            "arguments": tool.arguments,
                        }),
                        sequence_number: self.next_sequence_number(),
                    });
                }
            }
            IrEvent::Usage {
                input_tokens,
                output_tokens,
                cached_input_tokens,
            } => {
                self.usage = Some(Usage {
                    input_tokens: *input_tokens,
                    output_tokens: *output_tokens,
                    total_tokens: input_tokens.saturating_add(*output_tokens),
                    cached_input_tokens: *cached_input_tokens,
                    reasoning_tokens: 0,
                });
            }
            IrEvent::MessageEnd { stop_reason } => {
                events.extend(self.ensure_started());
                self.stop_reason = stop_reason.clone();
                events.extend(self.complete());
            }
            IrEvent::Error { .. } => {
                self.completed = true;
                events.push(BridgeEvent::Error {
                    code: "upstream_error",
                    message: "The upstream model provider returned an error.".to_owned(),
                    sequence_number: self.next_sequence_number(),
                });
            }
        }
        Ok(events)
    }

    pub fn finish(&mut self) -> Vec<BridgeEvent> {
        if self.completed {
            Vec::new()
        } else {
            let mut events = self.ensure_started();
            events.extend(self.complete());
            events
        }
    }

    fn ensure_started(&mut self) -> Vec<BridgeEvent> {
        if self.started {
            return Vec::new();
        }
        self.started = true;
        let response = responses_object(
            &self.response_id,
            &self.model,
            0,
            Vec::new(),
            StopReason::Unknown,
            None,
            false,
        );
        vec![
            BridgeEvent::ResponseStarted {
                response: response.clone(),
                sequence_number: self.next_sequence_number(),
            },
            BridgeEvent::ResponseInProgress {
                response,
                sequence_number: self.next_sequence_number(),
            },
        ]
    }

    fn append_text_delta(&mut self, delta: &str, events: &mut Vec<BridgeEvent>) {
        if self.message.is_none() {
            let index = self.allocate_output_index();
            let id = format!("msg_{}", self.response_id);
            self.message = Some(ResponsesMessageState {
                id: id.clone(),
                output_index: index,
                text: String::new(),
                content_added: false,
            });
            events.push(BridgeEvent::OutputItemAdded {
                output_index: index,
                item: json!({
                    "id": id,
                    "type": "message",
                    "status": "in_progress",
                    "role": "assistant",
                    "content": [{
                        "type": "output_text",
                        "text": "",
                        "annotations": [],
                    }],
                }),
                sequence_number: self.next_sequence_number(),
            });
        }
        let (output_index, item_id, content_added) = {
            let message = self.message.as_mut().expect("message was just initialized");
            let content_added = !message.content_added;
            if content_added {
                message.content_added = true;
            }
            message.text.push_str(delta);
            (message.output_index, message.id.clone(), content_added)
        };
        if content_added {
            events.push(BridgeEvent::ContentPartAdded {
                output_index,
                item_id: item_id.clone(),
                content_index: 0,
                part: json!({
                    "type": "output_text",
                    "text": "",
                    "annotations": [],
                }),
                sequence_number: self.next_sequence_number(),
            });
        }
        events.push(BridgeEvent::TextDelta {
            output_index,
            item_id,
            content_index: 0,
            delta: delta.to_owned(),
            sequence_number: self.next_sequence_number(),
        });
    }

    fn close_open_tool(&mut self, events: &mut Vec<BridgeEvent>) {
        let done = self.tool.as_mut().and_then(|tool| {
            if !tool.ended {
                tool.ended = true;
                Some(tool.clone())
            } else {
                None
            }
        });
        if let Some(tool) = done {
            events.push(BridgeEvent::FunctionCallArgumentsDone {
                output_index: tool.output_index,
                item_id: tool.item_id.clone(),
                call_id: tool.call_id.clone(),
                arguments: tool.arguments.clone(),
                sequence_number: self.next_sequence_number(),
            });
            events.push(BridgeEvent::OutputItemDone {
                output_index: tool.output_index,
                item: json!({
                    "id": tool.item_id,
                    "type": "function_call",
                    "status": "completed",
                    "call_id": tool.call_id,
                    "name": tool.name,
                    "arguments": tool.arguments,
                }),
                sequence_number: self.next_sequence_number(),
            });
        }
        self.tool = None;
    }

    fn complete(&mut self) -> Vec<BridgeEvent> {
        self.completed = true;
        let mut events = Vec::new();
        if let Some(message) = self.message.clone() {
            if message.content_added {
                events.push(BridgeEvent::TextDone {
                    output_index: message.output_index,
                    item_id: message.id.clone(),
                    content_index: 0,
                    text: message.text.clone(),
                    sequence_number: self.next_sequence_number(),
                });
                events.push(BridgeEvent::ContentPartDone {
                    output_index: message.output_index,
                    item_id: message.id.clone(),
                    content_index: 0,
                    part: json!({
                        "type": "output_text",
                        "text": message.text,
                        "annotations": [],
                    }),
                    sequence_number: self.next_sequence_number(),
                });
            }
            events.push(BridgeEvent::OutputItemDone {
                output_index: message.output_index,
                item: responses_message_item(&message.id, &message.text),
                sequence_number: self.next_sequence_number(),
            });
        }
        self.close_open_tool(&mut events);
        let incomplete = matches!(
            &self.stop_reason,
            StopReason::Length | StopReason::ContentFilter
        );
        let response = responses_object(
            &self.response_id,
            &self.model,
            0,
            self.output_items(),
            self.stop_reason.clone(),
            self.usage.clone(),
            true,
        );
        let sequence_number = self.next_sequence_number();
        events.push(if incomplete {
            BridgeEvent::Incomplete {
                response,
                sequence_number,
            }
        } else {
            BridgeEvent::Completed {
                response,
                sequence_number,
            }
        });
        events
    }

    fn output_items(&self) -> Vec<Value> {
        let mut output = Vec::new();
        if let Some(message) = &self.message {
            output.push((
                message.output_index,
                responses_message_item(&message.id, &message.text),
            ));
        }
        if let Some(tool) = &self.tool {
            output.push((
                tool.output_index,
                responses_function_call_item(&tool.call_id, &tool.name, &tool.arguments),
            ));
        }
        output.sort_by_key(|(index, _)| *index);
        output.into_iter().map(|(_, item)| item).collect()
    }

    fn allocate_output_index(&mut self) -> usize {
        let index = self.next_output_index;
        self.next_output_index += 1;
        index
    }

    fn next_sequence_number(&mut self) -> u64 {
        let sequence_number = self.next_sequence_number;
        self.next_sequence_number = self.next_sequence_number.saturating_add(1);
        sequence_number
    }
}

fn responses_object(
    id: &str,
    model: &str,
    created_at: u64,
    output: Vec<Value>,
    stop_reason: StopReason,
    usage: Option<Usage>,
    completed: bool,
) -> Value {
    let mut response = Map::new();
    response.insert("id".to_owned(), Value::String(id.to_owned()));
    response.insert("object".to_owned(), Value::String("response".to_owned()));
    response.insert("created_at".to_owned(), Value::from(created_at));
    response.insert("model".to_owned(), Value::String(model.to_owned()));
    response.insert("output".to_owned(), Value::Array(output));
    response.insert(
        "status".to_owned(),
        Value::String(
            if completed && matches!(stop_reason, StopReason::Length | StopReason::ContentFilter) {
                "incomplete".to_owned()
            } else if completed {
                "completed".to_owned()
            } else {
                "in_progress".to_owned()
            },
        ),
    );
    response.insert(
        "incomplete_details".to_owned(),
        if completed && matches!(stop_reason, StopReason::Length | StopReason::ContentFilter) {
            json!({
                "reason": if matches!(stop_reason, StopReason::ContentFilter) {
                    "content_filter"
                } else {
                    "max_output_tokens"
                }
            })
        } else {
            Value::Null
        },
    );
    response.insert(
        "usage".to_owned(),
        if completed {
            Usage::completed_responses_json(usage.as_ref())
        } else {
            usage
                .map(|usage| usage.to_responses_json())
                .unwrap_or(Value::Null)
        },
    );
    Value::Object(response)
}

fn responses_message_item(id: &str, text: &str) -> Value {
    json!({
        "id": id,
        "type": "message",
        "status": "completed",
        "role": "assistant",
        "content": [{
            "type": "output_text",
            "text": text,
            "annotations": [],
        }],
    })
}

fn responses_function_call_item(call_id: &str, name: &str, arguments: &str) -> Value {
    json!({
        "id": format!("fc_{call_id}"),
        "type": "function_call",
        "status": "completed",
        "call_id": call_id,
        "name": name,
        "arguments": arguments,
    })
}

/// Convert a completed non-streaming Responses object into neutral [`IrEvent`]s.
pub fn responses_output_to_ir(value: &Value) -> ProtocolResult<Vec<IrEvent>> {
    let object = value
        .as_object()
        .ok_or_else(|| ProtocolError::invalid_request("Responses body must be a JSON object."))?;
    if object.get("error").is_some() {
        return Ok(vec![IrEvent::Error {
            code: "upstream_error".to_owned(),
            message: "The upstream model provider returned an error.".to_owned(),
            retryable: false,
        }]);
    }

    let id = object
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("resp_agenthub")
        .to_owned();
    let model = object
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_owned();

    let mut events = vec![IrEvent::MessageStart {
        id: format!("msg_{id}"),
        model,
    }];

    let output = object
        .get("output")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            ProtocolError::invalid_request("Responses body requires an output array.")
        })?;

    let mut stop_reason = stop_reason_from_responses(object);
    for item in output {
        let item_object = item.as_object().ok_or_else(|| {
            ProtocolError::invalid_request("Each Responses output item must be an object.")
        })?;
        match item_object.get("type").and_then(Value::as_str) {
            Some("message") => {
                let content = item_object
                    .get("content")
                    .and_then(Value::as_array)
                    .ok_or_else(|| {
                        ProtocolError::invalid_request(
                            "Responses message items require a content array.",
                        )
                    })?;
                for part in content {
                    let part_object = part.as_object().ok_or_else(|| {
                        ProtocolError::invalid_request(
                            "Each Responses message content part must be an object.",
                        )
                    })?;
                    match part_object.get("type").and_then(Value::as_str) {
                        Some("output_text") | Some("text") => {
                            let text = part_object
                                .get("text")
                                .and_then(Value::as_str)
                                .unwrap_or_default();
                            if !text.is_empty() {
                                events.push(IrEvent::TextDelta {
                                    text: text.to_owned(),
                                });
                            }
                        }
                        Some("refusal") => {
                            let text = part_object
                                .get("refusal")
                                .or_else(|| part_object.get("text"))
                                .and_then(Value::as_str)
                                .unwrap_or_default();
                            if !text.is_empty() {
                                events.push(IrEvent::TextDelta {
                                    text: text.to_owned(),
                                });
                            }
                            stop_reason = StopReason::ContentFilter;
                        }
                        Some(_) => {
                            return Err(ProtocolError::unsupported(
                                "unsupported_output_content",
                                "This Responses content type is not supported by this bridge.",
                            ));
                        }
                        None => {
                            return Err(ProtocolError::invalid_request(
                                "Each Responses content part requires a type.",
                            ));
                        }
                    }
                }
            }
            Some("function_call") => {
                let call_id = item_object
                    .get("call_id")
                    .or_else(|| item_object.get("id"))
                    .and_then(Value::as_str)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| {
                        ProtocolError::invalid_request("Function call output requires a call_id.")
                    })?;
                let name = item_object
                    .get("name")
                    .and_then(Value::as_str)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| {
                        ProtocolError::invalid_request("Function call output requires a name.")
                    })?;
                let arguments = item_object
                    .get("arguments")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                events.push(IrEvent::ToolCallStart {
                    id: call_id.to_owned(),
                    name: name.to_owned(),
                });
                if !arguments.is_empty() {
                    events.push(IrEvent::ToolCallDelta {
                        id: call_id.to_owned(),
                        arguments_delta: arguments.to_owned(),
                    });
                }
                events.push(IrEvent::ToolCallEnd {
                    id: call_id.to_owned(),
                });
                stop_reason = StopReason::ToolCalls;
            }
            Some("reasoning") => {
                // Encrypted/opaque reasoning is not mapped across protocol surfaces.
                continue;
            }
            Some(_) => {
                return Err(ProtocolError::unsupported(
                    "unsupported_output",
                    "This Responses output item type is not supported by this bridge.",
                ));
            }
            None => {
                return Err(ProtocolError::invalid_request(
                    "Each Responses output item requires a type.",
                ));
            }
        }
    }

    if let Some(usage) = object.get("usage").and_then(Usage::from_responses_usage) {
        events.push(IrEvent::Usage {
            input_tokens: usage.input_tokens,
            output_tokens: usage.output_tokens,
            cached_input_tokens: usage.cached_input_tokens,
        });
    }

    events.push(IrEvent::MessageEnd { stop_reason });
    Ok(events)
}

/// Stateful Responses SSE → IR translator for the Codex upstream stream.
///
/// Feed each decoded `data:` JSON object to [`Self::push_event`]. Call [`Self::finish`]
/// if the upstream closes without a terminal completed/incomplete/failed event.
#[derive(Debug, Default)]
pub struct ResponsesStreamToIr {
    started: bool,
    completed: bool,
    message_id: String,
    model: String,
    open_tool: Option<OpenTool>,
    stop_reason: StopReason,
    usage: Option<Usage>,
    pending: Vec<IrEvent>,
}

#[derive(Debug, Clone)]
struct OpenTool {
    id: String,
    arguments: String,
    ended: bool,
}

impl ResponsesStreamToIr {
    pub fn new() -> Self {
        Self {
            message_id: "msg_agenthub".to_owned(),
            model: "unknown".to_owned(),
            stop_reason: StopReason::Stop,
            ..Self::default()
        }
    }

    pub fn push_event(&mut self, value: &Value) -> ProtocolResult<Vec<IrEvent>> {
        if self.completed {
            return Ok(Vec::new());
        }
        let object = value.as_object().ok_or_else(|| {
            ProtocolError::invalid_request("Each Responses SSE event must be a JSON object.")
        })?;
        let event_type = object
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();

        match event_type {
            "response.created" | "response.in_progress" => {
                if let Some(response) = object.get("response").and_then(Value::as_object) {
                    if let Some(id) = response.get("id").and_then(Value::as_str) {
                        self.message_id = format!("msg_{id}");
                    }
                    if let Some(model) = response.get("model").and_then(Value::as_str) {
                        self.model = model.to_owned();
                    }
                }
                self.ensure_message_start();
            }
            "response.output_text.delta" => {
                self.ensure_message_start();
                if let Some(delta) = object.get("delta").and_then(Value::as_str) {
                    if !delta.is_empty() {
                        self.pending.push(IrEvent::TextDelta {
                            text: delta.to_owned(),
                        });
                    }
                }
            }
            "response.output_text.done" => {
                self.ensure_message_start();
                // TextDone is redundant when deltas were already emitted.
            }
            "response.output_item.added" => {
                self.ensure_message_start();
                if let Some(item) = object.get("item").and_then(Value::as_object) {
                    if item.get("type").and_then(Value::as_str) == Some("function_call") {
                        self.close_open_tool();
                        let call_id = item
                            .get("call_id")
                            .or_else(|| item.get("id"))
                            .and_then(Value::as_str)
                            .filter(|value| !value.is_empty())
                            .unwrap_or("call_0");
                        let name = item
                            .get("name")
                            .and_then(Value::as_str)
                            .filter(|value| !value.is_empty())
                            .unwrap_or("tool");
                        self.open_tool = Some(OpenTool {
                            id: call_id.to_owned(),
                            arguments: item
                                .get("arguments")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_owned(),
                            ended: false,
                        });
                        self.pending.push(IrEvent::ToolCallStart {
                            id: call_id.to_owned(),
                            name: name.to_owned(),
                        });
                        self.stop_reason = StopReason::ToolCalls;
                    }
                }
            }
            "response.function_call_arguments.delta" => {
                self.ensure_message_start();
                let delta = object
                    .get("delta")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let call_id = object
                    .get("call_id")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned)
                    .or_else(|| self.open_tool.as_ref().map(|tool| tool.id.clone()))
                    .unwrap_or_else(|| "call_0".to_owned());
                if let Some(tool) = &mut self.open_tool {
                    if tool.id == call_id {
                        tool.arguments.push_str(delta);
                    }
                }
                if !delta.is_empty() {
                    self.pending.push(IrEvent::ToolCallDelta {
                        id: call_id,
                        arguments_delta: delta.to_owned(),
                    });
                }
            }
            "response.function_call_arguments.done" => {
                self.ensure_message_start();
                let call_id = object
                    .get("call_id")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned)
                    .or_else(|| self.open_tool.as_ref().map(|tool| tool.id.clone()));
                if let Some(call_id) = call_id {
                    if let Some(tool) = &mut self.open_tool {
                        if tool.id == call_id {
                            if let Some(arguments) = object.get("arguments").and_then(Value::as_str)
                            {
                                if tool.arguments.is_empty() && !arguments.is_empty() {
                                    self.pending.push(IrEvent::ToolCallDelta {
                                        id: call_id.clone(),
                                        arguments_delta: arguments.to_owned(),
                                    });
                                    tool.arguments = arguments.to_owned();
                                }
                            }
                            if !tool.ended {
                                tool.ended = true;
                                self.pending.push(IrEvent::ToolCallEnd { id: call_id });
                            }
                        }
                    } else {
                        self.pending.push(IrEvent::ToolCallEnd { id: call_id });
                    }
                }
            }
            "response.output_item.done" => {
                self.ensure_message_start();
                if let Some(item) = object.get("item").and_then(Value::as_object) {
                    if item.get("type").and_then(Value::as_str) == Some("function_call") {
                        let call_id = item
                            .get("call_id")
                            .or_else(|| item.get("id"))
                            .and_then(Value::as_str)
                            .unwrap_or("call_0");
                        if let Some(tool) = &mut self.open_tool {
                            if tool.id == call_id && !tool.ended {
                                if tool.arguments.is_empty() {
                                    if let Some(arguments) =
                                        item.get("arguments").and_then(Value::as_str)
                                    {
                                        if !arguments.is_empty() {
                                            self.pending.push(IrEvent::ToolCallDelta {
                                                id: call_id.to_owned(),
                                                arguments_delta: arguments.to_owned(),
                                            });
                                            tool.arguments = arguments.to_owned();
                                        }
                                    }
                                }
                                tool.ended = true;
                                self.pending.push(IrEvent::ToolCallEnd {
                                    id: call_id.to_owned(),
                                });
                            }
                        }
                        self.open_tool = None;
                        self.stop_reason = StopReason::ToolCalls;
                    }
                }
            }
            "response.completed" | "response.incomplete" => {
                self.ensure_message_start();
                self.close_open_tool();
                if let Some(response) = object.get("response").and_then(Value::as_object) {
                    if let Some(usage) = response.get("usage").and_then(Usage::from_responses_usage)
                    {
                        self.usage = Some(usage);
                    }
                    self.stop_reason = stop_reason_from_responses(response);
                }
                self.push_terminal();
            }
            "response.failed" | "error" => {
                self.completed = true;
                // Never surface upstream error bodies: they may contain prompts,
                // credentials fragments, or private request identifiers.
                self.pending.push(IrEvent::Error {
                    code: "upstream_error".to_owned(),
                    message: "The upstream model provider returned an error.".to_owned(),
                    retryable: false,
                });
            }
            _ => {
                // Ignore unknown lifecycle events rather than failing the whole stream.
            }
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

fn stop_reason_from_responses(object: &Map<String, Value>) -> StopReason {
    if let Some(reason) = object
        .get("incomplete_details")
        .and_then(Value::as_object)
        .and_then(|details| details.get("reason"))
        .and_then(Value::as_str)
    {
        return match reason {
            "max_output_tokens" => StopReason::Length,
            "content_filter" => StopReason::ContentFilter,
            _ => StopReason::Unknown,
        };
    }
    match object.get("status").and_then(Value::as_str) {
        Some("incomplete") => StopReason::Length,
        Some("failed") | Some("cancelled") => StopReason::Unknown,
        _ => {
            // Prefer tool stop when any function_call is present.
            if object
                .get("output")
                .and_then(Value::as_array)
                .is_some_and(|items| {
                    items.iter().any(|item| {
                        item.get("type").and_then(Value::as_str) == Some("function_call")
                    })
                })
            {
                StopReason::ToolCalls
            } else {
                StopReason::Stop
            }
        }
    }
}

fn append_kimi_messages(messages: &mut Vec<Value>, message: &BridgeMessage) {
    let text = message
        .content
        .iter()
        .filter_map(|content| match content {
            BridgeContent::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<String>();
    let tool_results = message
        .content
        .iter()
        .filter_map(|content| match content {
            BridgeContent::ToolResult { call_id, output } => Some((call_id, output)),
            _ => None,
        })
        .collect::<Vec<_>>();

    if !tool_results.is_empty() {
        for (call_id, output) in tool_results {
            messages.push(json!({
                "role": "tool",
                "tool_call_id": call_id,
                "content": output,
            }));
        }
        return;
    }

    let tool_calls = message
        .content
        .iter()
        .filter_map(|content| match content {
            BridgeContent::ToolCall {
                id,
                name,
                arguments,
                ..
            } => Some(json!({
                "id": id,
                "type": "function",
                "function": { "name": name, "arguments": arguments },
            })),
            _ => None,
        })
        .collect::<Vec<_>>();

    let role = match &message.role {
        // Kimi's Chat endpoint uses the Chat Completions role set; Responses' `developer`
        // instructions have equivalent highest-priority message semantics here.
        MessageRole::Developer => "system",
        _ => message.role.as_str(),
    };
    let mut output = Map::new();
    output.insert("role".to_owned(), Value::String(role.to_owned()));
    if let Some(name) = &message.name {
        output.insert("name".to_owned(), Value::String(name.clone()));
    }
    output.insert(
        "content".to_owned(),
        if text.is_empty() && !tool_calls.is_empty() {
            Value::Null
        } else {
            Value::String(text)
        },
    );
    if !tool_calls.is_empty() {
        output.insert("tool_calls".to_owned(), Value::Array(tool_calls));
    }
    messages.push(Value::Object(output));
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

fn render_tool_choice(choice: &ToolChoice) -> Value {
    match choice {
        ToolChoice::Auto => Value::String("auto".to_owned()),
        ToolChoice::None => Value::String("none".to_owned()),
        ToolChoice::Required => Value::String("required".to_owned()),
        ToolChoice::Function { name } => json!({
            "type": "function",
            "function": { "name": name },
        }),
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
fn grok_reasoning_effort(value: Option<&Value>) -> Option<String> {
    let effort = match value? {
        Value::Object(object) => object.get("effort").and_then(Value::as_str)?,
        Value::String(effort) => effort.as_str(),
        _ => return None,
    };
    matches!(effort, "low" | "medium" | "high" | "xhigh").then(|| effort.to_owned())
}
