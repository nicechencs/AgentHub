//! SQLite layer: WAL mode + versioned migrations.

mod account_repo;
mod adapter_profile_repo;
mod backup_repo;
mod binding_repo;
mod chat_repo;
mod connection_trash_repo;
mod migrations;
mod operation_repo;
mod provider_repo;
mod skill_repo;
mod usage_repo;

pub use account_repo::AccountRepo;
pub use adapter_profile_repo::AdapterProfileRepo;
pub use backup_repo::BackupRepo;
/// Test-only convenience wrapper; production code uses `binding_*_conn` helpers.
#[cfg(test)]
pub(crate) use binding_repo::ActiveBindingRepo;
/// Row DTO is public for diagnostics; writes go through ConnectionService conn helpers.
pub use binding_repo::ActiveBindingRow;
pub use chat_repo::ChatRepo;
pub use connection_trash_repo::ConnectionTrashRepo;
pub use operation_repo::OperationRepo;
pub use provider_repo::ProviderRepo;

// Connection-scoped helpers for multi-table transactions (ConnectionService).
pub(crate) use account_repo::{
    clear_current_conn as account_clear_current_conn, create_conn as account_create_conn,
    delete_for_agent_conn as account_delete_for_agent_conn,
    force_sole_current_conn as account_force_sole_current_conn,
    get_by_id_conn_pub as account_get_by_id_conn, list_current_conn as account_list_current_conn,
    select_current_conn as account_select_current_conn, update_conn as account_update_conn,
};
pub(crate) use binding_repo::{
    clear_conn as binding_clear_conn,
    clear_connection_refs_conn as binding_clear_connection_refs_conn,
    get_conn_pub as binding_get_conn, set_connection_refs_conn as binding_set_connection_refs_conn,
};
pub(crate) use provider_repo::{
    clear_current_conn as provider_clear_current_conn, create_conn as provider_create_conn,
    delete_for_agent_conn as provider_delete_for_agent_conn,
    force_sole_current_conn as provider_force_sole_current_conn,
    get_by_id_conn_pub as provider_get_by_id_conn, list_current_conn as provider_list_current_conn,
    select_current_conn as provider_select_current_conn, update_conn as provider_update_conn,
    upsert_conn as provider_upsert_conn,
};
pub use skill_repo::{SkillAssignmentRow, SkillPackageRow, SkillRepo};
pub use usage_repo::{UsageCursor, UsageRepo};

use std::path::Path;
use std::sync::{Arc, Mutex};

use rusqlite::Connection;

use crate::error::{AppError, Result};
use crate::models::AppSettings;

/// Shared database handle.
#[derive(Clone)]
pub struct Database {
    conn: Arc<Mutex<Connection>>,
}

impl Database {
    pub fn open(db_path: &Path) -> Result<Self> {
        match Self::open_inner(db_path) {
            Ok(db) => {
                crate::logging::log_info(
                    crate::logging::targets::STORAGE,
                    "open",
                    &format!("database opened path={}", db_path.display()),
                );
                Ok(db)
            }
            Err(e) => {
                crate::logging::log_app_error(crate::logging::targets::STORAGE, "open", &e);
                Err(e)
            }
        }
    }

    fn open_inner(db_path: &Path) -> Result<Self> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(db_path)?;
        conn.execute_batch(
            r#"
            PRAGMA foreign_keys = ON;
            PRAGMA journal_mode = WAL;
            PRAGMA busy_timeout = 5000;
            "#,
        )?;
        let db = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        db.migrate()?;
        Ok(db)
    }

    pub fn with_conn<F, T>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&Connection) -> Result<T>,
    {
        let guard = self
            .conn
            .lock()
            .map_err(|_| AppError::message("db.lock", "database lock poisoned"))?;
        f(&guard)
    }

    fn migrate(&self) -> Result<()> {
        self.with_conn(migrations::run)
    }

    pub fn ping(&self) -> Result<()> {
        self.with_conn(|conn| {
            conn.query_row("SELECT 1", [], |_| Ok(()))?;
            Ok(())
        })
    }

    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare("SELECT value FROM settings WHERE key = ?1")?;
            let mut rows = stmt.query([key])?;
            if let Some(row) = rows.next()? {
                let v: String = row.get(0)?;
                Ok(Some(v))
            } else {
                Ok(None)
            }
        })
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        self.with_conn(|conn| {
            conn.execute(
                r#"
                INSERT INTO settings (key, value) VALUES (?1, ?2)
                ON CONFLICT(key) DO UPDATE SET value = excluded.value
                "#,
                [key, value],
            )?;
            Ok(())
        })
    }

    pub fn load_app_settings(&self) -> Result<AppSettings> {
        let mut s = AppSettings::default();
        if let Some(v) = self.get_setting("theme")? {
            s.theme = v;
        }
        if let Some(v) = self.get_setting("language")? {
            s.language = v;
        }
        if let Some(v) = self.get_setting("log_level")? {
            s.log_level = v;
        }
        if let Some(v) = self.get_setting("log_retention_days")? {
            if let Ok(n) = v.parse::<u32>() {
                s.log_retention_days = n;
            }
        }
        if let Some(v) = self.get_setting("skill_market_source")? {
            if crate::catalog::market::SkillMarketSource::parse(&v).is_ok() {
                s.skill_market_source = v;
            }
        }
        if let Some(v) = self.get_setting("close_to_tray")? {
            s.close_to_tray = parse_stored_bool(&v);
        }
        if let Some(v) = self.get_setting("usage_collect_interval_min")? {
            if let Ok(n) = v.parse::<u32>() {
                if n <= crate::catalog::limits::MAX_USAGE_COLLECT_INTERVAL_MIN {
                    s.usage_collect_interval_min = Some(n);
                }
            }
        }
        Ok(s)
    }
}

fn parse_stored_bool(raw: &str) -> bool {
    match raw.trim().to_ascii_lowercase().as_str() {
        "0" | "false" | "no" | "off" => false,
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn database_open_creates_schema_and_settings_roundtrip() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("agenthub.db");
        let db = Database::open(&path).expect("open db");
        assert!(path.exists());
        db.ping().expect("ping");

        // Migration seeds theme/language/log_level/log_retention_days.
        assert_eq!(
            db.get_setting("theme").expect("get seeded").as_deref(),
            Some("system")
        );
        assert_eq!(
            db.get_setting("log_level")
                .expect("get log_level")
                .as_deref(),
            Some("info")
        );
        assert_eq!(
            db.get_setting("log_retention_days")
                .expect("get retention")
                .as_deref(),
            Some("14")
        );
        let settings = db.load_app_settings().expect("load settings");
        assert_eq!(settings.log_level, "info");
        assert_eq!(settings.log_retention_days, 14);
        assert_eq!(db.get_setting("missing_key").expect("get missing"), None);

        assert!(
            db.load_app_settings().expect("load").close_to_tray,
            "default close_to_tray is true when key missing"
        );
        db.set_setting("close_to_tray", "false").expect("set close");
        assert!(!db.load_app_settings().expect("load false").close_to_tray);
        db.set_setting("close_to_tray", "true")
            .expect("set close true");
        assert!(db.load_app_settings().expect("load true").close_to_tray);
        // Loose false tokens
        db.set_setting("close_to_tray", "off").expect("set off");
        assert!(!db.load_app_settings().expect("load off").close_to_tray);
        db.set_setting("close_to_tray", "no").expect("set no");
        assert!(!db.load_app_settings().expect("load no").close_to_tray);

        db.set_setting("theme", "dark").expect("set");
        assert_eq!(
            db.get_setting("theme").expect("get after set").as_deref(),
            Some("dark")
        );
        db.set_setting("theme", "light").expect("update");
        assert_eq!(
            db.get_setting("theme")
                .expect("get after update")
                .as_deref(),
            Some("light")
        );
        db.set_setting("custom_key", "custom_value")
            .expect("insert");
        assert_eq!(
            db.get_setting("custom_key").expect("get custom").as_deref(),
            Some("custom_value")
        );

        // Schema migration marker should exist after open.
        db.with_conn(|conn| {
            let count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM schema_migrations WHERE version = '0001_init'",
                [],
                |row| row.get(0),
            )?;
            assert_eq!(count, 1);
            let settings_ok: i64 = conn.query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='settings'",
                [],
                |row| row.get(0),
            )?;
            assert_eq!(settings_ok, 1);
            Ok(())
        })
        .expect("schema checks");
    }
}
