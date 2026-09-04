//! SQLite layer: WAL mode + versioned migrations.

mod account_repo;
mod adapter_profile_repo;
mod backup_repo;
mod binding_repo;
mod chat_repo;
mod connection_trash_repo;
pub(crate) mod gateway_usage_repo;
pub(crate) mod live_fingerprint_repo;
mod local_entry_key_repo;
mod migrations;
mod operation_repo;
mod provider_repo;
mod route_pool_repo;
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
pub(crate) use connection_trash_repo::TrashPayloadRow;
pub(crate) use local_entry_key_repo::{LocalEntryKey, LocalEntryKeyRepo};
pub use operation_repo::OperationRepo;
pub use provider_repo::ProviderRepo;
pub use route_pool_repo::RoutePoolRepo;

// Connection-scoped helpers for multi-table transactions (ConnectionService).
pub(crate) use account_repo::{
    clear_current_conn as account_clear_current_conn, create_conn as account_create_conn,
    delete_for_agent_conn as account_delete_for_agent_conn,
    delete_if_revision_conn as account_delete_if_revision_conn,
    force_sole_current_conn as account_force_sole_current_conn,
    get_by_id_conn_pub as account_get_by_id_conn, list_current_conn as account_list_current_conn,
    list_for_agent_conn as account_list_for_agent_conn,
    select_current_conn as account_select_current_conn, update_conn as account_update_conn,
    update_if_revision_conn as account_update_if_revision_conn,
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
    list_for_agent_conn as provider_list_for_agent_conn,
    select_current_conn as provider_select_current_conn, update_conn as provider_update_conn,
    update_if_revision_conn as provider_update_if_revision_conn,
    upsert_conn as provider_upsert_conn,
};
pub use skill_repo::{SkillAssignmentRow, SkillPackageRow, SkillRepo};
pub use usage_repo::{UsageCursor, UsageRepo};

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

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
        match Self::open_with_lock_retry(db_path) {
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

    fn open_with_lock_retry(db_path: &Path) -> Result<Self> {
        for attempt in 0..migrations::MIGRATION_RETRY_ATTEMPTS {
            match Self::open_inner(db_path) {
                Ok(db) => return Ok(db),
                Err(error)
                    if migrations::is_busy(&error)
                        && attempt + 1 < migrations::MIGRATION_RETRY_ATTEMPTS =>
                {
                    thread::sleep(migrations::MIGRATION_RETRY_DELAY);
                }
                Err(error) => return Err(error),
            }
        }

        unreachable!("database open retry loop always returns")
    }

    fn open_inner(db_path: &Path) -> Result<Self> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(db_path)?;
        // The C busy handler is what SQLite actually waits on; keep the PRAGMA
        // as well so `PRAGMA busy_timeout` readers observe the same value.
        conn.busy_timeout(Duration::from_millis(5000))?;
        conn.execute_batch(
            r#"
            PRAGMA foreign_keys = ON;
            PRAGMA busy_timeout = 5000;
            "#,
        )?;
        set_wal_journal_mode(&conn)?;
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
            if crate::logging::parse_level(&v).is_ok() {
                s.log_level = v.trim().to_ascii_lowercase();
            }
        }
        if let Some(v) = self.get_setting("log_retention_days")? {
            if let Ok(n) = crate::logging::parse_retention_days(&v) {
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
        if let Some(v) = self.get_setting("keep_live_file_copies")? {
            s.keep_live_file_copies = parse_stored_bool(&v);
        }
        if let Some(v) = self.get_setting("warn_duplicate_route_credential")? {
            s.warn_duplicate_route_credential = parse_stored_bool(&v);
        }
        if let Some(v) = self.get_setting("update_duplicate_route_url")? {
            s.update_duplicate_route_url = parse_stored_bool(&v);
        }
        Ok(s)
    }
}

/// Bootstrap read-only peek of `settings` rows.
///
/// Used before `Database::open` (migrations / WAL / app lock). Missing file,
/// open failure, or a missing/unreadable key yields an empty map or omits that key.
pub fn peek_settings(db_file: &Path, keys: &[&str]) -> HashMap<String, String> {
    let mut out = HashMap::new();
    if !db_file.exists() {
        return out;
    }
    let Ok(conn) = Connection::open(db_file) else {
        return out;
    };
    for key in keys {
        if let Ok(value) =
            conn.query_row("SELECT value FROM settings WHERE key = ?1", [*key], |r| {
                r.get::<_, String>(0)
            })
        {
            out.insert((*key).to_string(), value);
        }
    }
    out
}

fn set_wal_journal_mode(conn: &Connection) -> Result<()> {
    for attempt in 0..migrations::MIGRATION_RETRY_ATTEMPTS {
        // `sqlite3_exec` / execute_batch consumes the journal_mode result row.
        match conn.execute_batch("PRAGMA journal_mode = WAL;") {
            Ok(()) => return Ok(()),
            Err(error) => {
                let error = AppError::from(error);
                if migrations::is_busy(&error) && attempt + 1 < migrations::MIGRATION_RETRY_ATTEMPTS
                {
                    thread::sleep(migrations::MIGRATION_RETRY_DELAY);
                    continue;
                }
                return Err(error);
            }
        }
    }

    unreachable!("WAL journal_mode retry loop always returns")
}

fn parse_stored_bool(raw: &str) -> bool {
    match raw.trim().to_ascii_lowercase().as_str() {
        "0" | "false" | "no" | "off" => false,
        _ => true,
    }
}
