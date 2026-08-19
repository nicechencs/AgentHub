//! Kimi Chat Completions response and SSE translation to OpenAI Responses wire objects.

use std::collections::BTreeMap;

use serde_json::{json, Map, Value};

use crate::bridge::types::{
    BridgeEvent, IrEvent, ProtocolError, ProtocolResult, StopReason, ToolCallMap, Usage,
};

/// Translate one non-streaming Kimi Chat Completions response to an OpenAI Responses object.
pub fn translate_chat_response(value: &Value, response_id: Option<&str>) -> ProtocolResult<Value> {
    reject_upstream_error(value)?;
    let object = value.as_object().ok_or_else(ProtocolError::upstream)?;
    let choice = object
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .and_then(Value::as_object)
        .ok_or_else(ProtocolError::upstream)?;
    let message = choice
        .get("message")
        .and_then(Value::as_object)
        .ok_or_else(ProtocolError::upstream)?;
    let id = response_id
        .map(ToOwned::to_owned)
        .or_else(|| {
            object
                .get("id")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
        })
        .unwrap_or_else(|| "resp_agenthub".to_owned());
    let model = object
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_owned();
    let created_at = object
        .get("created")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    let stop_reason =
        StopReason::from_chat_finish_reason(choice.get("finish_reason").and_then(Value::as_str));
    let mut output = Vec::new();
    if let Some(text) = message.get("content").and_then(Value::as_str) {
        output.push(message_item(&message_id(&id), text));
    }
    if let Some(tool_calls) = message.get("tool_calls").and_then(Value::as_array) {
        for (index, tool_call) in tool_calls.iter().enumerate() {
            output.push(function_call_item(tool_call, index)?);
        }
    } else if let Some(function_call) = message.get("function_call") {
        // Legacy OpenAI-compatible providers may still return a singular function_call.
        output.push(legacy_function_call_item(function_call)?);
    }

    Ok(response_object(
        &id,
        &model,
        created_at,
        output,
        stop_reason,
        object.get("usage").and_then(Usage::from_chat_usage),
        true,
    ))
}

/// Stateful Chat Completions SSE → neutral IR translation for a Claude
/// Messages downstream. It deliberately shares the same upstream Chat shape
/// as the existing Kimi → Codex bridge but emits IR directly.
#[derive(Debug)]
pub struct ChatStreamToIr {
    response_id: String,
    model: String,
    started: bool,
    completed: bool,
    tools: BTreeMap<usize, ChatIrToolState>,
    usage: Option<Usage>,
    stop_reason: StopReason,
}

#[derive(Debug, Clone)]
struct ChatIrToolState {
    id: String,
    name: String,
}

impl ChatStreamToIr {
    pub fn new(response_id: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            response_id: response_id.into(),
            model: model.into(),
            started: false,
            completed: false,
            tools: BTreeMap::new(),
            usage: None,
            stop_reason: StopReason::Unknown,
        }
    }

    pub fn push_event(&mut self, value: &Value) -> ProtocolResult<Vec<IrEvent>> {
        reject_upstream_error(value)?;
        if self.completed {
            return Ok(Vec::new());
        }
        let object = value.as_object().ok_or_else(ProtocolError::upstream)?;
        if let Some(id) = object.get("id").and_then(Value::as_str) {
            if !id.is_empty() {
                self.response_id = id.to_owned();
            }
        }
        if let Some(model) = object.get("model").and_then(Value::as_str) {
            if !model.is_empty() {
                self.model = model.to_owned();
            }
        }
        if let Some(usage) = object.get("usage").and_then(Usage::from_chat_usage) {
            self.usage = Some(usage);
        }

        let mut events = Vec::new();
        if !self.started {
            self.started = true;
            events.push(IrEvent::MessageStart {
                id: format!("msg_{}", self.response_id),
                model: self.model.clone(),
            });
        }
        let Some(choice) = object
            .get("choices")
            .and_then(Value::as_array)
            .and_then(|choices| choices.first())
            .and_then(Value::as_object)
        else {
            return Ok(events);
        };
        if let Some(delta) = choice.get("delta").and_then(Value::as_object) {
            if let Some(text) = delta.get("content").and_then(Value::as_str) {
                if !text.is_empty() {
                    events.push(IrEvent::TextDelta {
                        text: text.to_owned(),
                    });
                }
            }
            if let Some(tool_calls) = delta.get("tool_calls").and_then(Value::as_array) {
                for (position, raw_tool) in tool_calls.iter().enumerate() {
                    self.push_tool_delta(raw_tool, position, &mut events)?;
                }
            }
            if let Some(function_call) = delta.get("function_call") {
                self.push_tool_delta(
                    &json!({ "index": 0, "function": function_call }),
                    0,
                    &mut events,
                )?;
            }
        }
        if let Some(reason) = choice.get("finish_reason").and_then(Value::as_str) {
            self.stop_reason = StopReason::from_chat_finish_reason(Some(reason));
        }
        Ok(events)
    }

    pub fn finish(&mut self) -> Vec<IrEvent> {
        if self.completed {
            return Vec::new();
        }
        self.completed = true;
        let mut events = Vec::new();
        if !self.started {
            self.started = true;
            events.push(IrEvent::MessageStart {
                id: format!("msg_{}", self.response_id),
                model: self.model.clone(),
            });
        }
        for tool in self.tools.values() {
            events.push(IrEvent::ToolCallEnd {
                id: tool.id.clone(),
            });
        }
        if let Some(usage) = &self.usage {
            events.push(IrEvent::Usage {
                input_tokens: usage.input_tokens,
                output_tokens: usage.output_tokens,
                cached_input_tokens: usage.cached_input_tokens,
            });
        }
        events.push(IrEvent::MessageEnd {
            stop_reason: self.stop_reason.clone(),
        });
        events
    }

    fn push_tool_delta(
        &mut self,
        raw_tool: &Value,
        fallback_index: usize,
        events: &mut Vec<IrEvent>,
    ) -> ProtocolResult<()> {
        let object = raw_tool.as_object().ok_or_else(ProtocolError::upstream)?;
        let index = object
            .get("index")
            .and_then(Value::as_u64)
            .map(|value| value as usize)
            .unwrap_or(fallback_index);
        let function = object.get("function").and_then(Value::as_object);
        let name = function
            .and_then(|function| function.get("name"))
            .and_then(Value::as_str)
            .unwrap_or_default();
        let arguments = function
            .and_then(|function| function.get("arguments"))
            .and_then(Value::as_str)
            .unwrap_or_default();
        if !self.tools.contains_key(&index) {
            let id = object
                .get("id")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .unwrap_or("")
                .to_owned();
            let id = if id.is_empty() {
                format!("call_{index}")
            } else {
                id
            };
            self.tools.insert(
                index,
                ChatIrToolState {
                    id: id.clone(),
                    name: name.to_owned(),
                },
            );
            events.push(IrEvent::ToolCallStart {
                id,
                name: name.to_owned(),
            });
        } else if !name.is_empty() {
            self.tools.get_mut(&index).expect("tool state exists").name = name.to_owned();
        }
        if !arguments.is_empty() {
            let id = self
                .tools
                .get(&index)
                .expect("tool state exists")
                .id
                .clone();
            events.push(IrEvent::ToolCallDelta {
                id,
                arguments_delta: arguments.to_owned(),
            });
        }
        Ok(())
    }
}

/// Stateful Kimi Chat Completions SSE chunk translator.
///
/// Feed each decoded `data:` JSON object to [`Self::push_chunk`].  At upstream `[DONE]`,
/// call [`Self::finish`] to close an otherwise unterminated response.
#[derive(Debug)]
pub struct ResponsesSseTranslator {
    response_id: String,
    model: String,
    created_at: u64,
    started: bool,
    completed: bool,
    message: Option<MessageState>,
    tool_calls: ToolCallMap<ToolCallState>,
    next_output_index: usize,
    next_sequence_number: u64,
    usage: Option<Usage>,
    stop_reason: StopReason,
}

impl ResponsesSseTranslator {
    /// `response_id` is supplied by the bridge host when it has already allocated one.
    pub fn new(response_id: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            response_id: response_id.into(),
            model: model.into(),
            created_at: 0,
            started: false,
            completed: false,
            message: None,
            tool_calls: ToolCallMap::new(),
            next_output_index: 0,
            next_sequence_number: 0,
            usage: None,
            stop_reason: StopReason::Unknown,
        }
    }

    /// Convert a single decoded upstream chunk to one or more Responses events.
    pub fn push_chunk(&mut self, chunk: &Value) -> ProtocolResult<Vec<BridgeEvent>> {
        reject_upstream_error(chunk)?;
        if self.completed {
            return Ok(Vec::new());
        }
        let object = chunk.as_object().ok_or_else(ProtocolError::upstream)?;
        if let Some(created) = object.get("created").and_then(Value::as_u64) {
            self.created_at = created;
        }
        if let Some(model) = object.get("model").and_then(Value::as_str) {
            self.model = model.to_owned();
        }
        if let Some(usage) = object.get("usage").and_then(Usage::from_chat_usage) {
            self.usage = Some(usage);
        }

        let mut events = self.ensure_started();
        let choices = object
            .get("choices")
            .and_then(Value::as_array)
            .map(|choices| choices.as_slice())
            .unwrap_or_default();
        let Some(choice) = choices.first().and_then(Value::as_object) else {
            return Ok(events);
        };

        if let Some(delta) = choice.get("delta").and_then(Value::as_object) {
            if let Some(content) = delta.get("content").and_then(Value::as_str) {
                if !content.is_empty() {
                    self.append_text_delta(content, &mut events);
                }
            }
            if let Some(tool_calls) = delta.get("tool_calls").and_then(Value::as_array) {
                for (position, tool_call) in tool_calls.iter().enumerate() {
                    self.append_tool_delta(tool_call, position, &mut events)?;
                }
            }
            if let Some(function_call) = delta.get("function_call") {
                self.append_legacy_tool_delta(function_call, &mut events)?;
            }
        }
        if let Some(finish_reason) = choice.get("finish_reason").and_then(Value::as_str) {
            self.stop_reason = StopReason::from_chat_finish_reason(Some(finish_reason));
        }
        Ok(events)
    }

    /// Emit terminal events at upstream `[DONE]`.  It is idempotent.
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
        let response = response_object(
            &self.response_id,
            &self.model,
            self.created_at,
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
            let id = message_id(&self.response_id);
            self.message = Some(MessageState {
                id: id.clone(),
                output_index: index,
                text: String::new(),
                content_added: false,
            });
            events.push(BridgeEvent::OutputItemAdded {
                output_index: index,
                item: message_item_with_status(&id, "", "in_progress"),
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
                part: text_part(""),
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

    fn append_tool_delta(
        &mut self,
        raw_tool_call: &Value,
        fallback_index: usize,
        events: &mut Vec<BridgeEvent>,
    ) -> ProtocolResult<()> {
        let object = raw_tool_call
            .as_object()
            .ok_or_else(ProtocolError::upstream)?;
        let source_index = object
            .get("index")
            .and_then(Value::as_u64)
            .map(|value| value as usize)
            .unwrap_or(fallback_index);
        let id = object.get("id").and_then(Value::as_str);
        let function = object.get("function").and_then(Value::as_object);
        let name = function
            .and_then(|function| function.get("name"))
            .and_then(Value::as_str);
        let arguments = function
            .and_then(|function| function.get("arguments"))
            .and_then(Value::as_str)
            .unwrap_or("");

        if !self.tool_calls.contains_key(&source_index) {
            let call_id = id
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| format!("call_{source_index}"));
            let output_index = self.allocate_output_index();
            let item_id = function_item_id(&call_id);
            let tool_call = ToolCallState {
                output_index,
                item_id: item_id.clone(),
                call_id: call_id.clone(),
                name: name.unwrap_or_default().to_owned(),
                arguments: String::new(),
            };
            events.push(BridgeEvent::OutputItemAdded {
                output_index,
                item: tool_call.item_with_status("in_progress"),
                sequence_number: self.next_sequence_number(),
            });
            self.tool_calls.insert(source_index, tool_call);
        }
        let argument_delta = {
            let tool_call = self
                .tool_calls
                .get_mut(&source_index)
                .expect("tool call was just initialized");
            if let Some(name) = name.filter(|value| !value.is_empty()) {
                tool_call.name = name.to_owned();
            }
            if arguments.is_empty() {
                None
            } else {
                tool_call.arguments.push_str(arguments);
                Some((
                    tool_call.output_index,
                    tool_call.item_id.clone(),
                    tool_call.call_id.clone(),
                ))
            }
        };
        if let Some((output_index, item_id, call_id)) = argument_delta {
            events.push(BridgeEvent::FunctionCallArgumentsDelta {
                output_index,
                item_id,
                call_id,
                delta: arguments.to_owned(),
                sequence_number: self.next_sequence_number(),
            });
        }
        Ok(())
    }

    fn append_legacy_tool_delta(
        &mut self,
        function_call: &Value,
        events: &mut Vec<BridgeEvent>,
    ) -> ProtocolResult<()> {
        self.append_tool_delta(&json!({ "index": 0, "function": function_call }), 0, events)
    }

    fn complete(&mut self) -> Vec<BridgeEvent> {
        self.completed = true;
        let mut events = Vec::new();
        let mut output = Vec::new();
        if let Some(message) = &self.message {
            output.push(FinalOutput::Message(message.clone()));
        }
        output.extend(self.tool_calls.values().cloned().map(FinalOutput::ToolCall));
        output.sort_by_key(FinalOutput::output_index);

        for item in output {
            match item {
                FinalOutput::Message(message) => {
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
                            part: text_part(&message.text),
                            sequence_number: self.next_sequence_number(),
                        });
                    }
                    events.push(BridgeEvent::OutputItemDone {
                        output_index: message.output_index,
                        item: message_item(&message.id, &message.text),
                        sequence_number: self.next_sequence_number(),
                    });
                }
                FinalOutput::ToolCall(tool_call) => {
                    events.push(BridgeEvent::FunctionCallArgumentsDone {
                        output_index: tool_call.output_index,
                        item_id: tool_call.item_id.clone(),
                        call_id: tool_call.call_id.clone(),
                        arguments: tool_call.arguments.clone(),
                        sequence_number: self.next_sequence_number(),
                    });
                    events.push(BridgeEvent::OutputItemDone {
                        output_index: tool_call.output_index,
                        item: tool_call.item(),
                        sequence_number: self.next_sequence_number(),
                    });
                }
            }
        }
        let incomplete = matches!(
            &self.stop_reason,
            StopReason::Length | StopReason::ContentFilter
        );
        let response = response_object(
            &self.response_id,
            &self.model,
            self.created_at,
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
                message_item(&message.id, &message.text),
            ));
        }
        output.extend(
            self.tool_calls
                .values()
                .map(|tool_call| (tool_call.output_index, tool_call.item())),
        );
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

/// Render one Responses event as a complete SSE record.
pub fn sse_frame(event: &BridgeEvent) -> String {
    format!("event: {}\ndata: {}\n\n", event.event_name(), event.data())
}

#[derive(Debug, Clone)]
struct MessageState {
    id: String,
    output_index: usize,
    text: String,
    content_added: bool,
}

#[derive(Debug, Clone)]
struct ToolCallState {
    output_index: usize,
    item_id: String,
    call_id: String,
    name: String,
    arguments: String,
}

#[derive(Debug)]
enum FinalOutput {
    Message(MessageState),
    ToolCall(ToolCallState),
}

impl FinalOutput {
    fn output_index(&self) -> usize {
        match self {
            Self::Message(message) => message.output_index,
            Self::ToolCall(tool_call) => tool_call.output_index,
        }
    }
}

impl ToolCallState {
    fn item(&self) -> Value {
        self.item_with_status("completed")
    }

    fn item_with_status(&self, status: &str) -> Value {
        json!({
            "id": self.item_id,
            "type": "function_call",
            "status": status,
            "call_id": self.call_id,
            "name": self.name,
            "arguments": self.arguments,
        })
    }
}

fn response_object(
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

fn message_item(id: &str, text: &str) -> Value {
    message_item_with_status(id, text, "completed")
}

fn message_item_with_status(id: &str, text: &str, status: &str) -> Value {
    json!({
        "id": id,
        "type": "message",
        "status": status,
        "role": "assistant",
        "content": [text_part(text)],
    })
}

fn text_part(text: &str) -> Value {
    json!({
        "type": "output_text",
        "text": text,
        "annotations": [],
    })
}

fn function_call_item(value: &Value, index: usize) -> ProtocolResult<Value> {
    let object = value.as_object().ok_or_else(ProtocolError::upstream)?;
    let function = object
        .get("function")
        .and_then(Value::as_object)
        .ok_or_else(ProtocolError::upstream)?;
    let call_id = object
        .get("id")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| format!("call_{index}"));
    let name = function
        .get("name")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(ProtocolError::upstream)?;
    let arguments = function
        .get("arguments")
        .and_then(Value::as_str)
        .unwrap_or_default();
    Ok(json!({
        "id": function_item_id(&call_id),
        "type": "function_call",
        "status": "completed",
        "call_id": call_id,
        "name": name,
        "arguments": arguments,
    }))
}

fn legacy_function_call_item(value: &Value) -> ProtocolResult<Value> {
    let object = value.as_object().ok_or_else(ProtocolError::upstream)?;
    let name = object
        .get("name")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(ProtocolError::upstream)?;
    let arguments = object
        .get("arguments")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let call_id = "call_0";
    Ok(json!({
        "id": function_item_id(call_id),
        "type": "function_call",
        "status": "completed",
        "call_id": call_id,
        "name": name,
        "arguments": arguments,
    }))
}

fn reject_upstream_error(value: &Value) -> ProtocolResult<()> {
    if value.get("error").is_some() {
        return Err(ProtocolError::upstream());
    }
    Ok(())
}

fn message_id(response_id: &str) -> String {
    format!("msg_{response_id}")
}

fn function_item_id(call_id: &str) -> String {
    format!("fc_{call_id}")
}
