//! Per-connection token totals.
//!
//! Production writes go to `{data_dir}/cache.db` (shared with dashboard /
//! gateway usage). Missing, corrupt, or deleted cache files never fail the
//! app — reads look empty and the next write recreates the file. Tests may
//! still open a dedicated sqlite path.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use rusqlite::{params, Connection, OptionalExtension};

use crate::error::Result;
use crate::logging::targets;
use crate::models::{
    ticket_id, AdapterSourceKind, AgentId, ConnectionUsageSummary, GatewayUsageRow, UsageRecord,
};
use crate::storage::Database;
use crate::utils::redact::redact_text;

pub(crate) const CONNECTION_USAGE_DDL: &str = r#"
CREATE TABLE IF NOT EXISTS connection_usage_events (
    event_key TEXT PRIMARY KEY,
    ticket_id TEXT NOT NULL,
    input_tokens INTEGER NOT NULL,
    output_tokens INTEGER NOT NULL,
    cache_read_tokens INTEGER NOT NULL DEFAULT 0,
    cache_write_tokens INTEGER NOT NULL DEFAULT 0,
    ts TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS connection_usage (
    ticket_id TEXT PRIMARY KEY,
    input_tokens INTEGER NOT NULL DEFAULT 0,
    output_tokens INTEGER NOT NULL DEFAULT 0,
    cache_read_tokens INTEGER NOT NULL DEFAULT 0,
    cache_write_tokens INTEGER NOT NULL DEFAULT 0,
    last_used_at TEXT,
    updated_at TEXT
);
CREATE INDEX IF NOT EXISTS idx_cue_ticket ON connection_usage_events(ticket_id);
"#;

#[derive(Debug, Clone)]
pub struct ConnectionUsageEvent {
    pub event_key: String,
    pub ticket_id: String,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_write_tokens: i64,
    pub ts: String,
}

enum StoreInner {
    Disabled,
    File {
        path: PathBuf,
        conn: Option<Connection>,
    },
    Shared(Database),
}

/// Fail-open store. Clone is cheap (`Arc`).
#[derive(Clone)]
pub struct ConnectionUsageStore {
    inner: Arc<Mutex<StoreInner>>,
}

impl ConnectionUsageStore {
    pub fn disabled() -> Self {
        Self {
            inner: Arc::new(Mutex::new(StoreInner::Disabled)),
        }
    }

    pub fn open(path: PathBuf) -> Self {
        Self {
            inner: Arc::new(Mutex::new(StoreInner::File { path, conn: None })),
        }
    }

    pub fn from_database(db: Database) -> Self {
        let _ = db.with_conn(|conn| {
            conn.execute_batch(CONNECTION_USAGE_DDL)?;
            Ok(())
        });
        Self {
            inner: Arc::new(Mutex::new(StoreInner::Shared(db))),
        }
    }

    pub fn record(&self, events: &[ConnectionUsageEvent]) {
        if events.is_empty() {
            return;
        }
        if self
            .with_conn(|conn| persist_events(conn, events))
            .is_none()
        {
            tracing::debug!(
                module = targets::USAGE,
                op = "connection_usage_record",
                "connection usage sidecar skipped (missing or unreadable)"
            );
        }
    }

    pub fn record_log_rows(&self, ticket_id: &str, rows: &[UsageRecord]) {
        let events: Vec<ConnectionUsageEvent> = rows
            .iter()
            .filter_map(|row| {
                let key = row.raw_hash.as_deref()?.trim();
                if key.is_empty() {
                    return None;
                }
                Some(ConnectionUsageEvent {
                    event_key: format!("log:{key}"),
                    ticket_id: ticket_id.to_string(),
                    input_tokens: row.input_tokens,
                    output_tokens: row.output_tokens,
                    cache_read_tokens: row.cache_read_tokens,
                    cache_write_tokens: row.cache_write_tokens,
                    ts: row.ts.clone(),
                })
            })
            .collect();
        self.record(&events);
    }

    pub fn record_gateway(&self, rows: &[GatewayUsageRow]) {
        let events: Vec<ConnectionUsageEvent> = rows
            .iter()
            .filter_map(|row| {
                let ticket = ticket_id_from_gateway(row)?;
                let key = row.request_id.trim();
                if key.is_empty() {
                    return None;
                }
                Some(ConnectionUsageEvent {
                    event_key: format!("gw:{key}"),
                    ticket_id: ticket,
                    input_tokens: row.input_tokens,
                    output_tokens: row.output_tokens,
                    cache_read_tokens: row.cached_input_tokens.unwrap_or(0),
                    cache_write_tokens: 0,
                    ts: row.ts.clone(),
                })
            })
            .collect();
        self.record(&events);
    }

    pub fn list_summaries(&self) -> Vec<ConnectionUsageSummary> {
        self.with_conn(load_summaries).unwrap_or_default()
    }

    fn with_conn<T>(&self, f: impl Fn(&Connection) -> rusqlite::Result<T>) -> Option<T> {
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        match &mut *inner {
            StoreInner::Disabled => None,
            StoreInner::Shared(db) => db.with_conn(|conn| f(conn).map_err(Into::into)).ok(),
            StoreInner::File { path, conn } => with_file_conn(path, conn, f),
        }
    }
}

fn with_file_conn<T>(
    path: &Path,
    conn: &mut Option<Connection>,
    f: impl Fn(&Connection) -> rusqlite::Result<T>,
) -> Option<T> {
    if conn.is_none() {
        match open_schema(path) {
            Ok(opened) => *conn = Some(opened),
            Err(error) => {
                tracing::debug!(
                    module = targets::USAGE,
                    op = "connection_usage_open",
                    error = %redact_text(&error.to_string()),
                    "connection usage sidecar unavailable"
                );
                return None;
            }
        }
    }
    let opened = conn.as_ref()?;
    match f(opened) {
        Ok(value) => Some(value),
        Err(error) => {
            tracing::debug!(
                module = targets::USAGE,
                op = "connection_usage_op",
                error = %redact_text(&error.to_string()),
                "connection usage sidecar op failed; will reopen"
            );
            *conn = None;
            match open_schema(path) {
                Ok(opened) => {
                    let result = f(&opened).ok();
                    *conn = Some(opened);
                    result
                }
                Err(_) => None,
            }
        }
    }
}

fn open_schema(path: &Path) -> rusqlite::Result<Connection> {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let conn = Connection::open(path)?;
    conn.execute_batch("PRAGMA journal_mode = DELETE;")?;
    conn.execute_batch(CONNECTION_USAGE_DDL)?;
    Ok(conn)
}

fn persist_events(conn: &Connection, events: &[ConnectionUsageEvent]) -> rusqlite::Result<()> {
    let mut insert_event = conn.prepare(
        r#"
        INSERT OR IGNORE INTO connection_usage_events (
            event_key, ticket_id, input_tokens, output_tokens,
            cache_read_tokens, cache_write_tokens, ts
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
        "#,
    )?;
    let mut upsert = conn.prepare(
        r#"
        INSERT INTO connection_usage (
            ticket_id, input_tokens, output_tokens, cache_read_tokens, cache_write_tokens,
            last_used_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, datetime('now'))
        ON CONFLICT(ticket_id) DO UPDATE SET
            input_tokens = input_tokens + excluded.input_tokens,
            output_tokens = output_tokens + excluded.output_tokens,
            cache_read_tokens = cache_read_tokens + excluded.cache_read_tokens,
            cache_write_tokens = cache_write_tokens + excluded.cache_write_tokens,
            last_used_at = CASE
                WHEN excluded.last_used_at IS NOT NULL
                     AND (last_used_at IS NULL OR excluded.last_used_at > last_used_at)
                THEN excluded.last_used_at
                ELSE last_used_at
            END,
            updated_at = excluded.updated_at
        "#,
    )?;
    for event in events {
        let ticket = event.ticket_id.trim();
        if ticket.is_empty() {
            continue;
        }
        let added = insert_event.execute(params![
            event.event_key,
            ticket,
            event.input_tokens,
            event.output_tokens,
            event.cache_read_tokens,
            event.cache_write_tokens,
            event.ts,
        ])?;
        if added == 0 {
            continue;
        }
        upsert.execute(params![
            ticket,
            event.input_tokens,
            event.output_tokens,
            event.cache_read_tokens,
            event.cache_write_tokens,
            event.ts,
        ])?;
    }
    Ok(())
}

fn load_summaries(conn: &Connection) -> rusqlite::Result<Vec<ConnectionUsageSummary>> {
    let mut stmt = conn.prepare(
        r#"
        SELECT ticket_id, input_tokens, output_tokens, cache_read_tokens, cache_write_tokens,
               last_used_at
        FROM connection_usage
        "#,
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(ConnectionUsageSummary {
            ticket_id: row.get(0)?,
            input_tokens: row.get(1)?,
            output_tokens: row.get(2)?,
            cache_read_tokens: row.get(3)?,
            cache_write_tokens: row.get(4)?,
            last_used_at: row.get(5)?,
        })
    })?;
    rows.collect()
}

pub(crate) fn ticket_id_from_gateway(row: &GatewayUsageRow) -> Option<String> {
    if let Some(id) = row
        .ticket_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return Some(id.to_string());
    }
    let kind = AdapterSourceKind::parse(row.account_source_kind.as_deref().unwrap_or(""))?;
    let source = row.account_source_id.as_deref()?.trim();
    if source.is_empty() {
        return None;
    }
    Some(ticket_id(kind, source))
}

/// Current login for an agent (`account:<id>` or `provider:<id>`). Read-only.
pub(crate) fn current_ticket_id_for_agent(db: &Database, agent: AgentId) -> Option<String> {
    db.with_conn(|conn| -> Result<Option<String>> {
        let account: Option<String> = conn
            .query_row(
                "SELECT id FROM accounts WHERE agent_id = ?1 AND is_current != 0 LIMIT 1",
                [agent.as_str()],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(id) = account.filter(|s| !s.trim().is_empty()) {
            return Ok(Some(ticket_id(AdapterSourceKind::Account, &id)));
        }
        let provider: Option<String> = conn
            .query_row(
                "SELECT id FROM providers WHERE agent_id = ?1 AND is_current != 0 LIMIT 1",
                [agent.as_str()],
                |row| row.get(0),
            )
            .optional()?;
        Ok(provider
            .filter(|s| !s.trim().is_empty())
            .map(|id| ticket_id(AdapterSourceKind::Provider, &id)))
    })
    .ok()
    .flatten()
}

#[cfg(test)]
mod tests;
