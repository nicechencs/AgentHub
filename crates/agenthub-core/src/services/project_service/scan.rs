//! Agent-specific project/session filesystem scanners.
//!
//! Owned by [`crate::platform::projects`] ProjectSource implementations and unit tests.
//! Still housed under project_service until a later split; do not grow new
//! AgentId match arms in ProjectService orchestration.

use chrono::Utc;
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::catalog::limits::{
    PROJECT_EXCERPT_CHARS as EXCERPT_CHARS, PROJECT_LIST_HEAD_BYTES as LIST_HEAD_BYTES,
    PROJECT_PREVIEW_CHARS as PREVIEW_CHARS, PROJECT_SCAN_BYTES as SCAN_BYTES,
    PROJECT_USER_TURN_EXCERPT_CHARS as USER_TURN_EXCERPT_CHARS,
};
use crate::error::{AppError, Result};
use crate::models::{AgentId, AgentProject, AgentProjectExcerpt, AgentSession};
use crate::utils::paths::{agent_home, first_env_path};
use crate::utils::project_path::{
    cursor_actual_path, cwd_storage_key, decode_claude_project_dir, decode_pi_session_dir,
    verified_actual_path, UNGROUPED_KEY,
};

use super::session_index::{file_size_mtime, IndexEntry, SessionIndexStore};
use super::{parse_project_id, parse_session_id, resolve_under_home, system_time_to_rfc3339};

pub(crate) fn list_claude_workbuddy_sessions(
    home: &Path,
    agent: AgentId,
    only_encoded: Option<&str>,
) -> Result<Vec<AgentSession>> {
    let root = home.join("projects");
    if !root.is_dir() {
        return Ok(vec![]);
    }
    let mut out = Vec::new();
    if let Some(encoded) = only_encoded {
        let dir = root.join(encoded);
        if dir.is_dir() {
            push_claude_dir_sessions(home, agent, &dir, encoded, &mut out);
        }
        return Ok(out);
    }
    let entries = match fs::read_dir(&root) {
        Ok(e) => e,
        Err(_) => return Ok(vec![]),
    };
    for ent in entries.flatten() {
        let dir = ent.path();
        if !dir.is_dir() {
            continue;
        }
        let encoded = dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        if encoded.is_empty() || encoded.starts_with('.') {
            continue;
        }
        push_claude_dir_sessions(home, agent, &dir, &encoded, &mut out);
    }
    Ok(out)
}

fn push_claude_dir_sessions(
    home: &Path,
    agent: AgentId,
    dir: &Path,
    encoded: &str,
    out: &mut Vec<AgentSession>,
) {
    let project_id = make_project_id(agent, encoded);
    let dir_entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for file_ent in dir_entries.flatten() {
        let path = file_ent.path();
        if !path.is_file() || !is_primary_session_file(agent, &path) {
            continue;
        }
        if let Some(rec) = build_session(agent, home, &path, &project_id, Some(encoded)) {
            out.push(rec);
        }
    }
}

/// Codex: recursive under `sessions/`, primary `*.jsonl` (typically `rollout-*.jsonl`).
/// Group by `payload.cwd` / cwd fields. `only_project_id` filters while scanning.
pub(crate) fn list_codex_sessions(
    home: &Path,
    only_project_id: Option<&str>,
    data_dir: Option<&Path>,
) -> Result<Vec<AgentSession>> {
    list_sessions_tree(home, AgentId::Codex, "sessions", only_project_id, data_dir)
}

/// Kimi: `sessions/<wd_id>/session_<uuid>/` one row per session (not per subagent wire).
/// Project cwd/title from `workspaces.json`; session title from `state.json`.
pub(crate) fn list_kimi_sessions(home: &Path, only_key: Option<&str>) -> Result<Vec<AgentSession>> {
    let root = home.join("sessions");
    if !root.is_dir() {
        return Ok(vec![]);
    }
    let workspaces = load_kimi_workspaces(home);
    let mut out = Vec::new();
    let entries = match fs::read_dir(&root) {
        Ok(e) => e,
        Err(_) => return Ok(vec![]),
    };
    for ent in entries.flatten() {
        let wd_dir = ent.path();
        if !wd_dir.is_dir() {
            continue;
        }
        let wd_id = match wd_dir.file_name().and_then(|n| n.to_str()) {
            Some(n) if !n.is_empty() && !n.starts_with('.') => n.to_string(),
            _ => continue,
        };
        let (key, cwd_opt, title_hint) = match workspaces.get(&wd_id) {
            Some(ws) => {
                let key = cwd_storage_key(&ws.root);
                (key, Some(ws.root.clone()), Some(ws.name.clone()))
            }
            None => (format!("ws/{wd_id}"), None, Some(wd_id.clone())),
        };
        if let Some(want) = only_key {
            if key != want {
                continue;
            }
        }
        let project_id = make_project_id(AgentId::Kimi, &key);
        let sess_entries = match fs::read_dir(&wd_dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for sess_ent in sess_entries.flatten() {
            let sess_path = sess_ent.path();
            if !sess_path.is_dir() {
                continue;
            }
            let sess_name = sess_path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if !sess_name.starts_with("session_") {
                continue;
            }
            if let Some(rec) = build_kimi_session(
                home,
                &sess_path,
                &project_id,
                cwd_opt.as_deref(),
                title_hint.as_deref(),
            ) {
                out.push(rec);
            }
        }
    }
    Ok(out)
}

#[derive(Clone)]
struct KimiWorkspaceMeta {
    root: String,
    name: String,
}

fn load_kimi_workspaces(home: &Path) -> BTreeMap<String, KimiWorkspaceMeta> {
    let path = home.join("workspaces.json");
    let mut map = BTreeMap::new();
    let Ok(raw) = fs::read_to_string(&path) else {
        return map;
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return map;
    };
    let Some(obj) = v.get("workspaces").and_then(|w| w.as_object()) else {
        return map;
    };
    for (id, meta) in obj {
        let root = meta
            .get("root")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string();
        if root.is_empty() {
            continue;
        }
        let name = meta
            .get("name")
            .and_then(|x| x.as_str())
            .map(|s| s.to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| {
                Path::new(&root)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(id.as_str())
                    .to_string()
            });
        map.insert(id.clone(), KimiWorkspaceMeta { root, name });
    }
    map
}

fn build_kimi_session(
    home: &Path,
    session_dir: &Path,
    project_id: &str,
    workspace_cwd: Option<&str>,
    workspace_title: Option<&str>,
) -> Option<AgentSession> {
    let state_path = session_dir.join("state.json");
    let main_wire = session_dir.join("agents").join("main").join("wire.jsonl");
    let primary = if main_wire.is_file() {
        main_wire
    } else if state_path.is_file() {
        state_path.clone()
    } else {
        // Fall back to any agent wire (first found).
        find_first_kimi_wire(session_dir).unwrap_or(state_path.clone())
    };
    if !primary.exists() {
        return None;
    }

    let (mut title, mut updated_from_state, state_cwd) = read_kimi_state(&state_path);
    let mut meta = if primary.extension().and_then(|e| e.to_str()) == Some("jsonl") {
        session_file_meta(AgentId::Kimi, &primary)
    } else {
        let (size_bytes, updated_at) = match fs::metadata(&primary) {
            Ok(m) => (
                m.len(),
                m.modified()
                    .ok()
                    .map(system_time_to_rfc3339)
                    .unwrap_or_else(|| Utc::now().to_rfc3339()),
            ),
            Err(_) => (0, Utc::now().to_rfc3339()),
        };
        SessionFileMeta {
            cwd: None,
            preview: None,
            message_count: None,
            size_bytes,
            updated_at,
            session_id: None,
        }
    };

    // Prefer workspace / state cwd over content.
    meta.cwd = workspace_cwd
        .map(|s| s.to_string())
        .or(state_cwd)
        .or(meta.cwd);

    if let Some(t) = title.take() {
        if !t.trim().is_empty() {
            // keep for title override after build
            title = Some(t);
        }
    }
    if let Some(u) = updated_from_state.take() {
        meta.updated_at = u;
    }

    // Size: prefer session dir shallow size for list realism.
    if let Some(sz) = dir_size_shallow(session_dir) {
        meta.size_bytes = sz;
    }
    // Prefer session dir name when content has no session id.
    if meta.session_id.is_none() {
        meta.session_id = native_session_id_from_path(AgentId::Kimi, session_dir);
    }

    let mut rec = build_session_from_meta(AgentId::Kimi, home, &primary, project_id, meta, None)?;
    if let Some(t) = title {
        rec.title = truncate_chars(t.trim(), 60);
    } else if let Some(wt) = workspace_title {
        // Only if preview didn't yield a better title
        if rec.preview.is_none() {
            rec.title = wt.to_string();
        }
    }
    rec.cwd = rec.cwd.or_else(|| workspace_cwd.map(|s| s.to_string()));
    if rec.session_id.is_none() {
        rec.session_id = native_session_id_from_path(AgentId::Kimi, session_dir);
    }
    Some(rec)
}

fn find_first_kimi_wire(session_dir: &Path) -> Option<PathBuf> {
    let agents = session_dir.join("agents");
    let rd = fs::read_dir(agents).ok()?;
    for ent in rd.flatten() {
        let wire = ent.path().join("wire.jsonl");
        if wire.is_file() {
            return Some(wire);
        }
    }
    None
}

fn read_kimi_state(path: &Path) -> (Option<String>, Option<String>, Option<String>) {
    let Ok(raw) = fs::read_to_string(path) else {
        return (None, None, None);
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return (None, None, None);
    };
    let title = v
        .get("title")
        .and_then(|x| x.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let updated = v
        .get("updatedAt")
        .or_else(|| v.get("updated_at"))
        .and_then(|x| x.as_str())
        .map(|s| s.to_string());
    let cwd = v
        .pointer("/cwd")
        .or_else(|| v.pointer("/workDir"))
        .or_else(|| v.pointer("/workdir"))
        .and_then(|x| x.as_str())
        .map(|s| s.to_string());
    (title, updated, cwd)
}

/// Pi: project = encoded dir under `agent/sessions/`; session = top-level `*.jsonl`.
pub(crate) fn list_pi_sessions(home: &Path, only_key: Option<&str>) -> Result<Vec<AgentSession>> {
    let root = home.join("agent").join("sessions");
    if !root.is_dir() {
        return Ok(vec![]);
    }
    let mut out = Vec::new();
    let entries = match fs::read_dir(&root) {
        Ok(e) => e,
        Err(_) => return Ok(vec![]),
    };
    for ent in entries.flatten() {
        let proj_dir = ent.path();
        if !proj_dir.is_dir() {
            continue;
        }
        let encoded = match proj_dir.file_name().and_then(|n| n.to_str()) {
            Some(n) if !n.is_empty() && !n.starts_with('.') => n.to_string(),
            _ => continue,
        };
        let (key, cwd_opt) = match decode_pi_session_dir(&encoded) {
            Some(decoded) => (cwd_storage_key(&decoded), Some(decoded)),
            None => (format!("dir/{encoded}"), None),
        };
        if let Some(want) = only_key {
            if key != want {
                continue;
            }
        }
        let project_id = make_project_id(AgentId::Pi, &key);
        let files = match fs::read_dir(&proj_dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for file_ent in files.flatten() {
            let path = file_ent.path();
            if !path.is_file() || !is_primary_session_file(AgentId::Pi, &path) {
                continue;
            }
            let mut meta = session_file_meta(AgentId::Pi, &path);
            if meta.cwd.is_none() {
                meta.cwd = cwd_opt.clone();
            }
            if let Some(rec) =
                build_session_from_meta(AgentId::Pi, home, &path, &project_id, meta, None)
            {
                out.push(rec);
            }
        }
    }
    Ok(out)
}

/// DSH: known persistence roots only (`sessions/`, `profiles/*/sessions`, `DSH_SESSION_ROOT`).
/// Never walks a random cwd `.sessions`.
pub(crate) fn list_dsh_sessions(
    home: &Path,
    only_project_id: Option<&str>,
    data_dir: Option<&Path>,
) -> Result<Vec<AgentSession>> {
    let mut out = list_sessions_tree(home, AgentId::Dsh, "sessions", only_project_id, data_dir)?;
    let profiles = home.join("profiles");
    if let Ok(entries) = fs::read_dir(&profiles) {
        for ent in entries.flatten() {
            let path = ent.path();
            if !path.is_dir() {
                continue;
            }
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if name.is_empty() || name.starts_with('.') {
                continue;
            }
            let sub = format!("profiles/{name}/sessions");
            out.extend(list_sessions_tree(
                home,
                AgentId::Dsh,
                &sub,
                only_project_id,
                data_dir,
            )?);
        }
    }
    if let Some(root) = first_env_path("DSH_SESSION_ROOT") {
        if root.is_dir() {
            if root.starts_with(home) {
                if let Ok(rel) = root.strip_prefix(home) {
                    let sub = rel.to_string_lossy().replace('\\', "/");
                    if !sub.is_empty() && sub != "sessions" && !sub.starts_with("profiles/") {
                        out.extend(list_sessions_tree(
                            home,
                            AgentId::Dsh,
                            &sub,
                            only_project_id,
                            data_dir,
                        )?);
                    }
                }
            } else {
                out.extend(list_sessions_tree(
                    &root,
                    AgentId::Dsh,
                    "",
                    only_project_id,
                    data_dir,
                )?);
            }
        }
    }
    Ok(out)
}

/// Grok: project = URL-encoded cwd directory under `sessions/`;
/// session = `<sessionId>/chat_history.jsonl` (sidecars ignored).
///
/// `only_key` is the project storage key (`cwd/...` or `__ungrouped__`).
pub(crate) fn list_grok_sessions(home: &Path, only_key: Option<&str>) -> Result<Vec<AgentSession>> {
    let root = home.join("sessions");
    if !root.is_dir() {
        return Ok(vec![]);
    }
    let mut out = Vec::new();
    let entries = match fs::read_dir(&root) {
        Ok(e) => e,
        Err(_) => return Ok(vec![]),
    };
    for ent in entries.flatten() {
        let proj_dir = ent.path();
        if !proj_dir.is_dir() {
            continue;
        }
        let encoded = match proj_dir.file_name().and_then(|n| n.to_str()) {
            Some(n) if !n.is_empty() && !n.starts_with('.') => n.to_string(),
            _ => continue,
        };
        // Skip non-session top-level noise (e.g. sqlite lives as file; dirs only here).
        let (key, cwd_opt) = grok_project_key_from_dir_name(&encoded);
        if let Some(want) = only_key {
            if key != want {
                continue;
            }
        }
        let project_id = make_project_id(AgentId::Grok, &key);
        let session_entries = match fs::read_dir(&proj_dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for sess_ent in session_entries.flatten() {
            let sess_path = sess_ent.path();
            // Flat chat_history under project dir (rare) or session-id subdir.
            if sess_path.is_file() {
                if is_primary_session_file(AgentId::Grok, &sess_path) {
                    if let Some(rec) =
                        build_grok_session(home, &sess_path, &project_id, cwd_opt.as_deref(), None)
                    {
                        out.push(rec);
                    }
                }
                continue;
            }
            if !sess_path.is_dir() {
                continue;
            }
            let session_id_name = sess_path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if session_id_name.is_empty() || session_id_name.starts_with('.') {
                continue;
            }
            let chat = sess_path.join("chat_history.jsonl");
            if !chat.is_file() {
                // Nested subagents etc. without primary transcript — skip.
                continue;
            }
            if let Some(rec) = build_grok_session(
                home,
                &chat,
                &project_id,
                cwd_opt.as_deref(),
                Some(&sess_path),
            ) {
                out.push(rec);
            }
        }
    }
    Ok(out)
}

fn grok_project_key_from_dir_name(encoded: &str) -> (String, Option<String>) {
    if let Some(decoded) = percent_decode_path(encoded) {
        if looks_like_path(&decoded) {
            return (cwd_storage_key(&decoded), Some(decoded));
        }
    }
    // Fallback: treat folder name as opaque storage key (not ungrouped if named).
    if encoded == UNGROUPED_KEY {
        return (UNGROUPED_KEY.to_string(), None);
    }
    // Non-encoded folder: still a distinct project bucket.
    (format!("dir/{encoded}"), None)
}

fn looks_like_path(s: &str) -> bool {
    let t = s.trim();
    if t.is_empty() {
        return false;
    }
    let b = t.as_bytes();
    // Windows drive or UNC / Unix absolute
    (b.len() >= 2 && b[1] == b':' && b[0].is_ascii_alphabetic())
        || t.starts_with('/')
        || t.starts_with("\\\\")
}

/// Generic recursive session walk under `{home}/{subdir}`, grouped by cwd.
/// Used by Codex / Kimi / Pi. `only_project_id` filters after grouping key is known.
///
/// When `data_dir` is set, uses mtime/size index to skip re-parsing unchanged files.
fn list_sessions_tree(
    home: &Path,
    agent: AgentId,
    subdir: &str,
    only_project_id: Option<&str>,
    data_dir: Option<&Path>,
) -> Result<Vec<AgentSession>> {
    let root = home.join(subdir);
    if !root.is_dir() {
        return Ok(vec![]);
    }
    let mut index = data_dir.map(SessionIndexStore::load);
    let mut seen: HashSet<String> = HashSet::new();
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(dir) = stack.pop() {
        let entries = match fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for ent in entries.flatten() {
            let path = ent.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if !is_primary_session_file(agent, &path) {
                continue;
            }
            let rel = match path.strip_prefix(home) {
                Ok(r) => path_to_rel(r),
                Err(_) => continue,
            };
            seen.insert(rel.clone());

            let (size, mtime_ms, updated_at) = match file_size_mtime(&path) {
                Some((sz, ms, st)) => (sz, ms, system_time_to_rfc3339(st)),
                None => continue,
            };

            // Index hit: rebuild session without reading file body.
            if let Some(store) = index.as_ref() {
                if let Some(ent) = store.get_fresh(agent, &rel, size, mtime_ms) {
                    let project_id = make_project_id(agent, &ent.project_key);
                    if let Some(want) = only_project_id {
                        if project_id != want {
                            continue;
                        }
                    }
                    out.push(AgentSession {
                        id: format!("{}:{}", agent.as_str(), rel),
                        project_id,
                        agent_id: agent,
                        title: ent.title.clone(),
                        cwd: ent.cwd.clone(),
                        path: path.display().to_string(),
                        relative_path: rel,
                        size_bytes: size,
                        updated_at: ent.updated_at.clone(),
                        preview: ent.preview.clone(),
                        message_count: ent.message_count,
                        session_id: ent.session_id.clone(),
                    });
                    continue;
                }
            }

            // Miss: single-pass head parse, then cache.
            let meta = session_file_meta(agent, &path);
            let key = match meta.cwd.as_deref() {
                Some(c) if !c.is_empty() => cwd_storage_key(c),
                _ => UNGROUPED_KEY.to_string(),
            };
            let project_id = make_project_id(agent, &key);
            if let Some(want) = only_project_id {
                if project_id != want {
                    // Still cache for full-list reuse.
                    if let Some(store) = index.as_mut() {
                        let title = title_from(meta.preview.as_deref(), meta.cwd.as_deref(), &path);
                        store.put(
                            agent,
                            &rel,
                            IndexEntry {
                                mtime_ms,
                                size,
                                project_key: key,
                                cwd: meta.cwd,
                                title,
                                preview: meta.preview,
                                message_count: meta.message_count,
                                updated_at,
                                session_id: meta.session_id,
                            },
                        );
                    }
                    continue;
                }
            }
            if let Some(store) = index.as_mut() {
                let title = title_from(meta.preview.as_deref(), meta.cwd.as_deref(), &path);
                store.put(
                    agent,
                    &rel,
                    IndexEntry {
                        mtime_ms,
                        size,
                        project_key: key.clone(),
                        cwd: meta.cwd.clone(),
                        title,
                        preview: meta.preview.clone(),
                        message_count: meta.message_count,
                        updated_at: meta.updated_at.clone(),
                        session_id: meta.session_id.clone(),
                    },
                );
            }
            if let Some(rec) = build_session_from_meta(agent, home, &path, &project_id, meta, None)
            {
                out.push(rec);
            }
        }
    }
    if let Some(store) = index.as_mut() {
        // Full scans prune deleted files; filtered scans keep other keys.
        if only_project_id.is_none() {
            store.retain_only(agent, &seen);
        }
        store.save_if_dirty();
    }
    Ok(out)
}

/// If `path` is a Grok primary transcript (or its session dir), return the session dir to wipe.
pub(crate) fn grok_session_dir_for_delete(path: &Path) -> Option<PathBuf> {
    if path.is_dir() {
        // Session id directory containing chat_history.jsonl
        if path.join("chat_history.jsonl").is_file() {
            return Some(path.to_path_buf());
        }
        return None;
    }
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if name.eq_ignore_ascii_case("chat_history.jsonl") {
        return path.parent().map(|p| p.to_path_buf());
    }
    None
}

/// Walk up until a `session_*` directory (Kimi layout).
pub(crate) fn kimi_session_dir_for_delete(path: &Path) -> Option<PathBuf> {
    let mut cur = path.to_path_buf();
    for _ in 0..8 {
        if let Some(name) = cur.file_name().and_then(|n| n.to_str()) {
            if name.starts_with("session_") && cur.is_dir() {
                return Some(cur);
            }
            if name.starts_with("session_") {
                // file under session dir? unlikely
            }
        }
        // If current is a file, check parent names.
        let parent = cur.parent()?.to_path_buf();
        if let Some(name) = parent.file_name().and_then(|n| n.to_str()) {
            if name.starts_with("session_") {
                return Some(parent);
            }
        }
        cur = parent;
    }
    None
}

struct SessionFileMeta {
    cwd: Option<String>,
    preview: Option<String>,
    message_count: Option<u32>,
    size_bytes: u64,
    updated_at: String,
    /// Native CLI session id when known from content / path.
    session_id: Option<String>,
}

fn session_file_meta(agent: AgentId, path: &Path) -> SessionFileMeta {
    let (size_bytes, updated_at) = match fs::metadata(path) {
        Ok(m) => (
            m.len(),
            m.modified()
                .ok()
                .map(system_time_to_rfc3339)
                .unwrap_or_else(|| Utc::now().to_rfc3339()),
        ),
        Err(_) => (0, Utc::now().to_rfc3339()),
    };
    let text = read_head(path, SCAN_BYTES).unwrap_or_default();
    let cwd = extract_cwd_from_text(agent, &text);
    let (preview, message_count) = scan_preview_from_text(&text);
    let session_id = extract_native_session_id(agent, path, &text);
    SessionFileMeta {
        cwd,
        preview,
        message_count,
        size_bytes,
        updated_at,
        session_id,
    }
}

/// Native CLI session id: content field first, then path heuristics.
fn extract_native_session_id(agent: AgentId, path: &Path, text: &str) -> Option<String> {
    if let Some(sid) = extract_session_id_from_text(agent, text) {
        return Some(sid);
    }
    native_session_id_from_path(agent, path)
}

fn extract_session_id_from_text(agent: AgentId, text: &str) -> Option<String> {
    for line in text.lines().take(40) {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if let Some(s) = session_id_from_json_value(agent, &v) {
            return Some(s);
        }
    }
    None
}

fn session_id_from_json_value(agent: AgentId, v: &serde_json::Value) -> Option<String> {
    let non_empty = |s: &str| {
        let t = s.trim();
        if t.is_empty() {
            None
        } else {
            Some(t.to_string())
        }
    };

    // Top-level keys common across Claude-like / Grok / Pi logs.
    for key in ["sessionId", "session_id", "sessionID"] {
        if let Some(s) = v.get(key).and_then(|x| x.as_str()).and_then(non_empty) {
            return Some(s);
        }
    }

    let ty = v.get("type").and_then(|x| x.as_str()).unwrap_or("");
    match agent {
        AgentId::Codex => {
            // Prefer explicit session_id; only use payload.id on session_meta
            // (other events use payload.id for message / item ids).
            if let Some(s) = v
                .pointer("/payload/session_id")
                .or_else(|| v.pointer("/payload/sessionId"))
                .and_then(|x| x.as_str())
                .and_then(non_empty)
            {
                return Some(s);
            }
            if ty == "session_meta" {
                if let Some(s) = v
                    .pointer("/payload/id")
                    .and_then(|x| x.as_str())
                    .and_then(non_empty)
                {
                    return Some(s);
                }
            }
            None
        }
        AgentId::Grok => v
            .pointer("/info/id")
            .or_else(|| v.pointer("/payload/session_id"))
            .and_then(|x| x.as_str())
            .and_then(non_empty),
        AgentId::Pi => {
            // Pi session header: {"type":"session","id":"…"} — avoid message ids.
            if ty == "session" {
                return v.get("id").and_then(|x| x.as_str()).and_then(non_empty);
            }
            None
        }
        AgentId::Kimi => None, // path `session_<uuid>` is the source of truth
        AgentId::Dsh => v
            .get("id")
            .or_else(|| v.pointer("/header/id"))
            .and_then(|x| x.as_str())
            .and_then(non_empty),
        // Claude / WorkBuddy / default
        _ => v
            .pointer("/session/id")
            .or_else(|| v.pointer("/message/sessionId"))
            .and_then(|x| x.as_str())
            .and_then(non_empty),
    }
}

/// Path-derived native id when content has no field.
fn native_session_id_from_path(agent: AgentId, path: &Path) -> Option<String> {
    match agent {
        AgentId::Claude | AgentId::WorkBuddy => {
            // `projects/<encoded>/<uuid>.jsonl` → stem is the session uuid
            path.file_stem()
                .and_then(|s| s.to_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
        }
        AgentId::Codex => {
            // Prefer `rollout-…-<sessionId>` tail; else whole stem.
            let stem = path.file_stem().and_then(|s| s.to_str())?.trim();
            if stem.is_empty() {
                return None;
            }
            if let Some(rest) = stem.strip_prefix("rollout-") {
                if let Some((_, tail)) = rest.rsplit_once('-') {
                    if !tail.is_empty() {
                        return Some(tail.to_string());
                    }
                }
            }
            Some(stem.to_string())
        }
        AgentId::Grok => {
            // Prefer parent dir when file is chat_history.jsonl
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name.eq_ignore_ascii_case("chat_history.jsonl") {
                return path
                    .parent()
                    .and_then(|p| p.file_name())
                    .and_then(|n| n.to_str())
                    .map(str::trim)
                    .filter(|s| !s.is_empty() && *s != "." && *s != "..")
                    .map(|s| s.to_string());
            }
            path.file_stem()
                .and_then(|s| s.to_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
        }
        AgentId::Kimi => {
            // Prefer `session_<uuid>` directory; strip prefix for CLI-native uuid.
            let mut cur = path.to_path_buf();
            for _ in 0..8 {
                if let Some(name) = cur.file_name().and_then(|n| n.to_str()) {
                    if let Some(rest) = name.strip_prefix("session_") {
                        let t = rest.trim();
                        if !t.is_empty() {
                            return Some(t.to_string());
                        }
                        return Some(name.to_string());
                    }
                    if name.starts_with("session") && name.len() > 7 {
                        return Some(name.to_string());
                    }
                }
                match cur.parent() {
                    Some(p) if p != cur => cur = p.to_path_buf(),
                    _ => break,
                }
            }
            None
        }
        AgentId::Pi => {
            // Filename often `agent_<sessionId>.jsonl` → take after first `_`
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                if let Some((_, sid)) = stem.split_once('_') {
                    if !sid.is_empty() {
                        return Some(sid.to_string());
                    }
                }
                if !stem.is_empty() {
                    return Some(stem.to_string());
                }
            }
            None
        }
        AgentId::Cursor => None,
        AgentId::Dsh => path
            .file_stem()
            .and_then(|s| s.to_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string()),
    }
}

fn build_session_from_meta(
    agent: AgentId,
    home: &Path,
    path: &Path,
    project_id: &str,
    meta: SessionFileMeta,
    encoded_dir: Option<&str>,
) -> Option<AgentSession> {
    let rel = path.strip_prefix(home).ok()?;
    let rel_str = path_to_rel(rel);
    let cwd = meta.cwd.or_else(|| match agent {
        AgentId::Claude | AgentId::WorkBuddy => {
            if let Some(encoded) = encoded_dir {
                if let Some(v) = verified_actual_path(encoded) {
                    return Some(v);
                }
                return decode_claude_project_dir(encoded);
            }
            None
        }
        _ => None,
    });
    let title = title_from(meta.preview.as_deref(), cwd.as_deref(), path);
    let session_id = meta
        .session_id
        .or_else(|| native_session_id_from_path(agent, path));
    Some(AgentSession {
        id: format!("{}:{}", agent.as_str(), rel_str),
        project_id: project_id.to_string(),
        agent_id: agent,
        title,
        cwd,
        path: path.display().to_string(),
        relative_path: rel_str,
        size_bytes: meta.size_bytes,
        updated_at: meta.updated_at,
        preview: meta.preview,
        message_count: meta.message_count,
        session_id,
    })
}

fn build_grok_session(
    home: &Path,
    chat_path: &Path,
    project_id: &str,
    dir_cwd: Option<&str>,
    session_dir: Option<&Path>,
) -> Option<AgentSession> {
    let mut meta = session_file_meta(AgentId::Grok, chat_path);
    // Prefer directory-decoded cwd, then summary.json, then content.
    if meta.cwd.is_none() {
        meta.cwd = dir_cwd.map(|s| s.to_string());
    }
    let mut title_override: Option<String> = None;
    let mut count_override: Option<u32> = None;
    if let Some(dir) = session_dir {
        if let Some((title, cwd, count, sid)) = read_grok_summary(dir) {
            if title_override.is_none() {
                title_override = title;
            }
            if meta.cwd.is_none() {
                meta.cwd = cwd;
            }
            count_override = count.or(count_override);
            // Prefer directory name (native layout); summary id is often truncated.
            if meta.session_id.is_none() {
                meta.session_id = sid;
            }
        }
        // Directory name is the stable Grok session id.
        if let Some(name) = dir
            .file_name()
            .and_then(|n| n.to_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
        {
            meta.session_id = Some(name);
        }
    }
    if let Some(c) = count_override {
        meta.message_count = Some(c);
    }
    let mut rec = build_session_from_meta(AgentId::Grok, home, chat_path, project_id, meta, None)?;
    if let Some(t) = title_override {
        if !t.trim().is_empty() {
            rec.title = truncate_chars(t.trim(), 60);
        }
    }
    // Prefer dir cwd for stability even if content had something else empty.
    if rec.cwd.is_none() {
        rec.cwd = dir_cwd.map(|s| s.to_string());
    }
    Some(rec)
}

fn read_grok_summary(
    session_dir: &Path,
) -> Option<(Option<String>, Option<String>, Option<u32>, Option<String>)> {
    let path = session_dir.join("summary.json");
    let raw = fs::read_to_string(&path).ok()?;
    let v: serde_json::Value = serde_json::from_str(&raw).ok()?;
    let title = v
        .get("generated_title")
        .or_else(|| v.get("session_summary"))
        .and_then(|x| x.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let cwd = v
        .pointer("/info/cwd")
        .and_then(|x| x.as_str())
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty());
    let count = v
        .get("num_chat_messages")
        .or_else(|| v.get("num_messages"))
        .and_then(|x| x.as_u64())
        .map(|n| n as u32);
    let session_id = v
        .pointer("/info/id")
        .or_else(|| v.get("session_id"))
        .or_else(|| v.get("sessionId"))
        .or_else(|| v.get("id"))
        .and_then(|x| x.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    Some((title, cwd, count, session_id))
}

/// Percent-decode a path segment (Grok session parent dirs).
fn percent_decode_path(input: &str) -> Option<String> {
    let bytes = input.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let h = std::str::from_utf8(&bytes[i + 1..i + 3]).ok()?;
                let v = u8::from_str_radix(h, 16).ok()?;
                out.push(v);
                i += 3;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8(out).ok()
}

pub(crate) fn aggregate_projects(
    agent: AgentId,
    home: &Path,
    sessions: &[AgentSession],
) -> Vec<AgentProject> {
    let mut buckets: BTreeMap<String, Vec<&AgentSession>> = BTreeMap::new();
    for s in sessions {
        buckets.entry(s.project_id.clone()).or_default().push(s);
    }
    let mut out = Vec::with_capacity(buckets.len());
    for (project_id, group) in buckets {
        let Some((_, key)) = parse_project_id(&project_id).ok() else {
            continue;
        };
        let (storage_path, relative_path, actual_path, title) =
            project_paths(agent, home, &key, &group);
        let session_count = group.len() as u32;
        let size_bytes = group
            .iter()
            .map(|s| s.size_bytes)
            .fold(0u64, |a, b| a.saturating_add(b));
        let message_count = {
            let sum: u32 = group.iter().filter_map(|s| s.message_count).sum();
            if sum > 0 {
                Some(sum)
            } else {
                None
            }
        };
        let updated_at = group
            .iter()
            .map(|s| s.updated_at.as_str())
            .max()
            .unwrap_or("")
            .to_string();
        let preview = group
            .iter()
            .max_by_key(|s| s.updated_at.as_str())
            .and_then(|s| s.preview.clone());
        out.push(AgentProject {
            id: project_id,
            agent_id: agent,
            title,
            storage_path,
            actual_path,
            relative_path,
            session_count,
            message_count,
            size_bytes,
            updated_at,
            preview,
            alias: None,
            hidden: false,
        });
    }
    out.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    out
}

fn project_paths(
    agent: AgentId,
    home: &Path,
    key: &str,
    group: &[&AgentSession],
) -> (String, String, Option<String>, String) {
    match agent {
        AgentId::Claude | AgentId::WorkBuddy => {
            let storage = home.join("projects").join(key);
            let relative = format!("projects/{key}");
            let decoded = decode_claude_project_dir(key);
            let mut actual = verified_actual_path(key);
            if actual.is_none() {
                actual = group
                    .iter()
                    .find_map(|s| s.cwd.clone())
                    .filter(|c| !c.is_empty() && Path::new(c).exists());
            }
            // Prefer native separators when the path exists (open-in-explorer friendly).
            if let Some(ref a) = actual {
                if Path::new(a).exists() {
                    #[cfg(windows)]
                    {
                        actual = Some(a.replace('/', "\\"));
                    }
                }
            }
            let title = title_from_actual(actual.as_deref().or(decoded.as_deref()), key);
            (storage.display().to_string(), relative, actual, title)
        }
        _ => {
            if key == UNGROUPED_KEY {
                let storage = home.join("sessions");
                (
                    storage.display().to_string(),
                    "sessions".into(),
                    None,
                    "未分类会话".into(),
                )
            } else if let Some(dir_name) = key.strip_prefix("dir/") {
                // Grok (or others): opaque sessions/<dir_name>/ bucket.
                let storage = home.join("sessions").join(dir_name);
                let title = title_from_actual(None, dir_name);
                (
                    storage.display().to_string(),
                    format!("sessions/{dir_name}"),
                    None,
                    title,
                )
            } else {
                let fwd = key.strip_prefix("cwd/").unwrap_or(key).to_string();
                let cwd_win = fwd.replace('/', "\\");
                // Prefer a form that exists on disk; on Windows surface native `\` so
                // "open in explorer" works without extra client normalization.
                let actual = {
                    if Path::new(&cwd_win).exists() {
                        Some(cwd_win)
                    } else if Path::new(&fwd).exists() {
                        Some(fwd.replace('/', std::path::MAIN_SEPARATOR_STR))
                    } else {
                        None
                    }
                };
                let title = title_from_actual(
                    actual
                        .as_deref()
                        .or((!fwd.is_empty()).then_some(fwd.as_str())),
                    key,
                );
                // Prefer project-level storage dirs for nested session trees.
                let storage = match agent {
                    AgentId::Grok => group
                        .first()
                        .and_then(|s| {
                            // chat_history → session_id dir → project dir
                            Path::new(&s.path)
                                .parent()
                                .and_then(|p| p.parent())
                                .map(|p| p.display().to_string())
                        })
                        .unwrap_or_else(|| home.join("sessions").display().to_string()),
                    AgentId::Kimi => group
                        .first()
                        .and_then(|s| {
                            // .../sessions/wd_*/session_*/agents/main/wire.jsonl → wd dir
                            let mut p = Path::new(&s.path);
                            for _ in 0..4 {
                                p = p.parent()?;
                            }
                            Some(p.display().to_string())
                        })
                        .unwrap_or_else(|| home.join("sessions").display().to_string()),
                    AgentId::Pi => group
                        .first()
                        .and_then(|s| Path::new(&s.path).parent().map(|p| p.display().to_string()))
                        .unwrap_or_else(|| {
                            home.join("agent").join("sessions").display().to_string()
                        }),
                    _ => group
                        .first()
                        .map(|s| {
                            Path::new(&s.path)
                                .parent()
                                .map(|p| p.display().to_string())
                                .unwrap_or_else(|| s.path.clone())
                        })
                        .unwrap_or_else(|| home.display().to_string()),
                };
                let relative = match agent {
                    AgentId::Grok | AgentId::Kimi | AgentId::Pi => group
                        .first()
                        .and_then(|s| {
                            let p = Path::new(&s.relative_path);
                            let mut comps = p.components();
                            let a = comps.next()?.as_os_str().to_string_lossy().into_owned();
                            let b = comps.next()?.as_os_str().to_string_lossy().into_owned();
                            // Pi: agent/sessions → need 3 components for encoded dir
                            if agent == AgentId::Pi {
                                let c = comps.next()?.as_os_str().to_string_lossy().into_owned();
                                Some(format!("{a}/{b}/{c}"))
                            } else {
                                Some(format!("{a}/{b}"))
                            }
                        })
                        .unwrap_or_else(|| key.to_string()),
                    _ => key.to_string(),
                };
                (storage, relative, actual, title)
            }
        }
    }
}

fn title_from_actual(actual: Option<&str>, fallback: &str) -> String {
    if let Some(c) = actual {
        if let Some(name) = Path::new(c).file_name().and_then(|n| n.to_str()) {
            if !name.is_empty() {
                return name.to_string();
            }
        }
    }
    if fallback == UNGROUPED_KEY {
        return "未分类会话".into();
    }
    fallback.rsplit('/').next().unwrap_or(fallback).to_string()
}

pub(crate) fn list_cursor_projects(home: &Path) -> Result<Vec<AgentProject>> {
    let root = home.join("projects");
    if !root.is_dir() {
        return Ok(vec![]);
    }
    let mut out = Vec::new();
    let entries = match fs::read_dir(&root) {
        Ok(e) => e,
        Err(_) => return Ok(vec![]),
    };
    for ent in entries.flatten() {
        let path = ent.path();
        if !path.is_dir() {
            continue;
        }
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        if name.is_empty() || name.starts_with('.') {
            continue;
        }
        let meta = match fs::metadata(&path) {
            Ok(m) => m,
            Err(_) => continue,
        };
        let updated = meta
            .modified()
            .ok()
            .map(system_time_to_rfc3339)
            .unwrap_or_else(|| Utc::now().to_rfc3339());
        let size = dir_size_shallow(&path).unwrap_or(0);
        let decoded = cursor_actual_path(&name);
        let actual = decoded.as_ref().filter(|p| Path::new(p).exists()).cloned();
        let title = title_from_actual(decoded.as_deref(), &name);
        let rel_str = format!("projects/{name}");
        out.push(AgentProject {
            id: make_project_id(AgentId::Cursor, &name),
            agent_id: AgentId::Cursor,
            title,
            storage_path: path.display().to_string(),
            actual_path: actual,
            relative_path: rel_str,
            session_count: 0,
            message_count: None,
            size_bytes: size,
            updated_at: updated,
            preview: None,
            alias: None,
            hidden: false,
        });
    }
    out.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(out)
}

/// Directory/stat listing for Claude + WorkBuddy. Peeks at most [`LIST_HEAD_BYTES`]
/// of the newest primary file for cwd/preview — never [`SCAN_BYTES`] × every session.
pub(crate) fn list_claude_workbuddy_projects(
    home: &Path,
    agent: AgentId,
) -> Result<Vec<AgentProject>> {
    let root = home.join("projects");
    if !root.is_dir() {
        return Ok(vec![]);
    }
    let mut out = Vec::new();
    let entries = match fs::read_dir(&root) {
        Ok(e) => e,
        Err(_) => return Ok(vec![]),
    };
    for ent in entries.flatten() {
        let dir = ent.path();
        if !dir.is_dir() {
            continue;
        }
        let encoded = dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        if encoded.is_empty() || encoded.starts_with('.') {
            continue;
        }
        let files = match fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        let mut session_count = 0u32;
        let mut size_bytes = 0u64;
        let mut newest_mtime: Option<SystemTime> = None;
        let mut newest_path: Option<PathBuf> = None;
        for file_ent in files.flatten() {
            let path = file_ent.path();
            if !path.is_file() || !is_primary_session_file(agent, &path) {
                continue;
            }
            let Ok(meta) = fs::metadata(&path) else {
                continue;
            };
            session_count = session_count.saturating_add(1);
            size_bytes = size_bytes.saturating_add(meta.len());
            if let Ok(mtime) = meta.modified() {
                if newest_mtime.map(|t| mtime >= t).unwrap_or(true) {
                    newest_mtime = Some(mtime);
                    newest_path = Some(path);
                }
            } else if newest_path.is_none() {
                newest_path = Some(path);
            }
        }
        if session_count == 0 {
            continue;
        }
        let decoded = decode_claude_project_dir(&encoded);
        let mut actual = verified_actual_path(&encoded);
        let mut preview = None;
        if let Some(path) = newest_path.as_deref() {
            let text = read_head(path, LIST_HEAD_BYTES).unwrap_or_default();
            if actual.is_none() {
                actual = extract_cwd_from_text(agent, &text)
                    .filter(|c| !c.is_empty() && Path::new(c).exists());
            }
            preview = scan_preview_from_text(&text).0;
        }
        actual = native_existing_path(actual);
        let title = title_from_actual(actual.as_deref().or(decoded.as_deref()), &encoded);
        out.push(cheap_project(
            agent,
            &encoded,
            title,
            dir.display().to_string(),
            actual,
            format!("projects/{encoded}"),
            session_count,
            size_bytes,
            rfc3339_mtime(newest_mtime),
            preview,
        ));
    }
    out.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(out)
}

/// Grok: one project per `sessions/<encoded>/` dir. Stats `chat_history.jsonl` only.
pub(crate) fn list_grok_projects(home: &Path) -> Result<Vec<AgentProject>> {
    let root = home.join("sessions");
    if !root.is_dir() {
        return Ok(vec![]);
    }
    let mut out = Vec::new();
    let entries = match fs::read_dir(&root) {
        Ok(e) => e,
        Err(_) => return Ok(vec![]),
    };
    for ent in entries.flatten() {
        let proj_dir = ent.path();
        if !proj_dir.is_dir() {
            continue;
        }
        let encoded = match proj_dir.file_name().and_then(|n| n.to_str()) {
            Some(n) if !n.is_empty() && !n.starts_with('.') => n.to_string(),
            _ => continue,
        };
        let (key, cwd_opt) = grok_project_key_from_dir_name(&encoded);
        let mut session_count = 0u32;
        let mut size_bytes = 0u64;
        let mut newest: Option<SystemTime> = None;
        let session_entries = match fs::read_dir(&proj_dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for sess_ent in session_entries.flatten() {
            let sess_path = sess_ent.path();
            if sess_path.is_file() {
                if is_primary_session_file(AgentId::Grok, &sess_path) {
                    add_file_stat(&sess_path, &mut session_count, &mut size_bytes, &mut newest);
                }
                continue;
            }
            if !sess_path.is_dir() {
                continue;
            }
            let session_id_name = sess_path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if session_id_name.is_empty() || session_id_name.starts_with('.') {
                continue;
            }
            let chat = sess_path.join("chat_history.jsonl");
            if chat.is_file() {
                add_file_stat(&chat, &mut session_count, &mut size_bytes, &mut newest);
            }
        }
        if session_count == 0 {
            continue;
        }
        let actual = native_existing_path(cwd_opt.clone());
        let title = title_from_actual(actual.as_deref().or(cwd_opt.as_deref()), &key);
        out.push(cheap_project(
            AgentId::Grok,
            &key,
            title,
            proj_dir.display().to_string(),
            actual,
            format!("sessions/{encoded}"),
            session_count,
            size_bytes,
            rfc3339_mtime(newest),
            None,
        ));
    }
    out.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(out)
}

/// Kimi: group `sessions/<wd_id>/session_*` dirs using `workspaces.json`. No `wire.jsonl` reads.
pub(crate) fn list_kimi_projects(home: &Path) -> Result<Vec<AgentProject>> {
    let root = home.join("sessions");
    if !root.is_dir() {
        return Ok(vec![]);
    }
    let workspaces = load_kimi_workspaces(home);
    let mut buckets: BTreeMap<String, CheapAcc> = BTreeMap::new();
    let entries = match fs::read_dir(&root) {
        Ok(e) => e,
        Err(_) => return Ok(vec![]),
    };
    for ent in entries.flatten() {
        let wd_dir = ent.path();
        if !wd_dir.is_dir() {
            continue;
        }
        let wd_id = match wd_dir.file_name().and_then(|n| n.to_str()) {
            Some(n) if !n.is_empty() && !n.starts_with('.') => n.to_string(),
            _ => continue,
        };
        let (key, cwd_opt, title_hint) = match workspaces.get(&wd_id) {
            Some(ws) => (
                cwd_storage_key(&ws.root),
                Some(ws.root.clone()),
                Some(ws.name.clone()),
            ),
            // Fallback key is workspace-id based (`ws/{wd_id}`), NOT the
            // cwd-based `cwd_storage_key` used when the workspace resolves.
            // If a workspace later appears in `workspaces`, the project gets a
            // different id and any frontend-persisted alias/hide state bound to
            // `ws/{wd_id}` will not follow. Changing this key risks breaking
            // existing persisted aliases, so it stays as-is by design.
            None => (format!("ws/{wd_id}"), None, Some(wd_id.clone())),
        };
        let sess_entries = match fs::read_dir(&wd_dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        let mut session_count = 0u32;
        let mut size_bytes = 0u64;
        let mut newest: Option<SystemTime> = None;
        for sess_ent in sess_entries.flatten() {
            let sess_path = sess_ent.path();
            if !sess_path.is_dir() {
                continue;
            }
            let sess_name = sess_path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if !sess_name.starts_with("session_") {
                continue;
            }
            session_count = session_count.saturating_add(1);
            size_bytes = size_bytes.saturating_add(dir_size_shallow(&sess_path).unwrap_or(0));
            bump_mtime(&mut newest, mtime_of(&sess_path));
            bump_mtime(&mut newest, mtime_of(&sess_path.join("state.json")));
        }
        if session_count == 0 {
            continue;
        }
        let actual = native_existing_path(cwd_opt.clone());
        let title = title_hint
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| title_from_actual(actual.as_deref().or(cwd_opt.as_deref()), &key));
        match buckets.get_mut(&key) {
            Some(acc) => acc.merge(session_count, size_bytes, newest),
            None => {
                buckets.insert(
                    key,
                    CheapAcc {
                        title,
                        storage_path: wd_dir.display().to_string(),
                        actual_path: actual,
                        relative_path: format!("sessions/{wd_id}"),
                        session_count,
                        size_bytes,
                        newest,
                    },
                );
            }
        }
    }
    let mut out: Vec<AgentProject> = buckets
        .into_iter()
        .map(|(key, acc)| acc.into_project(AgentId::Kimi, &key))
        .collect();
    out.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(out)
}

/// Pi: one (or merged) project per encoded dir under `agent/sessions/`. Metadata only.
pub(crate) fn list_pi_projects(home: &Path) -> Result<Vec<AgentProject>> {
    let root = home.join("agent").join("sessions");
    if !root.is_dir() {
        return Ok(vec![]);
    }
    let mut buckets: BTreeMap<String, CheapAcc> = BTreeMap::new();
    let entries = match fs::read_dir(&root) {
        Ok(e) => e,
        Err(_) => return Ok(vec![]),
    };
    for ent in entries.flatten() {
        let proj_dir = ent.path();
        if !proj_dir.is_dir() {
            continue;
        }
        let encoded = match proj_dir.file_name().and_then(|n| n.to_str()) {
            Some(n) if !n.is_empty() && !n.starts_with('.') => n.to_string(),
            _ => continue,
        };
        let (key, cwd_opt) = match decode_pi_session_dir(&encoded) {
            Some(decoded) => (cwd_storage_key(&decoded), Some(decoded)),
            None => (format!("dir/{encoded}"), None),
        };
        let files = match fs::read_dir(&proj_dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        let mut session_count = 0u32;
        let mut size_bytes = 0u64;
        let mut newest: Option<SystemTime> = None;
        for file_ent in files.flatten() {
            let path = file_ent.path();
            if !path.is_file() || !is_primary_session_file(AgentId::Pi, &path) {
                continue;
            }
            add_file_stat(&path, &mut session_count, &mut size_bytes, &mut newest);
        }
        if session_count == 0 {
            continue;
        }
        let actual = native_existing_path(cwd_opt.clone());
        let title = title_from_actual(actual.as_deref().or(cwd_opt.as_deref()), &key);
        match buckets.get_mut(&key) {
            Some(acc) => acc.merge(session_count, size_bytes, newest),
            None => {
                buckets.insert(
                    key,
                    CheapAcc {
                        title,
                        storage_path: proj_dir.display().to_string(),
                        actual_path: actual,
                        relative_path: format!("agent/sessions/{encoded}"),
                        session_count,
                        size_bytes,
                        newest,
                    },
                );
            }
        }
    }
    let mut out: Vec<AgentProject> = buckets
        .into_iter()
        .map(|(key, acc)| acc.into_project(AgentId::Pi, &key))
        .collect();
    out.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(out)
}

/// Metadata-only accumulator for the "cheap" fast-list paths
/// (`list_grok_projects` / `list_kimi_projects` / `list_pi_projects`).
///
/// Known, intentional tradeoffs vs the full [`aggregate_projects`] path:
/// - `message_count` stays `None` and `preview` stays `None`: recovering them
///   would require parsing session files, which defeats the purpose of these
///   O(stat) fast listings used to keep stale lists responsive while paging.
/// - `updated_at` derives from file mtimes (`rfc3339_mtime`), not from
///   session-record timestamps, so it can drift from real activity time when
///   files are copied/touched.
struct CheapAcc {
    title: String,
    storage_path: String,
    actual_path: Option<String>,
    relative_path: String,
    session_count: u32,
    size_bytes: u64,
    newest: Option<SystemTime>,
}

impl CheapAcc {
    fn merge(&mut self, session_count: u32, size_bytes: u64, newest: Option<SystemTime>) {
        self.session_count = self.session_count.saturating_add(session_count);
        self.size_bytes = self.size_bytes.saturating_add(size_bytes);
        bump_mtime(&mut self.newest, newest);
    }

    fn into_project(self, agent: AgentId, key: &str) -> AgentProject {
        cheap_project(
            agent,
            key,
            self.title,
            self.storage_path,
            self.actual_path,
            self.relative_path,
            self.session_count,
            self.size_bytes,
            rfc3339_mtime(self.newest),
            None,
        )
    }
}

fn cheap_project(
    agent: AgentId,
    key: &str,
    title: String,
    storage_path: String,
    actual_path: Option<String>,
    relative_path: String,
    session_count: u32,
    size_bytes: u64,
    updated_at: String,
    preview: Option<String>,
) -> AgentProject {
    AgentProject {
        id: make_project_id(agent, key),
        agent_id: agent,
        title,
        storage_path,
        actual_path,
        relative_path,
        session_count,
        message_count: None,
        size_bytes,
        updated_at,
        preview,
        alias: None,
        hidden: false,
    }
}

fn add_file_stat(
    path: &Path,
    session_count: &mut u32,
    size_bytes: &mut u64,
    newest: &mut Option<SystemTime>,
) {
    let Ok(meta) = fs::metadata(path) else {
        return;
    };
    *session_count = session_count.saturating_add(1);
    *size_bytes = size_bytes.saturating_add(meta.len());
    bump_mtime(newest, meta.modified().ok());
}

fn mtime_of(path: &Path) -> Option<SystemTime> {
    fs::metadata(path).ok().and_then(|m| m.modified().ok())
}

fn bump_mtime(slot: &mut Option<SystemTime>, t: Option<SystemTime>) {
    let Some(t) = t else {
        return;
    };
    *slot = Some(slot.map_or(t, |cur| cur.max(t)));
}

fn rfc3339_mtime(t: Option<SystemTime>) -> String {
    t.map(system_time_to_rfc3339)
        .unwrap_or_else(|| Utc::now().to_rfc3339())
}

/// Verified workspace path with native separators when the path exists.
fn native_existing_path(candidate: Option<String>) -> Option<String> {
    let a = candidate.filter(|c| !c.is_empty() && Path::new(c).exists())?;
    #[cfg(windows)]
    {
        Some(a.replace('/', "\\"))
    }
    #[cfg(not(windows))]
    {
        Some(a)
    }
}

fn dir_size_shallow(path: &Path) -> Option<u64> {
    let mut total = 0u64;
    let rd = fs::read_dir(path).ok()?;
    for ent in rd.flatten() {
        if let Ok(m) = ent.metadata() {
            total = total.saturating_add(m.len());
        }
    }
    Some(total)
}

/// Whether `path` is this agent's **primary** transcript (never sidecars).
fn is_primary_session_file(agent: AgentId, path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if name.is_empty() || name.starts_with('.') || name.ends_with(".bak") || name.contains("index")
    {
        return false;
    }
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match agent {
        AgentId::Grok => name == "chat_history.jsonl",
        AgentId::Codex => ext == "jsonl",
        // Kimi primary is agents/main/wire.jsonl (dedicated lister); keep wire.jsonl recognized.
        AgentId::Kimi => name == "wire.jsonl",
        AgentId::Pi | AgentId::Dsh => ext == "jsonl",
        AgentId::Claude | AgentId::WorkBuddy => {
            (ext == "jsonl" || ext == "json")
                && !matches!(
                    name.as_str(),
                    "summary.json"
                        | "meta.json"
                        | "events.jsonl"
                        | "updates.jsonl"
                        | "signals.json"
                        | "prompt_context.json"
                        | "prompt_history.jsonl"
                        | "resources_state.json"
                        | "announcement_state.json"
                        | "rewind_points.jsonl"
                )
        }
        AgentId::Cursor => false,
    }
}

/// Legacy helper name used by older tests.
#[cfg(test)]
pub(crate) fn is_session_file(path: &Path) -> bool {
    is_primary_session_file(AgentId::Claude, path)
}

fn build_session(
    agent: AgentId,
    home: &Path,
    path: &Path,
    project_id: &str,
    encoded_dir: Option<&str>,
) -> Option<AgentSession> {
    let mut meta = session_file_meta(agent, path);
    // Claude/WorkBuddy: directory encoding is the cwd source of truth.
    if matches!(agent, AgentId::Claude | AgentId::WorkBuddy) {
        if let Some(encoded) = encoded_dir {
            if let Some(v) = verified_actual_path(encoded) {
                meta.cwd = Some(v);
            } else if meta
                .cwd
                .as_ref()
                .is_some_and(|c| !c.is_empty() && Path::new(c).exists())
            {
                // Keep a recorded cwd that exists; do not overwrite with a missing decode.
            } else if let Some(decoded) = decode_claude_project_dir(encoded) {
                meta.cwd = Some(decoded);
            } else if meta.cwd.is_none() {
                meta.cwd =
                    extract_cwd_from_text(agent, &read_head(path, SCAN_BYTES).unwrap_or_default());
            }
        }
    }
    build_session_from_meta(agent, home, path, project_id, meta, encoded_dir)
}

fn title_from(preview: Option<&str>, cwd: Option<&str>, path: &Path) -> String {
    if let Some(p) = preview {
        let t = p.trim();
        if !t.is_empty() {
            return if t.chars().count() > 60 {
                let s: String = t.chars().take(59).collect();
                format!("{s}…")
            } else {
                t.to_string()
            };
        }
    }
    if let Some(c) = cwd {
        if let Some(name) = Path::new(c).file_name().and_then(|n| n.to_str()) {
            if !name.is_empty() {
                return name.to_string();
            }
        }
    }
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("session")
        .to_string()
}

fn extract_cwd_from_text(agent: AgentId, text: &str) -> Option<String> {
    // Prefer complete lines; Codex session_meta first line is large but usually < SCAN_BYTES.
    for line in text.lines().take(40) {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if let Some(s) = cwd_from_json_value(agent, &v) {
            return Some(s);
        }
    }
    None
}

fn cwd_from_json_value(agent: AgentId, v: &serde_json::Value) -> Option<String> {
    // Top-level keys (legacy fixtures + some CLIs).
    for key in [
        "cwd",
        "workdir",
        "workDir",
        "working_directory",
        "project_path",
        "projectPath",
    ] {
        if let Some(s) = v.get(key).and_then(|x| x.as_str()) {
            if !s.is_empty() {
                return Some(s.to_string());
            }
        }
    }
    // Nested pointers — Codex uses session_meta/turn_context → payload.cwd.
    let pointers: &[&str] = match agent {
        AgentId::Codex => &[
            "/payload/cwd",
            "/payload/workdir",
            "/payload/workspace_roots/0",
            "/message/cwd",
            "/session/cwd",
            "/info/cwd",
        ],
        AgentId::Grok => &[
            "/info/cwd",
            "/payload/cwd",
            "/message/cwd",
            "/session/cwd",
            "/cwd",
        ],
        _ => &["/payload/cwd", "/message/cwd", "/session/cwd", "/info/cwd"],
    };
    for p in pointers {
        if let Some(s) = v.pointer(p).and_then(|x| x.as_str()) {
            if !s.is_empty() {
                return Some(s.to_string());
            }
        }
    }
    // Codex: type-tagged objects with payload object.
    if matches!(agent, AgentId::Codex | AgentId::Kimi | AgentId::Pi) {
        if let Some(payload) = v.get("payload") {
            if let Some(s) = payload.get("cwd").and_then(|x| x.as_str()) {
                if !s.is_empty() {
                    return Some(s.to_string());
                }
            }
        }
    }
    None
}

fn scan_preview(path: &Path) -> (Option<String>, Option<u32>) {
    let Some(text) = read_head(path, SCAN_BYTES) else {
        return (None, None);
    };
    scan_preview_from_text(&text)
}

fn scan_preview_from_text(text: &str) -> (Option<String>, Option<u32>) {
    let mut preview: Option<String> = None;
    let mut count = 0u32;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        count = count.saturating_add(1);
        if preview.is_some() {
            continue;
        }
        if let Some(p) = extract_userish_text(line) {
            if let Some(visible) = visible_transcript_text(&p) {
                preview = Some(truncate_chars(&visible, PREVIEW_CHARS));
            }
        }
    }
    if preview.is_none() {
        for line in text.lines().take(20) {
            if let Some(p) = extract_any_text(line.trim()) {
                if let Some(visible) = visible_transcript_text(&p) {
                    preview = Some(truncate_chars(&visible, PREVIEW_CHARS));
                    break;
                }
            }
        }
    }
    (preview, if count > 0 { Some(count) } else { None })
}

fn is_noisy_preview(text: &str) -> bool {
    let t = text.trim();
    t.starts_with("<user_info>")
        || t.starts_with("<system-reminder>")
        || t.starts_with("<git_status>")
        || t.starts_with("<rules>")
        || t.starts_with("You are Grok")
        || t.starts_with("You are Codex")
        || t.starts_with("You are Claude")
        || t.contains("base_instructions")
        || t.len() > 4000 && t.contains("<agent_skills>")
}

/// Prefer `<user_query>` inner text (Grok chat_history wraps the real prompt).
fn visible_transcript_text(text: &str) -> Option<String> {
    if let Some(q) = unwrap_tagged_block(text, "user_query") {
        return Some(q);
    }
    if is_noisy_preview(text) {
        return None;
    }
    let t = text.trim();
    if t.is_empty() {
        None
    } else {
        Some(t.to_string())
    }
}

fn unwrap_tagged_block(text: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = text.find(&open)?;
    let rest = &text[start + open.len()..];
    let end = rest.find(&close)?;
    let inner = rest[..end].trim();
    if inner.is_empty() {
        None
    } else {
        Some(inner.to_string())
    }
}

struct ExcerptTurn {
    role: &'static str,
    text: String,
}

fn extract_jsonl_transcript_turns(text: &str) -> Vec<ExcerptTurn> {
    let mut turns = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(turn) = extract_transcript_turn_from_line(line) {
            turns.push(turn);
        }
    }
    turns
}

fn extract_transcript_turn_from_line(line: &str) -> Option<ExcerptTurn> {
    let v: serde_json::Value = serde_json::from_str(line).ok()?;
    if v.get("synthetic_reason")
        .and_then(|x| x.as_str())
        .is_some_and(|s| !s.is_empty())
    {
        return None;
    }
    if let Some(t) = extract_userish_text(line) {
        let visible = visible_transcript_text(&t)?;
        return Some(ExcerptTurn {
            role: "user",
            text: visible,
        });
    }
    extract_assistant_turn(&v)
}

fn extract_assistant_turn(v: &serde_json::Value) -> Option<ExcerptTurn> {
    let ty = v
        .get("type")
        .or_else(|| v.get("role"))
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if matches!(
        ty.as_str(),
        "system"
            | "reasoning"
            | "tool_result"
            | "tool_calls"
            | "function_call"
            | "tool"
            | "hook_execution"
    ) {
        return None;
    }
    let payload_ty = v
        .pointer("/payload/type")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if matches!(
        payload_ty.as_str(),
        "function_call"
            | "function_call_output"
            | "custom_tool_call"
            | "reasoning"
            | "tool_call"
            | "tool_result"
    ) {
        return None;
    }
    let role = v
        .get("role")
        .and_then(|r| r.as_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let payload_role = v
        .pointer("/payload/role")
        .and_then(|r| r.as_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let is_assistant = ty.contains("assistant")
        || ty == "agent"
        || role == "assistant"
        || payload_role == "assistant"
        || payload_ty.contains("assistant")
        || payload_ty == "agent_message";
    if !is_assistant {
        return None;
    }
    let text = extract_text_from_value(v)
        .or_else(|| v.get("payload").and_then(extract_text_from_value))?;
    let t = text.trim();
    if t.is_empty() {
        return None;
    }
    Some(ExcerptTurn {
        role: "assistant",
        text: t.to_string(),
    })
}

/// Grok `updates.jsonl`: stitch streamed user/agent message chunks.
fn extract_grok_update_turns(text: &str) -> Vec<ExcerptTurn> {
    let mut turns: Vec<ExcerptTurn> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Some((role, piece)) = grok_update_chunk(line) else {
            continue;
        };
        if piece.is_empty() {
            continue;
        }
        if let Some(last) = turns.last_mut() {
            if last.role == role {
                last.text.push_str(&piece);
                continue;
            }
        }
        turns.push(ExcerptTurn { role, text: piece });
    }
    polish_excerpt_turns(turns)
}

fn grok_update_chunk(line: &str) -> Option<(&'static str, String)> {
    let v: serde_json::Value = serde_json::from_str(line).ok()?;
    let update = v
        .pointer("/params/update")
        .or_else(|| v.get("update"))
        .unwrap_or(&v);
    let su = update
        .get("sessionUpdate")
        .or_else(|| update.get("session_update"))
        .and_then(|x| x.as_str())?;
    let role = match su {
        "user_message_chunk" | "user_message" => "user",
        "agent_message_chunk" | "agent_message" | "assistant_message_chunk" => "assistant",
        _ => return None,
    };
    let piece = grok_update_text(update)?;
    Some((role, piece))
}

fn grok_update_text(update: &serde_json::Value) -> Option<String> {
    if let Some(s) = update.pointer("/content/text").and_then(|x| x.as_str()) {
        if !s.is_empty() {
            return Some(s.to_string());
        }
    }
    if let Some(arr) = update.get("content").and_then(|x| x.as_array()) {
        let mut parts = Vec::new();
        for item in arr {
            if let Some(s) = item.as_str() {
                parts.push(s.to_string());
            } else if let Some(s) = item.get("text").and_then(|t| t.as_str()) {
                parts.push(s.to_string());
            }
        }
        let joined = parts.join("");
        if !joined.is_empty() {
            return Some(joined);
        }
    }
    extract_text_from_value(update)
}

fn polish_excerpt_turns(turns: Vec<ExcerptTurn>) -> Vec<ExcerptTurn> {
    turns
        .into_iter()
        .filter_map(|t| {
            if t.role == "user" {
                let text = visible_transcript_text(&t.text)?;
                Some(ExcerptTurn {
                    role: "user",
                    text,
                })
            } else if t.text.trim().is_empty() {
                None
            } else {
                Some(t)
            }
        })
        .collect()
}

/// Prefer Grok `updates.jsonl` (clean user chunks). If it has no assistant
/// text, keep those user turns and fill replies from `chat_history.jsonl`.
fn merge_grok_excerpt_turns(
    update_turns: Vec<ExcerptTurn>,
    chat_turns: Vec<ExcerptTurn>,
) -> Vec<ExcerptTurn> {
    if update_turns.is_empty() {
        return chat_turns;
    }
    if update_turns.iter().any(|t| t.role == "assistant") {
        return update_turns;
    }
    let mut out = update_turns;
    out.extend(chat_turns.into_iter().filter(|t| t.role == "assistant"));
    out
}

/// Role-tagged turns so markdown `---` inside a reply is not a splitter.
fn format_excerpt_turns(turns: &[ExcerptTurn]) -> String {
    let mut body = String::new();
    for t in turns {
        let text = t.text.trim();
        if text.is_empty() {
            continue;
        }
        let used = body.chars().count();
        if used >= EXCERPT_CHARS {
            break;
        }
        let header = format!("---turn:{}---\n", t.role);
        let header_len = header.chars().count();
        let remaining = EXCERPT_CHARS.saturating_sub(used.saturating_add(header_len + 1));
        if remaining == 0 {
            break;
        }
        // A wrapped user prompt must not consume the whole excerpt budget.
        let budget = if t.role == "user" {
            remaining.min(USER_TURN_EXCERPT_CHARS)
        } else {
            remaining
        };
        if budget == 0 {
            continue;
        }
        let piece = truncate_chars(text, budget);
        if !body.is_empty() {
            body.push('\n');
        }
        body.push_str(&header);
        body.push_str(&piece);
    }
    body
}

pub(crate) fn make_project_id(agent: AgentId, storage_key: &str) -> String {
    let key = storage_key.replace('\\', "/");
    format!("{}:proj:{}", agent.as_str(), key)
}

pub(crate) fn load_excerpt(id: &str, home_override: Option<&Path>) -> Result<AgentProjectExcerpt> {
    let (agent, rel) = parse_session_id(id)?;
    let abs = resolve_under_home(agent, &rel, home_override)?;
    if !abs.is_file() {
        return Err(AppError::NotFound(format!("project not found: {id}")));
    }
    let home_raw = match home_override {
        Some(h) => h.to_path_buf(),
        None => agent_home(agent)?,
    };
    let home = fs::canonicalize(&home_raw).unwrap_or(home_raw);
    let abs_path = fs::canonicalize(&abs).unwrap_or(abs);
    let project_id = infer_project_id_for_path(agent, &home, &abs_path);
    let rec = build_session(agent, &home, &abs_path, &project_id, None).unwrap_or_else(|| {
        let (preview, message_count) = scan_preview(&abs_path);
        let meta = fs::metadata(&abs_path).ok();
        AgentSession {
            id: id.to_string(),
            project_id,
            agent_id: agent,
            title: title_from(preview.as_deref(), None, &abs_path),
            cwd: None,
            path: abs_path.display().to_string(),
            relative_path: rel,
            size_bytes: meta.as_ref().map(|m| m.len()).unwrap_or(0),
            updated_at: meta
                .and_then(|m| m.modified().ok())
                .map(system_time_to_rfc3339)
                .unwrap_or_else(|| Utc::now().to_rfc3339()),
            preview,
            message_count,
            session_id: native_session_id_from_path(agent, &abs_path),
        }
    });
    let text = read_head(&abs_path, SCAN_BYTES * 2).unwrap_or_default();
    let chat_turns = extract_jsonl_transcript_turns(&text);
    let turns = if agent == AgentId::Grok {
        let updates = abs_path.with_file_name("updates.jsonl");
        let update_turns = if updates.is_file() {
            read_head(&updates, SCAN_BYTES * 2)
                .map(|u| extract_grok_update_turns(&u))
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        merge_grok_excerpt_turns(update_turns, chat_turns)
    } else {
        chat_turns
    };
    let body = if turns.is_empty() {
        String::new()
    } else {
        format_excerpt_turns(&turns)
    };
    Ok(AgentProjectExcerpt {
        id: rec.id,
        agent_id: rec.agent_id,
        title: rec.title,
        cwd: rec.cwd,
        updated_at: rec.updated_at,
        excerpt: body,
    })
}

fn infer_project_id_for_path(agent: AgentId, home: &Path, path: &Path) -> String {
    match agent {
        AgentId::Claude | AgentId::WorkBuddy => {
            let projects = home.join("projects");
            if let Ok(rel) = path.strip_prefix(&projects) {
                if let Some(encoded) = rel.components().next().and_then(|c| c.as_os_str().to_str())
                {
                    return make_project_id(agent, encoded);
                }
            }
            make_project_id(agent, UNGROUPED_KEY)
        }
        AgentId::Cursor => {
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(UNGROUPED_KEY);
            make_project_id(agent, name)
        }
        AgentId::Grok => {
            let sessions = home.join("sessions");
            if let Ok(rel) = path.strip_prefix(&sessions) {
                if let Some(encoded) = rel.components().next().and_then(|c| c.as_os_str().to_str())
                {
                    let (key, _) = grok_project_key_from_dir_name(encoded);
                    return make_project_id(agent, &key);
                }
            }
            let cwd =
                extract_cwd_from_text(agent, &read_head(path, SCAN_BYTES).unwrap_or_default());
            let key = match cwd.as_deref() {
                Some(c) if !c.is_empty() => cwd_storage_key(c),
                _ => UNGROUPED_KEY.to_string(),
            };
            make_project_id(agent, &key)
        }
        AgentId::Kimi => {
            let sessions = home.join("sessions");
            if let Ok(rel) = path.strip_prefix(&sessions) {
                if let Some(wd) = rel.components().next().and_then(|c| c.as_os_str().to_str()) {
                    let workspaces = load_kimi_workspaces(home);
                    if let Some(ws) = workspaces.get(wd) {
                        return make_project_id(agent, &cwd_storage_key(&ws.root));
                    }
                    return make_project_id(agent, &format!("ws/{wd}"));
                }
            }
            make_project_id(agent, UNGROUPED_KEY)
        }
        AgentId::Pi => {
            let sessions = home.join("agent").join("sessions");
            if let Ok(rel) = path.strip_prefix(&sessions) {
                if let Some(encoded) = rel.components().next().and_then(|c| c.as_os_str().to_str())
                {
                    if let Some(decoded) = decode_pi_session_dir(encoded) {
                        return make_project_id(agent, &cwd_storage_key(&decoded));
                    }
                    return make_project_id(agent, &format!("dir/{encoded}"));
                }
            }
            let cwd =
                extract_cwd_from_text(agent, &read_head(path, SCAN_BYTES).unwrap_or_default());
            let key = match cwd.as_deref() {
                Some(c) if !c.is_empty() => cwd_storage_key(c),
                _ => UNGROUPED_KEY.to_string(),
            };
            make_project_id(agent, &key)
        }
        _ => {
            let cwd =
                extract_cwd_from_text(agent, &read_head(path, SCAN_BYTES).unwrap_or_default());
            let key = match cwd.as_deref() {
                Some(c) if !c.is_empty() => cwd_storage_key(c),
                _ => UNGROUPED_KEY.to_string(),
            };
            make_project_id(agent, &key)
        }
    }
}

pub(crate) fn extract_userish_text(line: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(line).ok()?;
    let ty = v
        .get("type")
        .or_else(|| v.get("role"))
        .and_then(|x| x.as_str())
        .unwrap_or("");
    let ty_l = ty.to_ascii_lowercase();
    let role = v
        .get("role")
        .and_then(|r| r.as_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    // Codex: response_item.payload.role == user
    let payload_role = v
        .pointer("/payload/role")
        .and_then(|r| r.as_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    // Kimi wire: turn.prompt / context.append_message with user role
    if ty_l == "turn.prompt" {
        return extract_text_from_value(&v);
    }
    if ty_l == "context.append_message" || ty_l == "message" {
        if let Some(msg) = v.get("message") {
            let mrole = msg
                .get("role")
                .and_then(|r| r.as_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if mrole == "user" || mrole.is_empty() && ty_l == "context.append_message" {
                // append_message without role still often user turns; prefer role==user.
                if mrole == "user" || ty_l == "message" {
                    if let Some(s) = extract_text_from_value(msg) {
                        return Some(s);
                    }
                }
            }
            if mrole == "user" {
                if let Some(s) = extract_text_from_value(msg) {
                    return Some(s);
                }
            }
        }
    }
    let is_user =
        ty_l.contains("user") || ty_l == "human" || role == "user" || payload_role == "user";
    if !is_user {
        return None;
    }
    if let Some(payload) = v.get("payload") {
        if let Some(s) = extract_text_from_value(payload) {
            return Some(s);
        }
    }
    extract_text_from_value(&v)
}

pub(crate) fn extract_any_text(line: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(line).ok()?;
    extract_text_from_value(&v)
}

fn extract_text_from_value(v: &serde_json::Value) -> Option<String> {
    for key in ["text", "content", "message", "prompt", "input"] {
        if let Some(s) = v.get(key).and_then(|x| x.as_str()) {
            let t = s.trim();
            if !t.is_empty() {
                return Some(t.to_string());
            }
        }
        if let Some(arr) = v.get(key).and_then(|x| x.as_array()) {
            let mut parts = Vec::new();
            for item in arr {
                if let Some(s) = item.as_str() {
                    parts.push(s.to_string());
                } else if let Some(s) = item.get("text").and_then(|t| t.as_str()) {
                    parts.push(s.to_string());
                } else if let Some(s) = item.get("content").and_then(|t| t.as_str()) {
                    parts.push(s.to_string());
                }
            }
            let joined = parts.join("\n").trim().to_string();
            if !joined.is_empty() {
                return Some(joined);
            }
        }
    }
    if let Some(msg) = v.get("message") {
        if let Some(s) = extract_text_from_value(msg) {
            return Some(s);
        }
    }
    None
}

fn read_head(path: &Path, max_bytes: u64) -> Option<String> {
    use std::io::Read;
    let file = fs::File::open(path).ok()?;
    let mut handle = file.take(max_bytes);
    let mut buf = Vec::new();
    handle.read_to_end(&mut buf).ok()?;
    Some(String::from_utf8_lossy(&buf).into_owned())
}

fn truncate_chars(s: &str, max: usize) -> String {
    let count = s.chars().count();
    if count <= max {
        return s.to_string();
    }
    let t: String = s.chars().take(max.saturating_sub(1)).collect();
    format!("{t}…")
}

fn path_to_rel(rel: &Path) -> String {
    rel.components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}
