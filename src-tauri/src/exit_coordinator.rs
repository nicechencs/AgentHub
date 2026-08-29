//! One process-wide exit path for the GUI.
//!
//! Bridge listeners belong to the desktop process rather than an independent
//! daemon.  Any controllable exit therefore acquires this coordinator before
//! waiting for [`BridgeRuntimeHost::shutdown`].  The atomic gate makes repeated
//! tray clicks and overlapping Tauri exit events harmless.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[cfg(test)]
use agenthub_core::bridge::BridgeHostError;
use agenthub_core::bridge::BridgeRuntimeHost;
use tauri::{AppHandle, Runtime};
use tokio::sync::{OwnedRwLockReadGuard, OwnedRwLockWriteGuard, RwLock};

/// Prevents a host shutdown from racing an in-flight bridge lifecycle saga.
///
/// Bridge work enters through a shared permit.  Once shutdown starts we close
/// the gate synchronously, then the exclusive permit waits for every existing
/// saga to leave before the listener host is drained.  The second closed check
/// covers a request that observed the old value just before shutdown began.
pub(crate) struct LifecycleShutdownBarrier {
    closed: AtomicBool,
    gate: Arc<RwLock<()>>,
}

impl LifecycleShutdownBarrier {
    fn new() -> Self {
        Self {
            closed: AtomicBool::new(false),
            gate: Arc::new(RwLock::new(())),
        }
    }

    pub(crate) async fn enter(&self) -> Result<OwnedRwLockReadGuard<()>, String> {
        if self.closed.load(Ordering::SeqCst) {
            return Err("bridge lifecycle is shutting down".into());
        }
        let permit = Arc::clone(&self.gate).read_owned().await;
        if self.closed.load(Ordering::SeqCst) {
            drop(permit);
            return Err("bridge lifecycle is shutting down".into());
        }
        Ok(permit)
    }

    fn close(&self) {
        self.closed.store(true, Ordering::SeqCst);
    }

    async fn wait_for_sagas(&self) -> OwnedRwLockWriteGuard<()> {
        Arc::clone(&self.gate).write_owned().await
    }

    #[cfg(test)]
    fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }
}

/// A snapshot suitable for a future exit-confirmation UI.  This module does
/// not choose a UI policy; callers may use the active bridge count to explain
/// why a quit will stop local endpoints.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ExitPreparation {
    pub active_bridge_count: Option<usize>,
}

/// Result of attempting to take responsibility for process shutdown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExitBegin {
    Started(ExitPreparation),
    AlreadyInProgress,
}

/// User-facing choices offered before an exit would interrupt a local bridge.
/// Keeping this pure lets the native-dialog callback remain small and makes the
/// policy explicit on platforms whose dialog APIs only expose two buttons.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExitImpactChoice {
    HideToTray,
    StopBridgesAndExit,
    Cancel,
}

/// Effect of a bridge-impact choice. The actual shutdown still goes through
/// [`ExitCoordinator::request_exit`], never directly through `app.exit`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExitImpactAction {
    HideToTray,
    RequestCoordinatedExit,
    None,
}

/// Final process action after the shared listener drain. Update installation
/// uses `Restart`; ordinary tray/window quit uses `Exit`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CoordinatedShutdownAction {
    Exit,
    Restart,
}

pub(crate) fn exit_impact_action(choice: ExitImpactChoice) -> ExitImpactAction {
    match choice {
        ExitImpactChoice::HideToTray => ExitImpactAction::HideToTray,
        ExitImpactChoice::StopBridgesAndExit => ExitImpactAction::RequestCoordinatedExit,
        ExitImpactChoice::Cancel => ExitImpactAction::None,
    }
}

/// Serializes GUI shutdown.  It is deliberately independent of Tauri so the
/// atomic lifecycle contract can be tested without a live application.
pub(crate) struct ExitCoordinator {
    shutdown_started: AtomicBool,
    /// Set only after the asynchronous listener drain finishes, immediately
    /// before this coordinator asks Tauri to terminate the process.
    exit_ready: Arc<AtomicBool>,
    lifecycle_barrier: Arc<LifecycleShutdownBarrier>,
}

impl ExitCoordinator {
    pub(crate) fn new() -> Self {
        Self {
            shutdown_started: AtomicBool::new(false),
            exit_ready: Arc::new(AtomicBool::new(false)),
            lifecycle_barrier: Arc::new(LifecycleShutdownBarrier::new()),
        }
    }

    /// Inspect the active bridge count before an exit prompt is shown.
    ///
    /// A poisoned host registry must not block shutdown, so it is represented
    /// as `None` rather than silently reported as zero listeners.
    pub(crate) fn prepare_exit(&self, host: &BridgeRuntimeHost) -> ExitPreparation {
        ExitPreparation {
            active_bridge_count: host.statuses().ok().map(|statuses| statuses.len()),
        }
    }

    /// A failed status read is treated conservatively: do not silently turn an
    /// uncertain local listener into an immediate exit.
    pub(crate) fn requires_impact_confirmation(preparation: ExitPreparation) -> bool {
        !matches!(preparation.active_bridge_count, Some(0))
    }

    /// Atomically claims the shutdown flow.  Once a caller succeeds, all
    /// subsequent exit requests are ignored until process termination.
    pub(crate) fn begin_shutdown(&self, host: &BridgeRuntimeHost) -> ExitBegin {
        if self
            .shutdown_started
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return ExitBegin::AlreadyInProgress;
        }

        // Close new bridge lifecycle entries before the asynchronous drain is
        // scheduled, so a command cannot slip in between confirmation and
        // host shutdown.
        self.lifecycle_barrier.close();

        ExitBegin::Started(self.prepare_exit(host))
    }

    pub(crate) fn lifecycle_barrier(&self) -> Arc<LifecycleShutdownBarrier> {
        Arc::clone(&self.lifecycle_barrier)
    }

    pub(crate) fn shutdown_in_progress(&self) -> bool {
        self.shutdown_started.load(Ordering::SeqCst)
    }

    /// `RunEvent::ExitRequested` may be raised again while the first request
    /// is draining. Only the coordinator's own final `app.exit(0)` is allowed
    /// through; a second user/OS request remains prevented until then.
    pub(crate) fn exit_ready(&self) -> bool {
        self.exit_ready.load(Ordering::SeqCst)
    }

    /// Start an asynchronous drain and exit only after every bridge listener
    /// has been asked to stop. Returns false when another caller already owns
    /// the shutdown sequence.
    pub(crate) fn request_exit<R: Runtime>(
        &self,
        app: AppHandle<R>,
        host: Arc<BridgeRuntimeHost>,
    ) -> bool {
        self.request_shutdown(app, host, CoordinatedShutdownAction::Exit)
    }

    /// Same exclusive bridge drain as a normal exit, but request a Tauri
    /// relaunch after it completes. This keeps post-update restart from
    /// bypassing the bridge impact confirmation and shutdown lifecycle.
    pub(crate) fn request_restart<R: Runtime>(
        &self,
        app: AppHandle<R>,
        host: Arc<BridgeRuntimeHost>,
    ) -> bool {
        self.request_shutdown(app, host, CoordinatedShutdownAction::Restart)
    }

    fn request_shutdown<R: Runtime>(
        &self,
        app: AppHandle<R>,
        host: Arc<BridgeRuntimeHost>,
        action: CoordinatedShutdownAction,
    ) -> bool {
        let ExitBegin::Started(preparation) = self.begin_shutdown(&host) else {
            return false;
        };

        tracing::info!(
            target: "gui",
            op = "exit",
            action = ?action,
            active_bridge_count = ?preparation.active_bridge_count,
            "GUI shutdown requested; draining bridge listeners"
        );
        let exit_ready = Arc::clone(&self.exit_ready);
        let lifecycle_barrier = self.lifecycle_barrier();
        tauri::async_runtime::spawn(async move {
            // Keep this exclusive permit until `host.shutdown` returns. The
            // fixed lock ordering is barrier -> profile -> target/core, so no
            // bridge lifecycle operation can deadlock shutdown.
            //
            // Total watchdog: per-edge drain already has DRAIN_TIMEOUT, but
            // wait_for_sagas / host.shutdown as a whole previously had no
            // ceiling — a stuck saga left the process undead with no feedback.
            const EXIT_DRAIN_WATCHDOG: std::time::Duration = std::time::Duration::from_secs(12);
            let drain = async {
                let _exclusive_permit = lifecycle_barrier.wait_for_sagas().await;
                if let Err(error) = host.shutdown().await {
                    tracing::warn!(
                        target: "gui",
                        op = "exit",
                        error = %error,
                        "bridge shutdown failed while exiting"
                    );
                }
            };
            if tokio::time::timeout(EXIT_DRAIN_WATCHDOG, drain)
                .await
                .is_err()
            {
                tracing::warn!(
                    target: "gui",
                    op = "exit",
                    watchdog_secs = EXIT_DRAIN_WATCHDOG.as_secs(),
                    "bridge drain exceeded watchdog; forcing process exit"
                );
            }
            exit_ready.store(true, Ordering::SeqCst);
            match action {
                CoordinatedShutdownAction::Exit => app.exit(0),
                CoordinatedShutdownAction::Restart => app.request_restart(),
            }
        });
        true
    }

    #[cfg(test)]
    pub(crate) async fn shutdown_empty_host_for_test(
        &self,
        host: &BridgeRuntimeHost,
    ) -> Result<(), BridgeHostError> {
        host.shutdown().await
    }
}

impl Default for ExitCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;
