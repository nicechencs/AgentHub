//! Disposable usage sqlite (`{data_dir}/cache.db`).
//!
//! Token totals, API / gateway request rows, collect cursors, and per-login
//! usage live here — never in `agenthub.db`. Missing, corrupt, or deleted
//! files never fail the app: reads look empty and the next write recreates
//! the file.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::Connection;

use crate::error::{AppError, Result};
use crate::logging::{self, targets};
use crate::storage::connection_usage::CONNECTION_USAGE_DDL;
use crate::storage::Database;

const USAGE_TABLES: &[&str] = &[
    "usage_records",
    "usage_cursors",
    "usage_parser_health",
    "gateway_usage",
];

const USAGE_SETTING_KEYS: &[&str] = &["usage_token_layout", "usage_grok_parser"];

const CONNECTION_USAGE_TABLES: &[&str] = &["connection_usage_events", "connection_usage"];

/// Open `{data_dir}/cache.db`, recreating it when the file is unreadable.
pub fn open_cache(path: &Path) -> Database {
    match try_open_cache(path) {
        Ok(db) => db,
        Err(error) => {
            logging::log_warn(
                targets::STORAGE,
                "cache_open",
                &format!(
                    "usage cache unreadable path={} error={error}; recreating",
                    path.display()
                ),
            );
            quarantine_sqlite(path);
            match try_open_cache(path) {
                Ok(db) => db,
                Err(retry_error) => {
                    logging::log_warn(
                        targets::STORAGE,
                        "cache_open",
                        &format!(
                            "usage cache recreate failed path={} error={retry_error}; using memory",
                            path.display()
                        ),
                    );
                    Database::open_in_memory().expect("in-memory sqlite")
                }
            }
        }
    }
}

/// Copy usage rows out of the product database, then drop those tables there.
///
/// Failures are logged and never fail hub open. Drop runs only after a
/// successful copy (or when the cache already holds usage data).
pub fn isolate_usage_cache(main: &Database, cache: &Database, main_path: &Path, data_dir: &Path) {
    match extract_usage_from_main(main, cache, main_path) {
        Ok(()) => {
            fold_connection_usage_sidecar(cache, &data_dir.join("connection_usage.db"));
            if let Err(error) = drop_usage_from_main(main) {
                logging::log_warn(
                    targets::STORAGE,
                    "cache_drop",
                    &format!("could not drop usage tables from product db: {error}"),
                );
            }
        }
        Err(error) => {
            logging::log_warn(
                targets::STORAGE,
                "cache_extract",
                &format!("could not move usage rows into cache.db: {error}"),
            );
            fold_connection_usage_sidecar(cache, &data_dir.join("connection_usage.db"));
        }
    }
}

fn try_open_cache(path: &Path) -> Result<Database> {
    let db = Database::try_open(path)?;
    ensure_connection_usage_schema(&db)?;
    crate::logging::log_info(
        targets::STORAGE,
        "cache_open",
        &format!("usage cache opened path={}", path.display()),
    );
    Ok(db)
}

pub(crate) fn ensure_connection_usage_schema(db: &Database) -> Result<()> {
    db.with_conn(|conn| {
        conn.execute_batch(CONNECTION_USAGE_DDL)?;
        Ok(())
    })
}

fn extract_usage_from_main(main: &Database, cache: &Database, main_path: &Path) -> Result<()> {
    if cache_has_usage(cache)? {
        return Ok(());
    }
    if !main_has_usage_tables(main)? {
        return Ok(());
    }
    let Some(path_str) = main_path.to_str() else {
        return Err(AppError::message(
            "cache.extract",
            "product database path is not valid UTF-8",
        ));
    };
    cache.with_conn(|conn| {
        conn.execute("ATTACH DATABASE ?1 AS src", [path_str])?;
        let result = copy_attached_usage(conn);
        let _ = conn.execute("DETACH DATABASE src", []);
        result
    })
}

fn copy_attached_usage(conn: &Connection) -> Result<()> {
    conn.execute_batch("BEGIN IMMEDIATE;")?;
    let result = (|| -> Result<()> {
        for table in USAGE_TABLES {
            copy_table_from_src(conn, table)?;
        }
        conn.execute(
            r#"
            INSERT OR IGNORE INTO settings (key, value)
            SELECT key, value FROM src.settings
            WHERE key IN ('usage_token_layout', 'usage_grok_parser')
            "#,
            [],
        )?;
        Ok(())
    })();
    match result {
        Ok(()) => {
            conn.execute_batch("COMMIT;")?;
            Ok(())
        }
        Err(error) => {
            let _ = conn.execute_batch("ROLLBACK;");
            Err(error)
        }
    }
}

fn copy_table_from_src(conn: &Connection, table: &str) -> Result<()> {
    if !attached_table_exists(conn, "src", table)? {
        return Ok(());
    }
    conn.execute(
        &format!("INSERT OR IGNORE INTO {table} SELECT * FROM src.{table}"),
        [],
    )?;
    Ok(())
}

fn fold_connection_usage_sidecar(cache: &Database, sidecar: &Path) {
    if !sidecar.exists() {
        return;
    }
    let Some(path_str) = sidecar.to_str() else {
        return;
    };
    let folded = cache.with_conn(|conn| {
        conn.execute("ATTACH DATABASE ?1 AS sidecar", [path_str])?;
        let result = (|| -> Result<()> {
            conn.execute_batch("BEGIN IMMEDIATE;")?;
            let copy = (|| -> Result<()> {
                for table in CONNECTION_USAGE_TABLES {
                    if attached_table_exists(conn, "sidecar", table)? {
                        conn.execute(
                            &format!("INSERT OR IGNORE INTO {table} SELECT * FROM sidecar.{table}"),
                            [],
                        )?;
                    }
                }
                Ok(())
            })();
            match copy {
                Ok(()) => conn.execute_batch("COMMIT;")?,
                Err(error) => {
                    let _ = conn.execute_batch("ROLLBACK;");
                    return Err(error);
                }
            }
            Ok(())
        })();
        let _ = conn.execute("DETACH DATABASE sidecar", []);
        result
    });
    match folded {
        Ok(()) => {
            let _ = fs::remove_file(sidecar);
        }
        Err(error) => {
            logging::log_warn(
                targets::STORAGE,
                "cache_fold",
                &format!("could not fold connection_usage.db into cache.db: {error}"),
            );
        }
    }
}

fn drop_usage_from_main(main: &Database) -> Result<()> {
    main.with_conn(|conn| {
        conn.execute_batch(
            r#"
            DROP TABLE IF EXISTS usage_records;
            DROP TABLE IF EXISTS usage_cursors;
            DROP TABLE IF EXISTS usage_parser_health;
            DROP TABLE IF EXISTS gateway_usage;
            DELETE FROM settings WHERE key IN ('usage_token_layout', 'usage_grok_parser');
            "#,
        )?;
        Ok(())
    })
}

fn cache_has_usage(db: &Database) -> Result<bool> {
    db.with_conn(|conn| {
        for table in USAGE_TABLES.iter().chain(CONNECTION_USAGE_TABLES) {
            if table_exists(conn, table)? && table_count(conn, table)? > 0 {
                return Ok(true);
            }
        }
        for key in USAGE_SETTING_KEYS {
            let found: bool = conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM settings WHERE key = ?1)",
                [*key],
                |row| row.get(0),
            )?;
            if found {
                return Ok(true);
            }
        }
        Ok(false)
    })
}

fn main_has_usage_tables(db: &Database) -> Result<bool> {
    db.with_conn(|conn| {
        for table in USAGE_TABLES {
            if table_exists(conn, table)? {
                return Ok(true);
            }
        }
        Ok(false)
    })
}

fn table_exists(conn: &Connection, name: &str) -> rusqlite::Result<bool> {
    conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
        [name],
        |row| row.get(0),
    )
}

fn attached_table_exists(conn: &Connection, schema: &str, name: &str) -> rusqlite::Result<bool> {
    conn.query_row(
        &format!(
            "SELECT EXISTS(SELECT 1 FROM {schema}.sqlite_master WHERE type = 'table' AND name = ?1)"
        ),
        [name],
        |row| row.get(0),
    )
}

fn table_count(conn: &Connection, name: &str) -> rusqlite::Result<i64> {
    conn.query_row(&format!("SELECT COUNT(*) FROM {name}"), [], |row| {
        row.get(0)
    })
}

fn quarantine_sqlite(path: &Path) {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    for extra in ["", "-wal", "-shm", "-journal"] {
        let src = sqlite_sidecar(path, extra);
        if !src.exists() {
            continue;
        }
        let dest = sqlite_sidecar(path, &format!("{extra}.corrupt.{stamp}"));
        if fs::rename(&src, &dest).is_err() {
            let _ = fs::remove_file(&src);
        }
    }
}

fn sqlite_sidecar(path: &Path, extra: &str) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(extra);
    PathBuf::from(name)
}

#[cfg(test)]
mod tests;
