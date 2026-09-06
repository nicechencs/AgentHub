//! Read a markdown file for in-app preview (chat right pane).
//!
//! The file must live under the conversation working directory, use a markdown
//! extension, and not be reached through a symlink.

use std::fs;
use std::io::Read;
use std::path::Path;

use crate::catalog::limits::SKILL_MARKDOWN_PREVIEW_CHARS;
use crate::error::{AppError, Result};
use crate::models::MarkdownFilePreview;
use crate::utils::paths::expand_user_path;

const MARKDOWN_EXTS: &[&str] = &["md", "mdx", "markdown"];

pub fn is_markdown_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            MARKDOWN_EXTS
                .iter()
                .any(|allowed| ext.eq_ignore_ascii_case(allowed))
        })
}

pub fn read_markdown_file_preview(path: &str, cwd: &str) -> Result<MarkdownFilePreview> {
    if cwd.trim().is_empty() {
        return Err(AppError::InvalidArg(
            "working directory is required".into(),
        ));
    }
    if path.trim().is_empty() {
        return Err(AppError::InvalidArg("path is required".into()));
    }
    let cwd_raw = expand_user_path(cwd.trim())?;
    let path_raw = expand_user_path(path.trim())?;

    let candidate = if path_raw.is_absolute() {
        path_raw
    } else {
        cwd_raw.join(path_raw)
    };
    if !is_markdown_path(&candidate) {
        return Err(AppError::InvalidArg(
            "only markdown files can be previewed".into(),
        ));
    }

    let cwd_canon = fs::canonicalize(&cwd_raw)?;
    let meta = fs::symlink_metadata(&candidate)?;
    if meta.file_type().is_symlink() {
        return Err(AppError::InvalidArg(format!(
            "refusing to read via symlink: {}",
            candidate.display()
        )));
    }
    if !meta.is_file() {
        return Err(AppError::NotFound(format!(
            "markdown file not found: {}",
            candidate.display()
        )));
    }

    let file_canon = fs::canonicalize(&candidate)?;
    if !file_canon.starts_with(&cwd_canon) {
        return Err(AppError::InvalidArg(
            "file is outside the working directory".into(),
        ));
    }

    let mut file = fs::File::open(&file_canon)?;
    let mut buf = String::new();
    let cap = SKILL_MARKDOWN_PREVIEW_CHARS.saturating_add(1);
    let mut limited = (&mut file).take(cap as u64);
    limited.read_to_string(&mut buf)?;
    let truncated = buf.chars().count() > SKILL_MARKDOWN_PREVIEW_CHARS;
    if truncated {
        buf = buf.chars().take(SKILL_MARKDOWN_PREVIEW_CHARS).collect();
    }

    let name = file_canon
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("preview")
        .to_string();

    Ok(MarkdownFilePreview {
        path: file_canon,
        name,
        content: buf,
        truncated,
    })
}
