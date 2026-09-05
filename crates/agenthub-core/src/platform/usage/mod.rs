//! Platform Usage capability: ports, registry, and agent-agnostic collection.
//!
//! Agent-specific log discovery lives in [`crate::integrations`].

mod collect;
mod gateway_source;
mod registry;
mod source;
pub mod sources;

pub use collect::{collect_for_agent_id, collect_with_source, collect_with_source_for_agent_id};
pub use registry::{builtin_usage_registry, UsageSourceRegistry};
pub use source::{RawUsageEvent, TokenAccounting, UsageFileParser, UsageLineOutcome, UsageSource};

pub(crate) use gateway_source::{ingest_spool_dir_with, GatewaySpoolOutcome};

#[cfg(test)]
pub(crate) use collect::parse_file_for_agent_id;

/// Whether a registered UsageSource exists for this agent.
pub fn supports_usage_agent(agent: crate::models::AgentId) -> bool {
    builtin_usage_registry().get_agent_id(agent).is_some()
}

#[cfg(test)]
mod tests;
