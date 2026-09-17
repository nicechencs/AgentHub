//! Platform Session Title capability: SessionTitleSource port + registry + builtin sources.
//!
//! An Agent CLI writes its own conversation title into its own session store.
//! The registry maps `(agent key, native session id)` to that title so AgentHub
//! can adopt it for a Chat conversation.
//!
//! Per-agent sources live in [`crate::integrations`]; reading the store and
//! deciding whether a title may replace the stored one is
//! [`crate::services::ChatService::adopt_agent_title`].

mod registry;
mod source;
mod sources;

pub use registry::{empty_registry, SessionTitleRegistry};
pub use source::{is_path_safe_session_id, SessionTitleSource};
pub use sources::builtin_session_title_registry;

#[cfg(test)]
mod tests;
