use super::{
    file_path_to_display_string, path_to_display_string, pick_directory_default_title,
    starting_directory, DEFAULT_PICK_DIRECTORY_TITLE,
};
use crate::tray_i18n::TrayUiLanguage;
use std::fs;
use tauri_plugin_dialog::FilePath;
use tempfile::tempdir;

#[test]
fn starting_directory_none_for_blank() {
    assert!(starting_directory(None).is_none());
    assert!(starting_directory(Some("")).is_none());
    assert!(starting_directory(Some("   ")).is_none());
}

#[test]
fn starting_directory_uses_existing_dir() {
    let dir = tempdir().unwrap();
    let raw = dir.path().to_str().expect("tempdir is utf-8");
    let got = starting_directory(Some(raw)).unwrap();
    assert_eq!(got, dir.path());
}

#[test]
fn starting_directory_falls_back_to_parent_when_leaf_missing() {
    let dir = tempdir().unwrap();
    let missing = dir.path().join("no-such-child");
    let raw = missing.to_str().expect("tempdir is utf-8");
    let got = starting_directory(Some(raw)).unwrap();
    assert_eq!(got, dir.path());
}

#[test]
fn starting_directory_uses_parent_when_default_is_existing_file() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("notes.txt");
    fs::write(&file, b"x").unwrap();
    let raw = file.to_str().expect("tempdir is utf-8");
    let got = starting_directory(Some(raw)).unwrap();
    assert_eq!(got, dir.path());
}

#[test]
fn starting_directory_none_for_relative_single_component() {
    assert!(starting_directory(Some("foo")).is_none());
}

#[test]
fn pick_directory_default_title_zh_en() {
    assert_eq!(
        pick_directory_default_title(TrayUiLanguage::Zh),
        DEFAULT_PICK_DIRECTORY_TITLE
    );
    assert_eq!(
        pick_directory_default_title(TrayUiLanguage::Zh),
        "选择工作目录"
    );
    assert_eq!(
        pick_directory_default_title(TrayUiLanguage::En),
        "Select working directory"
    );
}

#[test]
fn file_path_to_display_string_roundtrips_pathbuf() {
    let dir = tempdir().unwrap();
    fs::create_dir_all(dir.path()).unwrap();
    let display = file_path_to_display_string(FilePath::from(dir.path().to_path_buf())).unwrap();
    assert_eq!(display, path_to_display_string(dir.path()));
    assert!(!display.trim().is_empty());
}
