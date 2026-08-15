//! SKILL.md frontmatter / metadata parsing (no YAML crate dependency).

use std::fs;
use std::io::Read;
use std::path::Path;

use crate::catalog::limits::SKILL_MARKDOWN_PREVIEW_CHARS;
use crate::error::{AppError, Result};
use crate::models::SkillMarkdownPreview;

pub(crate) fn read_skill_metadata(skill_dir: &Path, fallback_name: &str) -> (String, String) {
    let skill_md = skill_dir.join("SKILL.md");
    match fs::read_to_string(&skill_md) {
        Ok(content) => parse_skill_frontmatter(&content, fallback_name),
        Err(_) => (fallback_name.to_string(), String::new()),
    }
}

/// Load a skill markdown file for GUI preview (metadata name + capped body).
pub(crate) fn read_skill_md_file(
    skill_id: &str,
    skill_dir: &Path,
    skill_md: &Path,
) -> Result<SkillMarkdownPreview> {
    let meta = fs::symlink_metadata(skill_md)?;
    if meta.file_type().is_symlink() {
        return Err(AppError::InvalidArg(format!(
            "refusing to read SKILL.md via symlink: {}",
            skill_md.display()
        )));
    }
    if !meta.is_file() {
        return Err(AppError::NotFound(format!(
            "SKILL.md not found: {}",
            skill_md.display()
        )));
    }

    let mut file = fs::File::open(skill_md)?;
    let mut buf = String::new();
    // Read a little past the cap so we can set `truncated` accurately.
    let cap = SKILL_MARKDOWN_PREVIEW_CHARS.saturating_add(1);
    let mut limited = (&mut file).take(cap as u64);
    limited.read_to_string(&mut buf)?;
    let truncated = buf.chars().count() > SKILL_MARKDOWN_PREVIEW_CHARS;
    if truncated {
        buf = buf.chars().take(SKILL_MARKDOWN_PREVIEW_CHARS).collect();
    }

    let (name, _) = parse_skill_frontmatter(&buf, skill_id);
    // Prefer frontmatter name; if body was truncated before closing fence, fall back
    // to directory metadata scan which re-reads only when needed.
    let name = if name == skill_id {
        let (n, _) = read_skill_metadata(skill_dir, skill_id);
        n
    } else {
        name
    };

    Ok(SkillMarkdownPreview {
        skill_id: skill_id.to_string(),
        name,
        path: skill_md.to_path_buf(),
        content: buf,
        truncated,
    })
}

/// Conservatively parse simple YAML frontmatter at the top of `SKILL.md`.
///
/// Extracts `name` and `description` with optional matching single/double quotes.
/// Supports YAML block scalars (`|` / `>`) used by many real SKILL.md files.
/// Multi-line descriptions are collapsed to a single line (joined with spaces)
/// for UI list display. No YAML dependency; malformed blocks fall back safely.
pub(crate) fn parse_skill_frontmatter(content: &str, fallback_name: &str) -> (String, String) {
    let fallback = || (fallback_name.to_string(), String::new());

    // Accept optional UTF-8 BOM then optional whitespace before opening fence.
    let body = content.strip_prefix('\u{feff}').unwrap_or(content);
    let body = body.trim_start_matches([' ', '\t']);
    let after_open = if let Some(rest) = body.strip_prefix("---\r\n") {
        rest
    } else if let Some(rest) = body.strip_prefix("---\n") {
        rest
    } else if body == "---" || body.starts_with("---\r") {
        // Opening fence without a proper body terminator — treat as missing.
        return fallback();
    } else {
        return fallback();
    };

    // Closing fence must be a line that is exactly `---` (optional trailing \r).
    let mut fm_end: Option<usize> = None;
    let mut line_start = 0usize;
    let bytes = after_open.as_bytes();
    let mut i = 0usize;
    while i <= bytes.len() {
        if i == bytes.len() || bytes[i] == b'\n' {
            let mut line = &after_open[line_start..i];
            if let Some(stripped) = line.strip_suffix('\r') {
                line = stripped;
            }
            if line == "---" {
                fm_end = Some(line_start);
                break;
            }
            line_start = i + 1;
        }
        if i == bytes.len() {
            break;
        }
        i += 1;
    }

    let Some(end) = fm_end else {
        return fallback();
    };
    let frontmatter = &after_open[..end];

    let mut name: Option<String> = None;
    let mut description: Option<String> = None;

    let lines: Vec<&str> = frontmatter.lines().collect();
    let mut idx = 0usize;
    while idx < lines.len() {
        let raw_line = lines[idx];
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            idx += 1;
            continue;
        }
        let Some((key, value)) = line.split_once(':') else {
            idx += 1;
            continue;
        };
        let key = key.trim();
        let value_raw = value.trim();
        let key_indent = raw_line.len() - raw_line.trim_start().len();

        // YAML block scalar: `description: |` / `description: >-` etc.
        if is_yaml_block_scalar_marker(value_raw) {
            idx += 1;
            let block = collect_yaml_block_scalar(&lines, &mut idx, key_indent);
            match key {
                "name" if name.is_none() => {
                    if !block.is_empty() {
                        name = Some(block);
                    }
                }
                "description" if description.is_none() => {
                    description = Some(block);
                }
                _ => {}
            }
            continue;
        }

        let value = strip_simple_quotes(value_raw);
        // Never surface a bare block marker as the description value.
        if is_yaml_block_scalar_marker(value) {
            idx += 1;
            continue;
        }
        match key {
            "name" if name.is_none() => {
                if !value.is_empty() {
                    name = Some(value.to_string());
                }
            }
            "description" if description.is_none() => {
                description = Some(value.to_string());
            }
            _ => {}
        }
        idx += 1;
    }

    (
        name.unwrap_or_else(|| fallback_name.to_string()),
        description.unwrap_or_default(),
    )
}

/// True for YAML block scalar indicators: `|`, `>`, `|-`, `>+`, `|2`, etc.
pub(crate) fn is_yaml_block_scalar_marker(value: &str) -> bool {
    let v = value.trim();
    let mut chars = v.chars();
    match chars.next() {
        Some('|') | Some('>') => chars.all(|c| c.is_ascii_digit() || c == '-' || c == '+'),
        _ => false,
    }
}

/// Collect indented lines of a YAML block scalar, collapse to one display line.
pub(crate) fn collect_yaml_block_scalar(lines: &[&str], idx: &mut usize, key_indent: usize) -> String {
    let mut parts: Vec<&str> = Vec::new();
    while *idx < lines.len() {
        let raw = lines[*idx];
        // Blank lines are allowed inside a block; skip for single-line UI text.
        if raw.trim().is_empty() {
            *idx += 1;
            // End block if the next non-empty line is not indented deeper than the key.
            let mut look = *idx;
            while look < lines.len() && lines[look].trim().is_empty() {
                look += 1;
            }
            if look >= lines.len() {
                break;
            }
            let next_indent = lines[look].len() - lines[look].trim_start().len();
            if next_indent > key_indent {
                continue;
            }
            break;
        }
        let indent = raw.len() - raw.trim_start().len();
        if indent > key_indent {
            parts.push(raw.trim());
            *idx += 1;
        } else {
            break;
        }
    }
    parts.join(" ")
}

pub(crate) fn strip_simple_quotes(value: &str) -> &str {
    let bytes = value.as_bytes();
    if bytes.len() >= 2 {
        let first = bytes[0];
        let last = bytes[bytes.len() - 1];
        if (first == b'"' && last == b'"') || (first == b'\'' && last == b'\'') {
            return &value[1..value.len() - 1];
        }
    }
    value
}
