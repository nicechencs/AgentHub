//! Model pricing for cost estimation.
//!
//! Inspired by ccusage:
//! - Prefer log-provided costUSD (Auto mode)
//! - Else look up embedded per-1M rates (LiteLLM-style subset)
//! - Fuzzy alias matching for dated model ids (e.g. claude-sonnet-4-20250514)
//! - Long-context: whole-request switch when `longContextThreshold` is set
//!   (OpenAI 272K); otherwise LiteLLM `*_above_200k` is billed marginally
//! - 1-hour cache writes bill at `2 × input`
//! - Codex Fast / Priority multiplies the token cost
//!
//! Costs stay in the same unit as the pricing table (**USD per 1M tokens**).
//! No FX conversion at runtime.
//!
//! The embedded table is an **offline snapshot** refreshed by
//! `scripts/update-embedded-pricing.mjs` (manual `pnpm pricing:update` or daily CI).
//! Runtime never fetches pricing. Local-only models live in
//! `scripts/pricing/overrides.json`.
//!
//! `codex-auto-review` is a Codex log label, not a priced model. Cost lookup
//! uses the published backend OpenAI named for that date (GPT-5.4, then
//! GPT-5.6 Luna from 2026-07-30). The stored model id is left unchanged.

use std::collections::HashMap;
use std::sync::OnceLock;

use crate::models::AgentId;

/// Embedded USD-per-1M rates (same units as common public list prices).
const EMBEDDED_PRICING_JSON: &str = include_str!("embedded-pricing.json");

/// ccusage: 1-hour ephemeral cache writes are billed at 2× the input rate.
const CACHE_CREATE_1H_INPUT_MULTIPLIER: f64 = 2.0;

/// Default LiteLLM `*_above_200k_tokens` boundary when no per-model threshold is set.
const DEFAULT_LONG_CONTEXT_THRESHOLD_TOKENS: u64 = 200_000;

/// Per-token rates (USD).
#[derive(Debug, Clone, Copy)]
pub struct Rates {
    pub input: f64,
    pub output: f64,
    pub cache_create: f64,
    pub cache_read: f64,
    pub cache_read_explicit: bool,
    pub input_above_200k: Option<f64>,
    pub output_above_200k: Option<f64>,
    pub cache_create_above_200k: Option<f64>,
    pub cache_read_above_200k: Option<f64>,
    /// When set, the whole request switches to the `*_above_200k` rates.
    pub long_context_threshold: Option<u64>,
    pub fast_multiplier: f64,
}

/// Token buckets after parse (disjoint: input does not include cache).
#[derive(Debug, Clone, Copy, Default)]
pub struct CostTokens {
    pub input: i64,
    pub output: i64,
    pub cache_create: i64,
    pub cache_create_1h: i64,
    pub cache_read: i64,
    pub fast: bool,
}

impl CostTokens {
    pub fn from_parts(input: i64, output: i64, cache_create: i64, cache_read: i64) -> Self {
        Self {
            input,
            output,
            cache_create,
            cache_read,
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct EmbeddedRow {
    input: f64,
    output: f64,
    #[serde(default)]
    cache_create: Option<f64>,
    #[serde(default)]
    cache_read: Option<f64>,
    #[serde(default)]
    input_above_200k: Option<f64>,
    #[serde(default)]
    output_above_200k: Option<f64>,
    #[serde(default)]
    cache_create_above_200k: Option<f64>,
    #[serde(default)]
    cache_read_above_200k: Option<f64>,
    #[serde(default)]
    long_context_threshold: Option<u64>,
    #[serde(default)]
    fast_multiplier: Option<f64>,
}

struct PricingTable {
    /// lowercase key → rates
    by_key: HashMap<String, Rates>,
}

impl PricingTable {
    fn load() -> Self {
        let raw: HashMap<String, EmbeddedRow> =
            serde_json::from_str(EMBEDDED_PRICING_JSON).unwrap_or_default();
        let mut by_key = HashMap::new();
        for (k, row) in raw {
            let create = row.cache_create.unwrap_or(row.input);
            let cache_read_explicit = row.cache_read.is_some();
            let read = row.cache_read.unwrap_or(row.input * 0.1);
            let per_m = |v: f64| v / 1_000_000.0;
            let per_m_opt = |v: Option<f64>| v.map(per_m);
            let rates = Rates {
                input: per_m(row.input),
                output: per_m(row.output),
                cache_create: per_m(create),
                cache_read: per_m(read),
                cache_read_explicit,
                input_above_200k: per_m_opt(row.input_above_200k),
                output_above_200k: per_m_opt(row.output_above_200k),
                cache_create_above_200k: per_m_opt(row.cache_create_above_200k),
                cache_read_above_200k: per_m_opt(row.cache_read_above_200k),
                long_context_threshold: row.long_context_threshold.filter(|n| *n > 0),
                fast_multiplier: row.fast_multiplier.filter(|n| *n > 0.0).unwrap_or(1.0),
            };
            by_key.insert(normalize_key(&k), rates);
            // Also index bare model segment after last '/'
            if let Some((_, bare)) = k.rsplit_once('/') {
                by_key.entry(normalize_key(bare)).or_insert(rates);
            }
        }
        Self { by_key }
    }

    fn find(&self, model: &str) -> Option<Rates> {
        let key = normalize_key(model);
        if let Some(r) = self.by_key.get(&key) {
            return Some(*r);
        }
        // Strip provider prefix kimi-code/, anthropic/, etc.
        let stripped = key
            .rsplit_once('/')
            .map(|(_, b)| b.to_string())
            .unwrap_or_else(|| key.clone());
        if let Some(r) = self.by_key.get(&stripped) {
            return Some(*r);
        }
        // Dated suffix: claude-sonnet-4-20250514 → claude-sonnet-4
        if let Some(base) = strip_date_suffix(&stripped) {
            if let Some(r) = self.by_key.get(base) {
                return Some(*r);
            }
        }
        // Do not fuzzy-contains-match unknown model ids onto a longer/shorter
        // key — that silently applies the wrong rate. Unknown → unpriced.
        None
    }
}

fn table() -> &'static PricingTable {
    static T: OnceLock<PricingTable> = OnceLock::new();
    T.get_or_init(PricingTable::load)
}

fn normalize_key(s: &str) -> String {
    s.trim().to_ascii_lowercase()
}

/// Strip trailing `-YYYYMMDD` or `-vN` style version tails when present.
fn strip_date_suffix(s: &str) -> Option<&str> {
    // ...-20250514
    if s.len() > 9 {
        let (head, tail) = s.split_at(s.len() - 9);
        if tail.starts_with('-') && tail[1..].chars().all(|c| c.is_ascii_digit()) {
            return Some(head.trim_end_matches('-'));
        }
    }
    // ...-4-5-20251001 style: peel last -digits segment repeatedly
    let mut cur = s;
    for _ in 0..3 {
        if let Some((h, t)) = cur.rsplit_once('-') {
            if !t.is_empty() && t.chars().all(|c| c.is_ascii_digit()) && t.len() >= 6 {
                cur = h;
                continue;
            }
        }
        break;
    }
    if cur != s {
        Some(cur)
    } else {
        None
    }
}

/// Prefer log costUSD when present (ccusage CostMode::Auto).
///
/// - Log `costUSD` → trust it
/// - Embedded pricing row → token × rates (USD, no FX)
/// - **Unknown model (no table row)** → **$0** (UI/CLI shows yellow missing-pricing tip)
pub fn estimate_cost_usd(
    model: &str,
    input: i64,
    output: i64,
    cache_create: i64,
    cache_read: i64,
    cost_usd: Option<f64>,
) -> f64 {
    estimate_cost_from_tokens(
        model,
        CostTokens::from_parts(input, output, cache_create, cache_read),
        cost_usd,
    )
}

/// Codex `codex-auto-review` backend switch (OpenAI, 2026-07-30): GPT-5.4 → Luna.
const CODEX_AUTO_REVIEW_LUNA_ON: &str = "2026-07-30";

/// Model id used for the pricing table. Log labels that are not priced stay
/// on the row; only the lookup key is rewritten.
pub fn pricing_model_for<'a>(agent: AgentId, model: &'a str, as_of: Option<&str>) -> &'a str {
    if agent != AgentId::Codex || !model.eq_ignore_ascii_case("codex-auto-review") {
        return model;
    }
    if as_of
        .and_then(|ts| ts.get(..10))
        .is_some_and(|d| d < CODEX_AUTO_REVIEW_LUNA_ON)
    {
        "gpt-5.4"
    } else {
        "gpt-5.6-luna"
    }
}

pub fn has_embedded_pricing_for(agent: AgentId, model: &str, as_of: Option<&str>) -> bool {
    has_embedded_pricing(pricing_model_for(agent, model, as_of))
}

/// Agent-aware cost estimate.
///
/// All agents store **disjoint** buckets after parse:
/// - Claude/Kimi/Pi: Anthropic-style input + cache create/read
/// - Codex / Grok: ccusage non-cached `input` + separate `cache_read`
///
/// Codex Fast and missing cache-read prices follow ccusage's Codex bucket.
pub fn estimate_cost_usd_for_agent(
    agent: AgentId,
    model: &str,
    tokens: CostTokens,
    cost_usd: Option<f64>,
) -> f64 {
    estimate_cost_usd_for_agent_at(agent, model, tokens, cost_usd, None)
}

pub fn estimate_cost_usd_for_agent_at(
    agent: AgentId,
    model: &str,
    tokens: CostTokens,
    cost_usd: Option<f64>,
    as_of: Option<&str>,
) -> f64 {
    if let Some(usd) = cost_usd.filter(|c| c.is_finite() && *c >= 0.0) {
        return round2(usd);
    }
    let model = pricing_model_for(agent, model, as_of);
    let Some(mut r) = table().find(model) else {
        return 0.0;
    };
    if agent == AgentId::Codex && !r.cache_read_explicit {
        r.cache_read = r.input;
        r.cache_read_above_200k = r.input_above_200k.or(Some(r.input));
    }
    round2(calculate_cost_from_pricing(tokens, r))
}

pub fn estimate_cost_from_tokens(model: &str, tokens: CostTokens, cost_usd: Option<f64>) -> f64 {
    if let Some(usd) = cost_usd.filter(|c| c.is_finite() && *c >= 0.0) {
        return round2(usd);
    }
    let Some(r) = table().find(model) else {
        return 0.0;
    };
    round2(calculate_cost_from_pricing(tokens, r))
}

/// Stored Codex token layout is **already** ccusage-style after parse:
/// - `input` = non-cached billable input (`full_input - cached_input_tokens`)
/// - `cache_read` = `cached_input_tokens`
///
/// Do **not** peel `cache` from `input` again. The old heuristic
/// (`cache <= input` ⇒ treat input as full OpenAI total) double-subtracts on
/// every collect/stats pass whenever cache hit rate is ≤ 50% of the full
/// prompt — eroding billable input toward zero across recompute passes.
///
/// Full→billable conversion happens only in `extract_codex` at parse time.
pub fn codex_billable_tokens(input: i64, cache_read: i64) -> (i64, i64) {
    (input.max(0), cache_read.max(0))
}

fn calculate_cost_from_pricing(usage: CostTokens, r: Rates) -> f64 {
    let input = usage.input.max(0) as u64;
    let output = usage.output.max(0) as u64;
    let cache_create_5m = usage.cache_create.max(0) as u64;
    let cache_create_1h = usage.cache_create_1h.max(0) as u64;
    let cache_read = usage.cache_read.max(0) as u64;

    let cache_create_1h_cost = r.input * CACHE_CREATE_1H_INPUT_MULTIPLIER;
    let cache_create_1h_cost_above = r
        .input_above_200k
        .map(|c| c * CACHE_CREATE_1H_INPUT_MULTIPLIER);

    let usd = if let Some(threshold) = r.long_context_threshold {
        let context_tokens = input
            .saturating_add(cache_read)
            .saturating_add(cache_create_5m)
            .saturating_add(cache_create_1h);
        let long_context = context_tokens > threshold;
        let rate = |base: f64, above: Option<f64>| {
            if long_context {
                above.unwrap_or(base)
            } else {
                base
            }
        };
        input as f64 * rate(r.input, r.input_above_200k)
            + output as f64 * rate(r.output, r.output_above_200k)
            + cache_create_5m as f64 * rate(r.cache_create, r.cache_create_above_200k)
            + cache_create_1h as f64 * rate(cache_create_1h_cost, cache_create_1h_cost_above)
            + cache_read as f64 * rate(r.cache_read, r.cache_read_above_200k)
    } else {
        tiered_cost(
            input,
            r.input,
            r.input_above_200k,
            DEFAULT_LONG_CONTEXT_THRESHOLD_TOKENS,
        ) + tiered_cost(
            output,
            r.output,
            r.output_above_200k,
            DEFAULT_LONG_CONTEXT_THRESHOLD_TOKENS,
        ) + tiered_cost(
            cache_create_5m,
            r.cache_create,
            r.cache_create_above_200k,
            DEFAULT_LONG_CONTEXT_THRESHOLD_TOKENS,
        ) + tiered_cost(
            cache_create_1h,
            cache_create_1h_cost,
            cache_create_1h_cost_above,
            DEFAULT_LONG_CONTEXT_THRESHOLD_TOKENS,
        ) + tiered_cost(
            cache_read,
            r.cache_read,
            r.cache_read_above_200k,
            DEFAULT_LONG_CONTEXT_THRESHOLD_TOKENS,
        )
    };

    if usage.fast && r.fast_multiplier > 0.0 && r.fast_multiplier != 1.0 {
        usd * r.fast_multiplier
    } else {
        usd
    }
}

fn tiered_cost(tokens: u64, base: f64, above: Option<f64>, threshold: u64) -> f64 {
    if tokens == 0 {
        return 0.0;
    }
    if let Some(above) = above {
        if tokens > threshold {
            return (threshold as f64 * base) + ((tokens - threshold) as f64 * above);
        }
    }
    tokens as f64 * base
}

/// Backward-compatible helper (cache = create+read treated as read; non-Codex).
pub fn estimate_cost_usd_flat(model: &str, input: i64, output: i64, cache: i64) -> f64 {
    estimate_cost_usd(model, input, output, 0, cache, None)
}

/// Whether the model resolved to an embedded row (vs unknown → $0).
pub fn has_embedded_pricing(model: &str) -> bool {
    table().find(model).is_some()
}

/// Public rates lookup (per-token USD) for diagnostics.
/// Returns embedded rates only; unknown models yield `None` (cost is $0).
pub fn rates_for_embedded(model: &str) -> Option<Rates> {
    table().find(model)
}

/// Public rates lookup — embedded row, or zero rates when unknown.
pub fn rates_for(model: &str) -> Rates {
    table().find(model).unwrap_or(Rates {
        input: 0.0,
        output: 0.0,
        cache_create: 0.0,
        cache_read: 0.0,
        cache_read_explicit: false,
        input_above_200k: None,
        output_above_200k: None,
        cache_create_above_200k: None,
        cache_read_above_200k: None,
        long_context_threshold: None,
        fast_multiplier: 1.0,
    })
}

/// Persist with 6 decimal places (USD). Per-row `$0.01` rounding zeroed cheap
/// cache-heavy Codex turns (e.g. luna @ $0.20/1M) and understated totals vs ccusage.
fn round2(v: f64) -> f64 {
    (v * 1_000_000.0).round() / 1_000_000.0
}

#[cfg(test)]
mod tests;
