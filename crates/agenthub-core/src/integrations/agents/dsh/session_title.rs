//! DSH's own session title.
//!
//! Layout: `<home>/sessions/--<normalized-cwd>--/<session id>/session.vN.jsonl[.zstd]`,
//! with extra profile roots under `<home>/profiles/<name>/sessions/`. The title
//! is a `session/title` row inside the (possibly compressed) transcript, so the
//! read reuses [`crate::utils::dsh_session_log::head_meta`] and its guard
//! against DSH's truncated-first-prompt fallback.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use crate::catalog::limits::PROJECT_SCAN_BYTES;
use crate::error::Result;
use crate::integrations::shared::projects::builtin_key;
use crate::platform::session_title::SessionTitleSource;
use crate::utils::dsh_session_log::{head_meta, is_log_file};
use crate::utils::zstd_jsonl::read_decoded_head;

struct DshSessionTitle;

impl SessionTitleSource for DshSessionTitle {
    fn agent_key(&self) -> crate::platform::AgentKey {
        builtin_key("dsh")
    }

    fn title_for(&self, home: &Path, session_id: &str) -> Result<Option<String>> {
        let session_id = session_id.trim();
        if session_id.is_empty() {
            return Ok(None);
        }
        for log in session_log_candidates(home, session_id) {
            let Some(text) = read_decoded_head(&log, PROJECT_SCAN_BYTES) else {
                continue;
            };
            if let Some(title) = head_meta(&text).title {
                return Ok(Some(title));
            }
        }
        Ok(None)
    }
}

/// Newest transcript in `<session root>/<project>/<session id>/`, newest root first.
fn session_log_candidates(home: &Path, session_id: &str) -> Vec<PathBuf> {
    let mut roots = vec![home.join("sessions")];
    if let Ok(profiles) = fs::read_dir(home.join("profiles")) {
        for profile in profiles.flatten() {
            roots.push(profile.path().join("sessions"));
        }
    }
    let mut out: Vec<(SystemTime, PathBuf)> = Vec::new();
    for root in roots {
        let Ok(projects) = fs::read_dir(&root) else {
            continue;
        };
        for project in projects.flatten() {
            let session_dir = project.path().join(session_id);
            let Ok(entries) = fs::read_dir(&session_dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_file() || !is_log_file(&path) {
                    continue;
                }
                let modified = entry
                    .metadata()
                    .and_then(|meta| meta.modified())
                    .unwrap_or(SystemTime::UNIX_EPOCH);
                out.push((modified, path));
            }
        }
    }
    out.sort_by(|a, b| b.0.cmp(&a.0));
    out.into_iter().map(|(_, path)| path).collect()
}

pub fn register(ctx: &mut crate::integrations::IntegrationContext<'_>) {
    ctx.session_titles
        .register(Arc::new(DshSessionTitle))
        .expect("unique built-in session title source");
}

#[cfg(test)]
mod tests;
