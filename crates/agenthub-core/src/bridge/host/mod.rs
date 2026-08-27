//! Loopback bridge HTTP host: gateway lifecycle, auth, and upstream dispatch.
//!
//! Split for maintainability only — public paths stay
//! [`crate::bridge::host::{BridgeRuntimeHost, BridgeHostError}`].

mod admission;
mod continuation;
mod dispatch;
mod gateway;
mod http;
pub(crate) mod inbound;
mod lifecycle;
mod pair_policy;
mod stream;
mod surface;
mod transport;
mod upstream;

pub use gateway::BridgeHostError;
// Re-exported for bridge tests.
#[allow(unused_imports)]
pub(super) use gateway::CleanupCompletion;
pub use inbound::InboundRequestRecord;
pub use lifecycle::BridgeRuntimeHost;

#[cfg(test)]
pub(super) use http::sse_frame_end;

use std::time::Duration;

pub(super) const ANTHROPIC_API_VERSION: &str = "2023-06-01";

pub(super) const BODY_LIMIT_BYTES: usize = 1_048_576;
/// Streamed Completions/Responses traffic can exceed the request-body safety
/// ceiling; keep a hard cap while allowing realistic agent sessions.
pub(super) const STREAM_LIMIT_BYTES: usize = 32 * 1_048_576;
/// Desktop safety cap per local-bridge profile, not a conversation quota.
/// Claude/Codex fan-out holds an SSE slot until the stream ends; a handful of
/// slots 429s agent parallelism. 256 matches grok2api's per-account max.
/// Body size and idle timeouts remain the primary guards.
#[cfg(not(test))]
pub(super) const MAX_IN_FLIGHT_REQUESTS_PER_PROFILE: usize = 256;
/// Tests fill this gate against a slow upstream; keep the cap small.
#[cfg(test)]
pub(super) const MAX_IN_FLIGHT_REQUESTS_PER_PROFILE: usize = 4;
pub(super) const UPSTREAM_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
pub(super) const UPSTREAM_RESPONSE_HEADER_TIMEOUT: Duration = Duration::from_secs(30);
pub(super) const UPSTREAM_NON_STREAM_TIMEOUT: Duration = Duration::from_secs(120);
pub(super) const UPSTREAM_BODY_IDLE_TIMEOUT: Duration = Duration::from_secs(30);
#[cfg(not(test))]
pub(super) const UPSTREAM_STREAM_IDLE_TIMEOUT: Duration = Duration::from_secs(30);
// Keep the production policy above while making the idle-path regression test practical.
#[cfg(test)]
pub(super) const UPSTREAM_STREAM_IDLE_TIMEOUT: Duration = Duration::from_millis(100);
pub(super) const REQUEST_BODY_TIMEOUT: Duration = Duration::from_secs(30);
pub(super) const DRAIN_TIMEOUT: Duration = Duration::from_secs(5);
pub(super) const FORCE_CANCEL_GRACE: Duration = Duration::from_millis(200);
pub(super) const TASK_POLL_INTERVAL: Duration = Duration::from_millis(10);
