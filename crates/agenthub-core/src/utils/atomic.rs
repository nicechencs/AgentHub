//! Small, shared atomic-file replacement helper.

use std::io::Write;
use std::path::{Path, PathBuf};

use crate::error::{AppError, Result};

/// Write `contents` to a temporary sibling and atomically persist it at
/// `destination`.
///
/// Keeping the temporary file in the destination directory avoids
/// cross-volume rename failures. The temporary file is removed by
/// `NamedTempFile` if any write/sync/persist step fails.
pub fn atomic_write(destination: &Path, contents: &[u8]) -> Result<()> {
    let parent = destination.parent().ok_or_else(|| {
        AppError::InvalidArg(format!(
            "config destination has no parent: {}",
            destination.display()
        ))
    })?;
    std::fs::create_dir_all(parent)?;

    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    temp.write_all(contents)?;
    temp.flush()?;
    temp.as_file().sync_all()?;
    temp.persist(destination).map_err(|error| error.error)?;
    Ok(())
}

/// Snapshot of files that should be restored if a multi-file write fails.
struct FileBatchBackup {
    entries: Vec<(PathBuf, Option<Vec<u8>>)>,
}

impl FileBatchBackup {
    fn capture(paths: &[&Path]) -> Result<Self> {
        let mut entries = Vec::with_capacity(paths.len());
        let mut seen = std::collections::HashSet::new();
        for path in paths {
            if !seen.insert(path.to_path_buf()) {
                continue;
            }
            let bytes = if path.exists() {
                Some(std::fs::read(path)?)
            } else {
                None
            };
            entries.push((path.to_path_buf(), bytes));
        }
        Ok(Self { entries })
    }

    fn restore(&self) -> Result<()> {
        let mut first_error: Option<AppError> = None;
        for (path, bytes) in &self.entries {
            let result = match bytes {
                Some(contents) => atomic_write(path, contents),
                None => {
                    if path.exists() {
                        std::fs::remove_file(path).map_err(AppError::from)
                    } else {
                        Ok(())
                    }
                }
            };
            if let Err(error) = result {
                if first_error.is_none() {
                    first_error = Some(error);
                }
            }
        }
        match first_error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}

/// Run `f` after snapshotting `paths`. If `f` fails, restore every snapshot
/// (delete files that did not exist). Covers the common I/O-error case for
/// multi-file config writes; a crash between persists can still mix files.
pub fn with_restored_files<T>(paths: &[&Path], f: impl FnOnce() -> Result<T>) -> Result<T> {
    let backup = FileBatchBackup::capture(paths)?;
    match f() {
        Ok(value) => Ok(value),
        Err(error) => match backup.restore() {
            Ok(()) => Err(error),
            Err(restore) => Err(AppError::message(
                "config.write",
                format!("{error}; restore failed: {restore}"),
            )),
        },
    }
}

#[cfg(test)]
mod tests;
