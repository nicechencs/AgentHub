//! Kiro's own session title.
//!
//! CLI sessions are `<home>/sessions/cli/<session id>.json` with a `title`
//! field Kiro writes itself. The file stem and the stored `session_id` agree,
//! so either identifies the file for a conversation's native session id.

use std::fs;
use std::path::Path;
use std::sync::Arc;

use serde_json::Value;

use crate::error::Result;
use crate::integrations::shared::projects::builtin_key;
use crate::platform::session_title::SessionTitleSource;

/// Kiro's own placeholder for a session it has not titled yet.
const UNTITLED: &str = "New Session";

struct KiroSessionTitle;

impl SessionTitleSource for KiroSessionTitle {
    fn agent_key(&self) -> crate::platform::AgentKey {
        builtin_key("kiro")
    }

    fn title_for(&self, home: &Path, session_id: &str) -> Result<Option<String>> {
        let session_id = session_id.trim();
        if session_id.is_empty() {
            return Ok(None);
        }
        let dir = home.join("sessions").join("cli");
        // Fast path: Kiro names the record after the session id, so one probe
        // answers the common case without touching the rest of the history.
        let direct = dir.join(format!("{session_id}.json"));
        if direct.is_file() {
            return Ok(read_record(&direct).and_then(|value| kiro_title(&value)));
        }
        // An imported record may be named differently, so fall back to a stem
        // scan. It compares file names only: reading every record in the
        // directory would scale with the user's whole Kiro history, and the
        // stored `session_id` agrees with the stem anyway.
        if session_id.contains(':') {
            // Runtime ids such as `kiro-http:<cid>` never name a file.
            return Ok(None);
        }
        let Ok(entries) = fs::read_dir(&dir) else {
            return Ok(None);
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() || path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            if path.file_stem().and_then(|s| s.to_str()) != Some(session_id) {
                continue;
            }
            return Ok(read_record(&path).and_then(|value| kiro_title(&value)));
        }
        Ok(None)
    }
}

fn read_record(path: &Path) -> Option<Value> {
    let raw = fs::read_to_string(path).ok()?;
    serde_json::from_str::<Value>(&raw).ok()
}

/// Kiro's title for one session record; its placeholder counts as no title.
fn kiro_title(value: &Value) -> Option<String> {
    value
        .get("title")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty() && !s.eq_ignore_ascii_case(UNTITLED))
        .map(ToOwned::to_owned)
}

pub fn register(ctx: &mut crate::integrations::IntegrationContext<'_>) {
    ctx.session_titles
        .register(Arc::new(KiroSessionTitle))
        .expect("unique built-in session title source");
}

#[cfg(test)]
mod tests;
