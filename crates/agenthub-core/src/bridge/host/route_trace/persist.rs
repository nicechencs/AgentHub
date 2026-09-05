//! Disposable SQLite store for Activity / route monitoring traces.
//!
//! Lives next to, not inside, `agenthub.db`. Deleting this file only clears
//! monitoring history. Writes never fail the request path. Retention follows
//! `log_retention_days` in settings (days, not row count).

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection};

use super::{
    trace_matches_query, RouteRequestTrace, RouteTracePage, RouteTraceQuery, ROUTE_TRACE_CAP,
};
use crate::logging::{parse_retention_days, targets};
use crate::storage::peek_settings;

const SCHEMA_VERSION: i32 = 2;
const UNAUTHENTICATED_PROFILE_ID: &str = "";
const MS_PER_DAY: i64 = 86_400_000;

pub(super) struct RouteTraceDb {
    conn: Connection,
    path: PathBuf,
    fallback_retention_days: u32,
}

pub(super) struct PersistSnapshot {
    pub by_profile: HashMap<String, Vec<RouteRequestTrace>>,
    pub unauthenticated: Vec<RouteRequestTrace>,
}

impl RouteTraceDb {
    /// Open (or recreate) the monitoring sqlite file. Best-effort: `None` means
    /// traces stay in memory only. `fallback_retention_days` is used when the
    /// main settings row is missing.
    pub(super) fn open_with_retention(path: &Path, fallback_retention_days: u32) -> Option<Self> {
        match open_inner(path, fallback_retention_days) {
            Ok(db) => Some(db),
            Err(error) => {
                tracing::warn!(
                    target: targets::ADAPTER,
                    path = %path.display(),
                    error = %error,
                    "route trace sqlite unreadable; recreating"
                );
                remove_db_files(path);
                match open_inner(path, fallback_retention_days) {
                    Ok(db) => Some(db),
                    Err(error) => {
                        tracing::warn!(
                            target: targets::ADAPTER,
                            path = %path.display(),
                            error = %error,
                            "route trace sqlite unavailable; monitoring stays in-memory"
                        );
                        None
                    }
                }
            }
        }
    }

    pub(super) fn load_recent(&self) -> PersistSnapshot {
        load_recent(&self.conn)
    }

    pub(super) fn upsert(&self, trace: &RouteRequestTrace) {
        if let Err(error) = upsert_row(&self.conn, trace) {
            tracing::warn!(
                target: targets::ADAPTER,
                path = %self.path.display(),
                error = %error,
                "failed to persist route trace"
            );
            return;
        }
        if let Err(error) = prune_older_than(&self.conn, self.retention_days()) {
            tracing::warn!(
                target: targets::ADAPTER,
                path = %self.path.display(),
                error = %error,
                "failed to prune expired route traces"
            );
        }
    }

    pub(super) fn path(&self) -> &Path {
        &self.path
    }

    pub(super) fn delete_ids(&self, ids: &[String]) -> usize {
        match delete_ids(&self.conn, ids) {
            Ok(count) => count,
            Err(error) => {
                tracing::warn!(
                    target: targets::ADAPTER,
                    path = %self.path.display(),
                    error = %error,
                    "failed to delete route traces"
                );
                0
            }
        }
    }

    fn retention_days(&self) -> u32 {
        if let Some(settings_db) = settings_db_for_traces(&self.path) {
            if let Some(raw) =
                peek_settings(&settings_db, &["log_retention_days"]).remove("log_retention_days")
            {
                if let Ok(days) = parse_retention_days(&raw) {
                    return days;
                }
            }
        }
        self.fallback_retention_days
    }

    #[cfg(test)]
    pub(super) fn count(&self, profile_id: &str) -> usize {
        let key = persist_profile_id_key(Some(profile_id));
        self.conn
            .query_row(
                "SELECT COUNT(*) FROM route_traces WHERE profile_id = ?1",
                [key],
                |row| row.get::<_, i64>(0),
            )
            .ok()
            .map(|n| n.max(0) as usize)
            .unwrap_or(0)
    }
}

fn open_inner(path: &Path, fallback_retention_days: u32) -> crate::error::Result<RouteTraceDb> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let conn = Connection::open(path)?;
    conn.busy_timeout(Duration::from_millis(5000))?;
    let _ = conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL;");
    init_schema(&conn)?;
    let db = RouteTraceDb {
        conn,
        path: path.to_path_buf(),
        fallback_retention_days,
    };
    prune_older_than(&db.conn, db.retention_days())?;
    Ok(db)
}

fn init_schema(conn: &Connection) -> crate::error::Result<()> {
    let version: i32 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version != 0 && version != SCHEMA_VERSION {
        conn.execute_batch("DROP TABLE IF EXISTS route_traces;")?;
    }
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS route_traces (
            request_id TEXT PRIMARY KEY,
            profile_id TEXT NOT NULL,
            at_unix_ms INTEGER NOT NULL,
            input_tokens INTEGER,
            output_tokens INTEGER,
            cached_input_tokens INTEGER,
            reasoning_tokens INTEGER,
            payload TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_route_traces_profile_at
            ON route_traces(profile_id, at_unix_ms DESC);
        CREATE INDEX IF NOT EXISTS idx_route_traces_at
            ON route_traces(at_unix_ms);
        "#,
    )?;
    conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    Ok(())
}

fn load_recent(conn: &Connection) -> PersistSnapshot {
    let mut snapshot = PersistSnapshot {
        by_profile: HashMap::new(),
        unauthenticated: Vec::new(),
    };
    let Ok(mut stmt) = conn.prepare(
        r#"
        SELECT profile_id, payload FROM (
            SELECT profile_id, payload,
                   ROW_NUMBER() OVER (
                       PARTITION BY profile_id
                       ORDER BY at_unix_ms DESC, request_id DESC
                   ) AS rn
            FROM route_traces
        )
        WHERE rn <= ?1
        ORDER BY profile_id, rn
        "#,
    ) else {
        return snapshot;
    };
    let Ok(rows) = stmt.query_map([ROUTE_TRACE_CAP as i64], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    }) else {
        return snapshot;
    };
    for row in rows.flatten() {
        let (profile_id, payload) = row;
        let Ok(trace) = serde_json::from_str::<RouteRequestTrace>(&payload) else {
            continue;
        };
        if profile_id.is_empty() {
            snapshot.unauthenticated.push(trace);
        } else {
            snapshot
                .by_profile
                .entry(profile_id)
                .or_default()
                .push(trace);
        }
    }
    snapshot
}

fn upsert_row(conn: &Connection, trace: &RouteRequestTrace) -> crate::error::Result<()> {
    let payload = serde_json::to_string(trace)?;
    let profile_id = persist_profile_id(trace);
    let at_unix_ms = i64::try_from(trace.at_unix_ms).unwrap_or(i64::MAX);
    conn.execute(
        r#"
        INSERT INTO route_traces (
            request_id, profile_id, at_unix_ms,
            input_tokens, output_tokens, cached_input_tokens, reasoning_tokens,
            payload
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
        ON CONFLICT(request_id) DO UPDATE SET
            profile_id = excluded.profile_id,
            at_unix_ms = excluded.at_unix_ms,
            input_tokens = excluded.input_tokens,
            output_tokens = excluded.output_tokens,
            cached_input_tokens = excluded.cached_input_tokens,
            reasoning_tokens = excluded.reasoning_tokens,
            payload = excluded.payload
        "#,
        params![
            trace.request_id,
            profile_id,
            at_unix_ms,
            opt_i64(trace.input_tokens),
            opt_i64(trace.output_tokens),
            opt_i64(trace.cached_input_tokens),
            opt_i64(trace.reasoning_tokens),
            payload
        ],
    )?;
    Ok(())
}

fn prune_older_than(conn: &Connection, retention_days: u32) -> crate::error::Result<()> {
    let cutoff = cutoff_unix_ms(retention_days);
    conn.execute("DELETE FROM route_traces WHERE at_unix_ms < ?1", [cutoff])?;
    Ok(())
}

pub(super) fn query_at_path(path: &Path, query: &RouteTraceQuery) -> RouteTracePage {
    match Connection::open(path) {
        Ok(conn) => {
            let _ = conn.busy_timeout(Duration::from_millis(5000));
            query_or_empty(&conn, path, query)
        }
        Err(error) => {
            tracing::warn!(
                target: targets::ADAPTER,
                path = %path.display(),
                error = %error,
                "failed to open route traces for query"
            );
            RouteTracePage {
                rows: Vec::new(),
                total: 0,
                offset: query.offset,
                limit: query.limit,
            }
        }
    }
}

fn query_or_empty(conn: &Connection, path: &Path, query: &RouteTraceQuery) -> RouteTracePage {
    match query_page(conn, query) {
        Ok(page) => page,
        Err(error) => {
            tracing::warn!(
                target: targets::ADAPTER,
                path = %path.display(),
                error = %error,
                "failed to query route traces"
            );
            RouteTracePage {
                rows: Vec::new(),
                total: 0,
                offset: query.offset,
                limit: query.limit,
            }
        }
    }
}

fn query_page(conn: &Connection, query: &RouteTraceQuery) -> crate::error::Result<RouteTracePage> {
    let mut traces = Vec::new();
    if let Some(profile_id) = query.route_id.as_deref().or(query.pool_id.as_deref()) {
        let mut stmt = conn.prepare(
            "SELECT payload FROM route_traces WHERE profile_id = ?1 ORDER BY at_unix_ms DESC, request_id DESC",
        )?;
        let rows = stmt.query_map([profile_id], |row| row.get::<_, String>(0))?;
        push_payloads(rows, &mut traces);
    } else {
        let mut stmt = conn.prepare(
            "SELECT payload FROM route_traces ORDER BY at_unix_ms DESC, request_id DESC",
        )?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        push_payloads(rows, &mut traces);
    }
    traces.retain(|row| trace_matches_query(row, query));
    let total = traces.len() as u64;
    let start = (query.offset as usize).min(traces.len());
    let end = start.saturating_add(query.limit as usize).min(traces.len());
    Ok(RouteTracePage {
        rows: traces[start..end].to_vec(),
        total,
        offset: query.offset,
        limit: query.limit,
    })
}

fn push_payloads(
    rows: rusqlite::MappedRows<'_, impl FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<String>>,
    traces: &mut Vec<RouteRequestTrace>,
) {
    for payload in rows.flatten() {
        if let Ok(trace) = serde_json::from_str::<RouteRequestTrace>(&payload) {
            traces.push(trace);
        }
    }
}

fn delete_ids(conn: &Connection, ids: &[String]) -> crate::error::Result<usize> {
    if ids.is_empty() {
        return Ok(0);
    }
    let mut deleted = 0usize;
    for chunk in ids.chunks(200) {
        let placeholders = vec!["?"; chunk.len()].join(",");
        let sql = format!("DELETE FROM route_traces WHERE request_id IN ({placeholders})");
        let mut stmt = conn.prepare(&sql)?;
        let params: Vec<&dyn rusqlite::ToSql> =
            chunk.iter().map(|id| id as &dyn rusqlite::ToSql).collect();
        deleted += stmt.execute(params.as_slice())?;
    }
    Ok(deleted)
}

fn cutoff_unix_ms(retention_days: u32) -> i64 {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0);
    now.saturating_sub(i64::from(retention_days).saturating_mul(MS_PER_DAY))
}

fn opt_i64(value: Option<u64>) -> Option<i64> {
    value.and_then(|n| i64::try_from(n).ok())
}

fn persist_profile_id(trace: &RouteRequestTrace) -> &str {
    persist_profile_id_key(trace.profile_id.as_deref())
}

fn persist_profile_id_key(profile_id: Option<&str>) -> &str {
    profile_id
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .unwrap_or(UNAUTHENTICATED_PROFILE_ID)
}

fn settings_db_for_traces(persist_path: &Path) -> Option<PathBuf> {
    let cache_dir = persist_path.parent()?;
    if cache_dir.file_name()?.to_str()? != "cache" {
        return None;
    }
    let db = cache_dir.parent()?.join("agenthub.db");
    db.is_file().then_some(db)
}

fn remove_db_files(path: &Path) {
    let _ = fs::remove_file(path);
    let mut wal = path.as_os_str().to_owned();
    wal.push("-wal");
    let _ = fs::remove_file(PathBuf::from(&wal));
    let mut shm = path.as_os_str().to_owned();
    shm.push("-shm");
    let _ = fs::remove_file(PathBuf::from(&shm));
}
