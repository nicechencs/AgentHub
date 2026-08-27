//! OpenAI Responses request parsing and vendor request rendering.

mod codex;
mod kimi;
mod parse;

use serde_json::{json, Map, Value};

use crate::bridge::types::{
    BridgeEvent, BridgeRequest, IrEvent, ProtocolError, ProtocolResult, StopReason, Usage,
};

pub use codex::{
    apply_official_codex_model, is_leftover_bridge_model, prepare_official_codex_request,
    to_responses_request,
};
pub use kimi::{to_grok_chat_request, to_grok_responses_request, to_kimi_chat_request};
pub use parse::parse_responses_request;

pub(crate) use codex::fold_official_codex_system_items;
pub(crate) use parse::grok_reasoning_effort_from_thinking;

/// Convenience entry point for HTTP handlers.
pub fn translate_responses_request(value: &Value) -> ProtocolResult<(BridgeRequest, Value)> {
    let request = parse_responses_request(value)?;
    let kimi_request = to_kimi_chat_request(&request);
    Ok((request, kimi_request))
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
