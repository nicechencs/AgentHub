//! A loopback-only OpenAI Responses compatibility bridge.
//!
//! Protocol translation is deliberately kept in [`protocol`].  The runtime host owns
//! listeners and in-memory credentials only; it never reads profiles or the database.

pub mod account;
pub mod auth_reload;
mod bounded_ttl;
pub mod grok_cli;
pub mod host;
mod model_switch;
pub mod protocol;
pub mod request_fsm;
pub mod route_index;
pub mod runtime;
pub mod session;
pub mod types;
pub mod upstream_class;
mod usage;
pub mod usage_capture;

pub use account::{
    route_scoped_affinity_key, AccountPicker, BridgeMemberSpec, MemberHealth, MemberHealthSink,
    PickedMember,
};
pub use host::{BridgeHostError, BridgeRuntimeHost};
pub(crate) use model_switch::{decide_model_switch, ModelSwitchCandidate, ModelSwitchDecision};
pub use request_fsm::{AccountSwitchGate, RequestDecision, RequestFsm, SwitchClass};
pub use route_index::{
    index_from_member_listings, DispatchCandidate, EffectiveRouteIndex, MemberCapability,
    MemberCapabilitySnapshot, MemberListing, RouteRejectionReason, RouteResolveError,
};
pub use runtime::{
    BridgeLocalSurface, BridgeRuntimeState, BridgeRuntimeStatus, BridgeStartSpec,
    BridgeUpstreamConfig, BridgeUpstreamProtocol, BridgeUpstreamStatus, DownstreamResponsesProfile,
    ResolvedAuth, ResponsesDialect, UpstreamAuthReload,
};

#[cfg(test)]
mod tests;
