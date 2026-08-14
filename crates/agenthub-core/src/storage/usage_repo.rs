//! Usage records + parse cursor repository.

use rusqlite::{params, OptionalExtension};

use crate::error::Result;
use crate::models::{
    AgentId, CollectResult, ParserHealth, UsageQuery, UsageRecord, UsageTrendPoint,
};
use crate::storage::Database;

/// Insert or repair by dedupe key. Token fields are overwritten so a full
/// re-scan can fix corrupted Codex input after the double-peel bug.
const UPSERT_USAGE_SQL: &str = r#"
    INSERT INTO usage_records (
        id, agent_id, account_id, model,
        input_tokens, output_tokens, cache_tokens,
        cost_usd, session_id, ts, raw_hash
    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
    ON CONFLICT(agent_id, session_id, raw_hash) DO UPDATE SET
        model = excluded.model,
        input_tokens = excluded.input_tokens,
        output_tokens = excluded.output_tokens,
        cache_tokens = excluded.cache_tokens,
        cost_usd = excluded.cost_usd,
        ts = excluded.ts
"#;

pub struct UsageRepo {
    db: Database,
}

#[derive(Debug, Clone)]
pub struct UsageCursor {
    pub path: String,
    pub agent_id: AgentId,
    pub byte_offset: i64,
    pub file_mtime: i64,
}

impl UsageRepo {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    pub fn insert_batch(&self, rows: &[UsageRecord]) -> Result<u64> {
        if rows.is_empty() {
            return Ok(0);
        }
        self.db.with_conn(|conn| {
            let tx = conn.unchecked_transaction()?;
            let mut inserted = 0u64;
            {
                let mut stmt = tx.prepare(UPSERT_USAGE_SQL)?;
                for r in rows {
                    let n = stmt.execute(params![
                        r.id,
                        r.agent_id.as_str(),
                        r.account_id,
                        r.model,
                        r.input_tokens,
                        r.output_tokens,
                        r.cache_tokens,
                        r.cost_usd,
                        r.session_id,
                        r.ts,
                        r.raw_hash,
                    ])?;
                    inserted += n as u64;
                }
            }
            tx.commit()?;
            Ok(inserted)
        })
    }

    /// Settings key used for one-time usage token-layout repairs.
    pub fn get_meta(&self, key: &str) -> Result<Option<String>> {
        self.db.get_setting(key)
    }

    pub fn set_meta(&self, key: &str, value: &str) -> Result<()> {
        self.db.set_setting(key, value)
    }

    /// Force next collect to re-read every session file from byte 0.
    pub fn reset_all_cursors(&self) -> Result<u64> {
        self.db.with_conn(|conn| {
            let n = conn.execute(
                "UPDATE usage_cursors SET byte_offset = 0, file_mtime = 0, updated_at = datetime('now')",
                [],
            )?;
            Ok(n as u64)
        })
    }

    /// Drop all usage rows (used when token accounting semantics change).
    /// Cursors should be reset separately so the next collect rebuilds from logs.
    pub fn clear_all_records(&self) -> Result<u64> {
        self.db.with_conn(|conn| {
            let n = conn.execute("DELETE FROM usage_records", [])?;
            Ok(n as u64)
        })
    }

    /// Drop usage rows for one agent (parser rewrite; other agents stay).
    pub fn clear_records_for_agent(&self, agent: AgentId) -> Result<u64> {
        self.db.with_conn(|conn| {
            let n = conn.execute(
                "DELETE FROM usage_records WHERE agent_id = ?1",
                [agent.as_str()],
            )?;
            Ok(n as u64)
        })
    }

    /// Force next collect to re-read this agent's session files from byte 0.
    pub fn reset_cursors_for_agent(&self, agent: AgentId) -> Result<u64> {
        self.db.with_conn(|conn| {
            let n = conn.execute(
                "UPDATE usage_cursors SET byte_offset = 0, file_mtime = 0, updated_at = datetime('now') WHERE agent_id = ?1",
                [agent.as_str()],
            )?;
            Ok(n as u64)
        })
    }

    pub fn query(&self, q: &UsageQuery) -> Result<Vec<UsageRecord>> {
        let days = q.days.max(1) as i64;
        self.db.with_conn(|conn| {
            let mut sql = String::from(
                r#"
                SELECT id, agent_id, account_id, model,
                       input_tokens, output_tokens, cache_tokens,
                       cost_usd, session_id, ts, raw_hash
                FROM usage_records
                WHERE ts >= datetime('now', ?1)
                "#,
            );
            let day_arg = format!("-{days} days");
            let mut args: Vec<Box<dyn rusqlite::types::ToSql>> = vec![Box::new(day_arg)];

            if let Some(agent) = q.agent_id {
                sql.push_str(" AND agent_id = ?");
                args.push(Box::new(agent.as_str().to_string()));
            }
            if let Some(ref model) = q.model {
                if !model.is_empty() && model != "all" {
                    sql.push_str(" AND model = ?");
                    args.push(Box::new(model.clone()));
                }
            }
            // Soft cap: dashboard / stats need the full window; keep a high ceiling
            // so multi-agent heavy weeks are not truncated (was 5000 → undercounted cost).
            sql.push_str(" ORDER BY ts DESC LIMIT 100000");

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

    /// Distinct model names in a look-back window (no row limit — for missing-pricing tips).
    pub fn distinct_models(&self, days: u32) -> Result<Vec<String>> {
        let days = days.max(1) as i64;
        self.db.with_conn(|conn| {
            let day_arg = format!("-{days} days");
            let mut stmt = conn.prepare(
                r#"
                SELECT DISTINCT model FROM usage_records
                WHERE ts >= datetime('now', ?1)
                  AND (input_tokens + output_tokens + cache_tokens) > 0
                ORDER BY model
                "#,
            )?;
            let rows = stmt.query_map(params![day_arg], |row| row.get::<_, String>(0))?;
            let mut out = Vec::new();
            for r in rows {
                out.push(r?);
            }
            Ok(out)
        })
    }

    /// Patch selected usage rows (cost and/or input token layout).
    ///
    /// `patch` returns `Some((new_input, new_cost))` to update, or `None` to skip.
    /// Returns number of rows changed.
    pub fn recompute_costs<F>(&self, mut patch: F) -> Result<u64>
    where
        F: FnMut(AgentId, &str, i64, i64, i64) -> Option<(i64, f64)>,
    {
        self.db.with_conn(|conn| {
            let tx = conn.unchecked_transaction()?;
            let mut changed = 0u64;
            {
                let mut sel = tx.prepare(
                    r#"
                    SELECT id, agent_id, model, input_tokens, output_tokens, cache_tokens,
                           COALESCE(cost_usd, 0)
                    FROM usage_records
                    "#,
                )?;
                let rows = sel.query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, i64>(5)?,
                        row.get::<_, f64>(6)?,
                    ))
                })?;
                let mut updates: Vec<(i64, f64, String)> = Vec::new();
                for r in rows {
                    let (id, agent_s, model, input, output, cache, old_cost) = r?;
                    let Some(agent) = AgentId::parse(&agent_s) else {
                        continue;
                    };
                    let Some((new_input, new_cost)) = patch(agent, &model, input, output, cache)
                    else {
                        continue;
                    };
                    let input_changed = new_input != input;
                    let cost_changed = (new_cost - old_cost).abs() >= 5e-7;
                    if input_changed || cost_changed {
                        updates.push((new_input, new_cost, id));
                    }
                }
                drop(sel);
                let mut upd = tx.prepare(
                    "UPDATE usage_records SET input_tokens = ?1, cost_usd = ?2 WHERE id = ?3",
                )?;
                for (input, cost, id) in updates {
                    upd.execute(params![input, cost, id])?;
                    changed += 1;
                }
            }
            tx.commit()?;
            Ok(changed)
        })
    }

    pub fn trend(&self, days: u32, agent: Option<AgentId>) -> Result<Vec<UsageTrendPoint>> {
        let days = days.max(1) as i64;
        self.db.with_conn(|conn| {
            let day_arg = format!("-{days} days");
            let (sql, bind_agent): (String, Option<String>) = match agent {
                Some(a) => (
                    r#"
                    SELECT substr(ts, 1, 10) AS day, agent_id,
                           SUM(input_tokens + output_tokens) AS tokens
                    FROM usage_records
                    WHERE ts >= datetime('now', ?1) AND agent_id = ?2
                    GROUP BY day, agent_id
                    ORDER BY day
                    "#
                    .into(),
                    Some(a.as_str().into()),
                ),
                None => (
                    r#"
                    SELECT substr(ts, 1, 10) AS day, agent_id,
                           SUM(input_tokens + output_tokens) AS tokens
                    FROM usage_records
                    WHERE ts >= datetime('now', ?1)
                    GROUP BY day, agent_id
                    ORDER BY day
                    "#
                    .into(),
                    None,
                ),
            };

            let mut stmt = conn.prepare(&sql)?;
            let mut map: std::collections::BTreeMap<String, UsageTrendPoint> =
                std::collections::BTreeMap::new();

            if let Some(a) = bind_agent {
                let mut rows = stmt.query(params![day_arg, a])?;
                while let Some(row) = rows.next()? {
                    let day: String = row.get(0)?;
                    let agent_s: String = row.get(1)?;
                    let tokens: i64 = row.get(2)?;
                    let point = map
                        .entry(day.clone())
                        .or_insert_with(|| UsageTrendPoint::new(day));
                    if let Some(aid) = AgentId::parse(&agent_s) {
                        point.add_tokens(aid, tokens);
                    }
                }
            } else {
                let mut rows = stmt.query(params![day_arg])?;
                while let Some(row) = rows.next()? {
                    let day: String = row.get(0)?;
                    let agent_s: String = row.get(1)?;
                    let tokens: i64 = row.get(2)?;
                    let point = map
                        .entry(day.clone())
                        .or_insert_with(|| UsageTrendPoint::new(day));
                    if let Some(aid) = AgentId::parse(&agent_s) {
                        point.add_tokens(aid, tokens);
                    }
                }
            }
            Ok(map.into_values().collect())
        })
    }

    pub fn list_models(&self) -> Result<Vec<String>> {
        self.db.with_conn(|conn| {
            let mut stmt = conn.prepare(
                r#"
                SELECT DISTINCT model FROM usage_records
                WHERE model IS NOT NULL AND model != ''
                ORDER BY model
                "#,
            )?;
            let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
            let mut out = Vec::new();
            for r in rows {
                out.push(r?);
            }
            Ok(out)
        })
    }

    pub fn count_by_agent(&self) -> Result<Vec<(AgentId, u64)>> {
        self.db.with_conn(|conn| {
            let mut stmt =
                conn.prepare("SELECT agent_id, COUNT(*) FROM usage_records GROUP BY agent_id")?;
            let rows = stmt.query_map([], |row| {
                let a: String = row.get(0)?;
                let n: i64 = row.get(1)?;
                Ok((a, n as u64))
            })?;
            let mut out = Vec::new();
            for r in rows {
                let (a, n) = r?;
                if let Some(id) = AgentId::parse(&a) {
                    out.push((id, n));
                }
            }
            Ok(out)
        })
    }

    pub fn get_cursor(&self, path: &str) -> Result<Option<UsageCursor>> {
        self.db.with_conn(|conn| {
            conn.query_row(
                "SELECT path, agent_id, byte_offset, file_mtime FROM usage_cursors WHERE path = ?1",
                [path],
                |row| {
                    let agent_s: String = row.get(1)?;
                    let agent = AgentId::parse(&agent_s).ok_or_else(|| {
                        rusqlite::Error::InvalidColumnType(
                            1,
                            "agent_id".into(),
                            rusqlite::types::Type::Text,
                        )
                    })?;
                    Ok(UsageCursor {
                        path: row.get(0)?,
                        agent_id: agent,
                        byte_offset: row.get(2)?,
                        file_mtime: row.get(3)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
        })
    }

    pub fn upsert_cursor(&self, cursor: &UsageCursor) -> Result<()> {
        self.db.with_conn(|conn| {
            conn.execute(
                r#"
                INSERT INTO usage_cursors (path, agent_id, byte_offset, file_mtime, updated_at)
                VALUES (?1, ?2, ?3, ?4, datetime('now'))
                ON CONFLICT(path) DO UPDATE SET
                    byte_offset = excluded.byte_offset,
                    file_mtime = excluded.file_mtime,
                    updated_at = excluded.updated_at
                "#,
                params![
                    cursor.path,
                    cursor.agent_id.as_str(),
                    cursor.byte_offset,
                    cursor.file_mtime
                ],
            )?;
            Ok(())
        })
    }

    pub fn save_parser_health(&self, rows: &[ParserHealth]) -> Result<()> {
        self.db.with_conn(|conn| {
            let tx = conn.unchecked_transaction()?;
            for row in rows {
                tx.execute(
                    r#"
                    INSERT INTO usage_parser_health (
                        agent_id, supported, records, fail_rate_pct, skipped, updated_at
                    ) VALUES (?1, ?2, ?3, ?4, ?5, datetime('now'))
                    ON CONFLICT(agent_id) DO UPDATE SET
                        supported = excluded.supported,
                        records = excluded.records,
                        fail_rate_pct = excluded.fail_rate_pct,
                        skipped = excluded.skipped,
                        updated_at = excluded.updated_at
                    "#,
                    params![
                        row.agent_id.as_str(),
                        row.supported as i64,
                        row.records as i64,
                        row.fail_rate_pct,
                        row.skipped.map(|v| v as i64),
                    ],
                )?;
            }
            tx.commit()?;
            Ok(())
        })
    }

    pub fn load_parser_health(&self) -> Result<Vec<ParserHealth>> {
        self.db.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT agent_id, supported, records, fail_rate_pct, skipped \
                 FROM usage_parser_health ORDER BY agent_id",
            )?;
            let rows = stmt.query_map([], |row| {
                let agent_s: String = row.get(0)?;
                let agent_id = AgentId::parse(&agent_s).ok_or_else(|| {
                    rusqlite::Error::InvalidColumnType(
                        0,
                        "agent_id".into(),
                        rusqlite::types::Type::Text,
                    )
                })?;
                let records: i64 = row.get(2)?;
                let skipped: Option<i64> = row.get(4)?;
                Ok(ParserHealth {
                    agent_id,
                    supported: row.get::<_, i64>(1)? != 0,
                    records: records.max(0) as u64,
                    fail_rate_pct: row.get(3)?,
                    skipped: skipped.map(|v| v.max(0) as u64),
                })
            })?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row?);
            }
            Ok(out)
        })
    }

    /// Empty collect result helper for tests / no-op agents.
    pub fn empty_collect() -> CollectResult {
        CollectResult {
            inserted: 0,
            skipped: 0,
            failed: 0,
            agents: vec![],
            missing_pricing_models: vec![],
        }
    }

    pub fn parser_health_from_counts(
        supported: &[(AgentId, bool)],
        counts: &[(AgentId, u64)],
        skipped: &[(AgentId, u64)],
        failed: &[(AgentId, u64)],
    ) -> Vec<ParserHealth> {
        supported
            .iter()
            .map(|(agent, is_supported)| {
                let records = counts
                    .iter()
                    .find(|(a, _)| a == agent)
                    .map(|(_, n)| *n)
                    .unwrap_or(0);
                let sk = skipped
                    .iter()
                    .find(|(a, _)| a == agent)
                    .map(|(_, n)| *n)
                    .unwrap_or(0);
                let fl = failed
                    .iter()
                    .find(|(a, _)| a == agent)
                    .map(|(_, n)| *n)
                    .unwrap_or(0);
                let total_attempt = records + sk + fl;
                let fail_rate = if total_attempt > 0 && fl > 0 {
                    Some(((fl as f64) / (total_attempt as f64) * 100.0).round())
                } else {
                    None
                };
                ParserHealth {
                    agent_id: *agent,
                    supported: *is_supported,
                    records,
                    fail_rate_pct: fail_rate,
                    skipped: if sk > 0 { Some(sk) } else { None },
                }
            })
            .collect()
    }
}

fn map_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<UsageRecord> {
    let agent_s: String = row.get(1)?;
    let agent = AgentId::parse(&agent_s).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(
            1,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("unknown agent_id: {agent_s}"),
            )),
        )
    })?;
    Ok(UsageRecord {
        id: row.get(0)?,
        agent_id: agent,
        account_id: row.get(2)?,
        model: row.get::<_, Option<String>>(3)?.unwrap_or_default(),
        input_tokens: row.get(4)?,
        output_tokens: row.get(5)?,
        cache_tokens: row.get(6)?,
        cost_usd: row.get(7)?,
        session_id: row.get(8)?,
        ts: row.get(9)?,
        raw_hash: row.get(10)?,
    })
}

impl UsageRepo {
    /// Insert usage rows and advance their file cursors in one SQLite transaction.
    ///
    /// A cursor is only durable when the corresponding rows are durable too;
    /// otherwise a failed insert could make the next collection skip data.
    pub fn insert_batch_and_cursors(
        &self,
        rows: &[UsageRecord],
        cursors: &[UsageCursor],
    ) -> Result<u64> {
        self.db.with_conn(|conn| {
            let tx = conn.unchecked_transaction()?;
            let mut inserted = 0u64;
            if !rows.is_empty() {
                // UPSERT: re-parse after cursor reset repairs double-peeled Codex
                // input_tokens (INSERT OR IGNORE would leave corrupted rows forever).
                let mut stmt = tx.prepare(UPSERT_USAGE_SQL)?;
                for r in rows {
                    inserted += stmt.execute(params![
                        r.id,
                        r.agent_id.as_str(),
                        r.account_id,
                        r.model,
                        r.input_tokens,
                        r.output_tokens,
                        r.cache_tokens,
                        r.cost_usd,
                        r.session_id,
                        r.ts,
                        r.raw_hash,
                    ])? as u64;
                }
            }
            for cursor in cursors {
                tx.execute(
                    r#"
                    INSERT INTO usage_cursors (path, agent_id, byte_offset, file_mtime, updated_at)
                    VALUES (?1, ?2, ?3, ?4, datetime('now'))
                    ON CONFLICT(path) DO UPDATE SET
                        byte_offset = excluded.byte_offset,
                        file_mtime = excluded.file_mtime,
                        updated_at = excluded.updated_at
                    "#,
                    params![
                        cursor.path,
                        cursor.agent_id.as_str(),
                        cursor.byte_offset,
                        cursor.file_mtime
                    ],
                )?;
            }
            tx.commit()?;
            Ok(inserted)
        })
    }
}
