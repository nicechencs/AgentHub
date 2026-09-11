//! DeepSeek Harness (`dsh`) session-log layout and row shapes.
//!
//! On-disk layout (see `@deepseek-ai/dsh-session-persistence-jsonl`):
//!
//! ```text
//! <home>/sessions/--<normalized-cwd>--/<encoded-id>/session.vN.jsonl.zstd
//! ```
//!
//! Rows are `{"type": …, "seq": N, "time": <epoch ms>, "data": … }`. The fields
//! AgentHub needs are nested in `data`, not at the top level, so path-derived and
//! generic JSON lookups do not find them.

use std::path::Path;

use serde_json::Value;

/// Number of user messages kept for preview derivation.
const MAX_USER_TEXTS: usize = 8;

/// Facts read from the head of a DSH session log.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct DshHeadMeta {
    /// DSH's own session title (LLM title wins over the first-prompt fallback).
    pub title: Option<String>,
    /// Visible user messages in file order (system-injected blocks included).
    pub user_texts: Vec<String>,
    /// `user/message` + `assistant/message` rows.
    pub message_count: Option<u32>,
}

/// Primary DSH transcript: `session.jsonl` / `session.vN.jsonl`, compressed or not.
pub(crate) fn is_log_file(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    let name = name.to_ascii_lowercase();
    let base = name.strip_suffix(".zstd").unwrap_or(&name);
    let Some(stem) = base.strip_suffix(".jsonl") else {
        return false;
    };
    if stem == "session" {
        return true;
    }
    // Generations are `session.vN.jsonl`; nothing else is a primary transcript.
    stem.strip_prefix("session.v")
        .is_some_and(|generation| {
            !generation.is_empty() && generation.chars().all(|c| c.is_ascii_digit())
        })
}

/// Session id of a DSH log path: `…/--<cwd>--/<session-id>/session.vN.jsonl[.zstd]`.
///
/// Only the documented project-directory layout is accepted; a flat file has no
/// session id in its path and `None` keeps callers from inventing one.
pub(crate) fn session_id_from_log_path(path: &Path) -> Option<String> {
    if !is_log_file(path) {
        return None;
    }
    let session_dir = path.parent()?;
    let project_name = session_dir.parent()?.file_name()?.to_str()?;
    if !(project_name.starts_with("--") || project_name == "_no-cwd") {
        return None;
    }
    session_dir
        .file_name()
        .and_then(|n| n.to_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

/// Reads title / user texts / message count from decoded DSH rows.
pub(crate) fn head_meta(text: &str) -> DshHeadMeta {
    let mut meta = DshHeadMeta::default();
    let mut messages: u32 = 0;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(row) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        match row.get("type").and_then(Value::as_str).unwrap_or("") {
            "session/title" => {
                // DSH's fallback title is a hard-truncated first prompt
                // ("你是只读调查员。目标：彻底"); AgentHub's own preview carries the
                // same message untruncated, so only real titles are kept.
                if is_fallback_title(&row) {
                    continue;
                }
                if let Some(title) = non_empty_str(row.pointer("/data/title")) {
                    meta.title = Some(title.to_string());
                }
            }
            "user/message" => {
                messages = messages.saturating_add(1);
                if meta.user_texts.len() < MAX_USER_TEXTS {
                    if let Some(text) = content_text(row.pointer("/data/content")) {
                        meta.user_texts.push(text);
                    }
                }
            }
            "assistant/message" => messages = messages.saturating_add(1),
            _ => {}
        }
    }
    if messages > 0 {
        meta.message_count = Some(messages);
    }
    meta
}

/// True when a `session/title` row is DSH's truncated-first-prompt placeholder.
///
/// Anything else (a provider/LLM title, or a future kind) is a real title.
fn is_fallback_title(row: &Value) -> bool {
    row.pointer("/data/source/kind")
        .and_then(Value::as_str)
        .is_some_and(|kind| kind.eq_ignore_ascii_case("fallback"))
}

/// Visible text of a `user/message` row.
pub(crate) fn user_text(row: &Value) -> Option<String> {
    content_text(row.pointer("/data/content"))
}

/// Visible text of an `assistant/message` row (reasoning / tool parts dropped).
pub(crate) fn assistant_text(row: &Value) -> Option<String> {
    content_text(row.pointer("/data/message/content"))
}

/// Model that produced a row: request header config or the assistant message source.
pub(crate) fn model_from_row(row: &Value) -> Option<String> {
    for pointer in [
        "/data/header/config/model",
        "/data/message/source/model",
        "/data/message/model",
        "/data/model",
        "/model",
        "/modelName",
        "/model_name",
    ] {
        if let Some(model) = non_empty_str(row.pointer(pointer)) {
            return Some(model.to_string());
        }
    }
    None
}

/// Joins `text` parts of a `content` array (DSH message body).
fn content_text(content: Option<&Value>) -> Option<String> {
    let items = content?.as_array()?;
    let mut parts: Vec<&str> = Vec::new();
    for item in items {
        // Reasoning / tool-call / tool-result parts are not visible transcript.
        if item.get("type").and_then(Value::as_str) != Some("text") {
            continue;
        }
        if let Some(text) = non_empty_str(item.get("text")) {
            parts.push(text);
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("\n"))
    }
}

fn non_empty_str(value: Option<&Value>) -> Option<&str> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
}
