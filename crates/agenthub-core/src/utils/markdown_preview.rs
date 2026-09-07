//! Read a markdown file for in-app preview (chat right pane).
//!
//! The file must live under the conversation working directory, use a markdown
//! extension, and not be reached through a symlink or Windows reparse point.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

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
        return Err(AppError::InvalidArg("working directory is required".into()));
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

    let cwd_canon = simplified_canon(&cwd_raw)?;
    let meta = fs::symlink_metadata(&candidate)?;
    if is_denied_link(&meta) {
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

    let file_canon = simplified_canon(&candidate)?;
    if !is_within_dir(&file_canon, &cwd_canon) {
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

fn simplified_canon(path: &Path) -> std::io::Result<PathBuf> {
    let canon = fs::canonicalize(path)?;
    Ok(dunce::simplified(&canon).to_path_buf())
}

fn is_within_dir(file: &Path, dir: &Path) -> bool {
    let file_keys = path_keys(file);
    let dir_keys = path_keys(dir);
    dir_keys.len() <= file_keys.len()
        && dir_keys
            .iter()
            .zip(file_keys.iter())
            .all(|(left, right)| left == right)
}

fn path_keys(path: &Path) -> Vec<String> {
    path.components()
        .map(|component| {
            let value = component.as_os_str().to_string_lossy();
            #[cfg(windows)]
            {
                value.to_ascii_lowercase()
            }
            #[cfg(not(windows))]
            {
                value.into_owned()
            }
        })
        .collect()
}

fn is_denied_link(meta: &fs::Metadata) -> bool {
    if meta.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
        meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(not(windows))]
    {
        false
    }
}
