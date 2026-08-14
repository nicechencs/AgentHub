use serde::{Deserialize, Serialize};

use crate::catalog::market::SkillMarketSource;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub theme: String,
    pub language: String,
    pub log_level: String,
    /// Days to keep dated log files under `{data_dir}/logs/`.
    pub log_retention_days: u32,
    /// Remote skill market: `auto` | `skills.sh` | `skillhub.cn`.
    pub skill_market_source: String,
    /// When true, the main window close button hides to the system tray.
    pub close_to_tray: bool,
    /// Foreground usage collect interval in minutes. `0` = manual only.
    /// `None` when the SQLite key was never written (legacy clients may still
    /// have the value only in webview localStorage).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage_collect_interval_min: Option<u32>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme: "system".into(),
            language: "zh-CN".into(),
            log_level: "info".into(),
            log_retention_days: crate::catalog::limits::DEFAULT_LOG_RETENTION_DAYS,
            skill_market_source: SkillMarketSource::DEFAULT.as_str().into(),
            close_to_tray: true,
            usage_collect_interval_min: None,
        }
    }
}

impl AppSettings {
    pub fn skill_market_source_parsed(&self) -> SkillMarketSource {
        SkillMarketSource::parse(&self.skill_market_source).unwrap_or(SkillMarketSource::DEFAULT)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PathInfo {
    pub data_dir: String,
    pub db_path: String,
    pub backups_dir: String,
    pub logs_dir: String,
}
