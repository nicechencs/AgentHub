//! Pure window / tray policy helpers (unit-testable without a live Tauri runtime).

/// How the main window should react to a close request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseAction {
    /// Destroy the window / exit the process.
    AllowExit,
    /// Keep the process; hide the window to the system tray.
    HideToTray,
}

/// Decide close behavior from in-process flags.
///
/// - Explicit quit (tray "退出") always exits.
/// - Otherwise hide when `close_to_tray` is on, or a local bridge is running.
pub fn decide_close_action(
    exit_requested: bool,
    close_to_tray: bool,
    bridge_active: bool,
) -> CloseAction {
    if exit_requested {
        CloseAction::AllowExit
    } else if close_to_tray || bridge_active {
        CloseAction::HideToTray
    } else {
        CloseAction::AllowExit
    }
}

/// Parse loose bool strings used by settings / local prefs.
/// Unknown / empty values default to `true` (safe default: prefer hide-to-tray).
pub fn parse_bool_setting(raw: &str) -> bool {
    match raw.trim().to_ascii_lowercase().as_str() {
        "0" | "false" | "no" | "off" => false,
        _ => true,
    }
}

/// Values accepted when writing `close_to_tray` via `set_setting` (after core normalization).
pub fn is_close_to_tray_enabled(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

/// macOS Dock / app-icon reopen should always surface the main window.
///
/// After hide-to-tray the process is still running with a hidden window, so the
/// system often reports `has_visible_windows = false`. Even when a window is
/// already visible (minimized / behind others), focusing it is the expected UX.
// Call site is behind `#[cfg(target_os = "macos")]` in lib.rs.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn should_show_on_reopen(_has_visible_windows: bool) -> bool {
    true
}

#[cfg(test)]
mod tests;
