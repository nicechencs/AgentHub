use std::fmt;
use std::time::SystemTime;

/// An already resolved upstream credential. It is intentionally supplied by the caller and
/// retained only in the live runtime process; do not persist or serialise it.
#[derive(Clone)]
pub struct ResolvedAuth {
    bearer_token: String,
}

impl ResolvedAuth {
    pub fn bearer(token: impl Into<String>) -> Self {
        Self {
            bearer_token: token.into(),
        }
    }

    pub(crate) fn token(&self) -> &str {
        &self.bearer_token
    }
}

impl std::fmt::Debug for ResolvedAuth {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("ResolvedAuth(REDACTED)")
    }
}

/// Upstream Kimi configuration. `source_connection_id` is for status/audit correlation only;
/// the host neither resolves it nor touches AgentHub configuration storage.
#[derive(Debug, Clone)]
pub struct BridgeUpstreamConfig {
    pub base_url: String,
    pub model: Option<String>,
    pub source_connection_id: Option<String>,
    pub auth: ResolvedAuth,
}

/// Inputs required to start one independent local bridge instance.
#[derive(Clone)]
pub struct BridgeStartSpec {
    pub profile_id: String,
    /// A requested TCP port. `0` asks the OS for an available loopback port.
    pub port: u16,
    /// Bearer token accepted by the local HTTP endpoint. This value is never returned by status.
    pub local_token: String,
    pub upstream: BridgeUpstreamConfig,
}

impl fmt::Debug for BridgeStartSpec {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BridgeStartSpec")
            .field("profile_id", &self.profile_id)
            .field("port", &self.port)
            .field("local_token", &"REDACTED")
            .field("upstream", &self.upstream)
            .finish()
    }
}

impl BridgeStartSpec {
    pub fn new(
        profile_id: impl Into<String>,
        port: u16,
        local_token: impl Into<String>,
        upstream: BridgeUpstreamConfig,
    ) -> Self {
        Self {
            profile_id: profile_id.into(),
            port,
            local_token: local_token.into(),
            upstream,
        }
    }
}

/// Safe, credential-free runtime state exposed to callers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BridgeRuntimeStatus {
    pub profile_id: String,
    pub port: u16,
    pub running: bool,
    pub started_at: SystemTime,
    pub source_connection_id: Option<String>,
    /// Listener lifecycle only. It deliberately does not infer that the upstream accepts a
    /// credential or is currently reachable.
    pub state: BridgeRuntimeState,
    pub upstream_status: BridgeUpstreamStatus,
}

/// A bridge listener's observable lifecycle. A stopped or failed listener is never represented
/// as a successful-but-false `running` flag alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BridgeRuntimeState {
    Starting,
    Running,
    Stopping,
    Stopped,
    Error,
    Degraded,
}

/// Upstream reachability is intentionally not probed from health/status calls. That avoids
/// surprising billable requests and makes the unknown state explicit to callers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BridgeUpstreamStatus {
    Unknown,
}
