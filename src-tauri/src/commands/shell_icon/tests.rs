use super::ico::encode_ico_rgba;
use super::shortcuts::shortcut_update_script;
use super::{sanitize_accent_id, validate_shell_icon_rgba, SHELL_ICON_MAX, SHELL_ICON_MIN};
use std::path::Path;

#[test]
fn accepts_square_rgba() {
    let n = 32usize;
    let rgba = vec![0u8; n * n * 4];
    assert!(validate_shell_icon_rgba(&rgba, 32, 32).is_ok());
}

#[test]
fn rejects_non_square() {
    let rgba = vec![0u8; 32 * 16 * 4];
    assert!(validate_shell_icon_rgba(&rgba, 32, 16).is_err());
}

#[test]
fn rejects_length_mismatch() {
    let rgba = vec![0u8; 10];
    assert!(validate_shell_icon_rgba(&rgba, 32, 32).is_err());
}

#[test]
fn rejects_out_of_range_size() {
    assert!(validate_shell_icon_rgba(&[], SHELL_ICON_MIN - 1, SHELL_ICON_MIN - 1).is_err());
    let too_big = SHELL_ICON_MAX + 1;
    assert!(validate_shell_icon_rgba(&[], too_big, too_big).is_err());
}

#[test]
fn sanitize_accent_id_allows_palette_keys() {
    assert_eq!(sanitize_accent_id("indigo").unwrap(), "indigo");
    assert_eq!(sanitize_accent_id("teal").unwrap(), "teal");
    assert!(sanitize_accent_id("../x").is_err());
    assert!(sanitize_accent_id("BLUE").is_err());
    assert!(sanitize_accent_id("").is_err());
}

#[test]
fn encode_ico_writes_header_and_bottom_left_pixel() {
    // 2x2: top-left red, bottom-left green (bottom-up XOR starts with green).
    let mut rgba = vec![0u8; 16];
    rgba[0..4].copy_from_slice(&[255, 0, 0, 255]);
    rgba[8..12].copy_from_slice(&[0, 255, 0, 255]);
    let ico = encode_ico_rgba(&rgba, 2, 2).unwrap();
    assert_eq!(&ico[0..6], &[0, 0, 1, 0, 1, 0]);
    assert_eq!(ico[6], 2);
    assert_eq!(ico[7], 2);
    let xor = 22 + 40;
    assert_eq!(&ico[xor..xor + 4], &[0, 255, 0, 255]);
}

#[test]
fn shortcut_script_points_at_exe_and_ico() {
    let script = shortcut_update_script(
        Path::new(r"C:\Program Files\AgentHub\AgentHub.exe"),
        Path::new(r"C:\Users\a\appdata\shell-icon-blue.ico"),
    );
    assert!(script.contains("AgentHub.exe"));
    assert!(script.contains("shell-icon-blue.ico"));
    assert!(script.contains("GetFolderPath('Desktop')"));
    assert!(script.contains("IconLocation"));
}

#[test]
fn shortcut_script_escapes_single_quotes_in_paths() {
    let script = shortcut_update_script(Path::new(r"C:\O'Brien\app.exe"), Path::new(r"C:\x.ico"));
    assert!(script.contains("O''Brien"));
    assert!(!script.contains("O'Brien"));
}
