//! Shared project-source helpers.

use std::path::Path;

use crate::catalog::limits::PROJECT_MAX_PER_AGENT as MAX_PER_AGENT;
use crate::models::AgentSession;
use crate::platform::AgentKey;

pub(crate) fn builtin_key(key: &'static str) -> AgentKey {
    AgentKey::parse(key).expect("built-in project source key is valid")
}

pub(crate) fn empty_if_missing(home: &Path) -> bool {
    !home.exists()
}

pub(crate) fn finish_sessions(mut rows: Vec<AgentSession>) -> Vec<AgentSession> {
    rows.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    if rows.len() > MAX_PER_AGENT {
        rows.truncate(MAX_PER_AGENT);
    }
    rows
}
