//! In-place Grok Build Responses body normalization (Codex → cli-chat-proxy).

use serde_json::{json, Map, Value};

pub fn normalize_grok_build_tools(body: &mut Value) {
    let Some(Value::Array(tools)) = body.get("tools") else {
        return;
    };

    let mut declared_shell = tools
        .iter()
        .any(|tool| tool.get("type").and_then(Value::as_str) == Some("shell"));

    let mut out = Vec::with_capacity(tools.len());
    for tool in tools {
        match tool.get("type").and_then(Value::as_str) {
            Some("local_shell") => {
                if declared_shell {
                    continue;
                }
                out.push(local_shell_tool());
                declared_shell = true;
            }
            Some("apply_patch") => out.push(apply_patch_function_tool()),
            _ => out.push(tool.clone()),
        }
    }
    body["tools"] = Value::Array(out);
}

pub fn inject_prompt_cache_key(body: &mut Value, seed: Option<&str>) {
    let Some(seed) = seed.map(str::trim).filter(|seed| !seed.is_empty()) else {
        return;
    };
    let Some(obj) = body.as_object_mut() else {
        return;
    };
    let has_key = obj
        .get("prompt_cache_key")
        .and_then(Value::as_str)
        .map(str::trim)
        .is_some_and(|value| !value.is_empty());
    if has_key {
        return;
    }
    obj.insert(
        "prompt_cache_key".to_string(),
        Value::String(seed.to_string()),
    );
}

fn local_shell_tool() -> Value {
    json!({
        "type": "shell",
        "environment": { "type": "local" }
    })
}

fn apply_patch_function_tool() -> Value {
    json!({
        "type": "function",
        "name": "apply_patch",
        "description": "Apply a file change. operation.type is one of create_file, update_file, or delete_file. operation.path is the target path. operation.diff is the patch text for create_file and update_file; use an empty string for delete_file.",
        "strict": true,
        "parameters": {
            "type": "object",
            "properties": {
                "operation": {
                    "type": "object",
                    "properties": {
                        "type": {
                            "type": "string",
                            "enum": ["create_file", "update_file", "delete_file"]
                        },
                        "path": {
                            "type": "string",
                            "minLength": 1
                        },
                        "diff": {
                            "type": "string"
                        }
                    },
                    "required": ["type", "path", "diff"],
                    "additionalProperties": false
                }
            },
            "required": ["operation"],
            "additionalProperties": false
        }
    })
}

/// Tool types the xAI Responses schema accepts. Anything else fails the
/// whole request as `invalid_request`.
const GROK_RESPONSES_TOOL_TYPES: &[&str] = &[
    "function",
    "web_search",
    "x_search",
    "image_generation",
    "file_search",
    "code_interpreter",
    "mcp",
    "shell",
    "tool_search",
];

const GROK_REASONING_SUMMARIES: &[&str] = &["auto", "concise", "detailed"];

/// OpenAI web-search fields xAI documents as "request will be rejected".
const WEB_SEARCH_REJECT_FIELDS: &[&str] = &[
    "external_web_access",
    "search_context_size",
    "user_location",
];

/// Drop Codex Responses fields that xAI rejects, after tool-type normalization.
///
/// `store` booleans stay: xAI accepts them, and a simple responses body must
/// still forward. Non-boolean `store` is not a supported shape.
pub fn sanitize_grok_responses_request(body: &mut Value) {
    let Some(object) = body.as_object_mut() else {
        return;
    };
    object.remove("client_metadata");
    if object.get("store").is_some_and(|value| !value.is_boolean()) {
        object.remove("store");
    }
    sanitize_grok_reasoning(object);
    let kept_names = sanitize_grok_tools(object);
    sanitize_grok_tool_choice(object, kept_names.as_deref());
    sanitize_reasoning_input_items(object);
}

fn sanitize_grok_reasoning(object: &mut Map<String, Value>) {
    let Some(value) = object.get("reasoning") else {
        return;
    };
    if !value.is_object() {
        object.remove("reasoning");
        return;
    }
    let Some(mut reasoning) = object.remove("reasoning") else {
        return;
    };
    let Some(reasoning) = reasoning.as_object_mut() else {
        return;
    };
    reasoning.retain(|key, value| match key.as_str() {
        "effort" => value.is_null() || value.is_string(),
        "summary" | "generate_summary" => {
            value.is_null()
                || value
                    .as_str()
                    .is_some_and(|summary| GROK_REASONING_SUMMARIES.contains(&summary))
        }
        _ => false,
    });
    if !reasoning.is_empty() {
        object.insert("reasoning".to_string(), Value::Object(reasoning.clone()));
    }
}

fn sanitize_grok_tools(object: &mut Map<String, Value>) -> Option<Vec<String>> {
    let Some(Value::Array(tools)) = object.get_mut("tools") else {
        return None;
    };
    tools.retain(|tool| {
        tool.get("type")
            .and_then(Value::as_str)
            .is_some_and(|ty| GROK_RESPONSES_TOOL_TYPES.contains(&ty))
    });
    let mut kept_names = Vec::new();
    for tool in tools.iter_mut() {
        let Some(tool_obj) = tool.as_object_mut() else {
            continue;
        };
        if tool_obj.get("type").and_then(Value::as_str) == Some("web_search") {
            for key in WEB_SEARCH_REJECT_FIELDS {
                tool_obj.remove(*key);
            }
        }
        if let Some(name) = tool_obj.get("name").and_then(Value::as_str) {
            kept_names.push(name.to_owned());
        }
    }
    let tools_empty = tools.is_empty();
    if tools_empty {
        object.remove("tools");
    }
    Some(kept_names)
}

fn sanitize_grok_tool_choice(object: &mut Map<String, Value>, kept_names: Option<&[String]>) {
    let tools_present = object
        .get("tools")
        .and_then(Value::as_array)
        .is_some_and(|tools| !tools.is_empty());
    if !tools_present {
        if kept_names.is_some() {
            object.remove("tool_choice");
            object.remove("parallel_tool_calls");
        }
        return;
    }
    let Some(names) = kept_names else {
        return;
    };
    let drop = match object.get("tool_choice") {
        Some(Value::String(value)) => !matches!(value.as_str(), "auto" | "none" | "required"),
        Some(Value::Object(choice)) => {
            let name = choice.get("name").and_then(Value::as_str).or_else(|| {
                choice
                    .get("function")
                    .and_then(|function| function.get("name"))
                    .and_then(Value::as_str)
            });
            name.is_some_and(|name| !names.iter().any(|kept| kept == name))
        }
        Some(_) => true,
        None => false,
    };
    if drop {
        object.remove("tool_choice");
    }
}

fn sanitize_reasoning_input_items(object: &mut Map<String, Value>) {
    let Some(Value::Array(input)) = object.get_mut("input") else {
        return;
    };
    for item in input {
        let Some(item) = item.as_object_mut() else {
            continue;
        };
        if item.get("type").and_then(Value::as_str) != Some("reasoning") {
            continue;
        }
        // xAI's reasoning item expects `content` as an array. `null` fails
        // the untagged input enum and rejects the whole request.
        let content_bad = item
            .get("content")
            .is_some_and(|value| value.is_null() || !value.is_array());
        if content_bad {
            item.remove("content");
        }
    }
}
