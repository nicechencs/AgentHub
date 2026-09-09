//! Conversation working-directory helpers.
//!
//! Stored cwd is historical: a missing folder must not fail open/continue.
//! Runtime start only receives a directory that still exists.

use std::path::{Path, PathBuf};

use crate::error::{AppError, Result};

/// Trim empty cwd values to `None`.
pub fn normalize_cwd(cwd: Option<String>) -> Option<String> {
    cwd.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

pub fn cwd_is_dir(cwd: &str) -> bool {
    let trimmed = cwd.trim();
    !trimmed.is_empty() && Path::new(trimmed).is_dir()
}

/// True when a stored cwd is present but no longer an existing directory.
pub fn stored_cwd_missing(cwd: Option<&str>) -> bool {
    match cwd.map(str::trim).filter(|value| !value.is_empty()) {
        Some(path) => !Path::new(path).is_dir(),
        None => false,
    }
}

/// User is setting a new working directory — it must exist.
pub fn validate_existing_cwd(cwd: &str) -> Result<()> {
    if cwd_is_dir(cwd) {
        return Ok(());
    }
    Err(AppError::InvalidArg(format!(
        "cwd is not an existing directory: {cwd}"
    )))
}

/// Path handed to a subprocess. Never a deleted folder.
pub fn resolve_runtime_cwd(stored: Option<&str>) -> Result<PathBuf> {
    if let Some(path) = stored.map(str::trim).filter(|value| !value.is_empty()) {
        if Path::new(path).is_dir() {
            return Ok(PathBuf::from(path));
        }
    }
    std::env::current_dir().map_err(AppError::from)
}

#[cfg(test)]
mod tests;
