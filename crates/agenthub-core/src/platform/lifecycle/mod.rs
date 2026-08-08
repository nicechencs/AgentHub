//! Unified install-family lifecycle (install / upgrade / uninstall / repair).
//!
//! ProgressSink is per-call only — not a global event bus.
//! Database `operations` rows are audit/recovery; detector remains the install fact source.

mod coordinator;
mod executor;
mod progress;
mod types;

pub use coordinator::LifecycleCoordinator;
pub use executor::{BuiltinLifecycleInstallExecutor, LifecycleInstallExecutor};
pub use progress::{LogLineProgressSink, NullProgressSink, ProgressSink, VecProgressSink};
pub use types::{
    InstallationObserved, LifecycleError, LifecycleResult, OperationId, OperationKind,
    OperationRecord, OperationStatus, OperationStep, ProgressEvent,
};

#[cfg(test)]
mod tests;
