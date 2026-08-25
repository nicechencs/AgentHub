//! Extra Codex CLIs besides the spawn target (IDE / desktop).
//!
//! npm / native well-known copies are attached by shared detect.

use std::path::{Path, PathBuf};

use crate::models::DetectResult;

/// Fill IDE / desktop `extra_copies`. Does not change the spawn `binary_path`.
/// Leftover `~/.agenthub/npm` and well-known npm/native copies are attached
/// by shared detect, not here.
pub(crate) fn attach_codex_extra_copies(result: &mut DetectResult) {
    super::detect_binary::attach_extra_binary_copies(
        result,
        codex_extra_copy_candidates(),
        &["--version"],
        &[],
    );
}

pub(crate) fn ide_codex_bins_under(extensions_root: &Path) -> Vec<PathBuf> {
    let Ok(rd) = std::fs::read_dir(extensions_root) else {
        return Vec::new();
    };
    let mut bins = Vec::new();
    for ent in rd.flatten() {
        let name = ent.file_name();
        let name = name.to_string_lossy();
        if !name.to_ascii_lowercase().starts_with("openai.chatgpt-") {
            continue;
        }
        bins.extend(platform_ide_codex_bins(&ent.path().join("bin")));
    }
    bins
}

fn codex_extra_copy_candidates() -> Vec<(PathBuf, &'static str)> {
    let mut out = Vec::new();
    out.extend(ide_codex_bins().into_iter().map(|p| (p, "ide")));
    out.extend(desktop_codex_bins().into_iter().map(|p| (p, "desktop")));
    out
}

fn ide_codex_bins() -> Vec<PathBuf> {
    editor_extension_roots()
        .iter()
        .flat_map(|root| ide_codex_bins_under(root))
        .collect()
}

fn editor_extension_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Ok(home) = crate::utils::paths::home_dir() {
        roots.push(home.join(".vscode").join("extensions"));
        roots.push(home.join(".vscode-insiders").join("extensions"));
        roots.push(home.join(".cursor").join("extensions"));
        roots.push(home.join(".windsurf").join("extensions"));
    }
    roots
}

fn platform_ide_codex_bins(bin_dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    #[cfg(windows)]
    {
        out.push(bin_dir.join("windows-x86_64").join("codex.exe"));
        out.push(bin_dir.join("windows-arm64").join("codex.exe"));
        out.push(bin_dir.join("win32-x64").join("codex.exe"));
    }
    #[cfg(target_os = "macos")]
    {
        out.push(bin_dir.join("darwin-arm64").join("codex"));
        out.push(bin_dir.join("darwin-x86_64").join("codex"));
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        out.push(bin_dir.join("linux-x86_64").join("codex"));
        out.push(bin_dir.join("linux-arm64").join("codex"));
    }
    out
}

fn desktop_codex_bins() -> Vec<PathBuf> {
    let mut out = Vec::new();
    #[cfg(windows)]
    {
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            let local = PathBuf::from(local);
            out.push(
                local
                    .join("Programs")
                    .join("OpenAI")
                    .join("Codex")
                    .join("bin")
                    .join("codex.exe"),
            );
            if let Ok(rd) = std::fs::read_dir(local.join("OpenAI").join("Codex").join("bin")) {
                for ent in rd.flatten() {
                    let p = ent.path();
                    if p.is_dir() {
                        out.push(p.join("codex.exe"));
                    } else if p.file_name().and_then(|n| n.to_str()) == Some("codex.exe") {
                        out.push(p);
                    }
                }
            }
        }
        if let Ok(rd) = std::fs::read_dir(r"C:\Program Files\WindowsApps") {
            for ent in rd.flatten() {
                let name = ent.file_name();
                if name
                    .to_string_lossy()
                    .starts_with("OpenAI.Codex_")
                {
                    out.push(
                        ent.path()
                            .join("app")
                            .join("resources")
                            .join("codex.exe"),
                    );
                }
            }
        }
    }
    #[cfg(target_os = "macos")]
    {
        out.push(PathBuf::from(
            "/Applications/Codex.app/Contents/MacOS/codex",
        ));
        out.push(PathBuf::from(
            "/Applications/Codex.app/Contents/Resources/codex",
        ));
    }
    out
}


