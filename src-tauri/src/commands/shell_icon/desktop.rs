//! Windows desktop / Start-menu shortcut icon retint.

use std::path::{Path, PathBuf};

use super::shortcuts::shortcut_update_script;

pub(crate) fn publish_shortcut_icon(ico_path: &Path) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    retarget_shortcuts(&exe, ico_path)?;
    notify_shell(ico_path);
    Ok(())
}

fn retarget_shortcuts(exe: &Path, ico: &Path) -> Result<(), String> {
    let script = shortcut_update_script(exe, ico);
    let status = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-STA",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            &script,
        ])
        .status()
        .map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err("shortcut icon update failed".into())
    }
}

fn notify_shell(ico: &Path) {
    sh_change_notify(0x0800_0000, 0x1000, None);
    sh_change_notify(0x0000_2000, 0x0005, Some(ico));
    if let Some(desktop) = user_desktop_dir() {
        sh_change_notify(0x0000_2000, 0x0005, Some(&desktop));
    }
}

fn user_desktop_dir() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE").map(|home| PathBuf::from(home).join("Desktop"))
}

fn sh_change_notify(event: i32, flags: u32, path: Option<&Path>) {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "shell32")]
    extern "system" {
        fn SHChangeNotify(
            w_event_id: i32,
            u_flags: u32,
            dw_item1: *const u16,
            dw_item2: *const u16,
        );
    }
    let mut wide: Vec<u16> = Vec::new();
    let ptr = match path {
        Some(p) => {
            wide.extend(p.as_os_str().encode_wide());
            wide.push(0);
            wide.as_ptr()
        }
        None => std::ptr::null(),
    };
    unsafe {
        SHChangeNotify(event, flags, ptr, std::ptr::null());
    }
}
