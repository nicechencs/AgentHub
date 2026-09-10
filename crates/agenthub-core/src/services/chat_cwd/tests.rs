use std::path::PathBuf;

use super::*;
use tempfile::tempdir;

#[test]
fn normalize_cwd_drops_blank_values() {
    assert_eq!(normalize_cwd(None), None);
    assert_eq!(normalize_cwd(Some(String::new())), None);
    assert_eq!(normalize_cwd(Some("   ".into())), None);
    assert_eq!(normalize_cwd(Some(" /tmp ".into())), Some("/tmp".into()));
}

#[test]
fn stored_cwd_missing_only_when_path_is_gone() {
    let dir = tempdir().unwrap();
    let live = dir.path().to_string_lossy().into_owned();
    assert!(!stored_cwd_missing(None));
    assert!(!stored_cwd_missing(Some("")));
    assert!(!stored_cwd_missing(Some(&live)));
    assert!(stored_cwd_missing(Some(
        "/var/folders/zz/T/.tmp-agenthub-missing/workspace"
    )));
}

#[test]
fn validate_existing_cwd_rejects_deleted_folder() {
    let err = validate_existing_cwd("/this/path/does/not-exist-agenthub").unwrap_err();
    assert_eq!(err.code(), "invalid_arg");
    assert!(err.to_string().contains("cwd is not an existing directory"));
}

#[test]
fn resolve_runtime_cwd_skips_dead_path() {
    let dir = tempdir().unwrap();
    let live = dir.path().to_string_lossy().into_owned();
    assert_eq!(
        resolve_runtime_cwd(Some(&live)).unwrap(),
        dir.path().to_path_buf()
    );

    let fallback = resolve_runtime_cwd(Some("/var/folders/zz/T/.tmp-dead/workspace")).unwrap();
    assert!(fallback.is_dir(), "fallback={fallback:?}");
    assert_ne!(
        fallback,
        PathBuf::from("/var/folders/zz/T/.tmp-dead/workspace")
    );
}
