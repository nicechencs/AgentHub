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

/// Prefer `claude.cmd` / `claude.exe` when detect stored an extensionless npm shim.
pub(crate) fn resolve_cli_launch_path(path: &std::path::Path) -> std::path::PathBuf {
    if path.is_file() {
        return path.to_path_buf();
    }
    if path.extension().is_none() {
        for ext in ["cmd", "exe", "bat"] {
            let candidate = path.with_extension(ext);
            if candidate.is_file() {
                return candidate;
            }
        }
    }
    path.to_path_buf()
}

#[cfg(windows)]
fn powershell_invoke_command(path: &std::path::Path) -> String {
    let escaped = path.to_string_lossy().replace('\'', "''");
    format!("& '{escaped}'")
}

/// Start a CLI in a new terminal window.
pub(crate) fn launch_cli(path: &std::path::Path) -> Result<(), String> {
    let path = resolve_cli_launch_path(path);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;
        let invoke = powershell_invoke_command(&path);
        if std::process::Command::new("wt")
            .args([
                "powershell",
                "-NoLogo",
                "-NoExit",
                "-Command",
                &invoke,
            ])
            .spawn()
            .is_ok()
        {
            return Ok(());
        }
        std::process::Command::new("powershell")
            .args(["-NoLogo", "-NoExit", "-Command", &invoke])
            .creation_flags(CREATE_NEW_CONSOLE)
            .spawn()
            .map_err(|e| {
                let msg = format!("launch cli failed: {e}");
                tracing::warn!(target: targets::GUI, op = "launch_cli", "{msg}");
                msg
            })?;
        Ok(())
    }
    #[cfg(target_os = "macos")]
    {
        let escaped = path
            .display()
            .to_string()
            .replace('\\', "\\\\")
            .replace('"', "\\\"");
        let script = format!("tell application \"Terminal\" to do script \"{escaped}\"");
        std::process::Command::new("osascript")
            .args(["-e", &script])
            .spawn()
            .map_err(|e| {
                let msg = format!("launch cli failed: {e}");
                tracing::warn!(target: targets::GUI, op = "launch_cli", "{msg}");
                msg
            })?;
        Ok(())
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        for (bin, prefix) in [
            ("x-terminal-emulator", ["-e"].as_slice()),
            ("gnome-terminal", ["--"].as_slice()),
            ("konsole", ["-e"].as_slice()),
            ("xterm", ["-e"].as_slice()),
        ] {
            let mut cmd = std::process::Command::new(bin);
            cmd.args(prefix).arg(&path);
            if cmd.spawn().is_ok() {
                return Ok(());
            }
        }
        let msg = "launch cli failed: no terminal".to_string();
        tracing::warn!(target: targets::GUI, op = "launch_cli", "{msg}");
        Err(msg)
    }
    #[cfg(not(any(windows, unix)))]
    {
        let _ = path;
        Err("launch cli unsupported on this platform".into())
    }
}

/// Start a desktop app without attaching to this process.
pub(crate) fn launch_app(path: &std::path::Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        std::process::Command::new(path)
            .creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP)
            .spawn()
            .map_err(|e| {
                let msg = format!("launch app failed: {e}");
                tracing::warn!(target: targets::GUI, op = "launch_app", "{msg}");
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
                let msg = format!("launch app failed: {e}");
                tracing::warn!(target: targets::GUI, op = "launch_app", "{msg}");
                msg
            })?;
        Ok(())
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::process::Command::new(path)
            .spawn()
            .map_err(|e| {
                let msg = format!("launch app failed: {e}");
                tracing::warn!(target: targets::GUI, op = "launch_app", "{msg}");
                msg
            })?;
        Ok(())
    }
    #[cfg(not(any(windows, unix)))]
    {
        let _ = path;
        Err("launch app unsupported on this platform".into())
    }
}
