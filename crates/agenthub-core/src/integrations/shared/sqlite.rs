//! Read-only open of a third-party SQLite file (ZCode task/usage indexes).
//! Missing files, busy WAL, or unexpected schema → `None` / empty, never panic.

use std::path::Path;
use std::time::Duration;

use chrono::{DateTime, Utc};
use rusqlite::{Connection, OpenFlags};

use crate::logging::targets;
use crate::utils::redact::redact_text;

pub(crate) fn open_readonly(path: &Path) -> Option<Connection> {
    if !path.is_file() {
        return None;
    }
    match Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY) {
        Ok(conn) => {
            let _ = conn.busy_timeout(Duration::from_millis(250));
            Some(conn)
        }
        Err(e) => {
            tracing::warn!(
                module = targets::PROJECT,
                op = "sqlite_open",
                path = %path.display(),
                error = %redact_text(&e.to_string()),
                "readonly sqlite open failed"
            );
            None
        }
    }
}

pub(crate) fn table_exists(conn: &Connection, name: &str) -> bool {
    conn.prepare("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1 LIMIT 1")
        .and_then(|mut stmt| stmt.exists([name]))
        .unwrap_or(false)
}

/// ZCode / WorkBuddy timestamps are milliseconds; older rows may be seconds.
pub(crate) fn epoch_to_rfc3339(raw: i64) -> String {
    let dt = if raw.abs() >= 100_000_000_000 {
        DateTime::from_timestamp_millis(raw)
    } else {
        DateTime::from_timestamp(raw, 0)
    };
    dt.map(|d| d.to_rfc3339())
        .unwrap_or_else(|| Utc::now().to_rfc3339())
}
