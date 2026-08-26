//! A loopback-only OpenAI Responses compatibility bridge.
//!
//! Protocol translation is deliberately kept in [`protocol`].  The runtime host owns
//! listeners and in-memory credentials only; it never reads profiles or the database.

pub mod account;
pub mod grok_cli;
pub mod host;
pub mod protocol;
pub mod request_fsm;
pub mod route_index;
pub mod runtime;
pub mod session;
pub mod types;

pub use account::{AccountPicker, BridgeMemberSpec, MemberHealth, MemberHealthSink, PickedMember};
pub use host::{BridgeHostError, BridgeRuntimeHost};
pub use request_fsm::{AccountSwitchGate, RequestDecision, RequestFsm, SwitchClass};
pub use route_index::{
    DispatchCandidate, EffectiveRouteIndex, MemberCapability, MemberCapabilitySnapshot,
    RouteResolveError,
};
pub use runtime::{
    BridgeLocalSurface, BridgeRuntimeState, BridgeRuntimeStatus, BridgeStartSpec,
    BridgeUpstreamConfig, BridgeUpstreamProtocol, BridgeUpstreamStatus, ResolvedAuth,
    UpstreamAuthReload,
};

#[cfg(test)]
mod tests;
