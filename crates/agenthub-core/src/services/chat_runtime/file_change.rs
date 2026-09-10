//! Copy path + already-present edit text from file-change protocol payloads.
//!
//! Approval cards must not invent a diff. This module only reads fields the
//! runtime already sent (`diff`, `patch`, `content`, before/after). Path-only
//! payloads still yield a row so the card can show the path.

use serde_json::Value;

use crate::utils::redact::redact_text;

use super::types::RuntimeFileChange;

const PREVIEW_MAX_CHARS: usize = 1_600;
const PREVIEW_MAX_LINES: usize = 32;

const PREVIEW_KEYS: &[&str] = &["diff", "patch"];
const AFTER_KEYS: &[&str] = &[
    "content",
    "contents",
    "newText",
    "new_text",
    "new_string",
    "after",
];
const BEFORE_KEYS: &[&str] = &["before", "oldText", "old_text", "old_string"];

pub fn extract_file_changes(value: &Value) -> Vec<RuntimeFileChange> {
    let mut out = Vec::new();
    collect_file_changes(value, &mut out, 0);
    apply_fallback_kind(value, &mut out);
    dedupe_keep_first(out)
}

pub fn join_file_change_paths(paths: &[String]) -> String {
    paths
        .iter()
        .map(|path| redact_text(path))
        .collect::<Vec<_>>()
        .join("\n")
}

fn collect_file_changes(value: &Value, out: &mut Vec<RuntimeFileChange>, depth: u8) {
    if depth > 3 {
        return;
    }
    if let Some(rows) = value.get("changes").and_then(Value::as_array) {
        for row in rows {
            push_change(row, None, out);
        }
        if !out.is_empty() {
            return;
        }
    }
    if let Some(map) = value.get("fileChanges").and_then(Value::as_object) {
        for (path, row) in map {
            push_change(row, Some(path.as_str()), out);
        }
        if !out.is_empty() {
            return;
        }
    }
    if let Some(rows) = value.get("locations").and_then(Value::as_array) {
        for row in rows {
            push_change(row, None, out);
        }
        if !out.is_empty() {
            return;
        }
    }
    if let Some(rows) = value.get("files").and_then(Value::as_array) {
        for row in rows {
            push_change(row, row.as_str(), out);
        }
        if !out.is_empty() {
            return;
        }
    }
    if let Some(operation) = value.get("operation") {
        push_change(operation, None, out);
        if !out.is_empty() {
            return;
        }
    }
    push_change(value, None, out);
    if !out.is_empty() {
        return;
    }
    if let Some(item) = value.get("item") {
        collect_file_changes(item, out, depth + 1);
        if !out.is_empty() {
            return;
        }
    }
    if let Some(raw) = value.pointer("/toolCall/rawInput") {
        collect_file_changes(raw, out, depth + 1);
        if !out.is_empty() {
            return;
        }
    }
    if let Some(call) = value.get("toolCall") {
        collect_file_changes(call, out, depth + 1);
    }
}

fn push_change(value: &Value, fallback_path: Option<&str>, out: &mut Vec<RuntimeFileChange>) {
    let path = change_path(value, fallback_path);
    let preview = change_preview(value);
    let kind = change_kind(value);
    if path.is_none() && preview.is_none() {
        return;
    }
    let Some(path) = path else {
        return;
    };
    if path.is_empty() {
        return;
    }
    out.push(RuntimeFileChange {
        path: redact_text(&path),
        kind,
        preview,
    });
}

fn change_path(value: &Value, fallback_path: Option<&str>) -> Option<String> {
    const PATH_KEYS: &[&str] = &[
        "path",
        "file",
        "filePath",
        "file_path",
        "target_file",
        "targetFile",
        "uri",
        "fileUri",
        "file_uri",
    ];
    for key in PATH_KEYS {
        if let Some(path) = nonempty_str(value.get(*key)).and_then(|raw| normalize_path(&raw)) {
            return Some(path);
        }
    }
    fallback_path
        .and_then(normalize_path)
}

fn normalize_path(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(rest) = trimmed.strip_prefix("file://") {
        let path = rest.strip_prefix("localhost").unwrap_or(rest);
        let path = if path.starts_with('/') || path.starts_with('\\') {
            path
        } else if path.is_empty() {
            return None;
        } else {
            path
        };
        return (!path.is_empty()).then(|| path.to_string());
    }
    Some(trimmed.to_string())
}

fn change_kind(value: &Value) -> Option<String> {
    let raw = nonempty_str(value.get("kind"))
        .or_else(|| nonempty_str(value.pointer("/kind/type")))
        .or_else(|| nonempty_str(value.get("type")))
        .or_else(|| nonempty_str(value.pointer("/operation/type")))?;
    let normalized = normalize_kind(&raw);
    known_file_kind(&normalized).then_some(normalized)
}

fn normalize_kind(raw: &str) -> String {
    match raw.trim().to_ascii_lowercase().as_str() {
        "add" | "create" | "create_file" | "add_file" => "add".into(),
        "update" | "modify" | "edit" | "write" | "update_file" | "modify_file" | "write_file" => {
            "update".into()
        }
        "delete" | "remove" | "delete_file" | "remove_file" => "delete".into(),
        other => other.to_string(),
    }
}

fn known_file_kind(kind: &str) -> bool {
    matches!(kind, "add" | "update" | "delete")
}

fn apply_fallback_kind(value: &Value, out: &mut [RuntimeFileChange]) {
    if out.iter().all(|change| change.kind.is_some()) {
        return;
    }
    let Some(kind) = nonempty_str(value.pointer("/toolCall/kind"))
        .or_else(|| nonempty_str(value.get("kind")))
        .map(|raw| normalize_kind(&raw))
        .filter(|kind| known_file_kind(kind))
    else {
        return;
    };
    for change in out.iter_mut() {
        if change.kind.is_none() {
            change.kind = Some(kind.clone());
        }
    }
}

fn change_preview(value: &Value) -> Option<String> {
    for key in PREVIEW_KEYS {
        if let Some(text) = nonempty_str(value.get(*key)) {
            return Some(truncate_preview(&redact_text(&text)));
        }
    }
    let after = first_text(value, AFTER_KEYS);
    let before = first_text(value, BEFORE_KEYS);
    match (before, after) {
        (Some(before), Some(after)) => {
            Some(truncate_preview(&redact_text(&format!("{before}\n\n{after}"))))
        }
        (None, Some(after)) => Some(truncate_preview(&redact_text(&after))),
        (Some(before), None) => Some(truncate_preview(&redact_text(&before))),
        (None, None) => None,
    }
}

fn first_text(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| nonempty_str(value.get(*key)))
}

fn nonempty_str(value: Option<&Value>) -> Option<String> {
    match value {
        Some(Value::String(text)) => {
            let trimmed = text.trim();
            (!trimmed.is_empty()).then(|| text.clone())
        }
        _ => None,
    }
}

fn truncate_preview(text: &str) -> String {
    let mut lines = text.lines();
    let mut out = String::new();
    for (index, line) in lines.by_ref().enumerate() {
        if index >= PREVIEW_MAX_LINES || out.len() + line.len() + 1 > PREVIEW_MAX_CHARS {
            if !out.ends_with('\n') && !out.is_empty() {
                out.push('\n');
            }
            out.push('…');
            return out;
        }
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(line);
    }
    if text.ends_with('\n') && !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

fn dedupe_keep_first(changes: Vec<RuntimeFileChange>) -> Vec<RuntimeFileChange> {
    let mut seen = std::collections::BTreeSet::new();
    let mut out = Vec::with_capacity(changes.len());
    for change in changes {
        if seen.insert(change.path.clone()) {
            out.push(change);
        }
    }
    out
}

#[cfg(test)]
mod tests;
