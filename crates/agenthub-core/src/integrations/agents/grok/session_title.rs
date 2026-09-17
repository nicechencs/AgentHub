//! Grok's own session title.
//!
//! Layout: `<home>/sessions/<encoded-cwd>/<session id>/summary.json`. The
//! session directory name is Grok's stable session id, and the title it
//! generated for the conversation is `generated_title` (`session_summary` is
//! the same value under the older key).

use std::fs;
use std::path::Path;
use std::sync::Arc;

use serde_json::Value;

use crate::error::Result;
use crate::integrations::shared::projects::builtin_key;
use crate::platform::session_title::{is_path_safe_session_id, SessionTitleSource};

struct GrokSessionTitle;

impl SessionTitleSource for GrokSessionTitle {
    fn agent_key(&self) -> crate::platform::AgentKey {
        builtin_key("grok")
    }

    fn title_for(&self, home: &Path, session_id: &str) -> Result<Option<String>> {
        let session_id = session_id.trim();
        if !is_path_safe_session_id(session_id) {
            return Ok(None);
        }
        let Ok(projects) = fs::read_dir(home.join("sessions")) else {
            return Ok(None);
        };
        for project in projects.flatten() {
            let dir = project.path();
            if !dir.is_dir() {
                continue;
            }
            let summary = dir.join(session_id).join("summary.json");
            if !summary.is_file() {
                continue;
            }
            if let Some(title) = summary_title(&summary) {
                return Ok(Some(title));
            }
        }
        Ok(None)
    }
}

fn summary_title(path: &Path) -> Option<String> {
    let raw = fs::read_to_string(path).ok()?;
    let value: Value = serde_json::from_str(&raw).ok()?;
    ["generated_title", "session_summary"]
        .iter()
        .find_map(|key| {
            value
                .get(*key)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(ToOwned::to_owned)
        })
}

pub fn register(ctx: &mut crate::integrations::IntegrationContext<'_>) {
    ctx.session_titles
        .register(Arc::new(GrokSessionTitle))
        .expect("unique built-in session title source");
}

#[cfg(test)]
mod tests;
