//! Gateway usage row and dashboard DTOs (`gateway_usage` table, migration
//! 00024).
//!
//! Role owners: [`GatewayUsageRow`] is the persisted per-request row observed
//! by the local bridge runtime and the camelCase wire DTO. This table is
//! deliberately separate from [`super::UsageRecord`]: agent log collection
//! already records the same spend, and merging the two would double count.

use serde::{Deserialize, Serialize};

/// Persisted per-request gateway usage row (camelCase wire DTO).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayUsageRow {
    pub request_id: String,
    /// RFC3339 timestamp of the captured outcome.
    pub ts: String,
    pub profile_id: String,
    /// Downstream surface op name (`responses` / `messages` / `chat`).
    pub surface: String,
    pub upstream_channel: Option<String>,
    pub ticket_id: Option<String>,
    pub account_source_kind: Option<String>,
    pub account_source_id: Option<String>,
    /// Public model string from the client request body.
    pub model: Option<String>,
    pub upstream_model: Option<String>,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cached_input_tokens: Option<i64>,
    pub reasoning_tokens: Option<i64>,
    /// `ok` or `failed`.
    pub status: String,
    pub status_code: Option<i64>,
    pub error_class: Option<String>,
    pub latency_ms: Option<i64>,
    pub ttft_ms: Option<i64>,
    pub attempts: Option<i64>,
    pub session_id: Option<String>,
}

/// Query filter for gateway usage rows. The time range is optional; an empty
/// filter covers everything stored.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct GatewayUsageQuery {
    /// RFC3339 lower bound (`ts >= since`).
    #[serde(default)]
    pub since: Option<String>,
    /// RFC3339 upper bound (`ts <= until`).
    #[serde(default)]
    pub until: Option<String>,
    #[serde(default)]
    pub profile_id: Option<String>,
    /// Soft cap on returned rows (`ORDER BY ts DESC`). `None` → 100_000.
    #[serde(default)]
    pub limit: Option<u32>,
}

/// Aggregated gateway usage overview for a time window.
///
/// `p95_latency_ms` uses the nearest-rank method (sorted samples, index
/// `ceil(0.95 * n) - 1`) computed in Rust: SQLite ships no percentile
/// aggregate, and the sample count per window is dashboard-sized.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct GatewayUsageOverview {
    pub request_count: i64,
    pub ok_count: i64,
    pub failed_count: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cached_input_tokens: i64,
    pub reasoning_tokens: i64,
    pub avg_latency_ms: Option<f64>,
    pub p95_latency_ms: Option<i64>,
    pub avg_ttft_ms: Option<f64>,
}
