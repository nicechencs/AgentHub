// Ownership: gateway usage task. Persists per-request usage observed by the
// local bridge gateway (table `gateway_usage`, migration 00024).
//
// Inserts are insert-only by `request_id` (ON CONFLICT DO NOTHING): replaying
// a spool file after a crash must be idempotent, so duplicates are ignored
// rather than updated.

use rusqlite::{params, OptionalExtension, Transaction, TransactionBehavior};

use crate::error::Result;
use crate::models::{GatewayUsageOverview, GatewayUsageQuery, GatewayUsageRow};
use crate::storage::Database;

const INSERT_GATEWAY_USAGE_SQL: &str = r#"
    INSERT INTO gateway_usage (
        request_id, ts, profile_id, surface, upstream_channel,
        ticket_id, account_source_kind, account_source_id,
        model, upstream_model,
        input_tokens, output_tokens, cached_input_tokens, reasoning_tokens,
        status, status_code, error_class, latency_ms, ttft_ms, attempts, session_id
    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21)
    ON CONFLICT(request_id) DO NOTHING
"#;

/// `usage_cursors.agent_id` sentinel for gateway spool files. Gateway rows
/// never carry an AgentId, so the shared cursor table stores this literal.
pub(crate) const GATEWAY_CURSOR_AGENT: &str = "gateway";

pub(crate) struct GatewayUsageRepo {
    db: Database,
}

/// Byte cursor for one gateway spool file (mirrors `usage_repo::UsageCursor`
/// without an AgentId).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GatewaySpoolCursor {
    pub path: String,
    pub byte_offset: i64,
    pub file_mtime: i64,
    pub file_size: i64,
}

impl GatewayUsageRepo {
    pub(crate) fn new(db: Database) -> Self {
        Self { db }
    }

    /// Direct batch insert without a cursor (tests / future direct callers).
    #[allow(dead_code)]
    pub(crate) fn insert_batch(&self, rows: &[GatewayUsageRow]) -> Result<u64> {
        if rows.is_empty() {
            return Ok(0);
        }
        self.db.with_conn(|conn| {
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
            let inserted = insert_rows(&tx, rows)?;
            tx.commit()?;
            Ok(inserted)
        })
    }

    /// Insert rows and advance one spool-file cursor in one SQLite transaction.
    ///
    /// Mirrors `usage_repo::insert_batch_and_cursors`: a cursor is only durable
    /// when the corresponding rows are durable too. With `remove_cursor` the
    /// cursor row is dropped instead (the spool file has been deleted).
    pub(crate) fn insert_batch_and_cursor(
        &self,
        rows: &[GatewayUsageRow],
        cursor: &GatewaySpoolCursor,
        remove_cursor: bool,
    ) -> Result<u64> {
        self.db.with_conn(|conn| {
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
            let inserted = insert_rows(&tx, rows)?;
            if remove_cursor {
                tx.execute(
                    "DELETE FROM usage_cursors WHERE path = ?1",
                    params![cursor.path],
                )?;
            } else {
                tx.execute(
                    r#"
                    INSERT INTO usage_cursors (path, agent_id, byte_offset, file_mtime, file_size, updated_at)
                    VALUES (?1, ?2, ?3, ?4, ?5, datetime('now'))
                    ON CONFLICT(path) DO UPDATE SET
                        byte_offset = excluded.byte_offset,
                        file_mtime = excluded.file_mtime,
                        file_size = excluded.file_size,
                        updated_at = excluded.updated_at
                    "#,
                    params![
                        cursor.path,
                        GATEWAY_CURSOR_AGENT,
                        cursor.byte_offset,
                        cursor.file_mtime,
                        cursor.file_size
                    ],
                )?;
            }
            tx.commit()?;
            Ok(inserted)
        })
    }

    pub(crate) fn get_spool_cursor(&self, path: &str) -> Result<Option<GatewaySpoolCursor>> {
        self.db.with_conn(|conn| {
            let row = conn
                .query_row(
                    "SELECT byte_offset, file_mtime, file_size FROM usage_cursors
                     WHERE path = ?1 AND agent_id = ?2",
                    params![path, GATEWAY_CURSOR_AGENT],
                    |row| {
                        Ok(GatewaySpoolCursor {
                            path: path.to_owned(),
                            byte_offset: row.get(0)?,
                            file_mtime: row.get(1)?,
                            file_size: row.get(2)?,
                        })
                    },
                )
                .optional()?;
            Ok(row)
        })
    }

    pub(crate) fn query(&self, q: &GatewayUsageQuery) -> Result<Vec<GatewayUsageRow>> {
        let limit = q.limit.unwrap_or(100_000);
        self.db.with_conn(|conn| {
            let mut sql = String::from(
                r#"
                SELECT request_id, ts, profile_id, surface, upstream_channel,
                       ticket_id, account_source_kind, account_source_id,
                       model, upstream_model,
                       input_tokens, output_tokens, cached_input_tokens, reasoning_tokens,
                       status, status_code, error_class, latency_ms, ttft_ms, attempts, session_id
                FROM gateway_usage
                "#,
            );
            let mut args: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
            sql.push_str(&gateway_filter_clauses(&mut args, q));
            sql.push_str(" ORDER BY ts DESC LIMIT ?");
            args.push(Box::new(limit as i64));

            let mut stmt = conn.prepare(&sql)?;
            let params_ref: Vec<&dyn rusqlite::types::ToSql> =
                args.iter().map(|a| a.as_ref()).collect();
            let rows = stmt.query_map(params_ref.as_slice(), map_row)?;
            let mut out = Vec::new();
            for r in rows {
                out.push(r?);
            }
            Ok(out)
        })
    }

    /// Aggregated overview for the query window. p95 latency uses the
    /// nearest-rank method over the window's latency samples, computed in
    /// Rust (SQLite ships no percentile aggregate).
    pub(crate) fn overview(&self, q: &GatewayUsageQuery) -> Result<GatewayUsageOverview> {
        self.db.with_conn(|conn| {
            let mut sql = String::from(
                r#"
                SELECT COUNT(*),
                       COALESCE(SUM(status = 'ok'), 0),
                       COALESCE(SUM(status <> 'ok'), 0),
                       COALESCE(SUM(input_tokens), 0),
                       COALESCE(SUM(output_tokens), 0),
                       COALESCE(SUM(cached_input_tokens), 0),
                       COALESCE(SUM(reasoning_tokens), 0),
                       AVG(latency_ms),
                       AVG(ttft_ms)
                FROM gateway_usage
                "#,
            );
            let mut args: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
            sql.push_str(&gateway_filter_clauses(&mut args, q));
            let mut stmt = conn.prepare(&sql)?;
            let params_ref: Vec<&dyn rusqlite::types::ToSql> =
                args.iter().map(|a| a.as_ref()).collect();
            let aggregate = stmt.query_row(params_ref.as_slice(), |row| {
                Ok(GatewayUsageOverview {
                    request_count: row.get(0)?,
                    ok_count: row.get(1)?,
                    failed_count: row.get(2)?,
                    input_tokens: row.get(3)?,
                    output_tokens: row.get(4)?,
                    cached_input_tokens: row.get(5)?,
                    reasoning_tokens: row.get(6)?,
                    avg_latency_ms: row.get(7)?,
                    p95_latency_ms: None,
                    avg_ttft_ms: row.get(8)?,
                })
            })?;

            let mut latency_sql = String::from("SELECT latency_ms FROM gateway_usage");
            let mut latency_args: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
            let filter = gateway_filter_clauses(&mut latency_args, q);
            if filter.is_empty() {
                latency_sql.push_str(" WHERE latency_ms IS NOT NULL");
            } else {
                latency_sql.push_str(&filter);
                latency_sql.push_str(" AND latency_ms IS NOT NULL");
            }
            let mut latency_stmt = conn.prepare(&latency_sql)?;
            let params_ref: Vec<&dyn rusqlite::types::ToSql> =
                latency_args.iter().map(|a| a.as_ref()).collect();
            let mut samples: Vec<i64> = latency_stmt
                .query_map(params_ref.as_slice(), |row| row.get(0))?
                .collect::<rusqlite::Result<_>>()?;

            let mut overview = aggregate;
            overview.p95_latency_ms = nearest_rank_p95(&mut samples);
            Ok(overview)
        })
    }
}

fn insert_rows(tx: &Transaction<'_>, rows: &[GatewayUsageRow]) -> rusqlite::Result<u64> {
    let mut inserted = 0u64;
    if rows.is_empty() {
        return Ok(inserted);
    }
    let mut stmt = tx.prepare(INSERT_GATEWAY_USAGE_SQL)?;
    for r in rows {
        let n = stmt.execute(params![
            r.request_id,
            r.ts,
            r.profile_id,
            r.surface,
            r.upstream_channel,
            r.ticket_id,
            r.account_source_kind,
            r.account_source_id,
            r.model,
            r.upstream_model,
            r.input_tokens,
            r.output_tokens,
            r.cached_input_tokens,
            r.reasoning_tokens,
            r.status,
            r.status_code,
            r.error_class,
            r.latency_ms,
            r.ttft_ms,
            r.attempts,
            r.session_id,
        ])?;
        inserted += n as u64;
    }
    Ok(inserted)
}

/// Build the WHERE clause from the query filter, pushing bound args in order.
fn gateway_filter_clauses(
    args: &mut Vec<Box<dyn rusqlite::types::ToSql>>,
    q: &GatewayUsageQuery,
) -> String {
    let mut clauses: Vec<&'static str> = Vec::new();
    if let Some(since) = q.since.as_deref().filter(|s| !s.is_empty()) {
        args.push(Box::new(since.to_string()));
        clauses.push("ts >= ?");
    }
    if let Some(until) = q.until.as_deref().filter(|s| !s.is_empty()) {
        args.push(Box::new(until.to_string()));
        clauses.push("ts <= ?");
    }
    if let Some(profile_id) = q.profile_id.as_deref().filter(|p| !p.is_empty()) {
        args.push(Box::new(profile_id.to_string()));
        clauses.push("profile_id = ?");
    }
    if clauses.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", clauses.join(" AND "))
    }
}

/// p95 via nearest-rank over sorted latency samples; `None` without samples.
fn nearest_rank_p95(samples: &mut [i64]) -> Option<i64> {
    if samples.is_empty() {
        return None;
    }
    samples.sort_unstable();
    let rank = ((samples.len() as f64) * 0.95).ceil().max(1.0) as usize;
    let index = rank.min(samples.len()) - 1;
    Some(samples[index])
}

fn map_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<GatewayUsageRow> {
    Ok(GatewayUsageRow {
        request_id: row.get(0)?,
        ts: row.get(1)?,
        profile_id: row.get(2)?,
        surface: row.get(3)?,
        upstream_channel: row.get(4)?,
        ticket_id: row.get(5)?,
        account_source_kind: row.get(6)?,
        account_source_id: row.get(7)?,
        model: row.get(8)?,
        upstream_model: row.get(9)?,
        input_tokens: row.get(10)?,
        output_tokens: row.get(11)?,
        cached_input_tokens: row.get(12)?,
        reasoning_tokens: row.get(13)?,
        status: row.get(14)?,
        status_code: row.get(15)?,
        error_class: row.get(16)?,
        latency_ms: row.get(17)?,
        ttft_ms: row.get(18)?,
        attempts: row.get(19)?,
        session_id: row.get(20)?,
    })
}

#[cfg(test)]
mod tests;
