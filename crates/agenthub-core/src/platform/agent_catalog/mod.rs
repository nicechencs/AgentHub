//! Agent Catalog — read-only discovery of registered agents.
//!
//! Aggregates descriptors from the legacy [`AdapterRegistry`], capability
//! matrix, and install catalog. Callers should prefer this service over
//! hard-coded `AgentId` lists when listing product metadata.
//!
//! # Migration notes
//!
//! TODO(P03-P08): replace internal aggregation that still walks
//! [`crate::models::AgentId::ALL`] / legacy registry with
//! `integrations/agents/<key>` contributions. Delete the bridge once every
//! platform caller uses [`AgentKey`] + sparse ports and CodeGraph shows no
//! remaining `AgentId` match in platform services.

mod key;
mod model;
mod service;

pub use key::{parse_agent_key, AgentKey, AgentKeyError};
pub use model::{AgentDescriptor, InstallChannelDescriptor};
pub use service::AgentCatalogService;

#[cfg(test)]
mod tests;
