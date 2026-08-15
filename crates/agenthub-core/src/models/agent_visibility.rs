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
}

impl Default for AgentVisibilityFile {
    fn default() -> Self {
        Self {
            version: 1,
            hidden_agent_ids: Vec::new(),
        }
    }
}
