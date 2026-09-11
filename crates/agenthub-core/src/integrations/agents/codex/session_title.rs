//! Codex's own thread title.
//!
//! Two stores carry one, newest first:
//!
//! 1. `<home>/sqlite/*.db` → `local_thread_catalog.display_title`, keyed by
//!    `thread_id`. The app-server reconciles every local thread into it, so a
//!    conversation AgentHub started (`originator=agenthub-chat`) has a row.
//! 2. `<home>/session_index.jsonl` → `{"id": …, "thread_name": …}`. The Codex
//!    IDE / desktop client maintains this one and only lists the threads it
//!    started itself, so a fresh AgentHub conversation usually has no row.
//!    Kept as a fallback for builds without the catalog.
//!
//! The app-server thread id AgentHub runs a conversation with is the key in both.

use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use rusqlite::{Connection, OpenFlags};
use serde_json::Value;

use crate::error::Result;
use crate::integrations::shared::projects::builtin_key;
use crate::platform::session_title::SessionTitleSource;

const INDEX_FILE: &str = "session_index.jsonl";
const CATALOG_DIR: &str = "sqlite";
const CATALOG_TABLE: &str = "local_thread_catalog";

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
        if let Some(title) = catalog_title(home, session_id) {
            return Ok(Some(title));
        }
        let Ok(text) = fs::read_to_string(home.join(INDEX_FILE)) else {
            return Ok(None);
        };
        Ok(thread_name(&text, session_id))
    }
}

/// Newest `display_title` for `thread_id` from whichever catalog database has it.
fn catalog_title(home: &Path, session_id: &str) -> Option<String> {
    let entries = fs::read_dir(home.join(CATALOG_DIR)).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("db") {
            continue;
        }
        if let Some(title) = catalog_title_from(&path, session_id) {
            return Some(title);
        }
    }
    None
}

/// Codex is running while we read its database, so stay read-only and never
/// wait long: no title only means this conversation keeps the derived one.
fn catalog_title_from(path: &Path, session_id: &str) -> Option<String> {
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .ok()?;
    let _ = conn.busy_timeout(Duration::from_millis(200));
    let sql = format!(
        "SELECT display_title FROM {CATALOG_TABLE} \
         WHERE thread_id = ?1 AND display_title IS NOT NULL AND TRIM(display_title) != '' \
         ORDER BY source_updated_at DESC LIMIT 1"
    );
    let mut stmt = conn.prepare(&sql).ok()?;
    let title: String = stmt.query_row([session_id], |row| row.get(0)).ok()?;
    let title = title.trim();
    (!title.is_empty()).then(|| title.to_string())
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
