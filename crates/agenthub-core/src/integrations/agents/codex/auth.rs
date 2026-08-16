//! Shared Codex `auth.json` API-key writer (provider switch + projector).

use std::path::Path;

use serde_json::json;

use crate::error::{AppError, Result};
use crate::utils::atomic::atomic_write;

/// Write API-key mode `auth.json` and verify the key survived the write.
pub(crate) fn write_api_key_auth(path: &Path, api_key: &str) -> Result<()> {
    let body = json!({ "OPENAI_API_KEY": api_key });
    let mut bytes = serde_json::to_vec_pretty(&body)?;
    bytes.push(b'\n');
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    atomic_write(path, &bytes)?;
    let written = std::fs::read_to_string(path)?;
    let parsed: serde_json::Value = serde_json::from_str(&written)?;
    match parsed.get("OPENAI_API_KEY").and_then(|v| v.as_str()) {
        Some(v) if v == api_key => Ok(()),
        _ => Err(AppError::message(
            "provider.verify",
            "Codex auth.json OPENAI_API_KEY verification failed after write",
        )),
    }
}
