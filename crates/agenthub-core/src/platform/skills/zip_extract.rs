//! Zip extract without shelling out to `unzip` / Expand-Archive.

use std::fs;
use std::io;
use std::path::Path;

use crate::error::{AppError, Result};

pub(crate) fn extract_zip_file(zip: &Path, dest: &Path) -> Result<()> {
    fs::create_dir_all(dest)?;
    let file = fs::File::open(zip).map_err(|e| {
        AppError::message("skill.install", format!("open zip failed: {e}"))
    })?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| {
        AppError::message("skill.install", format!("invalid zip: {e}"))
    })?;
    archive.extract(dest).map_err(|e| match e {
        zip::result::ZipError::Io(io_err) if io_err.kind() == io::ErrorKind::InvalidInput => {
            AppError::message("skill.install", format!("unsafe zip path: {io_err}"))
        }
        other => AppError::message("skill.install", format!("extract zip failed: {other}")),
    })?;
    Ok(())
}
