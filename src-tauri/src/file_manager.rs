//! Open a directory or reveal a file in the OS file manager.
//! URLs stay on `agenthub_core::oauth::open_in_browser`.

use agenthub_core::logging::targets;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FileManagerAction {
    OpenDir(std::path::PathBuf),
    RevealFile(std::path::PathBuf),
}

pub(crate) fn file_manager_action(path: &std::path::Path) -> FileManagerAction {
    if path.is_file() {
        FileManagerAction::RevealFile(path.to_path_buf())
    } else {
        FileManagerAction::OpenDir(path.to_path_buf())
    }
}

/// Explorer switch fragment: `/select,"<path>"`.
///
/// Explorer does not use `CommandLineToArgvW`. It wants the `/select,` switch
/// and a quoted path as one raw command-line token. `Command::arg` wraps that
/// whole token when the path has spaces (`C:\Users\Nice Chen\…`), so Explorer
/// never sees a `/select` switch and opens the user folder instead.
#[cfg(any(test, windows))]
pub(crate) fn explorer_select_arg(path: &std::path::Path) -> String {
    let p = path.to_string_lossy().replace('/', "\\");
    format!("/select,\"{p}\"")
}

/// Strip storage-key prefixes, expand `~`, and normalize separators for the current OS.
pub(crate) fn normalize_open_path_input(raw: &str) -> std::path::PathBuf {
    let mut s = raw.trim().to_string();
    if let Some(rest) = s.strip_prefix("cwd/") {
        s = rest.to_string();
    }
    if s == "~" || s.starts_with("~/") || s.starts_with("~\\") {
        if let Ok(expanded) = agenthub_core::utils::paths::expand_user_path(&s) {
            s = expanded.to_string_lossy().into_owned();
        }
    }
    #[cfg(windows)]
    {
        s = s.replace('/', "\\");
    }
    std::path::PathBuf::from(s)
}

pub(crate) fn reveal_in_file_manager(path: &std::path::Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        std::process::Command::new("explorer")
            .raw_arg(explorer_select_arg(path))
            .spawn()
            .map_err(|e| {
                let msg = format!("open explorer failed: {e}");
                tracing::warn!(target: targets::GUI, op = "reveal_in_file_manager", "{msg}");
                msg
            })?;
        Ok(())
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg("-R")
            .arg(path)
            .spawn()
            .map_err(|e| {
                let msg = format!("open -R failed: {e}");
                tracing::warn!(target: targets::GUI, op = "reveal_in_file_manager", "{msg}");
                msg
            })?;
        Ok(())
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let parent = path.parent().unwrap_or(path);
        open_in_file_manager(parent)
    }
    #[cfg(not(any(windows, unix)))]
    {
        let _ = path;
        let msg = "reveal in file manager unsupported on this platform".to_string();
        tracing::warn!(target: targets::GUI, op = "reveal_in_file_manager", "{msg}");
        Err(msg)
    }
}

pub(crate) fn open_in_file_manager(path: &std::path::Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        let arg = path.to_string_lossy().replace('/', "\\");
        std::process::Command::new("explorer")
            .arg(arg)
            .spawn()
            .map_err(|e| {
                let msg = format!("open explorer failed: {e}");
                tracing::warn!(target: targets::GUI, op = "open_in_file_manager", "{msg}");
                msg
            })?;
        Ok(())
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(path)
            .spawn()
            .map_err(|e| {
                let msg = format!("open failed: {e}");
                tracing::warn!(target: targets::GUI, op = "open_in_file_manager", "{msg}");
                msg
            })?;
        Ok(())
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::process::Command::new("xdg-open")
            .arg(path)
            .spawn()
            .map_err(|e| {
                let msg = format!("xdg-open failed: {e}");
                tracing::warn!(target: targets::GUI, op = "open_in_file_manager", "{msg}");
                msg
            })?;
        Ok(())
    }
    #[cfg(not(any(windows, unix)))]
    {
        let _ = path;
        let msg = "open file manager unsupported on this platform".to_string();
        tracing::warn!(target: targets::GUI, op = "open_in_file_manager", "{msg}");
        Err(msg)
    }
}
