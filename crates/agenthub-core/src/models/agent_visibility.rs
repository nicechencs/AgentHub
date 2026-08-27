//! Agent visibility preferences (soft-hide). On-disk: `{data_dir}/agent_visibility.json`.

use serde::{Deserialize, Serialize};

/// Persisted user preference: which catalog agents are hidden in the UI.
///
/// Unknown ids are kept as strings so a catalog change does not invalidate the file.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentVisibilityFile {
    pub version: u32,
    #[serde(default)]
    pub hidden_agent_ids: Vec<String>,
    /// Dev store-stamp generation applied to this file (one-time default hides).
    #[serde(default)]
    pub store_stamp_version: u32,
}

impl Default for AgentVisibilityFile {
    fn default() -> Self {
        Self {
            version: 1,
            hidden_agent_ids: Vec::new(),
            store_stamp_version: 0,
        }
    }
}
