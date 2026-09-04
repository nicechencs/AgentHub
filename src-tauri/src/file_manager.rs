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

fn looks_like_unix_shebang(path: &std::path::Path) -> bool {
    use std::io::Read;
    let mut buf = [0u8; 2];
    std::fs::File::open(path)
        .and_then(|mut f| f.read_exact(&mut buf))
        .map(|_| buf == *b"#!")
        .unwrap_or(false)
}

fn windows_spawnable_cli_ext(path: &std::path::Path) -> bool {
    matches!(
        path.extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .as_deref(),
        Some("cmd" | "bat" | "exe" | "com")
    )
}

/// Prefer `claude.cmd` / `claude.exe` when detect stored an extensionless npm shim.
///
/// On Windows the extensionless npm file is a `#!/bin/sh` stub. CreateProcess
/// cannot run it; PowerShell also should not be pointed at it when a `.cmd`
/// sibling exists.
pub(crate) fn resolve_cli_launch_path(path: &std::path::Path) -> std::path::PathBuf {
    let skip_shebang = cfg!(windows)
        && path.extension().is_none()
        && path.is_file()
        && !windows_spawnable_cli_ext(path)
        && looks_like_unix_shebang(path);
    if path.is_file() && !skip_shebang {
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

/// Walk up to `Foo.app` so macOS `open` launches the bundle, not Contents/MacOS.
#[cfg_attr(not(any(test, target_os = "macos")), allow(dead_code))]
pub(crate) fn enclosing_app_bundle(path: &std::path::Path) -> Option<&std::path::Path> {
    path.ancestors().find(|p| {
        p.extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("app"))
    })
}

/// AppleScript that runs `path` in Terminal, shell-quoted via `quoted form of`.
#[cfg_attr(not(any(test, target_os = "macos")), allow(dead_code))]
pub(crate) fn applescript_terminal_do_script(path: &std::path::Path) -> String {
    let escaped = path
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    format!("tell application \"Terminal\" to do script (quoted form of \"{escaped}\")")
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
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        let invoke = powershell_invoke_command(&path);
        if std::process::Command::new("wt")
            .args(["powershell", "-NoLogo", "-NoExit", "-Command", &invoke])
            .creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP)
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
        let script = applescript_terminal_do_script(&path);
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

/// Bundled Codex CLI inside the desktop/Store install — not the window.
///
/// Path strings may use Windows `\\` separators even when unit tests run on
/// Unix (CI). Normalize before taking the basename so `Path::file_name`
/// does not treat the whole Windows path as a single component on Linux.
pub(crate) fn looks_like_codex_bundled_cli(path: &std::path::Path) -> bool {
    let s = path
        .to_string_lossy()
        .replace('\\', "/")
        .to_ascii_lowercase();
    let name = s.rsplit('/').next().unwrap_or("");
    if name != "codex.exe" && name != "codex" {
        return false;
    }
    let in_desktop_bin = s.contains("/openai/") && s.contains("/codex/") && s.contains("/bin/");
    in_desktop_bin
        || s.contains("/resources/")
        || s.contains("/node_modules/")
        || s.contains("/.vscode/")
        || s.contains("/windowsapps/")
        || s.contains("codex.app/")
        || s.contains("chatgpt.app/")
}

/// How "启动 App" should treat a Codex CLI path on this OS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CodexAppLaunchKind {
    /// WorkBuddy.exe / ZCode.exe / a real GUI binary.
    Direct,
    /// Windows Store ChatGPT/Codex window (`shell:AppsFolder\...`).
    WindowsStoreOrGui,
    /// macOS `open ChatGPT.app` / `Codex.app`.
    MacosBundle,
    /// No official Codex window on Linux.
    UnsupportedOnLinux,
}

pub(crate) fn codex_app_launch_kind(path: &std::path::Path) -> CodexAppLaunchKind {
    if !looks_like_codex_bundled_cli(path) {
        return CodexAppLaunchKind::Direct;
    }
    if cfg!(windows) {
        CodexAppLaunchKind::WindowsStoreOrGui
    } else if cfg!(target_os = "macos") {
        CodexAppLaunchKind::MacosBundle
    } else if cfg!(target_os = "linux") {
        CodexAppLaunchKind::UnsupportedOnLinux
    } else {
        CodexAppLaunchKind::Direct
    }
}

pub(crate) fn macos_codex_app_bundle_names() -> &'static [&'static str] {
    &[
        "ChatGPT.app",
        "Codex.app",
        "OpenAI Codex.app",
        "OpenAI.Codex.app",
    ]
}

/// `OpenAI.Codex_26.831.1445.0_x64__2p2nqsd0c76g0` → `OpenAI.Codex_2p2nqsd0c76g0!App`.
#[cfg_attr(not(any(test, windows)), allow(dead_code))]
pub(crate) fn windows_codex_app_id_from_package_full_name(
    package_full_name: &str,
) -> Option<String> {
    let package_name = package_full_name.split('_').next()?.trim();
    if !matches!(
        package_name,
        "OpenAI.Codex" | "OpenAI.CodexBeta" | "OpenAI.ChatGPT"
    ) {
        return None;
    }
    let publisher_id = package_full_name.rsplit('_').next()?.trim();
    if publisher_id.is_empty() || publisher_id == package_name {
        return None;
    }
    Some(format!("{package_name}_{publisher_id}!App"))
}

#[cfg_attr(not(any(test, windows)), allow(dead_code))]
pub(crate) fn parse_windows_codex_app_id_from_registry(output: &str) -> Option<String> {
    const PACKAGE_MARKER: &str = "\\AppModel\\Repository\\Packages\\";
    output.lines().find_map(|line| {
        let line = line.trim();
        let (_, package_full_name) = line.split_once(PACKAGE_MARKER)?;
        if package_full_name.contains('\\') {
            return None;
        }
        windows_codex_app_id_from_package_full_name(package_full_name)
    })
}

#[cfg(windows)]
fn find_windows_codex_store_app_id() -> Option<String> {
    const PACKAGES_KEY: &str = r"HKCU\Software\Classes\Local Settings\Software\Microsoft\Windows\CurrentVersion\AppModel\Repository\Packages";
    for package_name in ["OpenAI.Codex_", "OpenAI.CodexBeta_", "OpenAI.ChatGPT_"] {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let Ok(output) = std::process::Command::new("reg")
            .args(["query", PACKAGES_KEY, "/f", package_name, "/k", "/s"])
            .creation_flags(CREATE_NO_WINDOW)
            .output()
        else {
            continue;
        };
        if !output.status.success() {
            continue;
        }
        if let Some(app_id) =
            parse_windows_codex_app_id_from_registry(&String::from_utf8_lossy(&output.stdout))
        {
            return Some(app_id);
        }
    }
    find_windows_codex_app_id_from_windowsapps()
}

#[cfg(windows)]
fn find_windows_codex_app_id_from_windowsapps() -> Option<String> {
    let root = std::path::Path::new(r"C:\Program Files\WindowsApps");
    let rd = std::fs::read_dir(root).ok()?;
    let mut names: Vec<String> = rd
        .flatten()
        .filter_map(|ent| {
            let name = ent.file_name();
            let name = name.to_string_lossy();
            if name.starts_with("OpenAI.Codex_")
                || name.starts_with("OpenAI.CodexBeta_")
                || name.starts_with("OpenAI.ChatGPT_")
            {
                Some(name.into_owned())
            } else {
                None
            }
        })
        .collect();
    names.sort();
    names
        .into_iter()
        .rev()
        .find_map(|name| windows_codex_app_id_from_package_full_name(&name))
}

#[cfg(windows)]
fn find_windows_codex_gui_exe() -> Option<std::path::PathBuf> {
    let local = std::env::var_os("LOCALAPPDATA").map(std::path::PathBuf::from)?;
    let candidates = [
        local
            .join("Programs")
            .join("OpenAI")
            .join("ChatGPT")
            .join("ChatGPT.exe"),
        local.join("Programs").join("ChatGPT").join("ChatGPT.exe"),
        local.join("OpenAI").join("ChatGPT").join("ChatGPT.exe"),
        local
            .join("Programs")
            .join("OpenAI")
            .join("Codex")
            .join("Codex.exe"),
        local.join("Programs").join("Codex").join("Codex.exe"),
        local.join("OpenAI").join("Codex").join("Codex.exe"),
        local
            .join("Microsoft")
            .join("WindowsApps")
            .join("ChatGPT.exe"),
    ];
    candidates.into_iter().find(|path| path.is_file())
}

#[cfg(windows)]
fn spawn_detached_app(program: &std::path::Path) -> Result<(), String> {
    spawn_detached_command(std::process::Command::new(program))
}

#[cfg(windows)]
fn launch_windows_store_app(app_id: &str) -> Result<(), String> {
    let mut cmd = std::process::Command::new("explorer");
    cmd.arg(format!("shell:AppsFolder\\{app_id}"));
    spawn_detached_command(cmd)
}

#[cfg(windows)]
fn spawn_detached_command(mut cmd: std::process::Command) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    cmd.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP)
        .spawn()
        .map(|_| ())
        .map_err(|e| {
            let msg = format!("launch app failed: {e}");
            tracing::warn!(target: targets::GUI, op = "launch_app", "{msg}");
            msg
        })
}

#[cfg(windows)]
fn launch_codex_desktop_window() -> Result<(), String> {
    if let Some(app_id) = find_windows_codex_store_app_id() {
        tracing::info!(
            target: targets::GUI,
            op = "launch_app",
            via = "store",
            app_id = %app_id,
            "launching Codex app"
        );
        return launch_windows_store_app(&app_id);
    }
    if let Some(exe) = find_windows_codex_gui_exe() {
        tracing::info!(
            target: targets::GUI,
            op = "launch_app",
            via = "exe",
            path = %exe.display(),
            "launching Codex app"
        );
        return spawn_detached_app(&exe);
    }
    let msg = "codex app window not found".to_string();
    tracing::warn!(target: targets::GUI, op = "launch_app", "{msg}");
    Err(msg)
}

#[cfg(target_os = "macos")]
fn map_launch_app_error(err: std::io::Error) -> String {
    let msg = format!("launch app failed: {err}");
    tracing::warn!(target: targets::GUI, op = "launch_app", "{msg}");
    msg
}

#[cfg(target_os = "macos")]
fn open_macos_app(bundle: &std::path::Path) -> Result<(), String> {
    std::process::Command::new("open")
        .arg(bundle)
        .spawn()
        .map(|_| ())
        .map_err(map_launch_app_error)
}

#[cfg(target_os = "macos")]
fn find_macos_codex_app_bundle() -> Option<std::path::PathBuf> {
    let home = agenthub_core::utils::paths::home_dir().ok()?;
    for root in [
        std::path::PathBuf::from("/Applications"),
        home.join("Applications"),
    ] {
        for name in macos_codex_app_bundle_names() {
            let candidate = root.join(name);
            if candidate.is_dir() {
                return Some(candidate);
            }
        }
    }
    None
}

#[cfg(target_os = "macos")]
fn launch_macos_codex_app(path: &std::path::Path) -> Result<(), String> {
    let bundle = enclosing_app_bundle(path)
        .map(std::path::Path::to_path_buf)
        .or_else(find_macos_codex_app_bundle);
    let Some(bundle) = bundle else {
        let msg = "codex app window not found".to_string();
        tracing::warn!(target: targets::GUI, op = "launch_app", "{msg}");
        return Err(msg);
    };
    tracing::info!(
        target: targets::GUI,
        op = "launch_app",
        via = "bundle",
        path = %bundle.display(),
        "launching Codex app"
    );
    open_macos_app(&bundle)
}

/// Start a desktop app without attaching to this process.
///
/// Codex window ≠ bundled `codex` CLI:
/// - Windows: Store app via `shell:AppsFolder\OpenAI.Codex_*!App`
/// - macOS: `open` ChatGPT.app / Codex.app
/// - Linux: no official Codex window
pub(crate) fn launch_app(path: &std::path::Path) -> Result<(), String> {
    match codex_app_launch_kind(path) {
        CodexAppLaunchKind::WindowsStoreOrGui => {
            #[cfg(windows)]
            {
                return launch_codex_desktop_window();
            }
            #[cfg(not(windows))]
            {
                unreachable!("WindowsStoreOrGui only on Windows");
            }
        }
        CodexAppLaunchKind::MacosBundle => {
            #[cfg(target_os = "macos")]
            {
                return launch_macos_codex_app(path);
            }
            #[cfg(not(target_os = "macos"))]
            {
                unreachable!("MacosBundle only on macOS");
            }
        }
        CodexAppLaunchKind::UnsupportedOnLinux => {
            let msg = "Linux 上没有 Codex 窗口，请用启动 CLI".to_string();
            tracing::warn!(target: targets::GUI, op = "launch_app", "{msg}");
            return Err(msg);
        }
        CodexAppLaunchKind::Direct => {}
    }
    #[cfg(windows)]
    {
        spawn_detached_app(path)
    }
    #[cfg(target_os = "macos")]
    {
        if let Some(bundle) = enclosing_app_bundle(path) {
            return open_macos_app(bundle);
        }
        std::process::Command::new(path)
            .spawn()
            .map(|_| ())
            .map_err(map_launch_app_error)
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::process::Command::new(path)
            .spawn()
            .map(|_| ())
            .map_err(|e| {
                let msg = format!("launch app failed: {e}");
                tracing::warn!(target: targets::GUI, op = "launch_app", "{msg}");
                msg
            })
    }
    #[cfg(not(any(windows, unix)))]
    {
        let _ = path;
        Err("launch app unsupported on this platform".into())
    }
}
