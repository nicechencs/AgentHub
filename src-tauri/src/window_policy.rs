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
pub fn should_show_on_reopen(_has_visible_windows: bool) -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn close_hides_only_when_flag_on_and_not_exiting() {
        assert_eq!(
            decide_close_action(false, true, false),
            CloseAction::HideToTray
        );
        assert_eq!(
            decide_close_action(false, false, false),
            CloseAction::AllowExit
        );
        assert_eq!(
            decide_close_action(true, true, false),
            CloseAction::AllowExit
        );
        assert_eq!(
            decide_close_action(true, false, false),
            CloseAction::AllowExit
        );
    }

    #[test]
    fn close_hides_when_bridge_active_even_if_setting_off() {
        assert_eq!(
            decide_close_action(false, false, true),
            CloseAction::HideToTray
        );
        assert_eq!(
            decide_close_action(true, false, true),
            CloseAction::AllowExit
        );
        assert_eq!(
            decide_close_action(true, true, true),
            CloseAction::AllowExit
        );
    }

    #[test]
    fn dock_reopen_always_surfaces_main_window() {
        assert!(should_show_on_reopen(false));
        assert!(should_show_on_reopen(true));
    }

    #[test]
    fn parse_bool_setting_truth_table() {
        assert!(!parse_bool_setting("false"));
        assert!(!parse_bool_setting("FALSE"));
        assert!(!parse_bool_setting(" 0 "));
        assert!(!parse_bool_setting("no"));
        assert!(!parse_bool_setting("off"));
        assert!(parse_bool_setting("true"));
        assert!(parse_bool_setting("1"));
        assert!(parse_bool_setting("yes"));
        assert!(parse_bool_setting("on"));
        // Prefer hide-to-tray when value is missing/garbage.
        assert!(parse_bool_setting(""));
        assert!(parse_bool_setting("maybe"));
    }

    #[test]
    fn close_to_tray_write_values() {
        assert!(is_close_to_tray_enabled("true"));
        assert!(is_close_to_tray_enabled("TRUE"));
        assert!(is_close_to_tray_enabled("1"));
        assert!(is_close_to_tray_enabled("yes"));
        assert!(is_close_to_tray_enabled("on"));
        assert!(!is_close_to_tray_enabled("false"));
        assert!(!is_close_to_tray_enabled("0"));
        assert!(!is_close_to_tray_enabled("off"));
        assert!(!is_close_to_tray_enabled("no"));
        assert!(!is_close_to_tray_enabled("maybe"));
    }
}
