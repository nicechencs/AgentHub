//! Extra Codex CLIs besides the spawn target (IDE, desktop, leftover data-dir npm).

use std::path::{Path, PathBuf};

use crate::models::{AgentId, DetectedBinaryCopy, DetectResult};
use crate::utils::process::{run_capture_with_env, stdout_first_line};

use super::detect_binary::{
    extract_version_token, is_direct_spawnable, is_under_agenthub_user_npm_prefix,
    looks_like_version_line, well_known_bin_paths,
};

/// Fill `extra_copies` + a human note. Does not change the spawn `binary_path`.
/// Leftover `~/.agenthub/npm` is attached by shared detect, not here.
pub(crate) fn attach_codex_extra_copies(result: &mut DetectResult) {
    let primary = result.binary_path.as_deref();
    let mut copies = Vec::new();
    for (path, kind) in codex_extra_copy_candidates() {
        if !path.is_file() || !is_direct_spawnable(&path) {
            continue;
        }
        if primary.is_some_and(|p| paths_equal(p, &path)) {
            continue;
        }
        if result
            .extra_copies
            .iter()
            .any(|c| paths_equal(&c.path, &path))
        {
            continue;
        }
        if copies.iter().any(|c: &DetectedBinaryCopy| paths_equal(&c.path, &path)) {
            continue;
        }
        let version = probe_version(&path);
        let channel = match kind {
            "ide" | "desktop" => None,
            other => Some(other.to_string()),
        };
        copies.push(DetectedBinaryCopy {
            path,
            kind: kind.into(),
            version,
            channel,
        });
    }
    if copies.is_empty() {
        return;
    }
    let summary = copies
        .iter()
        .map(|c| {
            format!(
                "{} {} @ {}",
                c.kind,
                c.version.as_deref().unwrap_or("?"),
                c.path.display()
            )
        })
        .collect::<Vec<_>>()
        .join("；");
    result
        .notes
        .push(format!("另有 {} 份 Codex：{summary}", copies.len()));
    result.extra_copies.extend(copies);
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
    for (path, channel) in well_known_bin_paths(AgentId::Codex) {
        if is_under_agenthub_user_npm_prefix(&path) {
            continue;
        }
        out.push((path, channel));
    }
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

fn probe_version(path: &Path) -> Option<String> {
    let output = run_capture_with_env(path, &["--version"], &[]).ok()?;
    stdout_first_line(&output)
        .filter(|l| looks_like_version_line(l))
        .map(|l| extract_version_token(&l))
        .filter(|l| !l.is_empty())
}

fn paths_equal(a: &Path, b: &Path) -> bool {
    crate::utils::paths::same_path_identity(a, b).unwrap_or_else(|_| {
        a.to_string_lossy().eq_ignore_ascii_case(&b.to_string_lossy())
    })
}
