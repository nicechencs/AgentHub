//! Lifecycle operation types (install family only in P07).

use serde::{Deserialize, Serialize};

use crate::models::{DetectResult, DetectStatus, InstallOutcome};
use crate::platform::AgentKey;

/// Stable operation id (UUID string).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct OperationId(pub String);

impl OperationId {
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for OperationId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for OperationId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Install-family operation kinds (runtime start/stop is out of scope for P07).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationKind {
    Install,
    Upgrade,
    Uninstall,
    Repair,
}

impl OperationKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Install => "install",
            Self::Upgrade => "upgrade",
            Self::Uninstall => "uninstall",
            Self::Repair => "repair",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "install" => Some(Self::Install),
            "upgrade" => Some(Self::Upgrade),
            "uninstall" => Some(Self::Uninstall),
            "repair" => Some(Self::Repair),
            _ => None,
        }
    }
}

/// Operation lifecycle status (not combined with install/config/runtime).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
    /// Process crashed / restart found an unfinished row.
    Interrupted,
}

impl OperationStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Interrupted => "interrupted",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "queued" => Some(Self::Queued),
            "running" => Some(Self::Running),
            "succeeded" => Some(Self::Succeeded),
            "failed" => Some(Self::Failed),
            "cancelled" => Some(Self::Cancelled),
            "interrupted" => Some(Self::Interrupted),
            _ => None,
        }
    }
}

/// Coarse step labels for progress / audit (not a giant state machine).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationStep {
    Resolve,
    CapabilityCheck,
    AcquireLock,
    CreateRecord,
    Preflight,
    Plan,
    Execute,
    Redetect,
    Finalize,
    Custom(String),
}

impl OperationStep {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Resolve => "resolve",
            Self::CapabilityCheck => "capability_check",
            Self::AcquireLock => "acquire_lock",
            Self::CreateRecord => "create_record",
            Self::Preflight => "preflight",
            Self::Plan => "plan",
            Self::Execute => "execute",
            Self::Redetect => "redetect",
            Self::Finalize => "finalize",
            Self::Custom(s) => s.as_str(),
        }
    }

    pub fn from_stored(s: &str) -> Self {
        match s {
            "resolve" => Self::Resolve,
            "capability_check" => Self::CapabilityCheck,
            "acquire_lock" => Self::AcquireLock,
            "create_record" => Self::CreateRecord,
            "preflight" => Self::Preflight,
            "plan" => Self::Plan,
            "execute" => Self::Execute,
            "redetect" => Self::Redetect,
            "finalize" => Self::Finalize,
            other => Self::Custom(other.to_string()),
        }
    }
}

/// Observed installation fact from detector (not a combined enum).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallationObserved {
    pub status: DetectStatus,
    pub version: Option<String>,
    pub binary_path: Option<String>,
    pub channel: Option<String>,
    pub notes: Vec<String>,
}

impl From<DetectResult> for InstallationObserved {
    fn from(d: DetectResult) -> Self {
        Self {
            status: d.status,
            version: d.version,
            binary_path: d.binary_path.map(|p| p.display().to_string()),
            channel: d.channel,
            notes: d.notes,
        }
    }
}

/// Typed progress for one operation (callback/channel — not a bus).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressEvent {
    pub operation_id: String,
    pub agent_key: String,
    pub kind: OperationKind,
    pub step: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub percent: Option<u8>,
}

/// Persisted operation row.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationRecord {
    pub id: String,
    pub agent_key: String,
    pub kind: OperationKind,
    pub status: OperationStatus,
    pub step: Option<String>,
    pub error_code: Option<String>,
    pub summary: Option<String>,
    pub observed_status: Option<String>,
    pub observed_version: Option<String>,
    pub started_at: String,
    pub finished_at: Option<String>,
}

/// Result of a coordinated lifecycle run (maps to InstallOutcome for compatibility).
#[derive(Debug, Clone)]
pub struct LifecycleResult {
    pub operation_id: OperationId,
    pub outcome: InstallOutcome,
    pub observed: Option<InstallationObserved>,
}

/// Lifecycle-specific errors (also map to AppError where needed).
#[derive(Debug, Clone)]
pub struct LifecycleError {
    pub code: &'static str,
    pub message: String,
}

impl LifecycleError {
    /// Attach the operation id to a pre-record error without changing its
    /// stable error code. Progress has already exposed this id by the time
    /// these errors are returned, so callers need one value to correlate.
    pub fn with_operation_id(mut self, op_id: &OperationId) -> Self {
        self.message
            .push_str(&format!(" (operation_id: {})", op_id.as_str()));
        self
    }

    pub fn lock_held(agent_key: &str) -> Self {
        Self {
            code: "lifecycle.lock_held",
            message: format!("another lifecycle operation is running for {agent_key}"),
        }
    }

    pub fn unsupported(agent: &AgentKey, kind: OperationKind) -> Self {
        Self {
            code: "lifecycle.unsupported",
            message: format!(
                "{} is not supported for agent {}",
                kind.as_str(),
                agent.as_str()
            ),
        }
    }

    pub fn not_found(agent: &str) -> Self {
        Self {
            code: "not_found",
            message: format!("agent not registered: {agent}"),
        }
    }

    /// Persist failed before execute — operation must not run the install body.
    pub fn audit_persist_failed(
        op_id: &OperationId,
        step: &str,
        cause: impl std::fmt::Display,
    ) -> Self {
        Self {
            code: "lifecycle.audit_persist_failed",
            message: format!(
                "lifecycle audit persist failed for operation {} at step {step}: {cause}",
                op_id.as_str()
            ),
        }
    }

    /// Execute already ran (or attempted); audit write failed afterward.
    /// External install state may have changed — trust detector observed status.
    pub fn audit_partial_failure(
        op_id: &OperationId,
        step: &str,
        exec_summary: &str,
        observed: Option<&str>,
        cause: impl std::fmt::Display,
    ) -> Self {
        let observed = observed.unwrap_or("unknown");
        Self {
            code: "lifecycle.audit_partial_failure",
            message: format!(
                "lifecycle audit partial failure for operation {} at step {step}; \
                 execute summary: {exec_summary}; observed: {observed}; \
                 external state may have changed — trust detector as source of truth; \
                 cause: {cause}",
                op_id.as_str()
            ),
        }
    }
}

impl std::fmt::Display for LifecycleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for LifecycleError {}

impl From<LifecycleError> for crate::error::AppError {
    fn from(e: LifecycleError) -> Self {
        crate::error::AppError::message(e.code, e.message)
    }
}
