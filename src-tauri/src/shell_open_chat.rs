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
pub(crate) fn parse_open_chat_cwd_arg<S: AsRef<str>>(args: &[S]) -> Option<String> {
    let mut iter = args.iter().map(AsRef::as_ref).skip(1);
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

/// Folder to use as the chat working directory. Files resolve to their parent.
pub(crate) fn resolve_open_chat_cwd(raw: &str) -> Option<PathBuf> {
    let trimmed = unquote(raw);
    if trimmed.is_empty() {
        return None;
    }
    let path = crate::file_manager::normalize_open_path_input(&trimmed);
    if path.is_dir() {
        return Some(path);
    }
    if path.is_file() {
        return path.parent().filter(|p| !p.as_os_str().is_empty()).map(Path::to_path_buf);
    }
    None
}

pub(crate) fn windows_open_chat_command(exe: &Path) -> String {
    format!("\"{}\" {OPEN_CHAT_FLAG} \"%V\"", exe.display())
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

pub(crate) fn deliver_open_chat_cwd<R: Runtime>(app: &AppHandle<R>, cwd: PathBuf) {
    let cwd = cwd.to_string_lossy().into_owned();
    if let Some(state) = app.try_state::<AppState>() {
        state.set_pending_open_chat_cwd(cwd.clone());
    }
    tray::show_main_window(app);
    let _ = app.emit(OPEN_CHAT_CWD_EVENT, OpenChatCwdPayload { cwd });
}

pub(crate) fn ingest_args<R: Runtime>(app: &AppHandle<R>, args: &[String]) {
    let Some(raw) = parse_open_chat_cwd_arg(args) else {
        return;
    };
    let Some(cwd) = resolve_open_chat_cwd(&raw) else {
        tracing::warn!(
            target: "gui",
            op = "open_chat_cwd",
            "ignored --open-chat path that is not a folder"
        );
        return;
    };
    deliver_open_chat_cwd(app, cwd);
}

pub(crate) fn register_best_effort(lang: TrayUiLanguage) {
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
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
    let command = windows_open_chat_command(exe);
    let icon = format!("{},0", exe.display());
    let roots = [
        format!(r"HKCU\Software\Classes\Directory\shell\{MENU_KEY_ID}"),
        format!(r"HKCU\Software\Classes\Directory\Background\shell\{MENU_KEY_ID}"),
        format!(r"HKCU\Software\Classes\Drive\shell\{MENU_KEY_ID}"),
    ];
    for root in &roots {
        reg_add(root, None, label)?;
        reg_add(root, Some("Icon"), &icon)?;
        let command_key = format!(r"{root}\command");
        reg_add(&command_key, None, &command)?;
    }
    Ok(())
}

#[cfg(windows)]
fn reg_add(key: &str, name: Option<&str>, data: &str) -> Result<(), String> {
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
    let output = cmd
        .output()
        .map_err(|e| format!("reg add failed: {e}"))?;
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
    for stale in ["用 AgentHub 打开对话", "Open Chat in AgentHub", "AgentHubOpenChat"] {
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
pub async fn take_pending_open_chat_cwd(state: tauri::State<'_, AppState>) -> Result<Option<String>, String> {
    Ok(state.take_pending_open_chat_cwd())
}

#[cfg(test)]
mod tests;
