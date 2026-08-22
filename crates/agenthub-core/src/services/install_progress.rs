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

/// Install the hook for the duration of `f`, then clear it (even on panic).
pub fn with_install_log_hook<R>(hook: InstallLogHook, f: impl FnOnce() -> R) -> R {
    {
        let mut g = hook_slot().lock().unwrap_or_else(|e| e.into_inner());
        *g = Some(hook);
    }
    struct ClearOnDrop;
    impl Drop for ClearOnDrop {
        fn drop(&mut self) {
            if let Ok(mut g) = hook_slot().lock() {
                *g = None;
            }
        }
    }
    let _guard = ClearOnDrop;
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
