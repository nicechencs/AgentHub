//! Wire DTOs and serde shapes shared by GUI / CLI / services.
//!
//! Protocol planning tables (`ADAPTER_CAPABILITY_MATRIX`, agent bind
//! `accepts` / `writer`, `decide_adapter_capability`) live in
//! [`crate::domain::protocol_graph`] and are re-exported here for path
//! compatibility. This module is not a pure-data dump of every planning
//! rule — prefer the domain path for new planning-graph call sites.

mod account;
mod adapter;
mod adapter_model_mapping;
mod adapter_state_model;
mod agent;
mod agent_visibility;
mod backup;
mod capability;
mod chat;
mod claude_client_env;
mod connection_trash;
mod install;
mod project;
mod provider;
mod route_pool;
mod run;
mod runtime;
mod settings;
mod skill;
mod switch_preview;
mod ticket;
mod update;
mod usage;

#[cfg(test)]
mod tests;

pub use account::*;
pub use adapter::*;
pub use adapter_model_mapping::*;
pub use adapter_state_model::*;
pub use agent::*;
pub use agent_visibility::*;
pub use backup::*;
pub use capability::*;
pub use chat::*;
pub use claude_client_env::*;
pub use connection_trash::*;
pub use install::*;
pub use project::*;
pub use provider::*;
pub use route_pool::*;
pub use run::*;
pub use runtime::*;
pub use settings::*;
pub use skill::*;
pub use switch_preview::*;
pub use ticket::*;
pub use update::*;
pub use usage::*;

// Compatibility re-exports: historical `models::` call sites keep working.
pub use crate::domain::protocol_graph::*;
