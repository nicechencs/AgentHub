use std::path::{Path, PathBuf};

use crate::error::{AppError, Result};
use crate::models::{AppSettings, PathInfo};
use crate::storage::Database;
use crate::utils::paths::{backups_dir, db_path, logs_dir};

/// Whitelisted L1 settings keys for `config get/set`.
pub const SETTINGS_WHITELIST: &[&str] = &[
    "theme",
    "language",
    "log_level",
    "log_retention_days",
    "skill_market_source",
    "close_to_tray",
    "usage_collect_interval_min",
    "keep_live_file_copies",
    "warn_duplicate_route_credential",
    "update_duplicate_route_url",
];

/// Read-only keys: `config get` may return them; `config set` always rejects.
pub const SETTINGS_READONLY: &[&str] = &["app_version"];

pub struct SettingsService {
    data_dir: PathBuf,
    db: Database,
}

impl SettingsService {
    pub fn new(data_dir: PathBuf, db: Database) -> Self {
        Self { data_dir, db }
    }

    pub fn path_info(&self) -> PathInfo {
        PathInfo {
            data_dir: self.data_dir.display().to_string(),
            db_path: db_path(&self.data_dir).display().to_string(),
            backups_dir: backups_dir(&self.data_dir).display().to_string(),
            logs_dir: logs_dir(&self.data_dir).display().to_string(),
        }
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    pub fn load(&self) -> Result<AppSettings> {
        self.db.load_app_settings()
    }

    pub fn get(&self, key: &str) -> Result<Option<String>> {
        if SETTINGS_READONLY.contains(&key) {
            return Ok(Some(readonly_setting(key)));
        }
        if !SETTINGS_WHITELIST.contains(&key) {
            return Err(AppError::InvalidArg(format!(
                "settings key not allowed: {key} (allowed: {}, {})",
                SETTINGS_WHITELIST.join(", "),
                SETTINGS_READONLY.join(", ")
            )));
        }
        self.db.get_setting(key)
    }

    pub fn get_all(&self) -> Result<AppSettings> {
        self.load()
    }

    pub fn set(&self, key: &str, value: &str) -> Result<()> {
        let result = (|| {
            if SETTINGS_READONLY.contains(&key) {
                return Err(AppError::InvalidArg(format!(
                    "settings key is read-only: {key}"
                )));
            }
            if !SETTINGS_WHITELIST.contains(&key) {
                return Err(AppError::InvalidArg(format!(
                    "settings key not allowed: {key} (allowed: {})",
                    SETTINGS_WHITELIST.join(", ")
                )));
            }
            let normalized = match key {
                "log_level" => {
                    crate::logging::parse_level(value)?;
                    value.trim().to_ascii_lowercase()
                }
                "log_retention_days" => crate::logging::parse_retention_days(value)?.to_string(),
                "skill_market_source" => crate::catalog::market::SkillMarketSource::parse(value)
                    .map_err(AppError::InvalidArg)?
                    .as_str()
                    .to_string(),
                "close_to_tray"
                | "keep_live_file_copies"
                | "warn_duplicate_route_credential"
                | "update_duplicate_route_url" => normalize_bool_setting(value)?,
                "usage_collect_interval_min" => {
                    parse_usage_collect_interval_min(value)?.to_string()
                }
                _ => value.to_string(),
            };
            self.db.set_setting(key, &normalized)?;
            Ok(normalized)
        })();
        match &result {
            Ok(normalized) => {
                crate::logging::log_info(
                    crate::logging::targets::SETTINGS,
                    "set",
                    &format!(
                        "settings updated key={key} value={normalized} (log_level applies on next process start)"
                    ),
                );
            }
            Err(e) => {
                crate::logging::log_app_error(crate::logging::targets::SETTINGS, "set", e);
            }
        }
        result.map(|_| ())
    }

    pub fn db_ok(&self) -> Result<()> {
        self.db.ping()
    }
}

fn readonly_setting(key: &str) -> String {
    match key {
        "app_version" => env!("CARGO_PKG_VERSION").to_string(),
        other => other.to_string(),
    }
}

fn normalize_bool_setting(value: &str) -> Result<String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok("true".into()),
        "0" | "false" | "no" | "off" => Ok("false".into()),
        other => Err(AppError::InvalidArg(format!(
            "invalid boolean setting value: {other} (use true/false)"
        ))),
    }
}

/// Parse usage collect interval minutes: `0` = manual only; max 24h.
fn parse_usage_collect_interval_min(value: &str) -> Result<u32> {
    use crate::catalog::limits::MAX_USAGE_COLLECT_INTERVAL_MIN;
    let s = value.trim();
    let n: u32 = s.parse().map_err(|_| {
        AppError::InvalidArg(format!(
            "invalid usage_collect_interval_min '{s}', expected integer 0..={MAX_USAGE_COLLECT_INTERVAL_MIN}"
        ))
    })?;
    if n > MAX_USAGE_COLLECT_INTERVAL_MIN {
        return Err(AppError::InvalidArg(format!(
            "usage_collect_interval_min out of range: {n} (allowed 0..={MAX_USAGE_COLLECT_INTERVAL_MIN})"
        )));
    }
    Ok(n)
}

#[cfg(test)]
mod tests;
