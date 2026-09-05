//! ZCode project/session listing from the desktop task index (and CLI fallback).
//!
//! Primary: `v2/tasks-index.sqlite` `tasks` rows grouped by workspace path.
//! Fallback: `cli/db/db.sqlite` `session` rows with no parent (skip subagents).
//! Preview reads `message` + `part` text from the CLI db. Delete is not implemented.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use crate::catalog::limits::PROJECT_PREVIEW_CHARS;
use crate::error::{AppError, Result};
use crate::integrations::shared::projects::{builtin_key, empty_if_missing, finish_sessions};
use crate::integrations::shared::sqlite::{epoch_to_rfc3339, open_readonly, table_exists};
use crate::models::{AgentId, AgentProject, AgentProjectExcerpt, AgentSession};
use crate::platform::projects::{ProjectScanContext, ProjectSource};
use crate::utils::project_path::cwd_storage_key;

const TASKS_DB: &str = "v2/tasks-index.sqlite";

struct ZcodeProjectSource;

impl ProjectSource for ZcodeProjectSource {
    fn agent_key(&self) -> crate::platform::AgentKey {
        builtin_key("zcode")
    }

    fn list_projects(&self, ctx: &ProjectScanContext<'_>) -> Result<Vec<AgentProject>> {
        if empty_if_missing(ctx.home) {
            return Ok(vec![]);
        }
        Ok(list_zcode_projects(ctx.home))
    }

    fn list_sessions(&self, ctx: &ProjectScanContext<'_>) -> Result<Vec<AgentSession>> {
        if empty_if_missing(ctx.home) {
            return Ok(vec![]);
        }
        Ok(finish_sessions(list_zcode_sessions(ctx.home, None)))
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
        Ok(finish_sessions(list_zcode_sessions(ctx.home, Some(key))))
    }
}

pub fn register(ctx: &mut crate::integrations::IntegrationContext<'_>) {
    ctx.projects
        .register(Arc::new(ZcodeProjectSource))
        .expect("unique built-in project source");
}

struct TaskRow {
    workspace_path: String,
    task_id: String,
    title: String,
    updated_at: i64,
}

pub(crate) fn list_zcode_projects(home: &Path) -> Vec<AgentProject> {
    let rows = load_task_rows(home);
    let mut buckets: BTreeMap<String, ProjectAcc> = BTreeMap::new();
    for row in rows {
        let key = cwd_storage_key(&row.workspace_path);
        let actual = native_existing_path(&row.workspace_path);
        let title = title_from_path(&row.workspace_path);
        match buckets.get_mut(&key) {
            Some(acc) => acc.merge(&row),
            None => {
                buckets.insert(
                    key,
                    ProjectAcc {
                        title,
                        storage_path: row.workspace_path.clone(),
                        actual_path: actual,
                        session_count: 1,
                        newest: row.updated_at,
                    },
                );
            }
        }
    }
    let mut out: Vec<AgentProject> = buckets
        .into_iter()
        .map(|(key, acc)| acc.into_project(&key))
        .collect();
    out.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    out
}

/// Conversation preview for `zcode:v2/task/<session_id>`.
pub(crate) fn load_zcode_excerpt(home: &Path, id: &str, rel: &str) -> Result<AgentProjectExcerpt> {
    let native = native_session_id(rel)
        .ok_or_else(|| AppError::InvalidArg(format!("invalid zcode session id: {id}")))?;
    let meta = load_excerpt_meta(home, native);
    let mut turns = load_cli_turns(home, native);
    if turns.is_empty() {
        if let Some(fallback) = meta.as_ref().and_then(|m| m.searchable.clone()) {
            turns = turns_from_searchable(&fallback);
        }
    }
    if meta.is_none() && turns.is_empty() {
        return Err(AppError::NotFound(format!("project not found: {id}")));
    }
    let title = meta
        .as_ref()
        .map(|m| m.title.clone())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| native.to_string());
    let cwd = meta.as_ref().and_then(|m| {
        let p = m.workspace_path.trim();
        (!p.is_empty()).then(|| p.to_string())
    });
    let updated_at = meta
        .as_ref()
        .map(|m| epoch_to_rfc3339(m.updated_at))
        .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());
    Ok(AgentProjectExcerpt {
        id: id.to_string(),
        agent_id: AgentId::Zcode,
        title,
        cwd,
        updated_at,
        excerpt: format_excerpt_turns(&turns),
        truncated: false,
    })
}

pub(crate) fn list_zcode_sessions(home: &Path, only_key: Option<&str>) -> Vec<AgentSession> {
    let rows = load_task_rows(home);
    let mut out = Vec::new();
    for row in rows {
        let key = cwd_storage_key(&row.workspace_path);
        if let Some(want) = only_key {
            if key != want {
                continue;
            }
        }
        out.push(session_from_task(&row, &key));
    }
    out
}

fn load_task_rows(home: &Path) -> Vec<TaskRow> {
    let from_index = load_tasks_index(home);
    if !from_index.is_empty() {
        return from_index;
    }
    load_cli_sessions(home)
}

fn load_tasks_index(home: &Path) -> Vec<TaskRow> {
    let path = home.join("v2").join("tasks-index.sqlite");
    let Some(conn) = open_readonly(&path) else {
        return Vec::new();
    };
    if !table_exists(&conn, "tasks") {
        return Vec::new();
    }
    let mut stmt = match conn.prepare(
        "SELECT workspace_path, workspace_key, task_id, title, updated_at, created_at,
                IFNULL(deleted, 0)
         FROM tasks",
    ) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let iter = stmt.query_map([], |r| {
        Ok((
            r.get::<_, Option<String>>(0)?,
            r.get::<_, Option<String>>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, Option<String>>(3)?,
            r.get::<_, Option<i64>>(4)?,
            r.get::<_, Option<i64>>(5)?,
            r.get::<_, i64>(6)?,
        ))
    });
    let Ok(iter) = iter else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for row in iter.flatten() {
        let (workspace_path, workspace_key, task_id, title, updated, created, deleted) = row;
        if deleted != 0 || task_id.trim().is_empty() {
            continue;
        }
        let workspace = first_nonempty(&[
            workspace_path.as_deref().unwrap_or(""),
            workspace_key.as_deref().unwrap_or(""),
        ]);
        if workspace.is_empty() {
            continue;
        }
        let title = title
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(task_id.as_str())
            .to_string();
        out.push(TaskRow {
            workspace_path: workspace,
            task_id,
            title,
            updated_at: updated.or(created).unwrap_or(0),
        });
    }
    out
}

fn load_cli_sessions(home: &Path) -> Vec<TaskRow> {
    let path = home.join("cli").join("db").join("db.sqlite");
    let Some(conn) = open_readonly(&path) else {
        return Vec::new();
    };
    if !table_exists(&conn, "session") {
        return Vec::new();
    }
    let mut stmt = match conn.prepare(
        "SELECT id, directory, path, title, time_updated, time_created, parent_id, task_type
         FROM session",
    ) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let iter = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, Option<String>>(1)?,
            r.get::<_, Option<String>>(2)?,
            r.get::<_, Option<String>>(3)?,
            r.get::<_, Option<i64>>(4)?,
            r.get::<_, Option<i64>>(5)?,
            r.get::<_, Option<String>>(6)?,
            r.get::<_, Option<String>>(7)?,
        ))
    });
    let Ok(iter) = iter else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for row in iter.flatten() {
        let (id, directory, path, title, updated, created, parent_id, task_type) = row;
        if parent_id.as_deref().is_some_and(|p| !p.trim().is_empty()) {
            continue;
        }
        if task_type.as_deref() == Some("subagent_child") {
            continue;
        }
        if id.trim().is_empty() {
            continue;
        }
        let workspace = first_nonempty(&[
            directory.as_deref().unwrap_or(""),
            path.as_deref().unwrap_or(""),
        ]);
        if workspace.is_empty() {
            continue;
        }
        let title = title
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(id.as_str())
            .to_string();
        out.push(TaskRow {
            workspace_path: workspace,
            task_id: id,
            title,
            updated_at: updated.or(created).unwrap_or(0),
        });
    }
    out
}

struct ProjectAcc {
    title: String,
    storage_path: String,
    actual_path: Option<String>,
    session_count: u32,
    newest: i64,
}

impl ProjectAcc {
    fn merge(&mut self, row: &TaskRow) {
        self.session_count = self.session_count.saturating_add(1);
        if row.updated_at > self.newest {
            self.newest = row.updated_at;
        }
    }

    fn into_project(self, key: &str) -> AgentProject {
        AgentProject {
            id: format!("zcode:proj:{key}"),
            agent_id: AgentId::Zcode,
            title: self.title,
            storage_path: self.storage_path,
            actual_path: self.actual_path,
            relative_path: format!("{TASKS_DB}#{key}"),
            session_count: self.session_count,
            message_count: None,
            size_bytes: 0,
            updated_at: epoch_to_rfc3339(self.newest),
            preview: None,
            alias: None,
            hidden: false,
        }
    }
}

fn session_from_task(row: &TaskRow, project_key: &str) -> AgentSession {
    let preview = truncate_chars(&row.title, PROJECT_PREVIEW_CHARS);
    AgentSession {
        id: format!("zcode:v2/task/{}", row.task_id),
        project_id: format!("zcode:proj:{project_key}"),
        agent_id: AgentId::Zcode,
        title: row.title.clone(),
        cwd: Some(row.workspace_path.clone()),
        path: row.workspace_path.clone(),
        relative_path: format!("v2/task/{}", row.task_id),
        size_bytes: 0,
        updated_at: epoch_to_rfc3339(row.updated_at),
        preview: Some(preview),
        message_count: None,
        session_id: Some(row.task_id.clone()),
        parent_session_id: None,
        thread_kind: None,
    }
}

fn first_nonempty(parts: &[&str]) -> String {
    parts
        .iter()
        .map(|s| s.trim())
        .find(|s| !s.is_empty())
        .unwrap_or("")
        .to_string()
}

fn title_from_path(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(path)
        .to_string()
}

fn native_existing_path(candidate: &str) -> Option<String> {
    if candidate.is_empty() || !Path::new(candidate).exists() {
        return None;
    }
    #[cfg(windows)]
    {
        Some(candidate.replace('/', "\\"))
    }
    #[cfg(not(windows))]
    {
        Some(candidate.to_string())
    }
}

fn truncate_chars(s: &str, max: usize) -> String {
    let count = s.chars().count();
    if count <= max {
        return s.to_string();
    }
    s.chars().take(max).collect()
}

fn native_session_id(rel: &str) -> Option<&str> {
    let id = rel
        .strip_prefix("v2/task/")
        .unwrap_or(rel)
        .trim()
        .trim_start_matches('/');
    if id.is_empty() || id.contains("..") {
        return None;
    }
    Some(id)
}

struct ExcerptMeta {
    title: String,
    workspace_path: String,
    updated_at: i64,
    searchable: Option<String>,
}

struct ExcerptTurn {
    role: &'static str,
    text: String,
}

fn load_excerpt_meta(home: &Path, native: &str) -> Option<ExcerptMeta> {
    if let Some(meta) = load_task_meta(home, native) {
        return Some(meta);
    }
    load_cli_session_meta(home, native)
}

fn load_task_meta(home: &Path, native: &str) -> Option<ExcerptMeta> {
    let path = home.join("v2").join("tasks-index.sqlite");
    let conn = open_readonly(&path)?;
    if !table_exists(&conn, "tasks") {
        return None;
    }
    let mut stmt = conn
        .prepare(
            "SELECT title, workspace_path, workspace_key, updated_at, created_at,
                    searchable_text
             FROM tasks WHERE task_id = ?1 LIMIT 1",
        )
        .ok()
        .or_else(|| {
            conn.prepare(
                "SELECT title, workspace_path, workspace_key, updated_at, created_at, NULL
                 FROM tasks WHERE task_id = ?1 LIMIT 1",
            )
            .ok()
        })?;
    let row = stmt
        .query_row([native], |r| {
            Ok((
                r.get::<_, Option<String>>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, Option<i64>>(3)?,
                r.get::<_, Option<i64>>(4)?,
                r.get::<_, Option<String>>(5)?,
            ))
        })
        .ok()?;
    let (title, workspace_path, workspace_key, updated, created, searchable) = row;
    let workspace = first_nonempty(&[
        workspace_path.as_deref().unwrap_or(""),
        workspace_key.as_deref().unwrap_or(""),
    ]);
    let title = title
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(native)
        .to_string();
    let searchable = searchable
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    Some(ExcerptMeta {
        title,
        workspace_path: workspace,
        updated_at: updated.or(created).unwrap_or(0),
        searchable,
    })
}

fn load_cli_session_meta(home: &Path, native: &str) -> Option<ExcerptMeta> {
    let path = home.join("cli").join("db").join("db.sqlite");
    let conn = open_readonly(&path)?;
    if !table_exists(&conn, "session") {
        return None;
    }
    let mut stmt = conn
        .prepare(
            "SELECT title, directory, path, time_updated, time_created
             FROM session WHERE id = ?1 LIMIT 1",
        )
        .ok()?;
    let row = stmt
        .query_row([native], |r| {
            Ok((
                r.get::<_, Option<String>>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, Option<i64>>(3)?,
                r.get::<_, Option<i64>>(4)?,
            ))
        })
        .ok()?;
    let (title, directory, path, updated, created) = row;
    let workspace = first_nonempty(&[
        directory.as_deref().unwrap_or(""),
        path.as_deref().unwrap_or(""),
    ]);
    let title = title
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(native)
        .to_string();
    Some(ExcerptMeta {
        title,
        workspace_path: workspace,
        updated_at: updated.or(created).unwrap_or(0),
        searchable: None,
    })
}

fn load_cli_turns(home: &Path, native: &str) -> Vec<ExcerptTurn> {
    let path = home.join("cli").join("db").join("db.sqlite");
    let Some(conn) = open_readonly(&path) else {
        return Vec::new();
    };
    if !table_exists(&conn, "message") || !table_exists(&conn, "part") {
        return Vec::new();
    }
    let mut stmt = match conn.prepare(
        "SELECT m.data, p.data
         FROM message m
         INNER JOIN part p ON p.message_id = m.id
         WHERE m.session_id = ?1
         ORDER BY m.sequence ASC, p.sequence ASC",
    ) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let iter = match stmt.query_map([native], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
    }) {
        Ok(i) => i,
        Err(_) => return Vec::new(),
    };
    let mut out: Vec<ExcerptTurn> = Vec::new();
    for row in iter.flatten() {
        let (msg_raw, part_raw) = row;
        let Ok(msg) = serde_json::from_str::<serde_json::Value>(&msg_raw) else {
            continue;
        };
        let Ok(part) = serde_json::from_str::<serde_json::Value>(&part_raw) else {
            continue;
        };
        let Some(role) = message_role(&msg) else {
            continue;
        };
        let Some(text) = part_text(&part) else {
            continue;
        };
        if let Some(last) = out.last_mut() {
            if last.role == role {
                last.text.push('\n');
                last.text.push_str(&text);
                continue;
            }
        }
        out.push(ExcerptTurn { role, text });
    }
    out
}

fn message_role(msg: &serde_json::Value) -> Option<&'static str> {
    match msg.get("role").and_then(|v| v.as_str()).map(str::trim)? {
        "user" => Some("user"),
        "assistant" => Some("assistant"),
        _ => None,
    }
}

fn part_text(part: &serde_json::Value) -> Option<String> {
    let ty = part.get("type").and_then(|v| v.as_str()).unwrap_or("");
    if ty != "text" {
        return None;
    }
    let text = part
        .get("text")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())?;
    Some(text.to_string())
}

fn turns_from_searchable(text: &str) -> Vec<ExcerptTurn> {
    let text = text.trim();
    if text.is_empty() {
        return Vec::new();
    }
    match text.split_once('\n') {
        Some((user, rest)) => {
            let user = user.trim();
            let rest = rest.trim();
            let mut out = Vec::new();
            if !user.is_empty() {
                out.push(ExcerptTurn {
                    role: "user",
                    text: user.to_string(),
                });
            }
            if !rest.is_empty() {
                out.push(ExcerptTurn {
                    role: "assistant",
                    text: rest.to_string(),
                });
            }
            out
        }
        None => vec![ExcerptTurn {
            role: "user",
            text: text.to_string(),
        }],
    }
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
