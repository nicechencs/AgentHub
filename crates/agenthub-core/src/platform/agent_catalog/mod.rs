//! Agent Catalog — read-only discovery of registered agents.
//!
//! Aggregates descriptors from the live [`AdapterRegistry`] (registration
//! order → [`AgentKey`]), capability matrix, and install catalog. Callers
//! should prefer this service over hard-coded `AgentId` lists when listing
//! product metadata. [`crate::models::AgentId`] remains a compatibility DTO
//! for old API / DB columns — it is not deleted.
//!
//! # Composition
//!
//! [`AgentCatalogService::from_registry`] walks registry registration order as
//! [`AgentKey`]s. It does **not** iterate [`crate::models::AgentId::ALL`].
//! Unknown keys resolve to not_found / unavailable.

mod key;
mod model;
mod service;

pub use key::{parse_agent_key, AgentKey, AgentKeyError};
pub use model::{AgentDescriptor, InstallChannelDescriptor};
pub use service::AgentCatalogService;

#[cfg(test)]
mod tests;
