//! Decode Claude / WorkBuddy / Cursor project directory encodings.

use std::path::Path;

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
/// Examples (lossy):
/// - `d-demo-workspace-2026-AgentHub` → `D:\demo\workspace\2026\AgentHub`
/// - `empty-window` / pure digits → None
pub fn decode_cursor_project_dir(name: &str) -> Option<String> {
    if name.is_empty() || name == "empty-window" {
        return None;
    }
    if name.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let parts: Vec<&str> = name.split('-').filter(|s| !s.is_empty()).collect();
    if parts.is_empty() {
        return None;
    }
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

/// Cursor: prefer existing decoded path; otherwise still return candidate for display.
pub fn cursor_actual_path(name: &str) -> Option<String> {
    let candidate = decode_cursor_project_dir(name)?;
    Some(candidate)
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
        let lossy =
            decode_pi_session_dir("--C--Users-example-Downloads-pi-windows-x64--").unwrap();
        assert!(lossy.starts_with(r"C:\Users\example\Downloads\"));
        assert!(lossy.contains("pi"));
    }
}
