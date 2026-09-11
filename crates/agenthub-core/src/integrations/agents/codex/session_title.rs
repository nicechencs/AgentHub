//! Codex's own thread title.
//!
//! The CLI keeps `<home>/session_index.jsonl` with one row per thread:
//! `{"id": "<thread id>", "thread_name": "<title>", "updated_at": … }`.
//! The app-server thread id AgentHub runs a conversation with is that `id`, so
//! the row lookup is the same key the runtime stores as its thread id.

use std::fs;
use std::path::Path;
use std::sync::Arc;

use serde_json::Value;

use crate::error::Result;
use crate::integrations::shared::projects::builtin_key;
use crate::platform::session_title::SessionTitleSource;

const INDEX_FILE: &str = "session_index.jsonl";

struct CodexSessionTitle;

impl SessionTitleSource for CodexSessionTitle {
    fn agent_key(&self) -> crate::platform::AgentKey {
        builtin_key("codex")
    }

    fn title_for(&self, home: &Path, session_id: &str) -> Result<Option<String>> {
        let session_id = session_id.trim();
        if session_id.is_empty() {
            return Ok(None);
        }
        let Ok(text) = fs::read_to_string(home.join(INDEX_FILE)) else {
            return Ok(None);
        };
        Ok(thread_name(&text, session_id))
    }
}

/// Last non-empty `thread_name` written for `id`; Codex appends, so the newest wins.
fn thread_name(text: &str, session_id: &str) -> Option<String> {
    let mut found = None;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(row) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if row.get("id").and_then(Value::as_str) != Some(session_id) {
            continue;
        }
        if let Some(name) = row
            .get("thread_name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            found = Some(name.to_string());
        }
    }
    found
}

pub fn register(ctx: &mut crate::integrations::IntegrationContext<'_>) {
    ctx.session_titles
        .register(Arc::new(CodexSessionTitle))
        .expect("unique built-in session title source");
}

#[cfg(test)]
mod tests;
