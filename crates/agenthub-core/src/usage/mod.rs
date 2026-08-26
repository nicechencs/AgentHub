//! Pluggable session-log usage parsers (zero-proxy, read-only).
//!
//! Parsing strategy informed by **ccusage**: agent-specific loaders, prefilter,
//! message/request dedupe, prefer log costUSD, then token×rates.

pub(crate) mod grok;
mod pricing;
// Legacy parsers + helpers; platform UsageSource integrations call into this
// module. Collect entry is a façade over platform::usage.
pub mod session_jsonl;

pub use pricing::{
    codex_billable_tokens, estimate_cost_from_tokens, estimate_cost_usd, estimate_cost_usd_flat,
    estimate_cost_usd_for_agent, estimate_cost_usd_for_agent_at, has_embedded_pricing,
    has_embedded_pricing_for, pricing_model_for, rates_for, rates_for_embedded, CostTokens,
};
pub use session_jsonl::collect_for_agent;

use crate::models::AgentId;

/// Which agents have a registered [`crate::platform::usage::UsageSource`].
///
/// Cursor and any future agent without a source return false (unsupported).
pub fn supports_usage(agent: AgentId) -> bool {
    crate::platform::usage::supports_usage_agent(agent)
}
