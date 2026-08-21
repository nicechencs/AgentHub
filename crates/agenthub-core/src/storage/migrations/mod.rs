use std::thread;
use std::time::Duration;

use rusqlite::{Connection, Error as SqliteError, Transaction, TransactionBehavior};

use crate::error::{AppError, Result};

const MIGRATION_RETRY_ATTEMPTS: usize = 3;
const MIGRATION_RETRY_DELAY: Duration = Duration::from_millis(50);

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
];

pub fn run(conn: &Connection) -> Result<()> {
    for attempt in 0..MIGRATION_RETRY_ATTEMPTS {
        match run_once(conn, MIGRATIONS) {
            Ok(()) => return Ok(()),
            Err(error)
                if is_busy(&error) && attempt + 1 < MIGRATION_RETRY_ATTEMPTS =>
            {
                thread::sleep(MIGRATION_RETRY_DELAY);
            }
            Err(error) => return Err(error),
        }
    }

    unreachable!("migration retry loop always returns")
}

/// Run the pending migration list while holding one SQLite write transaction.
///
/// `BEGIN IMMEDIATE` makes the schema check and every DDL/DML/version marker
/// update observe one serialized writer. If any migration fails, dropping the
/// transaction rolls back the complete batch, including `schema_migrations`
/// creation and all earlier migration steps in this invocation.
fn run_once(conn: &Connection, migrations: &[(&str, &str)]) -> Result<()> {
    let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
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
fn apply_migration(conn: &Connection, version: &str, sql: &str) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    apply_migration_in_transaction(&tx, version, sql)?;
    tx.commit()?;
    Ok(())
}

fn apply_migration_in_transaction(
    conn: &Connection,
    version: &str,
    sql: &str,
) -> Result<()> {
    conn.execute_batch(sql)?;
    conn.execute(
        "INSERT INTO schema_migrations (version) VALUES (?1)",
        [version],
    )?;
    Ok(())
}

fn is_busy(error: &AppError) -> bool {
    matches!(
        error,
        AppError::Db(SqliteError::SqliteFailure(sqlite_error, _))
            if matches!(
                sqlite_error.code,
                rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked
            )
    )
}

#[cfg(test)]
mod tests;
