//! OS file-manager context menu: open a new chat using the selected folder.
//!
//! Windows Explorer, macOS Finder (Open With / Apple Event), and Linux
//! file-manager actions all launch this process with `--open-chat <dir>`.

use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, Runtime};

use crate::state::AppState;
use crate::tray;
use crate::tray_i18n::TrayUiLanguage;

pub(crate) const OPEN_CHAT_FLAG: &str = "--open-chat";
pub(crate) const OPEN_CHAT_CWD_EVENT: &str = "open-chat-cwd";
#[cfg_attr(not(windows), allow(dead_code))]
const MENU_KEY_ID: &str = "AgentHubOpenChat";

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OpenChatCwdPayload {
    pub cwd: String,
}

pub(crate) fn shell_menu_label(lang: TrayUiLanguage) -> &'static str {
    match lang {
        TrayUiLanguage::Zh => "用 AgentHub 打开对话",
        TrayUiLanguage::En => "Open Chat in AgentHub",
    }
}

/// First `--open-chat <path>` or `--open-chat=<path>` wins. Other flags are ignored.
/// The executable name is not required: Windows single-instance delivery may omit it.
pub(crate) fn parse_open_chat_cwd_arg<S: AsRef<str>>(args: &[S]) -> Option<String> {
    let mut iter = args.iter().map(AsRef::as_ref);
    while let Some(arg) = iter.next() {
        if let Some(rest) = arg.strip_prefix("--open-chat=") {
            let path = unquote(rest);
            return if path.is_empty() { None } else { Some(path) };
        }
        if arg == OPEN_CHAT_FLAG {
            let path = unquote(iter.next().unwrap_or(""));
            return if path.is_empty() { None } else { Some(path) };
        }
    }
    None
}

fn unquote(raw: &str) -> String {
    let t = raw.trim();
    if t.len() >= 2 {
        let bytes = t.as_bytes();
        if (bytes[0] == b'"' && bytes[bytes.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[bytes.len() - 1] == b'\'')
        {
            return t[1..t.len() - 1].trim().to_string();
        }
    }
    t.to_string()
}

/// Empty Explorer `%V` / `%1` expands to `\.`, `.`, or a lone slash — not a folder.
fn is_bare_dot_or_slash(s: &str) -> bool {
    s.trim_matches(|c| c == '/' || c == '\\' || c == '.').is_empty()
}

/// Folder to use as the chat working directory. Files resolve to their parent.
pub(crate) fn resolve_open_chat_cwd(raw: &str) -> Option<PathBuf> {
    let trimmed = unquote(raw);
    if trimmed.is_empty() || is_bare_dot_or_slash(&trimmed) {
        return None;
    }
    let mut path = crate::file_manager::normalize_open_path_input(&trimmed);
    // Windows Explorer command uses `"%V\."` so a drive root is `C:\.` not `C:\"`.
    if path.file_name().is_some_and(|name| name == ".") {
        path.pop();
    }
    if path.is_dir() {
        return Some(path);
    }
    if path.is_file() {
        return path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .map(Path::to_path_buf);
    }
    None
}

/// Resolve `--open-chat` from argv. If the path is missing or not a folder,
/// use the launching process's current directory (Explorer often sets that
/// to the right-clicked folder).
pub(crate) fn resolve_open_chat_from_launch<S: AsRef<str>>(
    args: &[S],
    fallback_cwd: Option<&str>,
) -> Option<PathBuf> {
    if let Some(raw) = parse_open_chat_cwd_arg(args) {
        if let Some(cwd) = resolve_open_chat_cwd(&raw) {
            return Some(cwd);
        }
    } else if !open_chat_arg_missing_folder(args) {
        return None;
    }
    fallback_cwd.and_then(resolve_open_chat_cwd)
}

#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn windows_open_chat_command(exe: &Path) -> String {
    windows_open_chat_command_with(exe, r"%V\.")
}

/// `placeholder` is an Explorer verb token such as `%1\.` or `%V\.`.
/// A trailing `.` keeps drive roots from becoming `C:\"` after quoting.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn windows_open_chat_command_with(exe: &Path, placeholder: &str) -> String {
    format!("\"{}\" {OPEN_CHAT_FLAG} \"{placeholder}\"", exe.display())
}

/// Cargo `target/debug` builds. Registering them overwrites the installed
/// Explorer verb, then the menu breaks when that debug exe is gone.
///
/// Split on `/` and `\` so Windows fixtures still match when this crate is
/// tested on Linux CI. `Path::iter` treats a `\` path as one component there.
pub(crate) fn is_cargo_debug_exe(exe: &Path) -> bool {
    let raw = exe.to_string_lossy();
    let parts: Vec<&str> = raw
        .split(['/', '\\'])
        .filter(|part| !part.is_empty())
        .collect();
    parts
        .windows(2)
        .any(|window| window[0] == "target" && window[1] == "debug")
}

pub(crate) fn should_write_shell_registration(exe: &Path, force: bool) -> bool {
    force || !is_cargo_debug_exe(exe)
}

/// Prefer a validated AppImage path; otherwise the running executable.
pub(crate) fn resolve_shell_register_exe(
    current_exe: Option<&Path>,
    appimage: Option<&Path>,
) -> Option<PathBuf> {
    if let Some(path) = appimage.filter(|p| is_usable_appimage(p)) {
        return Some(path.to_path_buf());
    }
    current_exe.map(Path::to_path_buf)
}

fn is_usable_appimage(path: &Path) -> bool {
    path.is_absolute() && path.is_file()
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) fn linux_servicemenu_desktop(exe: &Path, lang: TrayUiLanguage) -> String {
    let exec = format!("\"{}\" {OPEN_CHAT_FLAG} %f", exe.display());
    let name = shell_menu_label(lang);
    format!(
        "[Desktop Entry]\n\
         Type=Service\n\
         ServiceTypes=KonqPopupMenu/Plugin,inode/directory\n\
         MimeType=inode/directory;\n\
         Actions=openChat;\n\
         X-KDE-Priority=TopLevel\n\
         \n\
         [Desktop Action openChat]\n\
         Name={name}\n\
         Name[zh_CN]=用 AgentHub 打开对话\n\
         Name[en]=Open Chat in AgentHub\n\
         Icon=folder\n\
         Exec={exec}\n"
    )
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) fn linux_open_with_desktop(exe: &Path, lang: TrayUiLanguage) -> String {
    let exec = format!("\"{}\" {OPEN_CHAT_FLAG} %f", exe.display());
    let name = shell_menu_label(lang);
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name={name}\n\
         Name[zh_CN]=用 AgentHub 打开对话\n\
         Name[en]=Open Chat in AgentHub\n\
         NoDisplay=true\n\
         StartupNotify=false\n\
         MimeType=inode/directory;\n\
         Exec={exec}\n"
    )
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) fn linux_nautilus_script(exe: &Path) -> String {
    format!(
        "#!/bin/sh\n\
         exe=\"{}\"\n\
         if [ -n \"$NAUTILUS_SCRIPT_SELECTED_FILE_PATHS\" ]; then\n\
           printf '%s' \"$NAUTILUS_SCRIPT_SELECTED_FILE_PATHS\" | while IFS= read -r p; do\n\
             [ -n \"$p\" ] || continue\n\
             exec \"$exe\" {OPEN_CHAT_FLAG} \"$p\"\n\
           done\n\
         fi\n\
         [ -n \"$1\" ] && exec \"$exe\" {OPEN_CHAT_FLAG} \"$1\"\n",
        exe.display()
    )
}

/// Store `cwd` as the single pending handoff, then wake the UI so it can take
/// that pending. The event is not a second delivery path.
pub(crate) fn deliver_open_chat_cwd<R: Runtime>(app: &AppHandle<R>, cwd: PathBuf) {
    let cwd = cwd.to_string_lossy().into_owned();
    let Some(state) = app.try_state::<AppState>() else {
        tracing::warn!(
            target: "gui",
            op = "open_chat_cwd",
            "no app state; cannot queue folder"
        );
        return;
    };
    state.set_pending_open_chat_cwd(cwd.clone());
    tracing::info!(target: "gui", op = "open_chat_cwd", "queued folder for new chat");
    tray::show_main_window(app);
    emit_open_chat_wakeup(app, &cwd);
    schedule_open_chat_rewake(app);
}

/// Hidden windows often drop the first event; emit again if pending is still there.
pub(crate) fn rewake_pending_open_chat<R: Runtime>(app: &AppHandle<R>) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let Some(cwd) = state.peek_pending_open_chat_cwd() else {
        return;
    };
    tray::show_main_window(app);
    emit_open_chat_wakeup(app, &cwd);
}

fn emit_open_chat_wakeup<R: Runtime>(app: &AppHandle<R>, cwd: &str) {
    let _ = app.emit(
        OPEN_CHAT_CWD_EVENT,
        OpenChatCwdPayload {
            cwd: cwd.to_string(),
        },
    );
}

fn schedule_open_chat_rewake<R: Runtime>(app: &AppHandle<R>) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        rewake_pending_open_chat(&app);
    });
}

fn open_chat_arg_missing_folder<S: AsRef<str>>(args: &[S]) -> bool {
    parse_open_chat_cwd_arg(args).is_none()
        && args.iter().any(|arg| arg.as_ref().contains("open-chat"))
}

pub(crate) fn ingest_args<R: Runtime>(
    app: &AppHandle<R>,
    args: &[String],
    fallback_cwd: Option<&str>,
) {
    if let Some(raw) = parse_open_chat_cwd_arg(args) {
        if let Some(cwd) = resolve_open_chat_cwd(&raw) {
            deliver_open_chat_cwd(app, cwd);
            return;
        }
        if let Some(cwd) = fallback_cwd.and_then(resolve_open_chat_cwd) {
            tracing::warn!(
                target: "gui",
                op = "open_chat_cwd",
                path = %raw,
                "open-chat path unusable; using launch folder"
            );
            deliver_open_chat_cwd(app, cwd);
            return;
        }
        tracing::warn!(
            target: "gui",
            op = "open_chat_cwd",
            path = %raw,
            "ignored --open-chat path that is not a folder"
        );
        return;
    }
    if open_chat_arg_missing_folder(args) {
        if let Some(cwd) = fallback_cwd.and_then(resolve_open_chat_cwd) {
            deliver_open_chat_cwd(app, cwd);
            return;
        }
        tracing::warn!(
            target: "gui",
            op = "open_chat_cwd",
            "second instance had --open-chat but no folder"
        );
    }
}

/// Explorer launches a second process; the plugin delivers argv on a hidden
/// window thread. Show + emit must run on the GUI thread or a tray-hidden
/// window stays hidden and drops the event.
pub(crate) fn ingest_second_instance<R: Runtime>(
    app: &AppHandle<R>,
    args: Vec<String>,
    launch_cwd: String,
) {
    let handle = app.clone();
    let args_for_main = args.clone();
    let cwd_for_main = launch_cwd.clone();
    if app
        .run_on_main_thread(move || {
            ingest_args(&handle, &args_for_main, Some(cwd_for_main.as_str()));
            tray::show_main_window(&handle);
        })
        .is_err()
    {
        ingest_args(app, &args, Some(launch_cwd.as_str()));
        tray::show_main_window(app);
    }
}

pub(crate) fn register_best_effort(lang: TrayUiLanguage) {
    let current = std::env::current_exe().ok();
    let appimage = std::env::var_os("APPIMAGE").map(PathBuf::from);
    let Some(exe) = resolve_shell_register_exe(current.as_deref(), appimage.as_deref()) else {
        return;
    };
    let force = matches!(
        std::env::var("AGENTHUB_REGISTER_SHELL").as_deref(),
        Ok("1")
    );
    if !should_write_shell_registration(&exe, force) {
        tracing::info!(
            target: "gui",
            op = "shell_open_chat",
            "skip file-manager menu registration for cargo debug build"
        );
        return;
    }
    if let Err(e) = register_shell_open_chat(&exe, lang) {
        tracing::warn!(target: "gui", op = "shell_open_chat", error = %e, "register file-manager menu failed");
    }
}

pub(crate) fn register_shell_open_chat(exe: &Path, lang: TrayUiLanguage) -> Result<(), String> {
    #[cfg(windows)]
    {
        register_windows(exe, lang)?;
    }
    #[cfg(target_os = "linux")]
    {
        register_linux(exe, lang)?;
    }
    #[cfg(target_os = "macos")]
    {
        let _ = (exe, lang);
    }
    #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
    {
        let _ = (exe, lang);
    }
    Ok(())
}

#[cfg(windows)]
fn register_windows(exe: &Path, lang: TrayUiLanguage) -> Result<(), String> {
    let label = shell_menu_label(lang);
    let icon = format!("{},0", exe.display());
    // Directory: `%1` is the selected folder (more reliable than `%V` in
    // search / Quick Access). Background and Drive keep `%V`.
    let entries = [
        (
            format!(r"HKCU\Software\Classes\Directory\shell\{MENU_KEY_ID}"),
            windows_open_chat_command_with(exe, r"%1\."),
        ),
        (
            format!(r"HKCU\Software\Classes\Directory\Background\shell\{MENU_KEY_ID}"),
            windows_open_chat_command(exe),
        ),
        (
            format!(r"HKCU\Software\Classes\Drive\shell\{MENU_KEY_ID}"),
            windows_open_chat_command(exe),
        ),
    ];
    for (root, command) in &entries {
        reg_add(root, None, label)?;
        reg_add(root, Some("Icon"), &icon)?;
        let command_key = format!(r"{root}\command");
        reg_add(&command_key, None, command)?;
    }
    Ok(())
}

#[cfg(windows)]
fn reg_add(key: &str, name: Option<&str>, data: &str) -> Result<(), String> {
    use agenthub_core::utils::process::apply_no_window;
    let mut cmd = std::process::Command::new("reg");
    cmd.arg("add").arg(key).arg("/f").arg("/t").arg("REG_SZ");
    match name {
        Some(n) => {
            cmd.arg("/v").arg(n);
        }
        None => {
            cmd.arg("/ve");
        }
    }
    cmd.arg("/d").arg(data);
    apply_no_window(&mut cmd);
    let output = cmd.output().map_err(|e| format!("reg add failed: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("reg add {key} failed: {stderr}"));
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn register_linux(exe: &Path, lang: TrayUiLanguage) -> Result<(), String> {
    let home = std::env::var_os("HOME").ok_or_else(|| "HOME is unset".to_string())?;
    let home = PathBuf::from(home);
    write_text(
        &home
            .join(".local/share/kio/servicemenus")
            .join("agenthub-open-chat.desktop"),
        &linux_servicemenu_desktop(exe, lang),
    )?;
    write_text(
        &home
            .join(".local/share/file-manager/actions")
            .join("agenthub-open-chat.desktop"),
        &linux_servicemenu_desktop(exe, lang),
    )?;
    write_text(
        &home
            .join(".local/share/applications")
            .join("agenthub-open-chat.desktop"),
        &linux_open_with_desktop(exe, lang),
    )?;
    let nautilus_dir = home.join(".local/share/nautilus/scripts");
    for stale in [
        "用 AgentHub 打开对话",
        "Open Chat in AgentHub",
        "AgentHubOpenChat",
    ] {
        let path = nautilus_dir.join(stale);
        let _ = std::fs::remove_file(path);
    }
    let script_path = nautilus_dir.join(shell_menu_label(lang));
    write_text(&script_path, &linux_nautilus_script(exe))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(&script_path) {
            let mut perm = meta.permissions();
            perm.set_mode(0o755);
            let _ = std::fs::set_permissions(&script_path, perm);
        }
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn write_text(path: &Path, contents: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create {}: {e}", parent.display()))?;
    }
    std::fs::write(path, contents).map_err(|e| format!("write {}: {e}", path.display()))
}

#[tauri::command]
pub async fn take_pending_open_chat_cwd(
    state: tauri::State<'_, AppState>,
) -> Result<Option<String>, String> {
    Ok(state.take_pending_open_chat_cwd())
}

#[cfg(test)]
mod tests;
