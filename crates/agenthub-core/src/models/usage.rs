//! Usage / token statistics models.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::AgentId;

/// One persisted usage row (from agent session logs).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageRecord {
    pub id: String,
    pub agent_id: AgentId,
    pub account_id: Option<String>,
    pub model: String,
    pub input_tokens: i64,
    pub output_tokens: i64,
    /// Combined cache tokens (create + read) for DB column `cache_tokens`.
    pub cache_tokens: i64,
    /// Estimated cost in pricing-table currency (USD). No FX conversion.
    pub cost_usd: Option<f64>,
    pub session_id: Option<String>,
    /// RFC3339 / ISO-ish timestamp.
    pub ts: String,
    pub raw_hash: Option<String>,
}

/// Query filter for listing usage rows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageQuery {
    pub days: u32,
    pub agent_id: Option<AgentId>,
    pub model: Option<String>,
}

/// Trend chart point: `{ date, claude?: n, codex?: n, ... }` (dynamic agent keys).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UsageTrendPoint(pub Map<String, Value>);

impl UsageTrendPoint {
    pub fn new(date: impl Into<String>) -> Self {
        let mut m = Map::new();
        m.insert("date".into(), Value::String(date.into()));
        Self(m)
    }

    pub fn add_tokens(&mut self, agent: AgentId, tokens: i64) {
        let key = agent.as_str().to_string();
        let prev = self
            .0
            .get(&key)
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        self.0.insert(key, Value::from(prev + tokens));
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

/// Parsed line before insert (service fills id / cost).
///
/// Token layout follows ccusage `TokenUsageRaw` (cache create vs read split).
#[derive(Debug, Clone)]
pub struct ParsedUsageEvent {
    pub agent_id: AgentId,
    pub model: String,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_creation_tokens: i64,
    pub cache_read_tokens: i64,
    pub session_id: Option<String>,
    pub ts: String,
    /// Dedup key (message_id+request_id when available, else content hash).
    pub raw_hash: String,
    /// Log-provided USD cost (ccusage costUSD) when present.
    pub cost_usd: Option<f64>,
}

impl ParsedUsageEvent {
    pub fn cache_tokens_total(&self) -> i64 {
        self.cache_creation_tokens + self.cache_read_tokens
    }
}
