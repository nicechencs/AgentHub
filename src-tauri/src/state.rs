//! Application state managed by Tauri.
//! Holds a single shared AgentHub facade from agenthub-core.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use agenthub_core::bridge::BridgeRuntimeHost;
use agenthub_core::logging::{self, targets};
use agenthub_core::AgentHub;

use crate::adapter_bridge_controller::AdapterBridgeSagaCoordinator;
use crate::exit_coordinator::{ExitCoordinator, LifecycleShutdownBarrier};
use crate::window_policy::{self, parse_bool_setting};

/// Shared GUI state: one AgentHub opened at process start.
pub struct AppState {
    hub: Result<Arc<AgentHub>, String>,
    /// Process-owned loopback bridge listeners. The GUI owns this host because
    /// it can keep running in the tray after the main window is hidden.
    bridge_host: Arc<BridgeRuntimeHost>,
    /// Process-local authority for every adapter bridge lifecycle saga.
    bridge_saga_coordinator: Arc<AdapterBridgeSagaCoordinator>,
    /// Coordinates every controllable process exit through bridge shutdown.
    exit_coordinator: ExitCoordinator,
    /// Prevents a second tray click or window close from opening another
    /// bridge-impact prompt while the first native dialog is still visible.
    exit_confirmation_pending: AtomicBool,
    /// Set when the user chooses Quit from the tray (or equivalent).
    /// When true, window close is allowed to exit the process.
    exit_requested: AtomicBool,
    /// When true, the window close button hides to tray instead of quitting.
    close_to_tray: AtomicBool,
}

impl AppState {
    /// Create hub with default data dir (`AGENTHUB_HOME` / `~/.agenthub`).
    /// Initialization errors are retained so the GUI can still start and
    /// report them through commands instead of crashing before a window opens.
    pub fn new() -> Self {
        if let Err(e) = logging::init_for_app(None, "gui", false, env!("CARGO_PKG_VERSION")) {
            // Keep starting; surface via hub error path if needed.
            eprintln!("warning: logging init failed: {e}");
        }
        let hub = AgentHub::open(None).map(Arc::new).map_err(|error| {
            logging::log_app_error(targets::GUI, "open", &error);
            error.to_string()
        });
        if hub.is_ok() {
            tracing::info!(
                target: targets::GUI,
                module = targets::GUI,
                op = "ready",
                "GUI AppState ready"
            );
        }
        Self::from_hub(hub)
    }

    /// Build state around an already-opened hub (tests / alternate entry points).
    pub(crate) fn from_hub(hub: Result<Arc<AgentHub>, String>) -> Self {
        let close_to_tray = load_close_to_tray(&hub);
        Self {
            hub,
            bridge_host: Arc::new(BridgeRuntimeHost::new()),
            bridge_saga_coordinator: Arc::new(AdapterBridgeSagaCoordinator::new()),
            exit_coordinator: ExitCoordinator::new(),
            exit_confirmation_pending: AtomicBool::new(false),
            exit_requested: AtomicBool::new(false),
            close_to_tray: AtomicBool::new(close_to_tray),
        }
    }

    pub fn hub(&self) -> Result<&AgentHub, String> {
        self.hub.as_deref().map_err(|error| error.to_owned())
    }

    /// Clone the shared Arc for long-running / blocking commands (e.g. chat_send).
    pub fn hub_arc(&self) -> Result<Arc<AgentHub>, String> {
        self.hub
            .as_ref()
            .map(Arc::clone)
            .map_err(|error| error.to_owned())
    }

    /// Shared bridge listener host. Construction does not start or restore any
    /// adapter profile; that is intentionally owned by a later control-plane
    /// command layer.
    pub(crate) fn bridge_host(&self) -> Arc<BridgeRuntimeHost> {
        Arc::clone(&self.bridge_host)
    }

    pub(crate) fn bridge_saga_coordinator(&self) -> Arc<AdapterBridgeSagaCoordinator> {
        Arc::clone(&self.bridge_saga_coordinator)
    }

    pub(crate) fn exit_coordinator(&self) -> &ExitCoordinator {
        &self.exit_coordinator
    }

    /// Shared read/write barrier used by bridge lifecycle sagas and the
    /// coordinated exit/restart path.  Lifecycle callers must acquire this
    /// before a profile or target guard.
    pub(crate) fn lifecycle_shutdown_barrier(&self) -> Arc<LifecycleShutdownBarrier> {
        self.exit_coordinator.lifecycle_barrier()
    }

    /// Claim the one outstanding bridge-impact confirmation dialog.
    pub(crate) fn begin_exit_confirmation(&self) -> bool {
        self.exit_confirmation_pending
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    }

    /// Release the confirmation gate when the user hides to tray or cancels.
    pub(crate) fn finish_exit_confirmation(&self) {
        self.exit_confirmation_pending
            .store(false, Ordering::SeqCst);
    }

    #[cfg(test)]
    pub(crate) fn exit_confirmation_pending(&self) -> bool {
        self.exit_confirmation_pending.load(Ordering::SeqCst)
    }

    pub fn request_exit(&self) {
        self.exit_requested.store(true, Ordering::SeqCst);
    }

    pub fn should_exit(&self) -> bool {
        self.exit_requested.load(Ordering::SeqCst)
    }

    pub fn close_to_tray(&self) -> bool {
        self.close_to_tray.load(Ordering::SeqCst)
    }

    pub fn set_close_to_tray(&self, value: bool) {
        self.close_to_tray.store(value, Ordering::SeqCst);
    }

    /// Apply a `set_setting` write to the in-process close-to-tray flag when relevant.
    pub(crate) fn sync_setting_flag(&self, key: &str, value: &str) {
        if key == "close_to_tray" {
            self.set_close_to_tray(window_policy::is_close_to_tray_enabled(value));
        }
    }
}

fn load_close_to_tray(hub: &Result<Arc<AgentHub>, String>) -> bool {
    hub.as_ref()
        .ok()
        .and_then(|h| h.settings.get("close_to_tray").ok().flatten())
        .map(|v| parse_bool_setting(&v))
        .unwrap_or(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use agenthub_core::AgentHub;
    use tempfile::tempdir;

    fn hub_tmp() -> (tempfile::TempDir, Arc<AgentHub>) {
        let dir = tempdir().unwrap();
        let hub = Arc::new(AgentHub::open(Some(dir.path())).unwrap());
        (dir, hub)
    }

    #[test]
    fn defaults_close_to_tray_true_and_not_exiting() {
        let (_dir, hub) = hub_tmp();
        let state = AppState::from_hub(Ok(hub));
        assert!(state.close_to_tray());
        assert!(!state.should_exit());
    }

    #[test]
    fn loads_close_to_tray_false_from_settings() {
        let (_dir, hub) = hub_tmp();
        hub.settings.set("close_to_tray", "false").unwrap();
        let state = AppState::from_hub(Ok(hub));
        assert!(!state.close_to_tray());
    }

    #[test]
    fn hub_error_still_defaults_close_to_tray_true() {
        let state = AppState::from_hub(Err("open failed".into()));
        assert!(state.close_to_tray());
        assert!(state.hub().is_err());
        assert!(state.hub_arc().is_err());
    }

    #[test]
    fn request_exit_and_set_close_to_tray_flags() {
        let (_dir, hub) = hub_tmp();
        let state = AppState::from_hub(Ok(hub));
        state.set_close_to_tray(false);
        assert!(!state.close_to_tray());
        state.set_close_to_tray(true);
        assert!(state.close_to_tray());
        state.request_exit();
        assert!(state.should_exit());
    }

    #[test]
    fn exit_confirmation_gate_is_idempotent() {
        let (_dir, hub) = hub_tmp();
        let state = AppState::from_hub(Ok(hub));

        assert!(state.begin_exit_confirmation());
        assert!(state.exit_confirmation_pending());
        assert!(!state.begin_exit_confirmation());

        state.finish_exit_confirmation();
        assert!(!state.exit_confirmation_pending());
        assert!(state.begin_exit_confirmation());
    }

    #[test]
    fn sync_setting_flag_only_reacts_to_close_to_tray() {
        let (_dir, hub) = hub_tmp();
        let state = AppState::from_hub(Ok(hub));
        assert!(state.close_to_tray());

        state.sync_setting_flag("theme", "dark");
        assert!(state.close_to_tray());

        state.sync_setting_flag("close_to_tray", "false");
        assert!(!state.close_to_tray());

        state.sync_setting_flag("close_to_tray", "true");
        assert!(state.close_to_tray());

        state.sync_setting_flag("close_to_tray", "0");
        assert!(!state.close_to_tray());
    }
}
