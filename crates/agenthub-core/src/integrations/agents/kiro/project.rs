//! Kiro project/session listing from `~/.kiro/sessions/`.
//!
//! Verified on kiro-cli 2.21.1 + editor trees:
//! - CLI: `sessions/cli/<uuid>.json` (cwd / title / session_id) + companion `.jsonl`
//! - Editor: `sessions/<workspace>/sess_*/session.json` + `messages.jsonl`
//! Skip the `cli` bucket when walking workspace hashes. Do not invent paths.

use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use chrono::{DateTime, Utc};
use serde_json::Value;

use crate::catalog::limits::{PROJECT_PREVIEW_CHARS, PROJECT_SCAN_BYTES};
use crate::error::{AppError, Result};
use crate::integrations::shared::projects::{builtin_key, empty_if_missing, finish_sessions};
use crate::models::{AgentId, AgentProject, AgentProjectExcerpt, AgentSession};
use crate::platform::projects::{ProjectScanContext, ProjectSource};
use crate::services::project_service::aggregate_projects;
use crate::utils::project_path::{cwd_storage_key, UNGROUPED_KEY};

struct KiroProjectSource;

impl ProjectSource for KiroProjectSource {
    fn agent_key(&self) -> crate::platform::AgentKey {
        builtin_key("kiro")
    }

    fn list_projects(&self, ctx: &ProjectScanContext<'_>) -> Result<Vec<AgentProject>> {
        if empty_if_missing(ctx.home) {
            return Ok(vec![]);
        }
        Ok(list_kiro_projects(ctx.home))
    }

    fn list_sessions(&self, ctx: &ProjectScanContext<'_>) -> Result<Vec<AgentSession>> {
        if empty_if_missing(ctx.home) {
            return Ok(vec![]);
        }
        Ok(finish_sessions(list_kiro_sessions(ctx.home, None)))
    }

    fn list_sessions_in_project(
        &self,
        ctx: &ProjectScanContext<'_>,
        _project_id: &str,
        key: &str,
    ) -> Result<Vec<AgentSession>> {
        if empty_if_missing(ctx.home) {
            return Ok(vec![]);
        }
        Ok(finish_sessions(list_kiro_sessions(ctx.home, Some(key))))
    }
}

pub fn register(ctx: &mut crate::integrations::IntegrationContext<'_>) {
    ctx.projects
        .register(Arc::new(KiroProjectSource))
        .expect("unique built-in project source");
}

pub(crate) fn list_kiro_projects(home: &Path) -> Vec<AgentProject> {
    aggregate_projects(AgentId::Kiro, home, &list_kiro_sessions(home, None))
}

pub(crate) fn list_kiro_sessions(home: &Path, only_key: Option<&str>) -> Vec<AgentSession> {
    let mut out = Vec::new();
    collect_cli_sessions(home, only_key, &mut out);
    collect_editor_sessions(home, only_key, &mut out);
    out
}

pub(crate) fn load_kiro_excerpt(home: &Path, id: &str, rel: &str) -> Result<AgentProjectExcerpt> {
    let abs = resolve_rel(home, rel)?;
    if !abs.is_file() {
        return Err(AppError::NotFound(format!("project not found: {id}")));
    }
    let transcript = transcript_path_for(&abs);
    let (turns, truncated) = read_excerpt_turns(&transcript);
    let sessions = list_kiro_sessions(home, None);
    let rec = sessions.into_iter().find(|s| s.relative_path == rel);
    let title = rec
        .as_ref()
        .map(|s| s.title.clone())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            abs.file_stem()
                .and_then(|n| n.to_str())
                .unwrap_or(rel)
                .to_string()
        });
    let cwd = rec.as_ref().and_then(|s| s.cwd.clone());
    let updated_at = rec
        .as_ref()
        .map(|s| s.updated_at.clone())
        .unwrap_or_else(|| rfc3339_from_mtime(&abs));
    Ok(AgentProjectExcerpt {
        id: id.to_string(),
        agent_id: AgentId::Kiro,
        title,
        cwd,
        updated_at,
        excerpt: format_excerpt_turns(&turns),
        truncated,
    })
}

fn collect_cli_sessions(home: &Path, only_key: Option<&str>, out: &mut Vec<AgentSession>) {
    let dir = home.join("sessions").join("cli");
    let Ok(entries) = fs::read_dir(&dir) else {
        return;
    };
    for ent in entries.flatten() {
        let path = ent.path();
        if !path.is_file() {
            continue;
        }
        if !path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("json"))
        {
            continue;
        }
        if let Some(rec) = session_from_cli_json(home, &path) {
            if let Some(want) = only_key {
                let key = rec
                    .project_id
                    .strip_prefix("kiro:proj:")
                    .unwrap_or(rec.project_id.as_str());
                if key != want {
                    continue;
                }
            }
            out.push(rec);
        }
    }
}

fn collect_editor_sessions(home: &Path, only_key: Option<&str>, out: &mut Vec<AgentSession>) {
    let root = home.join("sessions");
    let Ok(workspaces) = fs::read_dir(&root) else {
        return;
    };
    for ws in workspaces.flatten() {
        let ws_path = ws.path();
        if !ws_path.is_dir() {
            continue;
        }
        let ws_name = ws_path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if ws_name.is_empty() || ws_name.starts_with('.') || ws_name.eq_ignore_ascii_case("cli") {
            continue;
        }
        let Ok(sessions) = fs::read_dir(&ws_path) else {
            continue;
        };
        for sess in sessions.flatten() {
            let sess_path = sess.path();
            if !sess_path.is_dir() {
                continue;
            }
            let sess_name = sess_path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if !sess_name.starts_with("sess_") {
                continue;
            }
            if let Some(rec) = session_from_editor_dir(home, &sess_path) {
                if let Some(want) = only_key {
                    let key = rec
                        .project_id
                        .strip_prefix("kiro:proj:")
                        .unwrap_or(rec.project_id.as_str());
                    if key != want {
                        continue;
                    }
                }
                out.push(rec);
            }
        }
    }
}

fn session_from_cli_json(home: &Path, path: &Path) -> Option<AgentSession> {
    let text = fs::read_to_string(path).ok()?;
    let root: Value = serde_json::from_str(&text).ok()?;
    let session_id = str_field(&root, &["session_id", "sessionId"]).or_else(|| {
        path.file_stem()
            .and_then(|s| s.to_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToOwned::to_owned)
    })?;
    let cwd = str_field(&root, &["cwd"]);
    let json_title = str_field(&root, &["title"]);
    let turns = root
        .pointer("/session_state/conversation_metadata/user_turn_metadatas")
        .and_then(Value::as_array)
        .map(|a| a.len())
        .unwrap_or(0);
    let jsonl = path.with_extension("jsonl");
    let (jsonl_preview, jsonl_count) = if jsonl.is_file() {
        first_user_preview(&jsonl)
    } else {
        (None, None)
    };
    if json_title.is_none() && turns == 0 && jsonl_count.unwrap_or(0) == 0 {
        return None;
    }
    let preview = jsonl_preview.or_else(|| {
        json_title
            .clone()
            .map(|t| truncate_chars(&t, PROJECT_PREVIEW_CHARS))
    });
    let title = json_title
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| preview.clone())
        .or_else(|| cwd.as_deref().map(title_from_cwd))
        .unwrap_or_else(|| session_id.clone());
    let key = project_key(cwd.as_deref());
    let updated_at = str_field(&root, &["updated_at", "updatedAt"])
        .and_then(|s| parse_rfc3339(&s))
        .unwrap_or_else(|| rfc3339_from_mtime(path));
    Some(build_session(
        home,
        path,
        &key,
        title,
        cwd,
        preview,
        jsonl_count.or_else(|| (turns > 0).then_some(turns as u32)),
        Some(session_id),
        updated_at,
    ))
}

fn session_from_editor_dir(home: &Path, sess_dir: &Path) -> Option<AgentSession> {
    let meta_path = sess_dir.join("session.json");
    let messages = sess_dir.join("messages.jsonl");
    let meta: Value = if meta_path.is_file() {
        let text = fs::read_to_string(&meta_path).ok()?;
        serde_json::from_str(&text).unwrap_or(Value::Null)
    } else {
        Value::Null
    };
    if !meta_path.is_file() && !messages.is_file() {
        return None;
    }
    let cwd = first_path_field(&meta, &["workspacePaths", "rootPaths"]);
    let json_title =
        str_field(&meta, &["title"]).filter(|t| !t.eq_ignore_ascii_case("New Session"));
    let session_id = str_field(&meta, &["id"]).or_else(|| {
        sess_dir
            .file_name()
            .and_then(|n| n.to_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToOwned::to_owned)
    })?;
    let (msg_preview, msg_count) = if messages.is_file() {
        first_user_preview(&messages)
    } else {
        (None, None)
    };
    if json_title.is_none() && msg_count.unwrap_or(0) == 0 && !messages.is_file() {
        return None;
    }
    let preview = msg_preview.or_else(|| {
        json_title
            .clone()
            .map(|t| truncate_chars(&t, PROJECT_PREVIEW_CHARS))
    });
    let title = json_title
        .or_else(|| preview.clone())
        .or_else(|| cwd.as_deref().map(title_from_cwd))
        .unwrap_or_else(|| session_id.clone());
    let key = project_key(cwd.as_deref());
    let primary = if messages.is_file() {
        messages
    } else {
        meta_path.clone()
    };
    let updated_at = str_field(&meta, &["lastModifiedAt", "last_modified_at", "createdAt"])
        .and_then(|s| parse_rfc3339(&s))
        .unwrap_or_else(|| rfc3339_from_mtime(&primary));
    Some(build_session(
        home,
        &primary,
        &key,
        title,
        cwd,
        preview,
        msg_count,
        Some(session_id),
        updated_at,
    ))
}

fn build_session(
    home: &Path,
    path: &Path,
    key: &str,
    title: String,
    cwd: Option<String>,
    preview: Option<String>,
    message_count: Option<u32>,
    session_id: Option<String>,
    updated_at: String,
) -> AgentSession {
    let rel = path
        .strip_prefix(home)
        .map(path_to_rel)
        .unwrap_or_else(|_| path_to_rel(path));
    let size_bytes = path.metadata().map(|m| m.len()).unwrap_or(0);
    AgentSession {
        id: format!("kiro:{rel}"),
        project_id: format!("kiro:proj:{key}"),
        agent_id: AgentId::Kiro,
        title,
        cwd,
        path: path.display().to_string(),
        relative_path: rel,
        size_bytes,
        updated_at,
        preview,
        message_count,
        session_id,
        parent_session_id: None,
        thread_kind: None,
        agent_role: None,
    }
}

fn first_user_preview(path: &Path) -> (Option<String>, Option<u32>) {
    let Ok(file) = File::open(path) else {
        return (None, None);
    };
    let mut reader = BufReader::new(file);
    let mut buf = String::new();
    let mut preview = None;
    let mut count = 0u32;
    let mut bytes = 0u64;
    loop {
        buf.clear();
        let n = match reader.read_line(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(_) => break,
        };
        bytes = bytes.saturating_add(n as u64);
        if bytes > PROJECT_SCAN_BYTES {
            break;
        }
        if let Some((role, text)) = parse_transcript_line(&buf) {
            count = count.saturating_add(1);
            if preview.is_none() && role == "user" && !text.is_empty() {
                preview = Some(truncate_chars(&text, PROJECT_PREVIEW_CHARS));
            }
        }
    }
    (preview, (count > 0).then_some(count))
}

fn read_excerpt_turns(path: &Path) -> (Vec<ExcerptTurn>, bool) {
    let Ok(file) = File::open(path) else {
        return (Vec::new(), false);
    };
    let mut reader = BufReader::new(file);
    let mut buf = String::new();
    let mut out: Vec<ExcerptTurn> = Vec::new();
    let mut bytes = 0u64;
    let mut truncated = false;
    loop {
        buf.clear();
        let n = match reader.read_line(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(_) => break,
        };
        bytes = bytes.saturating_add(n as u64);
        if bytes > PROJECT_SCAN_BYTES {
            truncated = true;
            break;
        }
        if let Some((role, text)) = parse_transcript_line(&buf) {
            if let Some(last) = out.last_mut() {
                if last.role == role {
                    last.text.push('\n');
                    last.text.push_str(&text);
                    continue;
                }
            }
            out.push(ExcerptTurn { role, text });
        }
    }
    (out, truncated)
}

fn parse_transcript_line(line: &str) -> Option<(&'static str, String)> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    let v: Value = serde_json::from_str(line).ok()?;
    if let Some(payload) = v.get("payload") {
        let ty = payload.get("type").and_then(Value::as_str).unwrap_or("");
        if matches!(ty, "user" | "assistant") {
            let text = json_text(payload.get("content")?)?;
            return Some((if ty == "user" { "user" } else { "assistant" }, text));
        }
    }
    let kind = v.get("kind").and_then(Value::as_str).unwrap_or("");
    let role = match kind {
        "Prompt" | "UserMessage" => "user",
        "AssistantMessage" => "assistant",
        _ => return None,
    };
    let text = json_text(v.get("data")?.get("content")?)?;
    Some((role, text))
}

fn json_text(v: &Value) -> Option<String> {
    if let Some(s) = v.as_str() {
        let t = s.trim();
        return (!t.is_empty()).then(|| t.to_string());
    }
    let arr = v.as_array()?;
    let mut out = String::new();
    for part in arr {
        let kind = part.get("kind").and_then(Value::as_str).unwrap_or("text");
        if kind != "text" {
            continue;
        }
        let piece = part
            .get("data")
            .and_then(|d| d.as_str().or_else(|| d.get("text").and_then(Value::as_str)))
            .or_else(|| part.get("text").and_then(Value::as_str))
            .map(str::trim)
            .filter(|s| !s.is_empty());
        if let Some(p) = piece {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(p);
        }
    }
    (!out.is_empty()).then_some(out)
}

fn transcript_path_for(abs: &Path) -> PathBuf {
    let name = abs.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if name.eq_ignore_ascii_case("session.json") {
        let sibling = abs.with_file_name("messages.jsonl");
        if sibling.is_file() {
            return sibling;
        }
    }
    if name
        .rsplit('.')
        .next()
        .is_some_and(|e| e.eq_ignore_ascii_case("json"))
    {
        let jsonl = abs.with_extension("jsonl");
        if jsonl.is_file() {
            return jsonl;
        }
    }
    abs.to_path_buf()
}

fn resolve_rel(home: &Path, rel: &str) -> Result<PathBuf> {
    if rel.is_empty() || rel.contains("..") {
        return Err(AppError::InvalidArg(format!(
            "invalid kiro session id: {rel}"
        )));
    }
    let mut abs = home.to_path_buf();
    for part in rel.split('/') {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." {
            return Err(AppError::InvalidArg("path traversal rejected".into()));
        }
        abs.push(part);
    }
    Ok(abs)
}

fn project_key(cwd: Option<&str>) -> String {
    match cwd.map(str::trim).filter(|s| !s.is_empty()) {
        Some(c) => cwd_storage_key(c),
        None => UNGROUPED_KEY.to_string(),
    }
}

fn title_from_cwd(cwd: &str) -> String {
    Path::new(cwd)
        .file_name()
        .and_then(|n| n.to_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(cwd)
        .to_string()
}

fn path_to_rel(rel: &Path) -> String {
    rel.to_string_lossy().replace('\\', "/")
}

fn first_path_field(v: &Value, keys: &[&str]) -> Option<String> {
    for k in keys {
        if let Some(arr) = v.get(*k).and_then(Value::as_array) {
            for item in arr {
                if let Some(s) = item.as_str().map(str::trim).filter(|s| !s.is_empty()) {
                    return Some(s.to_string());
                }
            }
        }
        if let Some(s) = v.get(*k).and_then(Value::as_str) {
            let t = s.trim();
            if !t.is_empty() {
                return Some(t.to_string());
            }
        }
    }
    None
}

fn str_field(v: &Value, keys: &[&str]) -> Option<String> {
    for k in keys {
        if let Some(s) = v.get(*k).and_then(Value::as_str) {
            let t = s.trim();
            if !t.is_empty() {
                return Some(t.to_string());
            }
        }
    }
    None
}

fn parse_rfc3339(raw: &str) -> Option<String> {
    DateTime::parse_from_rfc3339(raw.trim())
        .ok()
        .map(|dt| dt.with_timezone(&Utc).to_rfc3339())
}

fn rfc3339_from_mtime(path: &Path) -> String {
    path.metadata()
        .ok()
        .and_then(|m| m.modified().ok())
        .map(system_time_rfc3339)
        .unwrap_or_else(|| Utc::now().to_rfc3339())
}

fn system_time_rfc3339(t: SystemTime) -> String {
    DateTime::<Utc>::from(t).to_rfc3339()
}

fn truncate_chars(s: &str, max: usize) -> String {
    let count = s.chars().count();
    if count <= max {
        return s.to_string();
    }
    s.chars().take(max).collect()
}

struct ExcerptTurn {
    role: &'static str,
    text: String,
}

fn format_excerpt_turns(turns: &[ExcerptTurn]) -> String {
    let mut body = String::new();
    for t in turns {
        let text = t.text.trim();
        if text.is_empty() {
            continue;
        }
        if !body.is_empty() {
            body.push('\n');
        }
        body.push_str(&format!("---turn:{}---\n", t.role));
        body.push_str(text);
    }
    body
}

#[cfg(test)]
#[path = "project/tests.rs"]
mod tests;
