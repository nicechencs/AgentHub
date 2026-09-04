//! Live install/upgrade log streaming (GUI progress).
//!
//! Install runs are long (downloads, winget, native scripts). Historically logs
//! were only returned after the full command finished. This module holds a
//! process-wide hook so executors and install steps can emit lines as they
//! appear. Tauri sets the hook to `app.emit("install-progress", …)`; CLI leaves
//! it unset (stdout still comes from the final InstallOutcome).

use std::sync::{Arc, Mutex, OnceLock};

/// Callback invoked for each live install chunk.
///
/// Contract: `command_exec` emits raw UTF-8 prefixes (~8KiB), including empty
/// lines and newline boundaries. The hook must not `trim_end` / drop blank
/// pieces — that would splice adjacent text. Best-effort UI may skip chunks;
/// the process accumulator is authoritative.
pub type InstallLogHook = Arc<dyn Fn(&str) + Send + Sync>;

fn hook_slot() -> &'static Mutex<Option<InstallLogHook>> {
    static SLOT: OnceLock<Mutex<Option<InstallLogHook>>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(None))
}

/// Serialize scoped hook ownership. Without this, parallel tests (and any
/// overlapping install runs) race on the process-wide slot: one scope can
/// overwrite another's hook, and `ClearOnDrop` can clear a newer hook early —
/// which flakes `consecutive_empty_lines_reach_hook_and_accumulator`.
fn hook_run_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

/// Install the hook for the duration of `f`, then clear it (even on panic).
///
/// Only one scoped hook may be active at a time. Clears only the hook this
/// call installed (Arc pointer identity), so a raced clearer cannot wipe a
/// newer scope's callback.
pub fn with_install_log_hook<R>(hook: InstallLogHook, f: impl FnOnce() -> R) -> R {
    let _run = hook_run_lock().lock().unwrap_or_else(|e| e.into_inner());
    {
        let mut g = hook_slot().lock().unwrap_or_else(|e| e.into_inner());
        *g = Some(Arc::clone(&hook));
    }
    struct ClearOnDrop(InstallLogHook);
    impl Drop for ClearOnDrop {
        fn drop(&mut self) {
            if let Ok(mut g) = hook_slot().lock() {
                let still_ours = g
                    .as_ref()
                    .is_some_and(|current| Arc::ptr_eq(current, &self.0));
                if still_ours {
                    *g = None;
                }
            }
        }
    }
    let _guard = ClearOnDrop(hook);
    f()
}

/// Emit a live progress chunk when a hook is registered. No-op otherwise.
/// Passes `chunk` through unchanged (no trim).
pub fn emit_install_log(chunk: &str) {
    let hook = {
        let g = match hook_slot().lock() {
            Ok(g) => g,
            Err(e) => e.into_inner(),
        };
        g.clone()
    };
    if let Some(h) = hook {
        h(chunk);
    }
}

#[cfg(test)]
#[path = "install_progress_tests.rs"]
mod install_progress_tests;
