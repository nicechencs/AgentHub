//! Small, shared atomic-file replacement helper.

use std::io::Write;
use std::path::Path;

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
}
