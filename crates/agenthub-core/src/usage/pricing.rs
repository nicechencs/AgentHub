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
mod tests {
    use super::*;
    use crate::models::AgentId;

    #[test]
    fn prefers_log_cost_usd() {
        let c = estimate_cost_usd("claude-sonnet-4", 1_000_000, 0, 0, 0, Some(1.0));
        assert!((c - 1.0).abs() < 0.01);
    }

    #[test]
    fn embedded_sonnet_exact() {
        assert!(has_embedded_pricing("claude-sonnet-4"));
        // $3 / 1M input; 100k stays below the 200k long-context tier.
        let c = estimate_cost_usd("claude-sonnet-4", 100_000, 0, 0, 0, None);
        assert!((c - 0.3).abs() < 0.01, "got {c}");
    }

    #[test]
    fn small_row_not_rounded_to_zero() {
        // Cheap model + high cache: must not collapse to $0.00 at 2-dp storage.
        let c = estimate_cost_usd_for_agent(
            AgentId::Codex,
            "gpt-5.6-luna",
            CostTokens::from_parts(192_028, 62, 0, 191_232),
            None,
        );
        assert!(c > 0.001, "got {c}");
    }

    #[test]
    fn dated_model_alias() {
        assert!(has_embedded_pricing("claude-sonnet-4-20250514"));
        let a = rates_for("claude-sonnet-4");
        let b = rates_for("claude-sonnet-4-20250514");
        assert!((a.input - b.input).abs() < 1e-12);
    }

    #[test]
    fn kimi_for_coding_in_table() {
        assert!(has_embedded_pricing("kimi-for-coding"));
        assert!(has_embedded_pricing("moonshot/kimi-k2.6"));
    }

    #[test]
    fn cache_read_cheaper_than_create_for_sonnet() {
        let r = rates_for("claude-sonnet-4");
        assert!(r.cache_read < r.cache_create);
        assert!(r.cache_read < r.input);
    }

    #[test]
    fn unknown_model_costs_zero_without_log_cost() {
        assert!(!has_embedded_pricing("totally-unknown-model-xyz"));
        let c = estimate_cost_usd(
            "totally-unknown-model-xyz",
            1_000_000,
            1_000_000,
            0,
            0,
            None,
        );
        assert!((c - 0.0).abs() < 1e-12, "got {c}");
    }

    #[test]
    fn unknown_model_still_prefers_log_cost() {
        let c = estimate_cost_usd("totally-unknown-model-xyz", 1, 1, 0, 0, Some(2.5));
        assert!((c - 2.5).abs() < 0.01);
    }

    #[test]
    fn codex_billable_tokens_trusts_stored_non_cached_layout() {
        // Stored layout is already non-cached — never peel again.
        // full=1000, cache=250 → stored input=750; peel would wrongly yield 500.
        assert_eq!(codex_billable_tokens(750, 250), (750, 250));
        assert_eq!(codex_billable_tokens(100_000, 900_000), (100_000, 900_000));
        assert_eq!(codex_billable_tokens(500, 0), (500, 0));
        // Must be stable across repeated passes (old heuristic eroded toward 0).
        let mut input = 750i64;
        let cache = 250i64;
        for _ in 0..5 {
            let (b, c) = codex_billable_tokens(input, cache);
            assert_eq!((b, c), (750, 250));
            input = b;
        }
    }

    #[test]
    fn codex_cost_on_normalized_tokens() {
        // gpt-5.6-luna short: $0.2 / $1.2 / $0.02 per 1M — 100k billable, under 272K context
        let c = estimate_cost_usd_for_agent(
            AgentId::Codex,
            "gpt-5.6-luna",
            CostTokens::from_parts(100_000, 0, 0, 50_000),
            None,
        );
        // 0.1M * 0.2 + 0.05M * 0.02 = 0.021
        assert!((c - 0.021).abs() < 0.001, "got {c}");
    }

    #[test]
    fn cache_creation_1h_bills_at_twice_input() {
        // 100k 1h-cache writes on sonnet-4: 2 × $3/1M = $0.60 (not the 5m create rate).
        let c = estimate_cost_from_tokens(
            "claude-sonnet-4",
            CostTokens {
                cache_create_1h: 100_000,
                ..CostTokens::default()
            },
            None,
        );
        assert!((c - 0.6).abs() < 0.01, "got {c}");
    }

    #[test]
    fn sonnet_45_above_200k_is_marginal() {
        // LiteLLM above_200k on sonnet-4-5: first 200K at $3, rest at $6.
        let c = estimate_cost_usd("claude-sonnet-4-5", 1_000_000, 0, 0, 0, None);
        assert!((c - 5.4).abs() < 0.01, "got {c}");
        let c6 = estimate_cost_usd("claude-sonnet-4-6", 1_000_000, 0, 0, 0, None);
        assert!((c6 - 5.4).abs() < 0.01, "got {c6}");
    }

    #[test]
    fn gpt_56_sol_whole_request_switches_at_272k() {
        let long = estimate_cost_from_tokens(
            "gpt-5.6-sol",
            CostTokens::from_parts(300_000, 1_000, 0, 100),
            None,
        );
        // Whole request at long rates $8/$30/$0.8 per 1M.
        // 0.3*8 + 0.001*30 + 0.0001*0.8 = 2.43008
        assert!(
            (long - 2.43008).abs() < 1e-5,
            "long-context cost was {long}"
        );

        let short = estimate_cost_from_tokens(
            "gpt-5.6-sol",
            CostTokens::from_parts(100_000, 1_000, 0, 100),
            None,
        );
        // Short rates $4/$20/$0.4: 0.4 + 0.02 + 0.00004 = 0.42004
        assert!(
            (short - 0.42004).abs() < 1e-5,
            "short-context cost was {short}"
        );
    }

    #[test]
    fn cached_context_selects_long_context_tier() {
        let c = estimate_cost_from_tokens(
            "gpt-5.6-luna",
            CostTokens::from_parts(10_000, 1_000, 0, 500_000),
            None,
        );
        // 510K context > 272K → long $0.4/$1.8/$0.04 per 1M
        // 0.01*0.4 + 0.001*1.8 + 0.5*0.04 = 0.0258
        assert!((c - 0.0258).abs() < 1e-5, "cached-heavy cost was {c}");
    }

    #[test]
    fn auto_review_is_not_a_priced_model() {
        assert!(!has_embedded_pricing("codex-auto-review"));
        assert_eq!(
            pricing_model_for(AgentId::Codex, "codex-auto-review", None),
            "gpt-5.6-luna"
        );
        assert_eq!(
            pricing_model_for(
                AgentId::Codex,
                "codex-auto-review",
                Some("2026-07-29T23:59:59Z")
            ),
            "gpt-5.4"
        );
        assert_eq!(
            pricing_model_for(
                AgentId::Codex,
                "codex-auto-review",
                Some("2026-07-30T00:00:00Z")
            ),
            "gpt-5.6-luna"
        );
        assert_eq!(
            pricing_model_for(AgentId::Codex, "gpt-5.6-sol", None),
            "gpt-5.6-sol"
        );
    }

    #[test]
    fn auto_review_bills_at_published_backend_rates() {
        // Stay under the 272K whole-request switch so this isolates the rate card.
        let tokens = CostTokens::from_parts(100_000, 0, 0, 0);
        let luna = estimate_cost_usd_for_agent(AgentId::Codex, "gpt-5.6-luna", tokens, None);
        let review = estimate_cost_usd_for_agent_at(
            AgentId::Codex,
            "codex-auto-review",
            tokens,
            None,
            Some("2026-08-26T00:00:00Z"),
        );
        assert!((review - luna).abs() < 1e-12, "got {review} want {luna}");
        let old = estimate_cost_usd_for_agent_at(
            AgentId::Codex,
            "codex-auto-review",
            tokens,
            None,
            Some("2026-07-01T00:00:00Z"),
        );
        let gpt54 = estimate_cost_usd_for_agent(AgentId::Codex, "gpt-5.4", tokens, None);
        assert!((old - gpt54).abs() < 1e-12, "got {old} want {gpt54}");
        assert!((review - 0.02).abs() < 1e-9, "luna input got {review}");
        assert!((old - 0.25).abs() < 1e-9, "gpt-5.4 input got {old}");
    }

    #[test]
    fn fast_multiplier_applies_to_codex_token_cost() {
        // Stay under the 272K whole-request switch so this isolates Fast.
        let standard = estimate_cost_usd_for_agent(
            AgentId::Codex,
            "gpt-5.6-sol",
            CostTokens::from_parts(100_000, 0, 0, 0),
            None,
        );
        let fast = estimate_cost_usd_for_agent(
            AgentId::Codex,
            "gpt-5.6-sol",
            CostTokens {
                input: 100_000,
                fast: true,
                ..CostTokens::default()
            },
            None,
        );
        assert!((standard - 0.4).abs() < 0.01, "standard got {standard}");
        assert!((fast - 0.8).abs() < 0.01, "fast got {fast}");
    }

    #[test]
    fn log_cost_skips_fast_and_long_context() {
        let c = estimate_cost_usd_for_agent(
            AgentId::Codex,
            "gpt-5.6-sol",
            CostTokens {
                input: 1_000_000,
                fast: true,
                ..CostTokens::default()
            },
            Some(1.5),
        );
        assert!((c - 1.5).abs() < 0.01);
    }
}
