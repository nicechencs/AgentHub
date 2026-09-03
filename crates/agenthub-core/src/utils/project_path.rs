//! Decode Claude / WorkBuddy / Cursor project directory encodings.

use std::fs;
use std::path::{Path, PathBuf};

/// Claude / WorkBuddy encode `C:\Users\foo` as `-C-Users-foo` (drive + separators → `-`).
pub fn decode_claude_project_dir(encoded: &str) -> Option<String> {
    if encoded.is_empty() {
        return None;
    }
    // Windows absolute: -C-Users-...
    if let Some(rest) = encoded.strip_prefix('-') {
        let mut parts = rest.split('-').filter(|s| !s.is_empty());
        if let Some(drive) = parts.next() {
            if drive.len() == 1 && drive.chars().next()?.is_ascii_alphabetic() {
                let mut path = format!("{}:", drive.to_ascii_uppercase());
                for p in parts {
                    path.push('\\');
                    path.push_str(p);
                }
                return Some(path);
            }
        }
    }
    // Unix-style: -Users-foo-bar → /Users/foo/bar
    if encoded.starts_with('-') {
        let joined = encoded
            .trim_start_matches('-')
            .split('-')
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("/");
        if !joined.is_empty() {
            return Some(format!("/{joined}"));
        }
    }
    Some(encoded.replace('-', "/"))
}

/// Prefer decoded path only when it exists on disk.
pub fn verified_actual_path(encoded: &str) -> Option<String> {
    let candidate = decode_claude_project_dir(encoded)?;
    if Path::new(&candidate).exists() {
        Some(candidate)
    } else {
        None
    }
}

/// Best-effort decode of Cursor project folder names.
///
/// Cursor replaces both path separators and `_` with `-`, so this split is
/// lossy (`d-demo-chen-2026-AgentHub` → `D:\\demo\\chen\\2026\\AgentHub`).
/// Prefer [`cursor_actual_path`], which walks the disk when the naive path is missing.
///
/// Examples (lossy):
/// - `d-demo-workspace-2026-AgentHub` → `D:\\demo\\workspace\\2026\\AgentHub`
/// - `empty-window` / pure digits → None
pub fn decode_cursor_project_dir(name: &str) -> Option<String> {
    let parts = cursor_encoded_parts(name)?;
    // Windows drive: single alphabetic first segment.
    if parts[0].len() == 1 && parts[0].chars().next()?.is_ascii_alphabetic() {
        let drive = parts[0].to_ascii_uppercase();
        let rest = parts[1..].join("\\");
        if rest.is_empty() {
            return Some(format!("{drive}:\\"));
        }
        return Some(format!("{drive}:\\{rest}"));
    }
    // Unix-style absolute-ish
    if parts[0].eq_ignore_ascii_case("users")
        || parts[0].eq_ignore_ascii_case("home")
        || parts[0].eq_ignore_ascii_case("var")
        || parts[0].eq_ignore_ascii_case("tmp")
    {
        return Some(format!("/{}", parts.join("/")));
    }
    None
}

/// Cursor: existing naive decode, then a disk walk when `_`/`-` were collapsed;
/// otherwise still return the lossy candidate for display.
pub fn cursor_actual_path(name: &str) -> Option<String> {
    let candidate = decode_cursor_project_dir(name)?;
    if Path::new(&candidate).exists() {
        return Some(candidate);
    }
    if let Some(recovered) = recover_cursor_path(name) {
        return Some(recovered);
    }
    Some(candidate)
}

fn cursor_encoded_parts(name: &str) -> Option<Vec<&str>> {
    if name.is_empty() || name == "empty-window" {
        return None;
    }
    if name.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let parts: Vec<&str> = name.split('-').filter(|s| !s.is_empty()).collect();
    if parts.is_empty() {
        None
    } else {
        Some(parts)
    }
}

fn recover_cursor_path(name: &str) -> Option<String> {
    let parts = cursor_encoded_parts(name)?;
    if parts[0].len() == 1 && parts[0].chars().next()?.is_ascii_alphabetic() {
        let drive = parts[0].to_ascii_uppercase();
        let start = PathBuf::from(format!("{drive}:\\"));
        return recover_encoded_segments(&start, &parts[1..]).map(|p| p.display().to_string());
    }
    if parts[0].eq_ignore_ascii_case("users")
        || parts[0].eq_ignore_ascii_case("home")
        || parts[0].eq_ignore_ascii_case("var")
        || parts[0].eq_ignore_ascii_case("tmp")
    {
        return recover_encoded_segments(Path::new("/"), &parts).map(|p| p.display().to_string());
    }
    None
}

/// Reconstruct a path from hyphen-encoded segments by matching real directory names.
///
/// Cursor encodes `demo_chen` and `demo-chen` both as `demo-chen`. At each existing
/// prefix, pick the longest child whose encoded name is a prefix of the remainder.
pub(crate) fn recover_encoded_segments(start: &Path, parts: &[&str]) -> Option<PathBuf> {
    if parts.is_empty() {
        return start.exists().then(|| start.to_path_buf());
    }
    if !start.exists() {
        return None;
    }
    recover_encoded_segments_rec(start, parts)
}

fn recover_encoded_segments_rec(start: &Path, parts: &[&str]) -> Option<PathBuf> {
    if parts.is_empty() {
        return Some(start.to_path_buf());
    }
    let mut matches: Vec<(usize, PathBuf)> = Vec::new();
    let rd = fs::read_dir(start).ok()?;
    for ent in rd.flatten() {
        let name = ent.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if name.is_empty() || name == "." || name == ".." {
            continue;
        }
        let encoded = name.replace('_', "-");
        if let Some(consumed) = match_encoded_prefix(&encoded, parts) {
            matches.push((consumed, ent.path()));
        }
    }
    matches.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    for (consumed, path) in matches {
        if let Some(found) = recover_encoded_segments_rec(&path, &parts[consumed..]) {
            return Some(found);
        }
    }
    None
}

fn match_encoded_prefix(encoded_child: &str, parts: &[&str]) -> Option<usize> {
    let child_parts: Vec<&str> = encoded_child.split('-').filter(|s| !s.is_empty()).collect();
    if child_parts.is_empty() || child_parts.len() > parts.len() {
        return None;
    }
    let ok = child_parts.iter().zip(parts.iter()).all(|(a, b)| {
        #[cfg(windows)]
        {
            a.eq_ignore_ascii_case(b)
        }
        #[cfg(not(windows))]
        {
            *a == *b
        }
    });
    ok.then_some(child_parts.len())
}

/// Normalize a workspace path for stable grouping (slashes + Windows drive case).
///
/// - `\` → `/`
/// - trim trailing `/` (keep `D:/` drive roots)
/// - uppercase Windows drive letter (`d:/x` → `D:/x`)
pub fn normalize_cwd(cwd: &str) -> String {
    let mut s = cwd.trim().replace('\\', "/");
    while s.len() > 3 && s.ends_with('/') {
        s.pop();
    }
    let bytes = s.as_bytes();
    if bytes.len() >= 2 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic() {
        let drive = bytes[0].to_ascii_uppercase() as char;
        s = format!("{drive}{}", &s[1..]);
    }
    s
}

/// Normalize a cwd string into a stable storage key (forward slashes).
pub fn cwd_storage_key(cwd: &str) -> String {
    format!("cwd/{}", normalize_cwd(cwd))
}

/// Decode Pi session folder names under `~/.pi/agent/sessions/`.
///
/// Pi wraps a Claude-like encoding with doubled separators, e.g.
/// - `--C--Users-example--` → `C:\Users\example`
/// - `--C--Users-example-Downloads-pi-windows-x64--` → lossy path decode (hyphens in
///   segment names share Claude's ambiguity)
pub fn decode_pi_session_dir(encoded: &str) -> Option<String> {
    let s = encoded.trim();
    if s.is_empty() {
        return None;
    }
    let core = s
        .strip_prefix("--")
        .and_then(|x| x.strip_suffix("--"))
        .unwrap_or(s);
    // `C--Users-example` → Claude-style `-C-Users-example`
    let normalized = format!("-{}", core.replace("--", "-"));
    decode_claude_project_dir(&normalized)
}

pub const UNGROUPED_KEY: &str = "__ungrouped__";

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn decode_claude_windows_path() {
        let got = decode_claude_project_dir("-C-Users-example-demo").unwrap();
        assert_eq!(got, "C:\\Users\\example\\demo");
    }

    #[test]
    fn decode_claude_unix_path() {
        let got = decode_claude_project_dir("-Users-foo-bar").unwrap();
        assert_eq!(got, "/Users/foo/bar");
    }

    #[test]
    fn verified_rejects_missing() {
        // Extremely unlikely to exist on a real machine.
        let encoded = "-Z-ThisPathDoesNotExist-AgentHub-XYZ";
        assert!(verified_actual_path(encoded).is_none());
        assert!(decode_claude_project_dir(encoded).is_some());
    }

    #[test]
    fn decode_cursor_project_dir_drive() {
        let got = decode_cursor_project_dir("d-demo-workspace-2026-AgentHub").unwrap();
        assert!(got.starts_with("D:\\"));
        assert!(got.contains("AgentHub"));
        assert!(decode_cursor_project_dir("empty-window").is_none());
        assert!(decode_cursor_project_dir("1785382907533").is_none());
    }

    #[test]
    fn recover_encoded_segments_prefers_underscore_over_split() {
        let dir = tempfile::tempdir().unwrap();
        let decoy = dir.path().join("demo");
        fs::create_dir_all(&decoy).unwrap();
        let real = dir.path().join("demo_chen").join("2026").join("AgentHub");
        fs::create_dir_all(&real).unwrap();
        let got =
            recover_encoded_segments(dir.path(), &["demo", "chen", "2026", "AgentHub"]).unwrap();
        assert_eq!(got, real);
        assert_ne!(got, decoy.join("chen").join("2026").join("AgentHub"));
    }

    #[test]
    fn recover_encoded_segments_keeps_hyphen_in_folder_name() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir
            .path()
            .join("vibe-kanban-worktrees")
            .join("addd-review-AgentHub");
        fs::create_dir_all(&real).unwrap();
        let got = recover_encoded_segments(
            dir.path(),
            &["vibe", "kanban", "worktrees", "addd", "review", "AgentHub"],
        )
        .unwrap();
        assert_eq!(got, real);
    }

    #[test]
    fn recover_encoded_segments_rebuilds_uuid_folder() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("04a0406d-256b-4afb-8c62-6dd38beb8a48");
        fs::create_dir_all(&real).unwrap();
        let got = recover_encoded_segments(
            dir.path(),
            &["04a0406d", "256b", "4afb", "8c62", "6dd38beb8a48"],
        )
        .unwrap();
        assert_eq!(got, real);
    }

    #[test]
    fn cwd_storage_key_normalizes_slashes() {
        assert_eq!(cwd_storage_key(r"D:\work\repo"), "cwd/D:/work/repo");
    }

    #[test]
    fn normalize_cwd_drive_case_and_trailing_slash() {
        assert_eq!(
            normalize_cwd(r"d:\demo\workspace\2026\AgentHub"),
            "D:/demo/workspace/2026/AgentHub"
        );
        assert_eq!(normalize_cwd("D:/work/repo/"), "D:/work/repo");
        assert_eq!(normalize_cwd("D:/"), "D:/");
        assert_eq!(
            cwd_storage_key(r"d:\work\repo"),
            cwd_storage_key(r"D:\work\repo")
        );
    }

    #[test]
    fn decode_pi_session_dir_windows() {
        assert_eq!(
            decode_pi_session_dir("--C--Users-example--").as_deref(),
            Some(r"C:\Users\example")
        );
        // Hyphenated path segments are ambiguous (same as Claude encoding).
        let lossy = decode_pi_session_dir("--C--Users-example-Downloads-pi-windows-x64--").unwrap();
        assert!(lossy.starts_with(r"C:\Users\example\Downloads\"));
        assert!(lossy.contains("pi"));
    }
}
