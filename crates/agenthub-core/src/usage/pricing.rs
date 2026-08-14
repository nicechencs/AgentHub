//! Model pricing for cost estimation.
//!
//! Inspired by ccusage:
//! - Prefer log-provided costUSD (Auto mode)
//! - Else look up embedded per-1M rates (LiteLLM-style subset)
//! - Fuzzy alias matching for dated model ids (e.g. claude-sonnet-4-20250514)
//!
//! Costs stay in the same unit as the pricing table (**USD per 1M tokens**).
//! No FX conversion at runtime.
//!
//! The embedded table is an **offline snapshot** refreshed by
//! `scripts/update-embedded-pricing.mjs` (manual `pnpm pricing:update` or daily CI).
//! Runtime never fetches pricing. Local-only models live in
//! `scripts/pricing/overrides.json`.

use std::collections::HashMap;
use std::sync::OnceLock;

/// Embedded USD-per-1M rates (same units as common public list prices).
const EMBEDDED_PRICING_JSON: &str = include_str!("embedded-pricing.json");

/// Per-token rates (USD).
#[derive(Debug, Clone, Copy)]
pub struct Rates {
    pub input: f64,
    pub output: f64,
    pub cache_create: f64,
    pub cache_read: f64,
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
            let read = row.cache_read.unwrap_or(row.input * 0.1);
            // Convert USD/1M → USD/token
            let rates = Rates {
                input: row.input / 1_000_000.0,
                output: row.output / 1_000_000.0,
                cache_create: create / 1_000_000.0,
                cache_read: read / 1_000_000.0,
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
        // Fuzzy contains match against known keys (longest first)
        let mut best: Option<(&str, Rates)> = None;
        for (k, r) in &self.by_key {
            if stripped.contains(k.as_str()) || k.contains(stripped.as_str()) {
                if best
                    .as_ref()
                    .map(|(bk, _)| k.len() > bk.len())
                    .unwrap_or(true)
                {
                    best = Some((k.as_str(), *r));
                }
            }
        }
        best.map(|(_, r)| r)
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
///
/// `input_includes_cache`: Codex/OpenAI-style rows where `input_tokens` already
/// includes `cache_read` (bill `input - cache_read` + `cache_read × cache rate`).
/// Anthropic-style rows leave this `false` (input and cache are disjoint).
pub fn estimate_cost_usd(
    model: &str,
    input: i64,
    output: i64,
    cache_create: i64,
    cache_read: i64,
    cost_usd: Option<f64>,
) -> f64 {
    estimate_cost_usd_ex(
        model,
        input,
        output,
        cache_create,
        cache_read,
        cost_usd,
        false,
    )
}

/// Agent-aware cost estimate.
///
/// All agents store **disjoint** buckets after parse:
/// - Claude/Kimi/Pi: Anthropic-style input + cache create/read
/// - Codex / Grok: ccusage non-cached `input` + separate `cache_read`
///
/// Never set `input_includes_cache` here — that flag is only for raw OpenAI
/// totals at the parse boundary (`extract_codex`), not for stored rows.
pub fn estimate_cost_usd_for_agent(
    agent: crate::models::AgentId,
    model: &str,
    input: i64,
    output: i64,
    cache_create: i64,
    cache_read: i64,
    cost_usd: Option<f64>,
) -> f64 {
    let _ = agent;
    estimate_cost_usd_ex(
        model,
        input,
        output,
        cache_create,
        cache_read,
        cost_usd,
        false,
    )
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

fn estimate_cost_usd_ex(
    model: &str,
    input: i64,
    output: i64,
    cache_create: i64,
    cache_read: i64,
    cost_usd: Option<f64>,
    input_includes_cache: bool,
) -> f64 {
    if let Some(usd) = cost_usd.filter(|c| c.is_finite() && *c >= 0.0) {
        return round2(usd);
    }
    let Some(r) = table().find(model) else {
        // Unknown model: do not invent a price (was heuristic; now $0 + missing tip).
        return 0.0;
    };

    let input = input.max(0);
    let output = output.max(0);
    let cache_create = cache_create.max(0);
    let cache_read = cache_read.max(0);

    // OpenAI/Codex: cached_input_tokens ⊆ input_tokens — bill non-cached + cache rate.
    let (bill_input, bill_cache_read) = if input_includes_cache {
        let cr = cache_read.min(input);
        ((input - cr).max(0), cr)
    } else {
        (input, cache_read)
    };

    let usd = (bill_input as f64) * r.input
        + (output as f64) * r.output
        + (cache_create as f64) * r.cache_create
        + (bill_cache_read as f64) * r.cache_read;
    round2(usd)
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

    #[test]
    fn prefers_log_cost_usd() {
        let c = estimate_cost_usd("claude-sonnet-4", 1_000_000, 0, 0, 0, Some(1.0));
        assert!((c - 1.0).abs() < 0.01);
    }

    #[test]
    fn embedded_sonnet_exact() {
        assert!(has_embedded_pricing("claude-sonnet-4"));
        let c = estimate_cost_usd("claude-sonnet-4", 1_000_000, 0, 0, 0, None);
        // $3 / 1M input tokens
        assert!((c - 3.0).abs() < 0.01, "got {c}");
    }

    #[test]
    fn small_row_not_rounded_to_zero() {
        // Cheap model + high cache: must not collapse to $0.00 at 2-dp storage.
        let c = estimate_cost_usd_for_agent(
            crate::models::AgentId::Codex,
            "gpt-5.6-luna",
            192_028,
            62,
            0,
            191_232,
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
        use crate::models::AgentId;
        // gpt-5.6-luna: $0.2 / $1.2 / $0.02 per 1M — billable 100k + cache 900k
        let c = estimate_cost_usd_for_agent(
            AgentId::Codex,
            "gpt-5.6-luna",
            100_000,
            0,
            0,
            900_000,
            None,
        );
        assert!((c - 0.038).abs() < 0.001, "got {c}");
    }
}
