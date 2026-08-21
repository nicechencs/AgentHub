//! A loopback-only OpenAI Responses compatibility bridge.
//!
//! Protocol translation is deliberately kept in [`protocol`].  The runtime host owns
//! listeners and in-memory credentials only; it never reads profiles or the database.

pub mod grok_cli;
pub mod host;
pub mod protocol;
pub mod runtime;
pub mod session;
pub mod types;

pub use host::{BridgeHostError, BridgeRuntimeHost};
pub use runtime::{
    BridgeLocalSurface, BridgeRuntimeState, BridgeRuntimeStatus, BridgeStartSpec,
    BridgeUpstreamConfig, BridgeUpstreamProtocol, BridgeUpstreamStatus, ResolvedAuth,
    UpstreamAuthReload,
};

#[cfg(test)]
mod tests;
