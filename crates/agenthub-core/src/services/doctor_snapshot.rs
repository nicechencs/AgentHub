//! Persist the last doctor report so a restarted GUI can paint detect
//! results before spawning `--version` probes.

use std::fs;
use std::path::Path;

use crate::DoctorReport;

const SNAPSHOT_REL: &str = "cache/doctor-report.json";

pub fn snapshot_path(data_dir: &Path) -> std::path::PathBuf {
    data_dir.join(SNAPSHOT_REL)
}

pub fn save(data_dir: &Path, report: &DoctorReport) {
    let path = snapshot_path(data_dir);
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    match serde_json::to_vec(report) {
        Ok(bytes) => {
            if let Err(error) = fs::write(&path, bytes) {
                tracing::debug!(
                    error = %error,
                    path = %path.display(),
                    "doctor snapshot write skipped"
                );
            }
        }
        Err(error) => {
            tracing::debug!(error = %error, "doctor snapshot encode skipped");
        }
    }
}

pub fn load(data_dir: &Path) -> Option<DoctorReport> {
    let path = snapshot_path(data_dir);
    let bytes = fs::read(&path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

#[cfg(test)]
mod tests;
