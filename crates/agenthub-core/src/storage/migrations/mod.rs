use rusqlite::Connection;

use crate::error::Result;

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
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS schema_migrations (
            version TEXT PRIMARY KEY,
            applied_at TEXT NOT NULL DEFAULT (datetime('now'))
        );
        "#,
    )?;

    for (version, sql) in MIGRATIONS {
        let already: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE version = ?1)",
            [version],
            |row| row.get(0),
        )?;
        if already {
            continue;
        }
        apply_migration(conn, version, sql)?;
    }
    Ok(())
}

/// Applies a migration and records its version as one indivisible database
/// change.  If either the SQL script or marker insert fails, dropping the
/// transaction rolls the whole migration back.
fn apply_migration(conn: &Connection, version: &str, sql: &str) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    tx.execute_batch(sql)?;
    tx.execute(
        "INSERT INTO schema_migrations (version) VALUES (?1)",
        [version],
    )?;
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests;
