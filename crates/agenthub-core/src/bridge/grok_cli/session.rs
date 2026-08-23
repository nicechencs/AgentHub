//! Prompt-cache session seed extraction for Grok CLI identity.

use axum::http::HeaderMap;
use serde_json::Value;
use uuid::Uuid;

const MAX_SEED_LEN: usize = 1024;
/// Bound title-heuristic scan so large system prompts are not fully lowercased.
const MAX_TITLE_SCAN_LEN: usize = 4096;

pub fn extract_prompt_cache_seed(headers: &HeaderMap, body: &Value) -> Option<String> {
    let claude_session = header_nonempty(headers, "x-claude-code-session-id");
    if claude_session.is_some() && is_claude_title_request(body) {
        return None;
    }

    if let Some(session) = claude_session {
        let agent = header_nonempty(headers, "x-claude-code-agent-id").unwrap_or("main");
        return normalize_seed(&format!("claude:{session}:agent:{agent}"));
    }

    if let Some(raw) = header_nonempty(headers, "x-codex-turn-metadata") {
        if let Some(seed) = seed_from_codex_metadata_str(raw) {
            return Some(seed);
        }
    }

    if let Some(window_id) = header_nonempty(headers, "x-codex-window-id") {
        return normalize_seed(&format!("codex:window:{window_id}"));
    }

    for name in [
        "x-session-id",
        "session-id",
        "x-conversation-id",
        "x-client-session-id",
        "x-grok-conv-id",
    ] {
        if let Some(value) = header_nonempty(headers, name) {
            return normalize_seed(value);
        }
    }

    if let Some(seed) = body
        .get("prompt_cache_key")
        .and_then(Value::as_str)
        .and_then(normalize_seed)
    {
        return Some(seed);
    }

    if let Some(meta) = body.get("metadata") {
        for key in ["session_id", "sessionId"] {
            if let Some(seed) = meta
                .get(key)
                .and_then(Value::as_str)
                .and_then(normalize_seed)
            {
                return Some(seed);
            }
        }
    }

    if let Some(value) = body
        .get("client_metadata")
        .and_then(|meta| meta.get("x-codex-turn-metadata"))
    {
        if let Some(seed) = seed_from_codex_metadata_value(value) {
            return Some(seed);
        }
    }

    for key in [
        "session_id",
        "sessionId",
        "conversation_id",
        "conversationId",
    ] {
        if let Some(seed) = body
            .get(key)
            .and_then(Value::as_str)
            .and_then(normalize_seed)
        {
            return Some(seed);
        }
    }

    None
}

pub fn grok_session_id(seed: &str) -> Option<String> {
    grok_session_id_for_account(seed, None)
}

/// Mix `account_id` into the session hash so prompt-cache / replay cannot
/// cross accounts. `None` keeps the historical single-account hash.
pub fn grok_session_id_for_account(seed: &str, account_id: Option<&str>) -> Option<String> {
    let seed = seed.trim();
    if seed.is_empty() {
        return None;
    }
    let account = account_id.map(str::trim).filter(|value| !value.is_empty());
    if account.is_none() {
        if let Ok(uuid) = Uuid::parse_str(seed) {
            return Some(uuid.to_string());
        }
        return Some(
            Uuid::new_v5(
                &Uuid::NAMESPACE_URL,
                format!("agenthub:grok-session:{seed}").as_bytes(),
            )
            .to_string(),
        );
    }
    Some(
        Uuid::new_v5(
            &Uuid::NAMESPACE_URL,
            format!(
                "agenthub:grok-session:{}:{seed}",
                account.expect("filtered")
            )
            .as_bytes(),
        )
        .to_string(),
    )
}

fn header_nonempty<'a>(headers: &'a HeaderMap, name: &'static str) -> Option<&'a str> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn normalize_seed(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    let mut end = trimmed.len().min(MAX_SEED_LEN);
    while end > 0 && !trimmed.is_char_boundary(end) {
        end -= 1;
    }
    let sliced = &trimmed[..end];
    if sliced.is_empty() {
        None
    } else {
        Some(sliced.to_string())
    }
}

fn seed_from_codex_metadata_str(raw: &str) -> Option<String> {
    let parsed: Value = serde_json::from_str(raw).ok()?;
    seed_from_codex_metadata_object(&parsed)
}

fn seed_from_codex_metadata_value(value: &Value) -> Option<String> {
    match value {
        Value::String(raw) => seed_from_codex_metadata_str(raw),
        Value::Object(_) => seed_from_codex_metadata_object(value),
        _ => None,
    }
}

fn seed_from_codex_metadata_object(value: &Value) -> Option<String> {
    if let Some(seed) = value
        .get("prompt_cache_key")
        .and_then(Value::as_str)
        .and_then(normalize_seed)
    {
        return Some(seed);
    }
    value
        .get("window_id")
        .and_then(Value::as_str)
        .and_then(|id| normalize_seed(&format!("codex:window:{id}")))
}

fn is_claude_title_request(body: &Value) -> bool {
    let text = collect_title_scan_text(body).to_ascii_lowercase();
    text.contains("generate a concise") && text.contains("title") && text.contains("coding session")
}

fn collect_title_scan_text(body: &Value) -> String {
    let mut out = String::new();
    // User first: Claude Code title prompts live there, and a large system
    // prompt would otherwise exhaust MAX_TITLE_SCAN_LEN before we see them.
    append_role_text(&mut out, body.get("messages"), &["user"]);
    append_role_text(&mut out, body.get("input"), &["user"]);
    push_text(&mut out, body.get("system"));
    push_text(&mut out, body.get("instructions"));
    append_role_text(&mut out, body.get("messages"), &["system"]);
    append_role_text(&mut out, body.get("input"), &["system", "developer"]);
    out
}

fn append_role_text(out: &mut String, items: Option<&Value>, roles: &[&str]) {
    if out.len() >= MAX_TITLE_SCAN_LEN {
        return;
    }
    let Some(Value::Array(items)) = items else {
        return;
    };
    for item in items {
        if out.len() >= MAX_TITLE_SCAN_LEN {
            return;
        }
        let role = item.get("role").and_then(Value::as_str).unwrap_or("");
        if roles.iter().any(|wanted| role.eq_ignore_ascii_case(wanted)) {
            push_text(out, item.get("content"));
            push_text(out, item.get("text"));
        }
    }
}

fn push_text(out: &mut String, value: Option<&Value>) {
    if out.len() >= MAX_TITLE_SCAN_LEN {
        return;
    }
    let Some(value) = value else {
        return;
    };
    match value {
        Value::String(text) => append_chunk(out, text),
        Value::Array(items) => {
            for item in items {
                if out.len() >= MAX_TITLE_SCAN_LEN {
                    return;
                }
                match item {
                    Value::String(text) => append_chunk(out, text),
                    Value::Object(obj) => {
                        push_text(out, obj.get("text"));
                        push_text(out, obj.get("content"));
                    }
                    _ => {}
                }
            }
        }
        Value::Object(obj) => {
            push_text(out, obj.get("text"));
            push_text(out, obj.get("content"));
        }
        _ => {}
    }
}

fn append_chunk(out: &mut String, text: &str) {
    if text.is_empty() || out.len() >= MAX_TITLE_SCAN_LEN {
        return;
    }
    if !out.is_empty() {
        out.push(' ');
        if out.len() >= MAX_TITLE_SCAN_LEN {
            return;
        }
    }
    let remaining = MAX_TITLE_SCAN_LEN - out.len();
    let mut end = remaining.min(text.len());
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    if end > 0 {
        out.push_str(&text[..end]);
    }
}
