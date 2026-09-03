//! Usage persist, parse, and dashboard DTOs.
//!
//! Role owners (public names stay; this is not a rename):
//! - [`UsageRecord`]: persisted row and CLI/desktop camelCase wire. No
//!   `reasoning_tokens` column (that field belongs to protocol Usage IR).
//! - [`ParsedUsageEvent`]: session-log parse intermediate. Service fills
//!   `id` / `cost_usd`. `raw_hash` and 1h cache stay here, not on the row.
//! - Billing conversion: `usage_service/cost.rs` (`cost_for_event` /
//!   `event_missing_pricing`). Do not put unit-price formulas on these types.
//! - Display totals: [`UsageMetrics`] / [`UsageOverview`]. Frontend
//!   `usageTokenParts` must not peel cache again.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::AgentId;

/// Persisted usage row (from agent session logs) and camelCase wire DTO.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageRecord {
    pub id: String,
    pub agent_id: AgentId,
    pub account_id: Option<String>,
    pub model: String,
    pub input_tokens: i64,
    pub output_tokens: i64,
    /// Cache read tokens (hit / reuse). Billed at the cache-read rate.
    pub cache_read_tokens: i64,
    /// Cache write tokens (create + 1h ephemeral write). Billed at the write rate.
    pub cache_write_tokens: i64,
    /// Estimated cost in pricing-table currency (USD). No FX conversion.
    pub cost_usd: Option<f64>,
    pub session_id: Option<String>,
    /// RFC3339 / ISO-ish timestamp.
    pub ts: String,
    pub raw_hash: Option<String>,
    /// Codex Fast / Priority. Must persist so collect recompute keeps the multiplier.
    #[serde(default)]
    pub fast: bool,
}

impl UsageRecord {
    /// Combined cache (write + read) for totals / trend.
    pub fn cache_tokens_total(&self) -> i64 {
        self.cache_write_tokens + self.cache_read_tokens
    }
}

/// Query filter for listing usage rows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UsageQuery {
    pub days: u32,
    pub agent_id: Option<AgentId>,
    pub model: Option<String>,
    /// Soft cap on returned rows (`ORDER BY ts DESC`). `None` → 100_000.
    #[serde(default)]
    pub limit: Option<u32>,
    /// RFC3339 lower bound, AND-ed with the `days` window (`ts >= since`).
    #[serde(default)]
    pub since: Option<String>,
    /// Hidden / omitted agents. Applied before LIMIT so the table cap is among visible rows.
    #[serde(default)]
    pub exclude_agent_ids: Vec<AgentId>,
}

/// Dashboard metric totals from SQL aggregates.
///
/// `billable_input` is stored `input_tokens` (non-cached). Cache write and read
/// are stored separately because they bill at different rates. Full prompt size
/// is billable + write + read.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UsageMetrics {
    pub billable_input: i64,
    pub output: i64,
    pub cache_read: i64,
    pub cache_write: i64,
    pub cost_usd: f64,
}

/// One distribution bar: by `agent_id` when no agent filter, else by model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageDistributionSlice {
    pub key: String,
    pub tokens: i64,
    pub cost_usd: f64,
    pub billable_input: i64,
    pub output: i64,
    pub cache_read: i64,
    pub cache_write: i64,
}

/// First-paint dashboard payload: totals + distribution + model dropdown.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UsageOverview {
    pub metrics: UsageMetrics,
    pub distribution: Vec<UsageDistributionSlice>,
    pub models: Vec<String>,
}

/// Trend chart point: `{ date, claude?: n, ... }` (dynamic series keys).
///
/// Agent grouping uses agent ids; model grouping uses model names plus parallel
/// `__cost__:{series}` floats. `date` is local `YYYY-MM-DD` when `days > 1`, or
/// local `YYYY-MM-DD HH:00` when `days <= 1` (dashboard today / last 24h).
/// Empty buckets in the window are filled so a short range is not a single
/// categorical point.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UsageTrendPoint(pub Map<String, Value>);

/// Prefix for per-series cost on a model-grouped trend point.
pub const USAGE_TREND_COST_KEY_PREFIX: &str = "__cost__:";

impl UsageTrendPoint {
    pub fn new(date: impl Into<String>) -> Self {
        let mut m = Map::new();
        m.insert("date".into(), Value::String(date.into()));
        Self(m)
    }

    pub fn add_tokens(&mut self, agent: AgentId, tokens: i64) {
        self.add_named_tokens(agent.as_str(), tokens);
    }

    pub fn add_named_tokens(&mut self, key: impl AsRef<str>, tokens: i64) {
        let key = key.as_ref();
        if key.is_empty() || key == "date" || key.starts_with(USAGE_TREND_COST_KEY_PREFIX) {
            return;
        }
        let prev = self.0.get(key).and_then(|v| v.as_i64()).unwrap_or(0);
        self.0.insert(key.to_string(), Value::from(prev + tokens));
    }

    pub fn add_named_cost(&mut self, series: impl AsRef<str>, cost: f64) {
        let series = series.as_ref();
        if series.is_empty() || series == "date" || !cost.is_finite() {
            return;
        }
        let key = format!("{USAGE_TREND_COST_KEY_PREFIX}{series}");
        let prev = self.0.get(&key).and_then(|v| v.as_f64()).unwrap_or(0.0);
        self.0.insert(key, Value::from(prev + cost));
    }
}

/// Per-agent parser health for Dashboard footer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParserHealth {
    pub agent_id: AgentId,
    pub supported: bool,
    pub records: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fail_rate_pct: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skipped: Option<u64>,
}

/// Result of a collect pass.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectResult {
    pub inserted: u64,
    pub skipped: u64,
    pub failed: u64,
    pub agents: Vec<ParserHealth>,
    /// Models seen this collect with token>0, no log costUSD, and no embedded price row.
    /// Sorted unique — ccusage-style “missing pricing” hint list.
    #[serde(default)]
    pub missing_pricing_models: Vec<String>,
}

/// Parse intermediate before insert (service fills id / cost).
///
/// Token layout follows ccusage `TokenUsageRaw` (cache create vs read split).
/// Grok `reasoning_tokens` may appear in the log; they are not stored here.
#[derive(Debug, Clone)]
pub struct ParsedUsageEvent {
    pub agent_id: AgentId,
    pub model: String,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_creation_tokens: i64,
    /// 1-hour ephemeral cache writes (ccusage `ephemeral_1h_input_tokens`).
    pub cache_creation_1h_tokens: i64,
    pub cache_read_tokens: i64,
    pub session_id: Option<String>,
    pub ts: String,
    /// Dedup key (message_id+request_id when available, else content hash).
    pub raw_hash: String,
    /// Log-provided USD cost (ccusage costUSD) when present.
    pub cost_usd: Option<f64>,
    /// Codex Fast / Priority service tier for this turn.
    pub fast: bool,
}

impl ParsedUsageEvent {
    /// Cache writes (5m create + 1h ephemeral). Stored as `cache_write_tokens`.
    pub fn cache_write_tokens(&self) -> i64 {
        self.cache_creation_tokens + self.cache_creation_1h_tokens
    }

    pub fn cache_tokens_total(&self) -> i64 {
        self.cache_write_tokens() + self.cache_read_tokens
    }
}
