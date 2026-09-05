//! Usage collection + query service.

mod cost;

#[cfg(test)]
mod tests;

pub(super) use cost::{cost_for_event, event_missing_pricing};

use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use uuid::Uuid;

use crate::error::{AppError, Result};
use crate::logging::targets;
use crate::models::{
    AgentId, CollectResult, ConnectionUsageSummary, DetectResult, DetectStatus,
    GatewayUsageOverview, GatewayUsageQuery, GatewayUsageRow, ParserHealth, UsageOverview,
    UsageQuery, UsageRecord, UsageTrendPoint,
};
use crate::platform::usage::{
    builtin_usage_registry, collect_with_source, collect_with_source_for_agent_id,
    ingest_spool_dir_with, GatewaySpoolOutcome, TokenAccounting, UsageSourceRegistry,
};
use crate::platform::AgentKey;
use crate::storage::connection_usage::current_ticket_id_for_agent;
use crate::storage::gateway_usage_repo::GatewayUsageRepo;
use crate::storage::{ConnectionUsageStore, Database, UsageRepo};
use crate::usage::session_jsonl::CollectStats;
use crate::usage::{
    codex_billable_tokens, estimate_cost_usd_for_agent_at, has_embedded_pricing_for, CostTokens,
};
use crate::utils::redact::redact_text;

use super::agent_service::AgentService;
use super::agent_visibility_service::AgentVisibilityService;

/// Live (or test-injected) set of agents collect / parser_health may scan.
type CollectTargetResolver = Arc<dyn Fn() -> Result<HashSet<AgentId>> + Send + Sync>;

pub struct UsageService {
    db: Database,
    repo: UsageRepo,
    gateway_repo: GatewayUsageRepo,
    connection_usage: ConnectionUsageStore,
    registry: UsageSourceRegistry,
    collect_targets: Option<CollectTargetResolver>,
    /// Serializes collect, including one-shot repairs, across concurrent callers.
    collect_lock: Mutex<()>,
    /// Test-only spool dir override so unit tests never touch the real data
    /// dir. `None` in tests disables the gateway ingest step entirely.
    #[cfg(test)]
    gateway_spool_dir: Option<std::path::PathBuf>,
}

impl UsageService {
    pub fn new(db: Database) -> Self {
        Self::with_registry(db, builtin_usage_registry().clone())
    }

    /// Production constructor: skip hidden (visibility file) and not-installed (detect).
    ///
    /// `db` is the product database (current login lookup). `cache` holds
    /// token / API usage rows and can be deleted without breaking the app.
    pub fn with_live_scope(
        db: Database,
        cache: Database,
        visibility: AgentVisibilityService,
        agents: AgentService,
    ) -> Self {
        let connection_usage = ConnectionUsageStore::from_database(cache.clone());
        Self::with_registry_and_scope(
            db,
            cache,
            builtin_usage_registry().clone(),
            Some(live_collect_target_resolver(visibility, agents)),
            connection_usage,
        )
    }

    pub fn with_registry(db: Database, registry: UsageSourceRegistry) -> Self {
        Self::with_registry_and_scope(
            db.clone(),
            db,
            registry,
            None,
            ConnectionUsageStore::disabled(),
        )
    }

    fn with_registry_and_scope(
        db: Database,
        cache: Database,
        registry: UsageSourceRegistry,
        collect_targets: Option<CollectTargetResolver>,
        connection_usage: ConnectionUsageStore,
    ) -> Self {
        Self {
            db,
            repo: UsageRepo::new(cache.clone()),
            gateway_repo: GatewayUsageRepo::new(cache),
            connection_usage,
            registry,
            collect_targets,
            collect_lock: Mutex::new(()),
            #[cfg(test)]
            gateway_spool_dir: None,
        }
    }

    /// Test helper: ingest gateway usage from this spool dir instead of the
    /// resolved data dir (unit tests must not touch the real data dir).
    #[cfg(test)]
    pub(crate) fn with_gateway_spool_dir(mut self, dir: std::path::PathBuf) -> Self {
        self.gateway_spool_dir = Some(dir);
        self
    }

    /// Test helper: only these installed && !hidden agents are collect/health targets.
    #[cfg(test)]
    pub(crate) fn with_visible_installed(
        db: Database,
        registry: UsageSourceRegistry,
        visible_installed: impl IntoIterator<Item = AgentId>,
    ) -> Self {
        let allowed: HashSet<AgentId> = visible_installed.into_iter().collect();
        let allowed = Arc::new(allowed);
        Self::with_registry_and_scope(
            db.clone(),
            db,
            registry,
            Some(Arc::new(move || Ok((*allowed).clone()))),
            ConnectionUsageStore::disabled(),
        )
    }

    fn resolve_collect_agents(&self, requested: Option<AgentId>) -> Result<Vec<AgentId>> {
        let candidates: Vec<AgentId> = match requested {
            Some(a) => vec![a],
            None => self.registry.supported_agents(),
        };
        let Some(resolve) = &self.collect_targets else {
            return Ok(candidates);
        };
        let allowed = resolve()?;
        Ok(candidates
            .into_iter()
            .filter(|agent| allowed.contains(agent))
            .collect())
    }

    /// Key-native collection entry point.
    ///
    /// Usage persistence still stores AgentId. A registered open-key source
    /// therefore succeeds when it discovers no files, but returns the explicit
    /// legacy-persistence boundary error from collect_with_source as soon as
    /// it discovers data. No reverse AgentKey -> AgentId conversion occurs.
    pub fn collect_agent_key(&self, key: &AgentKey) -> Result<CollectStats> {
        self.collect_source(key, None)
    }

    fn collect_source(
        &self,
        key: &AgentKey,
        persistence_agent: Option<AgentId>,
    ) -> Result<CollectStats> {
        let source = self.registry.get(key).ok_or_else(|| {
            AppError::Unsupported(format!(
                "usage source is not registered for agent key '{key}'"
            ))
        })?;
        match persistence_agent {
            Some(agent) => collect_with_source_for_agent_id(source.as_ref(), agent, &self.repo),
            None => collect_with_source(source.as_ref(), &self.repo),
        }
    }

    /// Scan agent session logs and insert new rows (dedup by raw_hash).
    pub fn collect(&self, agent: Option<AgentId>) -> Result<CollectResult> {
        let _collect_guard = self
            .collect_lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let started = Instant::now();
        // One-time: reset file cursors so corrupted Codex rows (double-peel) are
        // re-parsed from logs and UPSERT'd with correct non-cached input.
        if let Err(e) = self.maybe_repair_token_layout() {
            let err_msg = redact_text(&e.to_string());
            tracing::warn!(
                module = targets::USAGE,
                code = e.code(),
                op = "token_layout_repair",
                error = %err_msg,
                "failed to schedule usage token layout repair"
            );
        }
        if let Err(e) = self.maybe_repair_grok_parser() {
            let err_msg = redact_text(&e.to_string());
            tracing::warn!(
                module = targets::USAGE,
                code = e.code(),
                op = "grok_parser_repair",
                error = %err_msg,
                "failed to schedule Grok usage parser repair"
            );
        }
        let agents = self.resolve_collect_agents(agent)?;

        let mut inserted = 0u64;
        let mut skipped = 0u64;
        let mut failed = 0u64;
        let mut health = Vec::new();
        let mut missing_pricing = std::collections::BTreeSet::new();

        for a in agents {
            let key = AgentKey::from_agent_id(a);
            if !self.registry.contains_key(&key) {
                health.push(ParserHealth {
                    agent_id: a,
                    supported: false,
                    records: 0,
                    fail_rate_pct: None,
                    skipped: None,
                });
                continue;
            }
            match self.collect_source(&key, Some(a)) {
                Ok(stats) => {
                    let stats_skipped = stats.skipped;
                    let stats_failed = stats.failed;
                    let cursors = stats.cursors;
                    let events = stats.events;
                    skipped += stats_skipped;
                    failed += stats_failed;
                    let mut rows = Vec::with_capacity(events.len());
                    for ev in events {
                        let tokens = ev.cache_tokens_total() + ev.input_tokens + ev.output_tokens;
                        if tokens > 0 && ev.cost_usd.is_none() && event_missing_pricing(&ev) {
                            missing_pricing.insert(ev.model.clone());
                        }
                        // ccusage Auto: prefer log costUSD / Grok ticks; else token × rates.
                        // Stored input is already billable (Codex/Grok peeled at parse).
                        // Unknown model (no table row, no log cost): $0 + missing tip.
                        let cost = cost_for_event(&ev);
                        let cache_write_tokens = ev.cache_write_tokens();
                        rows.push(UsageRecord {
                            id: Uuid::new_v4().to_string(),
                            agent_id: ev.agent_id,
                            account_id: None,
                            model: ev.model,
                            input_tokens: ev.input_tokens,
                            output_tokens: ev.output_tokens,
                            cache_read_tokens: ev.cache_read_tokens,
                            cache_write_tokens,
                            cost_usd: Some(cost),
                            session_id: ev.session_id,
                            ts: ev.ts,
                            raw_hash: Some(ev.raw_hash),
                            fast: ev.fast,
                        });
                    }
                    let n = self.repo.insert_batch_and_cursors(&rows, &cursors)?;
                    inserted += n;
                    if let Some(ticket_id) = current_ticket_id_for_agent(&self.db, a) {
                        self.connection_usage.record_log_rows(&ticket_id, &rows);
                    }
                    // counts for this agent after insert
                    let total = self
                        .repo
                        .count_by_agent()?
                        .into_iter()
                        .find(|(id, _)| *id == a)
                        .map(|(_, c)| c)
                        .unwrap_or(0);
                    let events_n = rows.len() as u64;
                    let billable_sum: i64 = rows.iter().map(|r| r.input_tokens).sum();
                    let cache_read_sum: i64 = rows.iter().map(|r| r.cache_read_tokens).sum();
                    let cache_write_sum: i64 = rows.iter().map(|r| r.cache_write_tokens).sum();
                    tracing::debug!(
                        module = targets::USAGE,
                        op = "collect_agent",
                        agent = a.as_str(),
                        events = events_n,
                        upserted = n,
                        skipped = stats_skipped,
                        failed = stats_failed,
                        files = cursors.len() as u64,
                        billable_input_sum = billable_sum,
                        cache_read_sum,
                        cache_write_sum,
                        records_total = total,
                        "usage collect agent batch"
                    );
                    health.push(ParserHealth {
                        agent_id: a,
                        supported: true,
                        records: total,
                        fail_rate_pct: if stats_failed > 0 {
                            let t =
                                (rows.len() as u64 + stats_failed + stats_skipped).max(1) as f64;
                            Some(((stats_failed as f64) / t * 100.0).round())
                        } else {
                            None
                        },
                        skipped: if stats_skipped > 0 {
                            Some(stats_skipped)
                        } else {
                            None
                        },
                    });
                }
                Err(e) => {
                    failed += 1;
                    let err_msg = redact_text(&e.to_string());
                    tracing::warn!(
                        module = targets::USAGE,
                        code = e.code(),
                        op = "collect",
                        agent = a.as_str(),
                        error = %err_msg,
                        "usage collect agent failed"
                    );
                    health.push(ParserHealth {
                        agent_id: a,
                        supported: true,
                        records: 0,
                        fail_rate_pct: Some(100.0),
                        skipped: None,
                    });
                }
            }
        }

        // Gateway (local bridge) requests spool into per-day JSONL files;
        // ingest them into the separate `gateway_usage` table. Never merged
        // into `usage_records` (log collection already records that spend).
        match self.collect_gateway_spool() {
            Ok(outcome) if outcome.inserted > 0 || outcome.malformed > 0 => {
                tracing::info!(
                    module = targets::USAGE,
                    op = "collect_gateway_spool",
                    files = outcome.files,
                    inserted = outcome.inserted,
                    malformed = outcome.malformed,
                    deleted_files = outcome.deleted_files,
                    "gateway usage spool ingested"
                );
            }
            Ok(_) => {}
            Err(e) => {
                let err_msg = redact_text(&e.to_string());
                tracing::warn!(
                    module = targets::USAGE,
                    code = e.code(),
                    op = "collect_gateway_spool",
                    error = %err_msg,
                    "gateway usage spool ingest failed"
                );
            }
        }

        // Repair historical rows once per collect:
        // - Codex: refresh cost from the latest table (tokens stay non-cached)
        // - Unknown models: keep insert-time cost (log USD / Grok ticks / $0)
        // Leave non-Codex known-model rows alone (may store log costUSD).
        match self.recompute_stored_costs() {
            Ok(n) if n > 0 => {
                tracing::info!(
                    module = targets::USAGE,
                    op = "recompute_costs",
                    changed = n,
                    "recomputed stored usage costs"
                );
            }
            Ok(_) => {}
            Err(e) => {
                let err_msg = redact_text(&e.to_string());
                tracing::warn!(
                    module = targets::USAGE,
                    code = e.code(),
                    op = "recompute_costs",
                    error = %err_msg,
                    "failed to recompute stored usage costs"
                );
            }
        }

        let missing_pricing_models: Vec<String> = missing_pricing.into_iter().collect();
        if !missing_pricing_models.is_empty() {
            tracing::warn!(
                module = targets::USAGE,
                op = "collect",
                models = ?missing_pricing_models,
                "missing embedded pricing for models (cost recorded as $0)"
            );
        }
        self.repo.save_parser_health(&health)?;
        let result = CollectResult {
            inserted,
            skipped,
            failed,
            agents: health,
            missing_pricing_models,
        };
        tracing::info!(
            module = targets::USAGE,
            op = "collect",
            inserted,
            skipped,
            failed,
            elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
            "usage collect done"
        );
        Ok(result)
    }

    pub fn query(&self, q: UsageQuery) -> Result<Vec<UsageRecord>> {
        self.repo.query(&q)
    }

    /// Per-connection token totals from the sidecar DB. Never errors: a missing
    /// or deleted file looks like an empty list.
    pub fn connection_usage_summaries(&self) -> Vec<ConnectionUsageSummary> {
        let _ = self.collect_gateway_spool();
        self.connection_usage.list_summaries()
    }

    /// Ingest the gateway usage spool dir into the `gateway_usage` table.
    fn collect_gateway_spool(&self) -> Result<GatewaySpoolOutcome> {
        let store = self.connection_usage.clone();
        let mut on_rows = |rows: &[GatewayUsageRow]| store.record_gateway(rows);
        #[cfg(test)]
        {
            // Unit tests never touch the real data dir: an injected dir is
            // ingested, otherwise the gateway step is a no-op.
            let Some(dir) = self.gateway_spool_dir.clone() else {
                return Ok(GatewaySpoolOutcome::default());
            };
            std::fs::create_dir_all(&dir)?;
            return ingest_spool_dir_with(&self.gateway_repo, &dir, &mut on_rows);
        }
        #[allow(unreachable_code)]
        {
            let dir = crate::utils::paths::usage_gateway_dir()?;
            std::fs::create_dir_all(&dir)?;
            ingest_spool_dir_with(&self.gateway_repo, &dir, &mut on_rows)
        }
    }

    /// Pull new JSONL spool lines into `gateway_usage` before a board read.
    /// Ingest failure is logged; the query still returns whatever is already stored.
    fn refresh_gateway_usage(&self) {
        if let Err(e) = self.collect_gateway_spool() {
            let err_msg = redact_text(&e.to_string());
            tracing::warn!(
                module = targets::USAGE,
                code = e.code(),
                op = "refresh_gateway_usage",
                error = %err_msg,
                "gateway usage spool ingest failed"
            );
        }
    }

    /// Query per-request gateway usage rows (local bridge runtime).
    pub fn gateway_usage_query(&self, q: GatewayUsageQuery) -> Result<Vec<GatewayUsageRow>> {
        self.refresh_gateway_usage();
        self.gateway_repo.query(&q)
    }

    /// Aggregated gateway usage overview for a time window.
    pub fn gateway_usage_overview(&self, q: GatewayUsageQuery) -> Result<GatewayUsageOverview> {
        self.refresh_gateway_usage();
        self.gateway_repo.overview(&q)
    }

    pub fn trend(
        &self,
        days: u32,
        agent: Option<AgentId>,
        model: Option<&str>,
        since: Option<&str>,
        exclude_agent_ids: &[AgentId],
        until: Option<&str>,
    ) -> Result<Vec<UsageTrendPoint>> {
        self.repo
            .trend(days, agent, model, since, exclude_agent_ids, until)
    }

    pub fn trend_by_model(
        &self,
        days: u32,
        agent: Option<AgentId>,
        model: Option<&str>,
        since: Option<&str>,
        exclude_agent_ids: &[AgentId],
        until: Option<&str>,
    ) -> Result<Vec<UsageTrendPoint>> {
        self.repo
            .trend_by_model(days, agent, model, since, exclude_agent_ids, until)
    }

    pub fn overview(
        &self,
        days: u32,
        agent: Option<AgentId>,
        model: Option<&str>,
        since: Option<&str>,
        exclude_agent_ids: &[AgentId],
        until: Option<&str>,
    ) -> Result<UsageOverview> {
        self.repo
            .overview(days, agent, model, since, exclude_agent_ids, until)
    }

    pub fn list_models(&self) -> Result<Vec<String>> {
        self.repo.list_models()
    }

    pub fn parser_health(&self) -> Result<Vec<ParserHealth>> {
        let stored = self.repo.load_parser_health()?;
        let counts = self.repo.count_by_agent()?;
        let agents = match &self.collect_targets {
            Some(_) => self.resolve_collect_agents(None)?,
            None => AgentId::ALL.to_vec(),
        };
        let mut out = Vec::new();
        for a in agents {
            let supported = self.registry.contains_key(&AgentKey::from_agent_id(a));
            let records = counts
                .iter()
                .find(|(id, _)| *id == a)
                .map(|(_, n)| *n)
                .unwrap_or(0);
            let mut health = ParserHealth {
                agent_id: a,
                supported,
                records,
                fail_rate_pct: None,
                skipped: None,
            };
            if let Some(saved) = stored.iter().find(|row| row.agent_id == a) {
                health = saved.clone();
                health.records = records.max(health.records);
            }
            out.push(health);
        }
        Ok(out)
    }

    /// Models in usage_records that lack embedded pricing (for UI/CLI WARN).
    /// Optionally limit to recent `days` window (0 = all).
    pub fn missing_pricing_models(&self, days: u32) -> Result<Vec<String>> {
        let days = if days == 0 { 3650 } else { days };
        let models = self.repo.distinct_models(days)?;
        Ok(models
            .into_iter()
            .filter(|m| !has_embedded_pricing_for(AgentId::Codex, m, None))
            .collect())
    }

    /// Recompute costs only; never rewrite token layout.
    ///
    /// Storage contract (aligned with ccusage):
    /// - Codex: `input_tokens` is already non-cached billable; `cache_read_tokens` is cache read
    /// - Others: input, cache write, and cache read are disjoint Anthropic-style buckets
    ///
    /// Historical bug: peeling `cache` from `input` when `cache <= input` double-subtracted
    /// on every collect and eroded Codex billable tokens toward zero.
    /// Unknown-model costs are left as stored (log / ticks / $0).
    pub fn recompute_stored_costs(&self) -> Result<u64> {
        self.repo.recompute_costs(
            |agent, model, input, output, cache_read, cache_write, fast| {
                let accounting = self
                    .registry
                    .get(&AgentKey::from_agent_id(agent))
                    .map(|s| s.token_accounting())
                    .unwrap_or(TokenAccounting::Standard);
                if accounting == TokenAccounting::CodexBillable {
                    // Trust stored non-cached input; refresh cost with latest rates + Fast.
                    let (bill_in, cache_r) = codex_billable_tokens(input, cache_read);
                    let cost = estimate_cost_usd_for_agent_at(
                        agent,
                        model,
                        CostTokens {
                            input: bill_in,
                            output,
                            cache_create: cache_write,
                            cache_read: cache_r,
                            fast,
                            ..CostTokens::default()
                        },
                        None,
                        None,
                    );
                    return Some((bill_in, cost));
                }
                // Known models may store log costUSD. Unknown models keep the
                // insert-time value (invoice ticks / log USD, or $0). Wiping
                // unknown rows to $0 dropped Grok ticks when the raw model id
                // missed the embedded table.
                None
            },
        )
    }

    /// One-time repair when token accounting semantics change.
    ///
    /// Clears stored rows + cursors so the next collect rebuilds from session logs
    /// with current cost rules (long-context, 1h cache, Codex Fast) and scans
    /// `archived_sessions/`. UPSERT cannot drop orphan rows from older parsers.
    fn maybe_repair_token_layout(&self) -> Result<()> {
        const KEY: &str = "usage_token_layout";
        // v5 = persist cache write vs read as separate columns (billing rates differ).
        const VER: &str = "5";
        let (deleted, n, already_current) = self.repo.repair_token_layout(KEY, VER)?;
        if already_current {
            tracing::debug!(
                module = targets::USAGE,
                op = "token_layout_repair",
                version = VER,
                "usage token layout already current; skip rebuild"
            );
            return Ok(());
        }
        tracing::info!(
            module = targets::USAGE,
            op = "token_layout_repair",
            rows_deleted = deleted,
            cursors_reset = n,
            to_version = VER,
            "cleared usage rows and cursors; next scan rebuilds from session logs"
        );
        Ok(())
    }

    /// One-time Grok rebuild: previous parser treated Grok as Claude-like JSONL
    /// and scanned every `*.jsonl` under sessions. ccusage only counts
    /// `updates.jsonl` `turn_completed` rows, peels cache out of input, and
    /// prefers `costUsdTicks`. Old hashes / session ids (`updates`) cannot UPSERT
    /// onto the new rows.
    fn maybe_repair_grok_parser(&self) -> Result<()> {
        const KEY: &str = "usage_grok_parser";
        const VER: &str = "1";
        let (deleted, n, already_current) = self.repo.repair_grok_parser(KEY, VER)?;
        if already_current {
            return Ok(());
        }
        tracing::info!(
            module = targets::USAGE,
            op = "grok_parser_repair",
            rows_deleted = deleted,
            cursors_reset = n,
            to_version = VER,
            "cleared Grok usage rows and cursors; next scan rebuilds from updates.jsonl"
        );
        Ok(())
    }
}

fn live_collect_target_resolver(
    visibility: AgentVisibilityService,
    agents: AgentService,
) -> CollectTargetResolver {
    Arc::new(move || {
        let hidden = visibility.list_hidden_agents()?;
        let detect = agents.detect_all();
        Ok(visible_installed_agent_ids(&hidden, &detect))
    })
}

/// Same rule as frontend `visibleInstalledIds`: installed && !hidden.
pub(crate) fn visible_installed_agent_ids(
    hidden: &[String],
    detect: &[DetectResult],
) -> HashSet<AgentId> {
    let hidden: HashSet<&str> = hidden.iter().map(String::as_str).collect();
    detect
        .iter()
        .filter(|row| row.status == DetectStatus::Installed && !hidden.contains(row.agent.as_str()))
        .map(|row| row.agent)
        .collect()
}
