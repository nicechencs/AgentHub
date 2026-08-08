//! mtime/size cache for content-scanned session trees (Codex / Kimi / Pi).
//!
//! Stored under AgentHub `data_dir/project_session_index.json` — never under agent homes.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::models::AgentId;
use crate::utils::atomic::atomic_write;

const INDEX_FILE: &str = "project_session_index.json";
const INDEX_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SessionIndexFile {
    pub version: u32,
    #[serde(default)]
    pub agents: BTreeMap<String, AgentIndex>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AgentIndex {
    /// Keyed by relative path under agent home (`/` separators).
    #[serde(default)]
    pub files: BTreeMap<String, IndexEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexEntry {
    pub mtime_ms: u64,
    pub size: u64,
    pub project_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_count: Option<u32>,
    pub updated_at: String,
}

pub struct SessionIndexStore {
    path: PathBuf,
    doc: SessionIndexFile,
    dirty: bool,
}

impl SessionIndexStore {
    pub fn load(data_dir: &Path) -> Self {
        // Always resolve against an absolute data_dir so a relative cwd (e.g. cargo
        // package root) cannot leave privacy-sensitive caches in the source tree.
        let data_dir = if data_dir.is_absolute() {
            data_dir.to_path_buf()
        } else {
            std::env::current_dir()
                .map(|cwd| cwd.join(data_dir))
                .unwrap_or_else(|_| data_dir.to_path_buf())
        };
        let path = data_dir.join(INDEX_FILE);
        let doc = load_doc(&path);
        Self {
            path,
            doc,
            dirty: false,
        }
    }

    pub fn get_fresh(
        &self,
        agent: AgentId,
        rel: &str,
        size: u64,
        mtime_ms: u64,
    ) -> Option<&IndexEntry> {
        let agent_ix = self.doc.agents.get(agent.as_str())?;
        let ent = agent_ix.files.get(rel)?;
        if ent.size == size && ent.mtime_ms == mtime_ms {
            Some(ent)
        } else {
            None
        }
    }

    pub fn put(&mut self, agent: AgentId, rel: &str, entry: IndexEntry) {
        let agent_ix = self
            .doc
            .agents
            .entry(agent.as_str().to_string())
            .or_default();
        match agent_ix.files.get(rel) {
            Some(old)
                if old.size == entry.size
                    && old.mtime_ms == entry.mtime_ms
                    && old.project_key == entry.project_key
                    && old.cwd == entry.cwd
                    && old.title == entry.title
                    && old.preview == entry.preview
                    && old.message_count == entry.message_count =>
            {
                // unchanged
            }
            _ => {
                agent_ix.files.insert(rel.to_string(), entry);
                self.dirty = true;
            }
        }
    }

    /// Drop entries whose relative paths were not seen in this scan (deleted files).
    pub fn retain_only(&mut self, agent: AgentId, keep: &std::collections::HashSet<String>) {
        let Some(agent_ix) = self.doc.agents.get_mut(agent.as_str()) else {
            return;
        };
        let before = agent_ix.files.len();
        agent_ix.files.retain(|k, _| keep.contains(k));
        if agent_ix.files.len() != before {
            self.dirty = true;
        }
    }

    pub fn save_if_dirty(&mut self) {
        if !self.dirty {
            return;
        }
        self.doc.version = INDEX_VERSION;
        if let Ok(bytes) = serde_json::to_vec_pretty(&self.doc) {
            if atomic_write(&self.path, &bytes).is_ok() {
                self.dirty = false;
            }
        }
    }
}

fn load_doc(path: &Path) -> SessionIndexFile {
    if !path.exists() {
        return SessionIndexFile {
            version: INDEX_VERSION,
            agents: BTreeMap::new(),
        };
    }
    let Ok(raw) = fs::read_to_string(path) else {
        return SessionIndexFile::default();
    };
    if raw.trim().is_empty() {
        return SessionIndexFile {
            version: INDEX_VERSION,
            agents: BTreeMap::new(),
        };
    }
    serde_json::from_str(&raw).unwrap_or_default()
}

pub fn mtime_ms_from_system(t: SystemTime) -> u64 {
    t.duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub fn file_size_mtime(path: &Path) -> Option<(u64, u64, SystemTime)> {
    let meta = fs::metadata(path).ok()?;
    let modified = meta.modified().ok()?;
    Some((meta.len(), mtime_ms_from_system(modified), modified))
}
