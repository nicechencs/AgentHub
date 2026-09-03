//! Usage records + parse cursor repository.

use rusqlite::{params, OptionalExtension, Transaction, TransactionBehavior};

use crate::error::Result;
use crate::models::{
    canonical_usage_model, unique_canonical_usage_models, usage_model_filter_aliases, AgentId,
    CollectResult, ParserHealth, UsageDistributionSlice, UsageMetrics, UsageOverview, UsageQuery,
    UsageRecord, UsageTrendPoint,
};
use crate::storage::Database;

/// Insert or repair by dedupe key. Token fields are overwritten so a full
/// re-scan can fix corrupted Codex input after the double-peel bug.
const UPSERT_USAGE_SQL: &str = r#"
    INSERT INTO usage_records (
        id, agent_id, account_id, model,
        input_tokens, output_tokens, cache_read_tokens, cache_write_tokens,
        cost_usd, session_id, ts, raw_hash, fast
    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
    ON CONFLICT(agent_id, ifnull(session_id, ''), ifnull(raw_hash, '')) DO UPDATE SET
        model = excluded.model,
        input_tokens = excluded.input_tokens,
        output_tokens = excluded.output_tokens,
        cache_read_tokens = excluded.cache_read_tokens,
        cache_write_tokens = excluded.cache_write_tokens,
        cost_usd = excluded.cost_usd,
        ts = excluded.ts,
        fast = excluded.fast
"#;

#[derive(Debug, Clone, Copy)]
enum UsageTrendGroup {
    Agent,
    Model,
}

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
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
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
                        r.cache_read_tokens,
                        r.cache_write_tokens,
                        r.cost_usd,
                        r.session_id.as_deref().unwrap_or(""),
                        r.ts,
                        r.raw_hash.as_deref().unwrap_or(""),
                        r.fast,
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
        let limit = q.limit.unwrap_or(100_000);
        self.db.with_conn(|conn| {
            let mut sql = String::from(
                r#"
                SELECT id, agent_id, account_id, model,
                       input_tokens, output_tokens, cache_read_tokens, cache_write_tokens,
                       cost_usd, session_id, ts, raw_hash, fast
                FROM usage_records
                "#,
            );
            let mut args: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
            append_usage_filter(&mut sql, &mut args, q, true);
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

    /// Distinct model names in a look-back window (no row limit — for missing-pricing tips).
    pub fn distinct_models(&self, days: u32) -> Result<Vec<String>> {
        let days = days.max(1) as i64;
        self.db.with_conn(|conn| {
            let day_arg = format!("-{days} days");
            let mut stmt = conn.prepare(
                r#"
                SELECT DISTINCT model FROM usage_records
                WHERE unixepoch(ts) >= unixepoch('now', ?1)
                  AND (input_tokens + output_tokens + cache_read_tokens + cache_write_tokens) > 0
                ORDER BY model
                "#,
            )?;
            let rows = stmt.query_map(params![day_arg], |row| row.get::<_, String>(0))?;
            let mut out = Vec::new();
            for r in rows {
                out.push(r?);
            }
            Ok(unique_canonical_usage_models(out))
        })
    }

    /// Patch selected usage rows (cost and/or input token layout).
    ///
    /// `patch` returns `Some((new_input, new_cost))` to update, or `None` to skip.
    /// Args: agent, model, input, output, cache_read, cache_write, fast.
    /// Returns number of rows changed.
    pub fn recompute_costs<F>(&self, mut patch: F) -> Result<u64>
    where
        F: FnMut(AgentId, &str, i64, i64, i64, i64, bool) -> Option<(i64, f64)>,
    {
        self.db.with_conn(|conn| {
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
            let mut changed = 0u64;
            {
                let mut sel = tx.prepare(
                    r#"
                    SELECT id, agent_id, model, input_tokens, output_tokens,
                           cache_read_tokens, cache_write_tokens,
                           COALESCE(cost_usd, 0), COALESCE(fast, 0)
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
                        row.get::<_, i64>(6)?,
                        row.get::<_, f64>(7)?,
                        row.get::<_, i64>(8)? != 0,
                    ))
                })?;
                let mut updates: Vec<(i64, f64, String)> = Vec::new();
                for r in rows {
                    let (
                        id,
                        agent_s,
                        model,
                        input,
                        output,
                        cache_read,
                        cache_write,
                        old_cost,
                        fast,
                    ) = r?;
                    let Some(agent) = AgentId::parse(&agent_s) else {
                        continue;
                    };
                    let Some((new_input, new_cost)) =
                        patch(agent, &model, input, output, cache_read, cache_write, fast)
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

    /// Token trend for the dashboard area chart.
    ///
    /// Look-back matches `overview` / `query`: rolling `unixepoch('now', '-N days')`
    /// so RFC3339 (`T` / `Z` / offset) and SQLite datetime strings compare as
    /// instants. When `since` is present it is AND-ed (dashboard "today" passes
    /// local midnight). Cards stay rolling; the chart rebuckets included rows
    /// into local hours (`YYYY-MM-DD HH:00`) when `days <= 1`, otherwise local
    /// days (`YYYY-MM-DD`), then fills empty buckets so a 1-day window is not a
    /// single point on a categorical axis.
    pub fn trend(
        &self,
        days: u32,
        agent: Option<AgentId>,
        model: Option<&str>,
        since: Option<&str>,
        exclude_agent_ids: &[AgentId],
    ) -> Result<Vec<UsageTrendPoint>> {
        self.trend_grouped(
            days,
            agent,
            model,
            since,
            exclude_agent_ids,
            UsageTrendGroup::Agent,
        )
    }

    pub fn trend_by_model(
        &self,
        days: u32,
        agent: Option<AgentId>,
        model: Option<&str>,
        since: Option<&str>,
        exclude_agent_ids: &[AgentId],
    ) -> Result<Vec<UsageTrendPoint>> {
        self.trend_grouped(
            days,
            agent,
            model,
            since,
            exclude_agent_ids,
            UsageTrendGroup::Model,
        )
    }

    fn trend_grouped(
        &self,
        days: u32,
        agent: Option<AgentId>,
        model: Option<&str>,
        since: Option<&str>,
        exclude_agent_ids: &[AgentId],
        group: UsageTrendGroup,
    ) -> Result<Vec<UsageTrendPoint>> {
        let q = usage_query_from_parts(days, agent, model, since, exclude_agent_ids);
        let days = q.days.max(1) as i64;
        let grain = TrendGrain::from_days(days);
        let (series_col, group_sql) = match group {
            UsageTrendGroup::Agent => ("agent_id", " GROUP BY ts, agent_id"),
            UsageTrendGroup::Model => ("model", " GROUP BY ts, model"),
        };
        self.db.with_conn(|conn| {
            let mut sql = format!(
                r#"
                    SELECT ts, {series_col},
                           SUM(input_tokens + cache_read_tokens + cache_write_tokens + output_tokens) AS tokens,
                           SUM(COALESCE(cost_usd, 0)) AS cost
                    FROM usage_records
                "#,
            );
            let mut args: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
            append_usage_filter(&mut sql, &mut args, &q, true);
            // Local hour/day buckets need real timestamp parsing, which is
            // done below in Rust; SQL only pre-aggregates per raw ts value.
            sql.push_str(group_sql);

            let mut stmt = conn.prepare(&sql)?;
            let params_ref: Vec<&dyn rusqlite::types::ToSql> =
                args.iter().map(|a| a.as_ref()).collect();
            let mut rows = stmt.query(params_ref.as_slice())?;
            let mut map: std::collections::BTreeMap<String, UsageTrendPoint> =
                std::collections::BTreeMap::new();
            while let Some(row) = rows.next()? {
                let ts: String = row.get(0)?;
                let series: String = row.get(1)?;
                let tokens: i64 = row.get(2)?;
                let cost: f64 = row.get(3)?;
                let Some(bucket) = local_trend_bucket(&ts, grain) else {
                    continue;
                };
                let point = map
                    .entry(bucket.clone())
                    .or_insert_with(|| UsageTrendPoint::new(bucket));
                match group {
                    UsageTrendGroup::Agent => {
                        if let Some(aid) = AgentId::parse(&series) {
                            point.add_tokens(aid, tokens);
                        }
                    }
                    UsageTrendGroup::Model => {
                        let series = canonical_usage_model(&series);
                        if series.is_empty() {
                            continue;
                        }
                        point.add_named_tokens(&series, tokens);
                        point.add_named_cost(&series, cost);
                    }
                }
            }
            if !map.is_empty() {
                fill_trend_window(&mut map, days, q.since.as_deref(), grain);
            }
            Ok(map.into_values().collect())
        })
    }

    /// SQL aggregates for dashboard first paint (metrics + distribution + models).
    ///
    /// `models` uses the same window + agent filter but ignores `model` so the
    /// dropdown stays populated while a model is selected.
    pub fn overview(
        &self,
        days: u32,
        agent: Option<AgentId>,
        model: Option<&str>,
        since: Option<&str>,
        exclude_agent_ids: &[AgentId],
    ) -> Result<UsageOverview> {
        let q = usage_query_from_parts(days, agent, model, since, exclude_agent_ids);
        self.db.with_conn(|conn| {
            let mut metrics_sql = String::from(
                r#"
                    SELECT
                        COALESCE(SUM(input_tokens), 0),
                        COALESCE(SUM(output_tokens), 0),
                        COALESCE(SUM(cache_read_tokens), 0),
                        COALESCE(SUM(cache_write_tokens), 0),
                        COALESCE(SUM(COALESCE(cost_usd, 0)), 0)
                    FROM usage_records
                "#,
            );
            let mut metrics_args: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
            append_usage_filter(&mut metrics_sql, &mut metrics_args, &q, true);

            let metrics = {
                let mut stmt = conn.prepare(&metrics_sql)?;
                let params_ref: Vec<&dyn rusqlite::types::ToSql> =
                    metrics_args.iter().map(|a| a.as_ref()).collect();
                stmt.query_row(params_ref.as_slice(), |row| {
                    Ok(UsageMetrics {
                        billable_input: row.get(0)?,
                        output: row.get(1)?,
                        cache_read: row.get(2)?,
                        cache_write: row.get(3)?,
                        cost_usd: row.get(4)?,
                    })
                })?
            };

            let group_col = if q.agent_id.is_none() {
                "agent_id"
            } else {
                "model"
            };
            let mut dist_sql = format!(
                r#"
                    SELECT {group_col} AS key,
                           SUM(input_tokens + cache_read_tokens + cache_write_tokens + output_tokens) AS tokens,
                           COALESCE(SUM(COALESCE(cost_usd, 0)), 0),
                           COALESCE(SUM(input_tokens), 0),
                           COALESCE(SUM(output_tokens), 0),
                           COALESCE(SUM(cache_read_tokens), 0),
                           COALESCE(SUM(cache_write_tokens), 0)
                    FROM usage_records
                "#
            );
            let mut dist_args: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
            append_usage_filter(&mut dist_sql, &mut dist_args, &q, true);
            dist_sql.push_str(" GROUP BY key ORDER BY tokens DESC");

            let distribution = {
                let mut stmt = conn.prepare(&dist_sql)?;
                let params_ref: Vec<&dyn rusqlite::types::ToSql> =
                    dist_args.iter().map(|a| a.as_ref()).collect();
                let mut rows = stmt.query(params_ref.as_slice())?;
                let mut out = Vec::new();
                while let Some(row) = rows.next()? {
                    let key: String = row.get(0)?;
                    if key.is_empty() {
                        continue;
                    }
                    out.push(UsageDistributionSlice {
                        key,
                        tokens: row.get(1)?,
                        cost_usd: row.get(2)?,
                        billable_input: row.get(3)?,
                        output: row.get(4)?,
                        cache_read: row.get(5)?,
                        cache_write: row.get(6)?,
                    });
                }
                out
            };

            let mut models_sql = String::from(
                r#"
                    SELECT DISTINCT model FROM usage_records
                "#,
            );
            let mut models_args: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
            // Same window / agent / exclude as metrics; skip model so the
            // dropdown stays populated while a model is selected.
            append_usage_filter(&mut models_sql, &mut models_args, &q, false);
            models_sql.push_str(" AND model IS NOT NULL AND model != '' ORDER BY model");

            let models = {
                let mut stmt = conn.prepare(&models_sql)?;
                let params_ref: Vec<&dyn rusqlite::types::ToSql> =
                    models_args.iter().map(|a| a.as_ref()).collect();
                let rows = stmt.query_map(params_ref.as_slice(), |row| row.get::<_, String>(0))?;
                let mut out = Vec::new();
                for r in rows {
                    out.push(r?);
                }
                unique_canonical_usage_models(out)
            };

            Ok(UsageOverview {
                metrics,
                distribution: if q.agent_id.is_some() {
                    merge_model_distribution(distribution)
                } else {
                    distribution
                },
                models,
            })
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
            Ok(unique_canonical_usage_models(out))
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
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
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

#[derive(Clone, Copy)]
enum TrendGrain {
    Hour,
    Day,
}

impl TrendGrain {
    fn from_days(days: i64) -> Self {
        if days <= 1 {
            Self::Hour
        } else {
            Self::Day
        }
    }

    fn strftime(self) -> &'static str {
        match self {
            Self::Hour => "%Y-%m-%d %H:00",
            Self::Day => "%Y-%m-%d",
        }
    }
}

/// Local hour (`YYYY-MM-DD HH:00`) or calendar day (`YYYY-MM-DD`).
/// UTC prefixes would split a local day around midnight; skip non-RFC3339 `ts`.
fn local_trend_bucket(ts: &str, grain: TrendGrain) -> Option<String> {
    chrono::DateTime::parse_from_rfc3339(ts).ok().map(|dt| {
        dt.with_timezone(&chrono::Local)
            .format(grain.strftime())
            .to_string()
    })
}

fn truncate_local_hour(dt: chrono::DateTime<chrono::Local>) -> chrono::DateTime<chrono::Local> {
    use chrono::Timelike;
    let naive = dt
        .date_naive()
        .and_hms_opt(dt.hour(), 0, 0)
        .expect("hour 0-23 is a valid naive time");
    naive
        .and_local_timezone(chrono::Local)
        .earliest()
        .unwrap_or(dt)
}

/// Fill missing hour/day keys so the series spans the look-back window.
/// Skip when `map` is empty so a true-zero window stays an empty series.
fn fill_trend_window(
    map: &mut std::collections::BTreeMap<String, UsageTrendPoint>,
    days: i64,
    since: Option<&str>,
    grain: TrendGrain,
) {
    let now = chrono::Local::now();
    let rolling_start = now - chrono::Duration::days(days);
    let start = since
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&chrono::Local))
        .filter(|s| *s > rolling_start)
        .unwrap_or(rolling_start);

    match grain {
        TrendGrain::Hour => {
            let mut t = truncate_local_hour(start);
            let end = truncate_local_hour(now);
            let mut n = 0usize;
            while t <= end && n < 48 {
                let key = t.format(grain.strftime()).to_string();
                map.entry(key.clone())
                    .or_insert_with(|| UsageTrendPoint::new(key));
                t = t + chrono::Duration::hours(1);
                n += 1;
            }
        }
        TrendGrain::Day => {
            let mut d = start.date_naive();
            let end = now.date_naive();
            let mut n = 0usize;
            while d <= end && n < 40 {
                let key = d.format(grain.strftime()).to_string();
                map.entry(key.clone())
                    .or_insert_with(|| UsageTrendPoint::new(key));
                match d.checked_add_signed(chrono::Duration::days(1)) {
                    Some(next) => d = next,
                    None => break,
                }
                n += 1;
            }
        }
    }
}

fn usage_query_from_parts(
    days: u32,
    agent_id: Option<AgentId>,
    model: Option<&str>,
    since: Option<&str>,
    exclude_agent_ids: &[AgentId],
) -> UsageQuery {
    UsageQuery {
        days,
        agent_id,
        model: model.map(str::to_string),
        since: since.map(str::to_string),
        exclude_agent_ids: exclude_agent_ids.to_vec(),
        ..Default::default()
    }
}

fn merge_model_distribution(
    slices: Vec<UsageDistributionSlice>,
) -> Vec<UsageDistributionSlice> {
    let mut map = std::collections::BTreeMap::<String, UsageDistributionSlice>::new();
    for slice in slices {
        let key = canonical_usage_model(&slice.key);
        if key.is_empty() {
            continue;
        }
        let entry = map.entry(key.clone()).or_insert(UsageDistributionSlice {
            key,
            tokens: 0,
            cost_usd: 0.0,
            billable_input: 0,
            output: 0,
            cache_read: 0,
            cache_write: 0,
        });
        entry.tokens += slice.tokens;
        entry.cost_usd += slice.cost_usd;
        entry.billable_input += slice.billable_input;
        entry.output += slice.output;
        entry.cache_read += slice.cache_read;
        entry.cache_write += slice.cache_write;
    }
    let mut out: Vec<_> = map.into_values().collect();
    out.sort_by(|a, b| b.tokens.cmp(&a.tokens).then_with(|| a.key.cmp(&b.key)));
    out
}

/// Shared WHERE for query / trend / overview: days, since, agent, model, exclude.
///
/// `days` is `max(1)` rolling `unixepoch('now', '-N days')`. `since` is AND-ed
/// as instants (`unixepoch`); `Z` and `+00:00` match. Empty / `"all"` model is
/// ignored. `include_model` is false for overview `models` so the dropdown
/// stays populated while a model is selected. Exclude is `NOT IN` before LIMIT.
fn append_usage_filter(
    sql: &mut String,
    args: &mut Vec<Box<dyn rusqlite::types::ToSql>>,
    q: &UsageQuery,
    include_model: bool,
) {
    let days = q.days.max(1) as i64;
    sql.push_str(" WHERE unixepoch(ts) >= unixepoch('now', ?1)");
    args.push(Box::new(format!("-{days} days")));
    if let Some(since) = q.since.as_deref().filter(|s| !s.is_empty()) {
        sql.push_str(" AND unixepoch(ts) >= unixepoch(?)");
        args.push(Box::new(since.to_string()));
    }
    if let Some(agent) = q.agent_id {
        sql.push_str(" AND agent_id = ?");
        args.push(Box::new(agent.as_str().to_string()));
    }
    if include_model {
        if let Some(model) = q.model.as_deref().filter(|m| !m.is_empty() && *m != "all") {
            let aliases = usage_model_filter_aliases(model);
            sql.push_str(" AND model IN (");
            for (i, alias) in aliases.iter().enumerate() {
                if i > 0 {
                    sql.push(',');
                }
                sql.push('?');
                args.push(Box::new(alias.clone()));
            }
            sql.push(')');
        }
    }
    if q.exclude_agent_ids.is_empty() {
        return;
    }
    sql.push_str(" AND agent_id NOT IN (");
    for (i, id) in q.exclude_agent_ids.iter().enumerate() {
        if i > 0 {
            sql.push(',');
        }
        sql.push('?');
        args.push(Box::new(id.as_str().to_string()));
    }
    sql.push(')');
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
        model: canonical_usage_model(&row.get::<_, Option<String>>(3)?.unwrap_or_default()),
        input_tokens: row.get(4)?,
        output_tokens: row.get(5)?,
        cache_read_tokens: row.get(6)?,
        cache_write_tokens: row.get(7)?,
        cost_usd: row.get(8)?,
        session_id: row.get(9)?,
        ts: row.get(10)?,
        raw_hash: row.get(11)?,
        fast: row.get::<_, i64>(12)? != 0,
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
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
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
                        r.cache_read_tokens,
                        r.cache_write_tokens,
                        r.cost_usd,
                        r.session_id,
                        r.ts,
                        r.raw_hash,
                        r.fast,
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
