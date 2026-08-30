use super::{validate_shell_icon_rgba, SHELL_ICON_MAX, SHELL_ICON_MIN};

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
