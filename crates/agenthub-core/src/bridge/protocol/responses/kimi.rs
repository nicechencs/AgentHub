//! Kimi Chat Completions (and Grok Chat) vendor policy.
//!
//! [`MessageRole::Developer`] renders as Chat `"system"`. `instructions` become
//! one system message. Streaming requests set `stream_options.include_usage`.

use serde_json::{json, Map, Value};

use crate::bridge::types::{BridgeContent, BridgeMessage, BridgeRequest, MessageRole, ToolChoice};

use super::codex::to_responses_request;
use super::parse::grok_reasoning_effort;

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

/// Same Responses shape as [`to_responses_request`], plus Grok `reasoning` when
/// passthrough has a mappable `reasoning_effort`. Codex / Kimi keep using
/// [`to_responses_request`] so they never receive this object.
pub fn to_grok_responses_request(request: &BridgeRequest) -> Value {
    let mut body = to_responses_request(request);
    let Some(object) = body.as_object_mut() else {
        return body;
    };

    if let Some(effort) = grok_reasoning_effort(request.passthrough.get("reasoning_effort")) {
        object.insert(
            "reasoning".to_owned(),
            json!({ "effort": effort, "summary": "detailed" }),
        );
        let include_item = "reasoning.encrypted_content";
        if let Some(Value::Array(items)) = object.get_mut("include") {
            if !items.iter().any(|item| item.as_str() == Some(include_item)) {
                items.push(Value::String(include_item.to_owned()));
            }
        } else {
            object.insert("include".to_owned(), json!([include_item]));
        }
    }

    if !object.contains_key("prompt_cache_key") {
        if let Some(key) = request
            .passthrough
            .get("prompt_cache_key")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
        {
            object.insert("prompt_cache_key".to_owned(), Value::String(key.to_owned()));
        }
    }

    body
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
