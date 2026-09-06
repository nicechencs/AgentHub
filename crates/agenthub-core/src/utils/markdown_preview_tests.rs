use std::fs;

use crate::catalog::limits::SKILL_MARKDOWN_PREVIEW_CHARS;
use crate::utils::markdown_preview::{is_markdown_path, read_markdown_file_preview};
use crate::utils::test_temp::real_tempdir;

#[test]
fn detects_markdown_extensions() {
    assert!(is_markdown_path(std::path::Path::new("README.md")));
    assert!(is_markdown_path(std::path::Path::new("notes.MDX")));
    assert!(is_markdown_path(std::path::Path::new("doc.markdown")));
    assert!(!is_markdown_path(std::path::Path::new("src/foo.ts")));
    assert!(!is_markdown_path(std::path::Path::new("docs")));
}

#[test]
fn reads_markdown_under_cwd() {
    let dir = real_tempdir();
    let file = dir.path().join("README.md");
    fs::write(&file, "# Hello\n").unwrap();
    let preview = read_markdown_file_preview(file.to_str().unwrap(), dir.path().to_str().unwrap())
        .unwrap();
    assert_eq!(preview.name, "README.md");
    assert_eq!(preview.content, "# Hello\n");
    assert!(!preview.truncated);
}

#[test]
fn resolves_relative_path_against_cwd() {
    let dir = real_tempdir();
    fs::create_dir_all(dir.path().join("docs")).unwrap();
    fs::write(dir.path().join("docs/guide.md"), "guide").unwrap();
    let preview =
        read_markdown_file_preview("docs/guide.md", dir.path().to_str().unwrap()).unwrap();
    assert_eq!(preview.content, "guide");
}

#[test]
fn rejects_non_markdown() {
    let dir = real_tempdir();
    let file = dir.path().join("main.ts");
    fs::write(&file, "export {}\n").unwrap();
    let err = read_markdown_file_preview(file.to_str().unwrap(), dir.path().to_str().unwrap())
        .unwrap_err();
    assert!(err.to_string().contains("markdown"));
}

#[test]
fn rejects_path_outside_cwd() {
    let cwd = real_tempdir();
    let other = real_tempdir();
    let file = other.path().join("secret.md");
    fs::write(&file, "nope").unwrap();
    let err = read_markdown_file_preview(file.to_str().unwrap(), cwd.path().to_str().unwrap())
        .unwrap_err();
    assert!(err.to_string().contains("outside"));
}

#[test]
fn truncates_large_body() {
    let dir = real_tempdir();
    let file = dir.path().join("huge.md");
    let mut body = String::from("# big\n");
    body.push_str(&"x".repeat(SKILL_MARKDOWN_PREVIEW_CHARS + 64));
    fs::write(&file, &body).unwrap();
    let preview = read_markdown_file_preview(file.to_str().unwrap(), dir.path().to_str().unwrap())
        .unwrap();
    assert!(preview.truncated);
    assert!(preview.content.chars().count() <= SKILL_MARKDOWN_PREVIEW_CHARS);
}

#[test]
fn rejects_symlink() {
    let dir = real_tempdir();
    let real = dir.path().join("real.md");
    fs::write(&real, "ok").unwrap();
    let link = dir.path().join("link.md");
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&real, &link).unwrap();
        let err = read_markdown_file_preview(link.to_str().unwrap(), dir.path().to_str().unwrap())
            .unwrap_err();
        assert!(err.to_string().contains("symlink"));
    }
    #[cfg(not(unix))]
    {
        let _ = (real, link);
    }
}
