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
mod tests {
    use super::*;

    #[test]
    fn creates_and_replaces_destination_without_leaking_temp_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("config.txt");

        atomic_write(&path, b"first").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"first");

        atomic_write(&path, b"second").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"second");
        assert_eq!(
            std::fs::read_dir(path.parent().unwrap()).unwrap().count(),
            1
        );
    }

    #[test]
    fn restored_files_roll_back_earlier_writes_on_failure() {
        let dir = tempfile::tempdir().unwrap();
        let first = dir.path().join("first.txt");
        let second = dir.path().join("second.txt");
        atomic_write(&first, b"old-first").unwrap();
        atomic_write(&second, b"old-second").unwrap();

        let err = with_restored_files(&[&first, &second], || -> Result<()> {
            atomic_write(&first, b"new-first")?;
            Err(AppError::InvalidArg("boom".into()))
        })
        .unwrap_err();
        assert_eq!(err.code(), "invalid_arg");
        assert_eq!(std::fs::read(&first).unwrap(), b"old-first");
        assert_eq!(std::fs::read(&second).unwrap(), b"old-second");
    }

    #[test]
    fn restored_files_delete_newly_created_path_on_failure() {
        let dir = tempfile::tempdir().unwrap();
        let created = dir.path().join("created.txt");
        let err = with_restored_files(&[&created], || -> Result<()> {
            atomic_write(&created, b"new")?;
            Err(AppError::InvalidArg("boom".into()))
        })
        .unwrap_err();
        assert_eq!(err.code(), "invalid_arg");
        assert!(!created.exists());
    }
}
