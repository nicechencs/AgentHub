//! Consistent pre-upgrade copy for the first durable-chat migration.
//!
//! VACUUM INTO reads a SQLite snapshot including committed WAL pages. Copying
//! only the database file would lose those pages. A failed backup stops the
//! upgrade; an existing backup is never overwritten or automatically restored.

use std::fs::OpenOptions;
use std::path::{Path, PathBuf};

use rusqlite::Connection;

use crate::error::{AppError, Result};

pub(super) fn before_upgrade(conn: &Connection, database_path: &Path) -> Result<Option<PathBuf>> {
    let has_migrations: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'schema_migrations')",
        [], |row| row.get(0),
    )?;
    if !has_migrations {
        return Ok(None);
    }
    let needed: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE version = '0002_chat') AND NOT EXISTS(SELECT 1 FROM schema_migrations WHERE version = '00031_chat_runtime')",
        [], |row| row.get(0),
    )?;
    if !needed {
        return Ok(None);
    }

    let mut filename = database_path
        .file_name()
        .ok_or_else(|| AppError::InvalidArg("database path has no filename".into()))?
        .to_os_string();
    filename.push(format!(
        ".before-chat-runtime-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let backup = database_path.with_file_name(filename);
    let backup_text = backup
        .to_str()
        .ok_or_else(|| AppError::InvalidArg("database backup path must be valid UTF-8".into()))?;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    // Reserve this exact path without following or replacing existing files.
    // SQLite accepts an existing empty output file for VACUUM INTO.
    let reserved = options.open(&backup)?;
    drop(reserved);
    if let Err(error) = conn.execute("VACUUM INTO ?1", [backup_text]) {
        let _ = std::fs::remove_file(&backup);
        return Err(error.into());
    }
    std::fs::File::open(&backup)?.sync_all()?;
    #[cfg(unix)]
    {
        let parent = backup
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        std::fs::File::open(parent)?.sync_all()?;
    }
    Ok(Some(backup))
}

#[cfg(test)]
mod tests;
