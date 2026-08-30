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
mod tests {
    use super::*;
    use crate::storage::Database;
    use crate::utils::paths::ensure_data_layout;

    fn svc_tmp() -> (tempfile::TempDir, SettingsService) {
        let dir = tempfile::tempdir().unwrap();
        ensure_data_layout(dir.path()).unwrap();
        let db = Database::open(&crate::utils::paths::db_path(dir.path())).unwrap();
        let svc = SettingsService::new(dir.path().to_path_buf(), db);
        (dir, svc)
    }

    #[test]
    fn path_info_includes_logs_dir() {
        let (dir, svc) = svc_tmp();
        let info = svc.path_info();
        assert!(
            info.data_dir
                .contains(dir.path().file_name().unwrap().to_str().unwrap())
                || info.data_dir == dir.path().display().to_string()
        );
        assert!(
            info.logs_dir.ends_with("logs") || info.logs_dir.replace('\\', "/").ends_with("/logs")
        );
        assert!(
            info.db_path.ends_with("agenthub.db")
                || info.db_path.replace('\\', "/").ends_with("agenthub.db")
        );
    }

    #[test]
    fn log_level_and_retention_roundtrip_and_validation() {
        let (_dir, svc) = svc_tmp();
        let defaults = svc.get_all().unwrap();
        assert_eq!(defaults.log_level, "info");
        assert_eq!(defaults.log_retention_days, 14);

        svc.set("log_level", "DEBUG").unwrap();
        svc.set("log_retention_days", "21").unwrap();
        let loaded = svc.get_all().unwrap();
        assert_eq!(loaded.log_level, "debug");
        assert_eq!(loaded.log_retention_days, 21);

        assert!(svc.set("log_level", "verbose").is_err());
        assert!(svc.set("log_retention_days", "0").is_err());
        assert!(svc.set("log_retention_days", "999").is_err());
        assert!(svc.set("not_a_key", "x").is_err());

        // invalid write must not clobber previous good values
        let after = svc.get_all().unwrap();
        assert_eq!(after.log_level, "debug");
        assert_eq!(after.log_retention_days, 21);

        assert_eq!(after.skill_market_source, "auto");
        svc.set("skill_market_source", "skillhub.cn").unwrap();
        assert_eq!(svc.get_all().unwrap().skill_market_source, "skillhub.cn");
        assert!(svc.set("skill_market_source", "nope").is_err());
        assert_eq!(svc.get_all().unwrap().skill_market_source, "skillhub.cn");

        assert!(svc.get_all().unwrap().close_to_tray);
        svc.set("close_to_tray", "false").unwrap();
        assert!(!svc.get_all().unwrap().close_to_tray);
        svc.set("close_to_tray", "1").unwrap();
        assert!(svc.get_all().unwrap().close_to_tray);
        assert!(svc.set("close_to_tray", "maybe").is_err());
        assert!(svc.get_all().unwrap().close_to_tray);
    }

    #[test]
    fn usage_collect_interval_roundtrip_and_validation() {
        let (_dir, svc) = svc_tmp();
        assert_eq!(svc.get_all().unwrap().usage_collect_interval_min, None);

        svc.set("usage_collect_interval_min", "0").unwrap();
        assert_eq!(svc.get_all().unwrap().usage_collect_interval_min, Some(0));
        svc.set("usage_collect_interval_min", "45").unwrap();
        assert_eq!(svc.get_all().unwrap().usage_collect_interval_min, Some(45));
        svc.set("usage_collect_interval_min", "1440").unwrap();
        assert_eq!(
            svc.get_all().unwrap().usage_collect_interval_min,
            Some(1440)
        );

        assert!(svc.set("usage_collect_interval_min", "1441").is_err());
        assert!(svc.set("usage_collect_interval_min", "-1").is_err());
        assert!(svc.set("usage_collect_interval_min", "nope").is_err());
        assert_eq!(
            svc.get_all().unwrap().usage_collect_interval_min,
            Some(1440)
        );
    }

    #[test]
    fn whitelist_get_and_theme() {
        let (_dir, svc) = svc_tmp();
        assert!(svc.get("log_level").unwrap().is_some());
        svc.set("theme", "dark").unwrap();
        assert_eq!(svc.get("theme").unwrap().as_deref(), Some("dark"));
    }

    #[test]
    fn app_version_is_read_only() {
        let (_dir, svc) = svc_tmp();
        assert_eq!(
            svc.get("app_version").unwrap().as_deref(),
            Some(env!("CARGO_PKG_VERSION"))
        );
        let err = svc.set("app_version", "9.9.9").unwrap_err();
        assert_eq!(err.code(), "invalid_arg");
        assert!(err.to_string().contains("read-only"));
    }
}
