//! In-place Grok Build Responses body normalization (Codex → cli-chat-proxy).

use serde_json::{json, Value};

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
