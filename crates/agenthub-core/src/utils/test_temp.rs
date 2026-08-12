//! Shared tempfile helpers for unit tests.
//!
//! macOS exposes `/tmp` as a symlink to `/private/tmp`. Skills safety checks
//! intentionally refuse paths that traverse a symlink/reparse component, so
//! fixtures created under the default temp dir fail with:
//! `path must not traverse a symlink or reparse point: /tmp`.
//!
//! Prefer [`real_tempdir`] in skill / path-safety tests so the fixture root is a
//! fully resolved, non-symlink path on every platform.

#![cfg(test)]

use std::fs;
use std::path::PathBuf;

use tempfile::{Builder, TempDir};

/// Create a temporary directory whose absolute path does not traverse a symlink.
///
/// Falls back to the process CWD when the system temp root cannot be resolved.
pub fn real_tempdir() -> TempDir {
    let base = canonical_temp_base();
    fs::create_dir_all(&base).expect("create real temp base");
    Builder::new()
        .prefix("agenthub-test-")
        .tempdir_in(&base)
        .expect("create real tempdir")
}

fn canonical_temp_base() -> PathBuf {
    let candidates = [
        std::env::temp_dir(),
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
    ];
    for candidate in candidates {
        if let Ok(canon) = fs::canonicalize(&candidate) {
            return canon;
        }
        if candidate.is_dir() {
            return candidate;
        }
    }
    PathBuf::from(".")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::skills::ensure_no_symlink_in_existing_prefix;

    #[test]
    fn real_tempdir_path_has_no_symlink_prefix() {
        let tmp = real_tempdir();
        ensure_no_symlink_in_existing_prefix(tmp.path()).expect("fixture root must be link-free");
        assert!(tmp.path().is_dir());
    }
}
