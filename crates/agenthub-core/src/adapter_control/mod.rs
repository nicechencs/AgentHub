//! Tauri-neutral adapter / local_bridge control contract (P2-5 steps 1–3).
//!
//! Desktop (and a future sidecar client) implement [`AdapterControl`]. Shell
//! commands only parse wire input and delegate. Process-local profile / target
//! gates live here so command files do not embed lock types or depend on
//! `tauri::State` for concurrency authority.
//!
//! Sidecar binary, IPC, and schema lease are intentionally out of scope.

mod contract;
mod coordinator;
mod status;

#[cfg(test)]
mod tests;

pub use contract::{
    resolve_bind_action, resolve_unbind_action, AdapterControl, BindAction, UnbindAction,
};
pub use coordinator::AdapterSagaCoordinator;
/// Historical desktop name for the same process-local saga gate.
pub type AdapterBridgeSagaCoordinator = AdapterSagaCoordinator;
pub use status::AdapterBridgeStatus;
