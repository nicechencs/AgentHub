use super::*;
use crate::platform::skills::ensure_no_symlink_in_existing_prefix;

#[test]
fn real_tempdir_path_has_no_symlink_prefix() {
    let tmp = real_tempdir();
    ensure_no_symlink_in_existing_prefix(tmp.path()).expect("fixture root must be link-free");
    assert!(tmp.path().is_dir());
}
