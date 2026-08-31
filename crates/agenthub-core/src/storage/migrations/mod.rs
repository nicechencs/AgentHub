use std::thread;
use std::time::Duration;

use rusqlite::{Connection, Error as SqliteError, Transaction, TransactionBehavior};

use crate::error::{AppError, Result};

pub(super) const MIGRATION_RETRY_ATTEMPTS: usize = 16;
pub(super) const MIGRATION_RETRY_DELAY: Duration = Duration::from_millis(50);

const MIGRATIONS: &[(&str, &str)] = &[
    ("0001_init", include_str!("0001_init.sql")),
    ("0002_chat", include_str!("0002_chat.sql")),
    (
        "0003_drop_unused_skills",
        include_str!("0003_drop_unused_skills.sql"),
    ),
    ("0004_log_settings", include_str!("0004_log_settings.sql")),
    ("0005_usage_cursors", include_str!("0005_usage_cursors.sql")),
    (
        "0006_usage_cost_usd",
        include_str!("0006_usage_cost_usd.sql"),
    ),
    (
        "0007_usage_parser_health",
        include_str!("0007_usage_parser_health.sql"),
    ),
    ("0008_operations", include_str!("0008_operations.sql")),
    (
        "0009_agent_active_bindings",
        include_str!("0009_agent_active_bindings.sql"),
    ),
    (
        "00010_skill_assignments",
        include_str!("00010_skill_assignments.sql"),
    ),
    (
        "00011_connection_trash",
        include_str!("00011_connection_trash.sql"),
    ),
    (
        "00012_adapter_profiles",
        include_str!("00012_adapter_profiles.sql"),
    ),
    (
        "00013_adapter_bridge_profiles",
        include_str!("00013_adapter_bridge_profiles.sql"),
    ),
    (
        "00014_adapter_profile_mode",
        include_str!("00014_adapter_profile_mode.sql"),
    ),
    (
        "00015_chat_native_session",
        include_str!("00015_chat_native_session.sql"),
    ),
    ("00016_route_pools", include_str!("00016_route_pools.sql")),
    ("00017_usage_fast", include_str!("00017_usage_fast.sql")),
    (
        "00018_usage_cache_read_write",
        include_str!("00018_usage_cache_read_write.sql"),
    ),
    (
        "00019_model_route_rules",
        include_str!("00019_model_route_rules.sql"),
    ),
    (
        "00020_keep_live_file_copies",
        include_str!("00020_keep_live_file_copies.sql"),
    ),
    (
        "00021_usage_dedup_nulls",
        include_str!("00021_usage_dedup_nulls.sql"),
    ),
    (
        "00022_route_duplicate_settings",
        include_str!("00022_route_duplicate_settings.sql"),
    ),
    (
        "00023_live_write_fingerprints",
        include_str!("00023_live_write_fingerprints.sql"),
    ),
    (
        "00024_gateway_usage",
        include_str!("00024_gateway_usage.sql"),
    ),
];

pub fn run(conn: &Connection) -> Result<()> {
    for attempt in 0..MIGRATION_RETRY_ATTEMPTS {
        match run_once(conn, MIGRATIONS) {
            Ok(()) => return Ok(()),
            Err(error) if is_busy(&error) && attempt + 1 < MIGRATION_RETRY_ATTEMPTS => {
                thread::sleep(MIGRATION_RETRY_DELAY);
            }
            Err(error) => return Err(error),
        }
    }

    unreachable!("migration retry loop always returns")
}

/// Run the pending migration list while holding one SQLite write transaction.
///
/// `BEGIN EXCLUSIVE` makes the schema check and every DDL/DML/version marker
/// update observe one serialized writer, including against concurrent readers
/// still converting the file to WAL. If any migration fails, dropping the
/// transaction rolls back the complete batch, including `schema_migrations`
/// creation and all earlier migration steps in this invocation.
fn run_once(conn: &Connection, migrations: &[(&str, &str)]) -> Result<()> {
    let tx = Transaction::new_unchecked(conn, TransactionBehavior::Exclusive)?;
    tx.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS schema_migrations (
            version TEXT PRIMARY KEY,
            applied_at TEXT NOT NULL DEFAULT (datetime('now'))
        );
        "#,
    )?;

    for (version, sql) in migrations {
        let already: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE version = ?1)",
            [version],
            |row| row.get(0),
        )?;
        if already {
            continue;
        }
        apply_migration_in_transaction(&tx, version, sql)?;
    }
    tx.commit()?;
    Ok(())
}

/// Applies one migration and records its version as one indivisible database
/// change. This helper remains useful for focused migration tests; production
/// startup uses `run_once`, which wraps the complete pending batch in one
/// `BEGIN IMMEDIATE` transaction.
// Referenced only from `tests.rs` in this crate; keep for test coverage.
#[allow(dead_code)]
fn apply_migration(conn: &Connection, version: &str, sql: &str) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    apply_migration_in_transaction(&tx, version, sql)?;
    tx.commit()?;
    Ok(())
}

fn apply_migration_in_transaction(conn: &Connection, version: &str, sql: &str) -> Result<()> {
    conn.execute_batch(sql)?;
    conn.execute(
        "INSERT INTO schema_migrations (version) VALUES (?1)",
        [version],
    )?;
    Ok(())
}

pub(super) fn is_busy(error: &AppError) -> bool {
    match error {
        AppError::Db(err) => sqlite_error_is_busy(err),
        AppError::Io(err) => io_error_is_lock(err),
        _ => false,
    }
}

fn sqlite_error_is_busy(error: &SqliteError) -> bool {
    match error {
        SqliteError::SqliteFailure(sqlite_error, _) => matches!(
            sqlite_error.code,
            rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked
        ),
        _ => false,
    }
}

fn io_error_is_lock(error: &std::io::Error) -> bool {
    if matches!(
        error.kind(),
        std::io::ErrorKind::WouldBlock
            | std::io::ErrorKind::TimedOut
            | std::io::ErrorKind::Interrupted
    ) {
        return true;
    }
    // Raw errno values are platform-specific: 32 is EPIPE on Unix but
    // ERROR_SHARING_VIOLATION on Windows, and 11/16 mean entirely different
    // things per platform. Only compare codes guarded by the target OS.
    #[cfg(unix)]
    {
        const EINTR: i32 = 4;
        const EAGAIN: i32 = 11;
        const EBUSY: i32 = 16;
        matches!(error.raw_os_error(), Some(EINTR | EAGAIN | EBUSY))
    }
    #[cfg(windows)]
    {
        const ERROR_SHARING_VIOLATION: i32 = 32;
        const ERROR_LOCK_VIOLATION: i32 = 33;
        matches!(
            error.raw_os_error(),
            Some(ERROR_SHARING_VIOLATION | ERROR_LOCK_VIOLATION)
        )
    }
    #[cfg(not(any(unix, windows)))]
    {
        false
    }
}

#[cfg(test)]
mod tests;
