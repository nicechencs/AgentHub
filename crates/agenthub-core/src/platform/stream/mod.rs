//! Platform stream parsing: StreamParser port + registry.
//!
//! Line buffering and UI StreamOutput mapping stay in
//! [`crate::utils::stream_parse::StreamSession`]; agent-specific NDJSON
//! decoding is registered here.
//!
//! Per-agent NDJSON decoders live in [`crate::integrations`].

mod parser;
mod registry;
pub mod sources;

pub use parser::{StreamParseError, StreamParser};
pub use registry::{builtin_stream_registry, StreamParserRegistry, StreamParserRegistryError};

/// Whether a structured StreamParser is registered for this agent.
pub fn has_stream_parser(agent: crate::models::AgentId) -> bool {
    builtin_stream_registry().contains(agent)
}

#[cfg(test)]
mod tests;
