//! Usage collection + query service.

use std::time::Instant;

use uuid::Uuid;

use crate::error::{AppError, Result};
use crate::logging::targets;
use crate::models::{
    AgentId, CollectResult, ParserHealth, UsageQuery, UsageRecord, UsageTrendPoint,
};
use crate::platform::usage::{
    builtin_usage_registry, collect_with_source, collect_with_source_for_agent_id, TokenAccounting,
    UsageSourceRegistry,
};
use crate::platform::AgentKey;
use crate::storage::{Database, UsageRepo};
use crate::usage::session_jsonl::CollectStats;
use crate::usage::{
    codex_billable_tokens, estimate_cost_usd, estimate_cost_usd_for_agent, has_embedded_pricing,
};
use crate::utils::redact::redact_text;

pub struct UsageService {
    repo: UsageRepo,
    registry: UsageSourceRegistry,
}

impl UsageService {
    pub fn new(db: Database) -> Self {
        Self::with_registry(db, builtin_usage_registry().clone())
    }

    pub fn with_registry(db: Database, registry: UsageSourceRegistry) -> Self {
        Self {
            repo: UsageRepo::new(db),
            registry,
        }
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
        let agents: Vec<AgentId> = match agent {
            Some(a) => vec![a],
            // Supported agents come from UsageSource registry (not a hard-coded name list).
            None => self.registry.supported_agents(),
        };

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
                        let tokens = ev.input_tokens
                            + ev.output_tokens
                            + ev.cache_creation_tokens
                            + ev.cache_read_tokens;
                        if tokens > 0 && ev.cost_usd.is_none() && !has_embedded_pricing(&ev.model) {
                            missing_pricing.insert(ev.model.clone());
                        }
                        // ccusage Auto: prefer log costUSD; else token × rates.
                        // Stored input is already billable (Codex peeled at parse).
                        // Unknown model (no table row, no log cost): $0 + missing tip.
                        let cost = estimate_cost_usd_for_agent(
                            ev.agent_id,
                            &ev.model,
                            ev.input_tokens,
                            ev.output_tokens,
                            ev.cache_creation_tokens,
                            ev.cache_read_tokens,
                            ev.cost_usd,
                        );
                        let cache_tokens = ev.cache_tokens_total();
                        rows.push(UsageRecord {
                            id: Uuid::new_v4().to_string(),
                            agent_id: ev.agent_id,
                            account_id: None,
                            model: ev.model,
                            input_tokens: ev.input_tokens,
                            output_tokens: ev.output_tokens,
                            cache_tokens,
                            cost_usd: Some(cost),
                            session_id: ev.session_id,
                            ts: ev.ts,
                            raw_hash: Some(ev.raw_hash),
                        });
                    }
                    let n = self.repo.insert_batch_and_cursors(&rows, &cursors)?;
                    inserted += n;
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
                    let cache_sum: i64 = rows.iter().map(|r| r.cache_tokens).sum();
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
                        cache_sum,
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

        // Repair historical rows once per collect:
        // - Codex: fix input-includes-cache double billing + apply latest table rates
        // - Unknown models: force $0 (no heuristic)
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

    pub fn trend(&self, days: u32, agent: Option<AgentId>) -> Result<Vec<UsageTrendPoint>> {
        self.repo.trend(days, agent)
    }

    pub fn list_models(&self) -> Result<Vec<String>> {
        self.repo.list_models()
    }

    pub fn parser_health(&self) -> Result<Vec<ParserHealth>> {
        let stored = self.repo.load_parser_health()?;
        let counts = self.repo.count_by_agent()?;
        let mut out = Vec::new();
        for a in AgentId::ALL {
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
            .filter(|m| !has_embedded_pricing(m))
            .collect())
    }

    /// Recompute costs only; never rewrite token layout.
    ///
    /// Storage contract (aligned with ccusage):
    /// - Codex: `input_tokens` is already non-cached billable; `cache_tokens` is cache read
    /// - Others: input and cache are disjoint Anthropic-style buckets
    ///
    /// Historical bug: peeling `cache` from `input` when `cache <= input` double-subtracted
    /// on every collect and eroded Codex billable tokens toward zero.
    pub fn recompute_stored_costs(&self) -> Result<u64> {
        self.repo
            .recompute_costs(|agent, model, input, output, cache| {
                let accounting = self
                    .registry
                    .get(&AgentKey::from_agent_id(agent))
                    .map(|s| s.token_accounting())
                    .unwrap_or(TokenAccounting::Standard);
                if accounting == TokenAccounting::CodexBillable {
                    // Trust stored non-cached input; only refresh cost with latest rates.
                    let (bill_in, cache_r) = codex_billable_tokens(input, cache);
                    let cost = estimate_cost_usd(model, bill_in, output, 0, cache_r, None);
                    return Some((bill_in, cost));
                }
                if !has_embedded_pricing(model) {
                    return Some((input, 0.0));
                }
                // Known non-Codex models may store log costUSD — leave them alone.
                None
            })
    }

    /// One-time repair when token accounting semantics change.
    ///
    /// Clears stored rows + cursors so the next collect rebuilds from session logs
    /// with the current rules (non-cached Codex input, cumulative-advance filter,
    /// fork rewritten-history burst skip). UPSERT alone cannot drop orphan rows that
    /// previous parsers over-inserted.
    fn maybe_repair_token_layout(&self) -> Result<()> {
        const KEY: &str = "usage_token_layout";
        // v3 = non-cached Codex input + ccusage cumulative-advance + fork burst skip.
        const VER: &str = "3";
        let cur = self.repo.get_meta(KEY)?;
        if cur.as_deref() == Some(VER) {
            tracing::debug!(
                module = targets::USAGE,
                op = "token_layout_repair",
                version = VER,
                "usage token layout already current; skip rebuild"
            );
            return Ok(());
        }
        let from = cur.as_deref().unwrap_or("unset");
        tracing::info!(
            module = targets::USAGE,
            op = "token_layout_repair",
            from_version = from,
            to_version = VER,
            "usage token layout migration starting (clear rows + reset cursors)"
        );
        let deleted = self.repo.clear_all_records()?;
        let n = self.repo.reset_all_cursors()?;
        self.repo.set_meta(KEY, VER)?;
        tracing::info!(
            module = targets::USAGE,
            op = "token_layout_repair",
            rows_deleted = deleted,
            cursors_reset = n,
            from_version = from,
            to_version = VER,
            "cleared usage rows and cursors; next scan rebuilds from session logs"
        );
        Ok(())
    }
}
