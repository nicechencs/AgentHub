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
    let got = recover_encoded_segments(dir.path(), &["demo", "chen", "2026", "AgentHub"]).unwrap();
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
fn recover_encoded_segments_folds_unicode_hyphen() {
    let dir = tempfile::tempdir().unwrap();
    let real = dir.path().join("Cowork").join("VPS\u{2011}Hub");
    fs::create_dir_all(&real).unwrap();
    let got = recover_encoded_segments(dir.path(), &["Cowork", "VPS", "Hub"]).unwrap();
    assert_eq!(got, real);
}

#[test]
fn recover_encoded_segments_suffix_child_skips_extra_prefix() {
    let dir = tempfile::tempdir().unwrap();
    let real = dir.path().join("Cowork").join("codex-subagent");
    fs::create_dir_all(&real).unwrap();
    let got = recover_encoded_segments(dir.path(), &["Cowork", "subagent"]).unwrap();
    assert_eq!(got, real);
}

#[test]
fn cursor_encode_and_best_path_match_folder() {
    assert_eq!(
        cursor_encode_abs_path(r"D:\demo_test\DimBom_Haier").as_deref(),
        Some("d-demo-test-DimBom-Haier")
    );
    let nb = format!(r"D:\Cowork\VPS{}Hub", '\u{2011}');
    assert_eq!(
        cursor_encode_abs_path(&nb).as_deref(),
        Some("d-Cowork-VPS-Hub")
    );
    let picked = best_cursor_path_for_folder(
        "d-Cowork-subagent",
        [r"C:\Windows", r"D:\Cowork\codex-subagent"],
    )
    .unwrap();
    assert!(picked.ends_with("codex-subagent"));
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
