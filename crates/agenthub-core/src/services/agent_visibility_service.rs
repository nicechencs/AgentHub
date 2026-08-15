//! Persist which agents the user has soft-hidden.
//!
//! Hidden is a display preference: detect / install / credentials / usage / backups
//! are unchanged. The file lives under `{data_dir}/agent_visibility.json`.

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{AppError, Result};
use crate::models::{AgentId, AgentVisibilityFile};
use crate::utils::atomic::atomic_write;

const VISIBILITY_FILE: &str = "agent_visibility.json";

pub struct AgentVisibilityService {
    data_dir: PathBuf,
}

impl AgentVisibilityService {
    pub fn new(data_dir: PathBuf) -> Self {
        Self { data_dir }
    }

    fn path(&self) -> PathBuf {
        self.data_dir.join(VISIBILITY_FILE)
    }

    pub fn get_document(&self) -> Result<AgentVisibilityFile> {
        load_visibility(&self.path())
    }

    pub fn list_hidden_agents(&self) -> Result<Vec<String>> {
        Ok(self.get_document()?.hidden_agent_ids)
    }

    pub fn is_hidden(&self, agent_id: AgentId) -> Result<bool> {
        let id = agent_id.as_str();
        Ok(self
            .get_document()?
            .hidden_agent_ids
            .iter()
            .any(|item| item == id))
    }

    pub fn set_agent_hidden(&self, agent_id: AgentId, hidden: bool) -> Result<()> {
        let key = agent_id.as_str().to_string();
        let mut doc = self.get_document()?;
        let idx = doc.hidden_agent_ids.iter().position(|item| item == &key);
        if hidden {
            if idx.is_none() {
                doc.hidden_agent_ids.push(key);
            }
        } else if let Some(i) = idx {
            doc.hidden_agent_ids.remove(i);
        }
        save_visibility(&self.path(), &doc)
    }
}

fn normalize_ids(ids: Vec<String>) -> Vec<String> {
    let mut out = Vec::new();
    for raw in ids {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        let normalized = AgentId::parse(trimmed)
            .map(|id| id.as_str().to_string())
            .unwrap_or_else(|| trimmed.to_ascii_lowercase());
        if !out.iter().any(|item| item == &normalized) {
            out.push(normalized);
        }
    }
    out
}

fn load_visibility(path: &Path) -> Result<AgentVisibilityFile> {
    if !path.exists() {
        return Ok(AgentVisibilityFile::default());
    }
    let raw = fs::read_to_string(path)?;
    if raw.trim().is_empty() {
        return Ok(AgentVisibilityFile::default());
    }
    let mut doc: AgentVisibilityFile = serde_json::from_str(&raw)
        .map_err(|e| AppError::InvalidArg(format!("invalid agent_visibility.json: {e}")))?;
    doc.version = 1;
    doc.hidden_agent_ids = normalize_ids(doc.hidden_agent_ids);
    Ok(doc)
}

fn save_visibility(path: &Path, doc: &AgentVisibilityFile) -> Result<()> {
    let mut out = doc.clone();
    out.version = 1;
    out.hidden_agent_ids = normalize_ids(out.hidden_agent_ids);
    let bytes = serde_json::to_vec_pretty(&out)
        .map_err(|e| AppError::InvalidArg(format!("serialize agent_visibility.json: {e}")))?;
    atomic_write(path, &bytes)?;
    Ok(())
}

#[cfg(test)]
mod tests;
