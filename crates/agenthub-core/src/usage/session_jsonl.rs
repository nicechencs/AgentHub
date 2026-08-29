//! Session-log usage extraction.
//!
//! Design notes borrowed from **ccusage** (https://github.com/ccusage/ccusage):
//! - Claude / WorkBuddy: `message.usage` + optional `costUSD`; message+request dedupe.
//! - Codex: `turn_context` model inheritance + `token_count`/`last_token_usage`.
//!   - Skip `last_token_usage` when `total_token_usage` is unchanged (duplicate snapshots).
//!   - Fork/subagent sessions: skip rewritten parent-history burst at open (≤1s gaps).
//!   - Store **non-cached** input (`full − cached`) to match ccusage `inputTokens`.
//!   - Also scan `archived_sessions/` (live `sessions/` wins on the same relative path).
//!   - Fast/Priority from `thread_settings_applied` or config.toml `service_tier`.
//! - Kimi: only `wire.jsonl`; old StatusUpdate/token_usage + new usage.record (turn only).
//! - Pi: type=message role=assistant; usage.input/output/cacheRead/cacheWrite/cost.total.
//! - Grok: `sessions/**/updates.jsonl` `turn_completed` only (ccusage adapter-grok).
//!   OpenAI-style `inputTokens` includes cache; peel cachedRead/cacheCreation.
//!   Prefer `costUsdTicks` (1e-10 USD). Do not add `reasoningTokens` to totals.
//! - DSH: provider usage on assistant/step events; inherit model from `request/header`.
//!   Skip Token Meter heuristics (`surfaceTokens` / `estimated`). Do not scan cwd `.sessions`.
//! - Paths: CLAUDE_CONFIG_DIR / XDG, KIMI_DATA_DIR, PI_AGENT_DIR, GROK_HOME, DSH_HOME / DSH_SESSION_ROOT.
//! - Pricing: prefer log costUSD / Grok ticks (Auto), else token × rates
//!   (long-context whole-request switch, 1h cache at 2× input, Codex Fast).
//!
//! AgentHub adds SQLite incremental cursors (ccusage is primarily report-oriented).

use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::error::Result;
use crate::models::{AgentId, ParsedUsageEvent};
use crate::storage::{UsageCursor, UsageRepo};
use crate::utils::paths::{agent_home, home_dir};

pub struct CollectStats {
    pub events: Vec<ParsedUsageEvent>,
    pub cursors: Vec<UsageCursor>,
    pub skipped: u64,
    pub failed: u64,
}

/// Collect new usage events for one agent from its known log roots.
///
/// Compatibility façade: delegates to [`crate::platform::usage`] UsageSource
/// registry. Prefer platform collect for new call sites.
pub fn collect_for_agent(agent: AgentId, repo: &UsageRepo) -> Result<CollectStats> {
    crate::platform::usage::collect_for_agent_id(agent, repo)
}

// ---------------------------------------------------------------------------
// Per-agent discovery (used by platform UsageSource integrations).
// Platform collect must not match on concrete AgentId — sources call these.
// ---------------------------------------------------------------------------

pub(crate) fn discover_claude_files() -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    for root in claude_project_roots()? {
        walk_jsonl(&root, &mut out, None);
    }
    finish_files(out)
}

/// WorkBuddy: prefer Claude-isomorphic `projects/**/*.jsonl` when present;
/// also scan home for any other usage jsonl (real installs may omit projects/).
pub(crate) fn discover_workbuddy_files() -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    let home = agent_home(AgentId::WorkBuddy)?;
    let projects = home.join("projects");
    if projects.is_dir() {
        walk_jsonl(&projects, &mut out, None);
    }
    if out.is_empty() {
        walk_jsonl(&home, &mut out, None);
    }
    finish_files(out)
}

pub(crate) fn discover_codex_files() -> Result<Vec<PathBuf>> {
    let home = agent_home(AgentId::Codex)?;
    finish_files(discover_codex_files_in(&home))
}

/// ccusage: `sessions/` plus `archived_sessions/`. When both have the same
/// relative path, keep the live `sessions/` copy.
pub(crate) fn discover_codex_files_in(home: &Path) -> Vec<PathBuf> {
    let sessions = home.join("sessions");
    let archived = home.join("archived_sessions");
    let mut active = Vec::new();
    walk_jsonl(&sessions, &mut active, None);
    let mut extra = Vec::new();
    walk_jsonl(&archived, &mut extra, None);
    let active_rel: std::collections::HashSet<PathBuf> = active
        .iter()
        .filter_map(|p| p.strip_prefix(&sessions).ok().map(|r| r.to_path_buf()))
        .collect();
    extra.retain(|p| match p.strip_prefix(&archived) {
        Ok(rel) => !active_rel.contains(rel),
        Err(_) => true,
    });
    active.extend(extra);
    active
}

pub(crate) fn discover_kimi_files() -> Result<Vec<PathBuf>> {
    finish_files(discover_kimi_wire_files()?)
}

pub(crate) fn discover_pi_files() -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    for root in pi_session_roots()? {
        walk_jsonl(&root, &mut out, None);
    }
    finish_files(out)
}

/// Known DSH persistence roots only — never walk a random cwd `.sessions`.
pub(crate) fn discover_dsh_files() -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    let home = agent_home(AgentId::Dsh)?;
    walk_jsonl(&home.join("sessions"), &mut out, None);
    let profiles = home.join("profiles");
    if let Ok(entries) = fs::read_dir(&profiles) {
        for ent in entries.flatten() {
            let path = ent.path();
            if path.is_dir() {
                walk_jsonl(&path.join("sessions"), &mut out, None);
            }
        }
    }
    if let Ok(raw) = std::env::var("DSH_SESSION_ROOT") {
        let root = PathBuf::from(raw.trim());
        if root.is_dir() {
            walk_jsonl(&root, &mut out, None);
        }
    }
    out.retain(|p| {
        let s = p.to_string_lossy();
        !s.contains("node_modules") && !s.contains(".db")
    });
    finish_files(out)
}

#[cfg(test)]
pub(crate) use crate::usage::grok::discover_grok_files;

fn finish_files(mut files: Vec<PathBuf>) -> Result<Vec<PathBuf>> {
    files.sort();
    files.dedup();
    Ok(files)
}

/// Legacy dispatcher kept for unit tests that still call it directly.
#[cfg(test)]
fn discover_usage_files(agent: AgentId) -> Result<Vec<PathBuf>> {
    match agent {
        AgentId::Claude => discover_claude_files(),
        AgentId::WorkBuddy => discover_workbuddy_files(),
        AgentId::Codex => discover_codex_files(),
        AgentId::Kimi => discover_kimi_files(),
        AgentId::Pi => discover_pi_files(),
        AgentId::Grok => discover_grok_files(),
        AgentId::Cursor => Ok(Vec::new()),
        AgentId::Dsh => discover_dsh_files(),
        AgentId::Zcode => Ok(Vec::new()),
    }
}

/// ccusage Kimi: only `wire.jsonl` under sessions (old 3-depth or new agents/ 5-depth).
fn discover_kimi_wire_files() -> Result<Vec<PathBuf>> {
    let mut roots = Vec::new();
    let mut seen = std::collections::HashSet::new();

    if let Ok(env_paths) = std::env::var("KIMI_DATA_DIR") {
        for raw in env_paths
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            let p = PathBuf::from(raw);
            if p.is_dir() && seen.insert(p.clone()) {
                roots.push(p);
            }
        }
    } else {
        let home = home_dir()?;
        for dir in [".kimi-code", ".kimi"] {
            let p = home.join(dir);
            if p.is_dir() && seen.insert(p.clone()) {
                roots.push(p);
            }
        }
    }

    let mut files = Vec::new();
    for root in roots {
        let sessions = root.join("sessions");
        let mut candidates = Vec::new();
        walk_jsonl(&sessions, &mut candidates, Some("wire.jsonl"));
        for f in candidates {
            if is_kimi_wire_layout(&sessions, &f) {
                files.push(f);
            }
        }
    }
    Ok(files)
}

fn is_kimi_wire_layout(sessions_path: &Path, file_path: &Path) -> bool {
    if file_path.file_name().and_then(|n| n.to_str()) != Some("wire.jsonl") {
        return false;
    }
    let Ok(rel) = file_path.strip_prefix(sessions_path) else {
        return false;
    };
    // Old: group/session/wire.jsonl → 3
    // New: ws/session/agents/agent/wire.jsonl → 5
    let n = rel.components().count();
    n == 3 || n == 5
}

/// ccusage Pi: `PI_AGENT_DIR` or `~/.pi/agent/sessions`.
fn pi_session_roots() -> Result<Vec<PathBuf>> {
    let mut roots = Vec::new();
    let mut seen = std::collections::HashSet::new();
    if let Ok(env_paths) = std::env::var("PI_AGENT_DIR") {
        for raw in env_paths
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            let p = PathBuf::from(raw);
            if p.is_dir() && seen.insert(p.clone()) {
                roots.push(p);
            }
        }
        if !roots.is_empty() {
            return Ok(roots);
        }
    }
    let home = home_dir()?;
    let def = home.join(".pi").join("agent").join("sessions");
    roots.push(def);
    Ok(roots)
}

fn claude_project_roots() -> Result<Vec<PathBuf>> {
    let mut roots = Vec::new();
    let mut seen = std::collections::HashSet::new();

    if let Ok(env_paths) = std::env::var("CLAUDE_CONFIG_DIR") {
        for raw in env_paths
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            let base = PathBuf::from(raw);
            let projects = if base.file_name().is_some_and(|n| n == "projects") {
                base
            } else {
                base.join("projects")
            };
            if projects.is_dir() && seen.insert(projects.clone()) {
                roots.push(projects);
            }
        }
        if !roots.is_empty() {
            return Ok(roots);
        }
    }

    let home = home_dir()?;
    let xdg = std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| home.join(".config"));
    for base in [xdg.join("claude"), home.join(".claude")] {
        let projects = base.join("projects");
        if projects.is_dir() && seen.insert(projects.clone()) {
            roots.push(projects);
        }
    }
    // Always include default even if missing (collect will no-op).
    if roots.is_empty() {
        roots.push(home.join(".claude").join("projects"));
    }
    Ok(roots)
}

pub(crate) fn session_id_from_path(path: &Path) -> Option<String> {
    // Kimi: parent session dir (or grandparent under agents/)
    if path.file_name().and_then(|n| n.to_str()) == Some("wire.jsonl") {
        return kimi_session_id_from_path(path);
    }
    // Grok: sessions/<cwd>/<session-uuid>/updates.jsonl → session-uuid
    if let Some(sid) = crate::usage::grok::session_id_from_updates_path(path) {
        return Some(sid);
    }
    // Pi: filename often `agent_<sessionId>.jsonl` → take after first `_`
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

fn kimi_session_id_from_path(path: &Path) -> Option<String> {
    let parent = path.parent()?;
    let session_dir = if parent
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        == Some("agents")
    {
        parent.parent()?.parent()
    } else {
        Some(parent)
    }?;
    session_dir
        .file_name()
        .and_then(|n| n.to_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

/// Recursively collect `*.jsonl` under `root`.
/// When `only_name` is set (e.g. `wire.jsonl`), filter by exact file name.
fn walk_jsonl(root: &Path, out: &mut Vec<PathBuf>, only_name: Option<&str>) {
    if !root.exists() {
        return;
    }
    walk_jsonl_inner(root, out, only_name);
}

fn walk_jsonl_inner(dir: &Path, out: &mut Vec<PathBuf>, only_name: Option<&str>) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for ent in entries.flatten() {
        let path = ent.path();
        if path.is_dir() {
            walk_jsonl_inner(&path, out, only_name);
        } else if path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("jsonl"))
            .unwrap_or(false)
        {
            if let Some(name) = only_name {
                if path.file_name().and_then(|n| n.to_str()) != Some(name) {
                    continue;
                }
            }
            out.push(path);
        }
    }
}

pub(crate) fn line_might_have_usage_claude_like(line: &str) -> bool {
    line.contains("\"usage\"") || line.contains("\"input_tokens\"")
}

pub(crate) fn line_might_have_usage_pi(line: &str) -> bool {
    line.contains("\"usage\"") && line.contains("\"message\"")
}

pub(crate) fn line_might_have_usage_kimi(line: &str) -> bool {
    line.contains("\"usage.record\"")
        || line.contains("\"token_usage\"")
        || line.contains("\"llm.request\"")
        || (line.contains("\"usage\"") && line.contains("\"usageScope\""))
}

pub(crate) fn line_might_have_usage_codex(line: &str) -> bool {
    line.contains("token_count")
        || line.contains("last_token_usage")
        || line.contains("turn_context")
        || line.contains("thread_settings_applied")
        || line.contains("\"usage\"")
}

pub(crate) fn line_might_have_usage_dsh(line: &str) -> bool {
    if line.contains("token-meter") || line.contains("tokenMeter") || line.contains("surfaceTokens")
    {
        return false;
    }
    (line.contains("\"usage\"")
        || line.contains("\"input_tokens\"")
        || line.contains("\"output_tokens\"")
        || line.contains("\"inputTokens\"")
        || line.contains("\"outputTokens\""))
        && !line.contains("\"estimated\":true")
}

pub(crate) fn note_dsh_model_from_line(line: &str, model: &mut Option<String>) {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
        return;
    };
    let ty = v.get("type").and_then(|t| t.as_str()).unwrap_or("");
    if ty == "request/header" || ty == "session" || ty.ends_with("/header") {
        if let Some(found) = find_model(&v) {
            *model = Some(found);
        }
    }
}

/// Resolve kimi data root from a wire.jsonl path (ccusage layout).
pub(crate) fn kimi_root_from_wire_path(file_path: &Path) -> Option<PathBuf> {
    // wire.jsonl parent directory
    let parent = file_path.parent()?;
    // New: root/sessions/<ws>/<session>/agents/<agent>/wire.jsonl
    // parent=agent, parent.parent=agents
    if parent
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        == Some("agents")
    {
        // agent → agents → session → ws → sessions → root
        return parent
            .parent()? // agents
            .parent()? // session
            .parent()? // workspace
            .parent()? // sessions
            .parent() // root
            .map(Path::to_path_buf);
    }
    // Old: root/sessions/<group>/<session>/wire.jsonl
    // parent=session → group → sessions → root
    parent
        .parent()? // group
        .parent()? // sessions
        .parent() // root
        .map(Path::to_path_buf)
}

/// Read Codex default model from real `~/.codex/config.toml` (`model = "..."`).
/// Never invents a product id — returns None when config is missing/unreadable.
pub(crate) fn read_codex_default_model(root: &Path) -> Option<String> {
    let toml_path = root.join("config.toml");
    if let Ok(text) = fs::read_to_string(&toml_path) {
        if let Ok(doc) = text.parse::<toml_edit::DocumentMut>() {
            if let Some(v) = doc.get("model").and_then(|item| item.as_str()) {
                let t = v.trim();
                if !t.is_empty() {
                    return Some(t.to_string());
                }
            }
        }
        // Line scan fallback (simple TOML assignment)
        for line in text.lines() {
            let t = line.trim();
            if t.starts_with('#') {
                continue;
            }
            if let Some(rest) = t.strip_prefix("model") {
                // avoid model_reasoning_effort / model_* keys
                let rest = rest.trim_start();
                if !rest.starts_with('=') {
                    continue;
                }
                let val = rest
                    .trim_start_matches('=')
                    .trim()
                    .trim_matches('"')
                    .trim_matches('\'');
                if !val.is_empty() && !val.contains('\n') {
                    return Some(val.to_string());
                }
            }
        }
    }
    let json_path = root.join("config.json");
    if let Ok(text) = fs::read_to_string(&json_path) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
            if let Some(m) = v.get("model").and_then(|x| x.as_str()) {
                let t = m.trim();
                if !t.is_empty() {
                    return Some(t.to_string());
                }
            }
        }
    }
    None
}

/// ccusage Auto Fast: top-level `service_tier = "fast"|"priority"` in config.toml.
pub(crate) fn read_codex_fast_service_tier(root: &Path) -> bool {
    let toml_path = root.join("config.toml");
    let Ok(text) = fs::read_to_string(&toml_path) else {
        return false;
    };
    if let Ok(doc) = text.parse::<toml_edit::DocumentMut>() {
        return doc
            .get("service_tier")
            .and_then(|item| item.as_str())
            .is_some_and(|v| matches!(v.trim(), "fast" | "priority"));
    }
    for line in text.lines() {
        let setting = line.split('#').next().unwrap_or_default();
        if setting.starts_with(' ') || setting.starts_with('\t') {
            continue;
        }
        let setting = setting.trim();
        let Some((key, value)) = setting.split_once('=') else {
            continue;
        };
        if key.trim() != "service_tier" {
            continue;
        }
        let value = value.trim().trim_matches(['"', '\'']);
        return matches!(value, "fast" | "priority");
    }
    false
}

/// Read Pi default model from real `~/.pi/agent/settings.json` (`defaultModel`).
pub(crate) fn read_pi_default_model(agent_dir: &Path) -> Option<String> {
    let path = agent_dir.join("settings.json");
    let text = fs::read_to_string(path).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    v.get("defaultModel")
        .or_else(|| v.get("default_model"))
        .and_then(|x| x.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

/// Read default model from real Kimi Code `config.toml` or legacy `config.json`.
/// Never invents a product name — returns None if config is missing/unreadable.
pub(crate) fn read_kimi_default_model(root: &Path) -> Option<String> {
    let toml_path = root.join("config.toml");
    if let Ok(text) = fs::read_to_string(&toml_path) {
        if let Ok(doc) = text.parse::<toml_edit::DocumentMut>() {
            if let Some(v) = doc.get("default_model").and_then(|item| item.as_str()) {
                let n = normalize_kimi_model(v);
                if !n.is_empty() {
                    return Some(n);
                }
            }
        }
        // Fallback: line scan if DocumentMut path differs
        for line in text.lines() {
            let t = line.trim();
            if let Some(rest) = t.strip_prefix("default_model") {
                if let Some(eq) = rest.find('=') {
                    let val = rest[eq + 1..].trim().trim_matches('"').trim_matches('\'');
                    let n = normalize_kimi_model(val);
                    if !n.is_empty() {
                        return Some(n);
                    }
                }
            }
        }
    }
    // ccusage legacy config.json
    let json_path = root.join("config.json");
    if let Ok(text) = fs::read_to_string(&json_path) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
            if let Some(m) = v.get("model").and_then(|x| x.as_str()) {
                let n = normalize_kimi_model(m);
                if !n.is_empty() {
                    return Some(n);
                }
            }
        }
    }
    None
}

/// Strip `kimi-code/` provider prefix for stable storage / pricing keys.
fn normalize_kimi_model(raw: &str) -> String {
    let t = raw.trim();
    t.strip_prefix("kimi-code/").unwrap_or(t).trim().to_string()
}

/// Update kimi_model from llm.request / usage.record lines (real wire logs).
pub(crate) fn note_kimi_model_from_line(line: &str, current: &mut Option<String>) {
    if !line.contains("\"model\"") {
        return;
    }
    let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
        return;
    };
    // Prefer modelAlias (full id) then model
    if let Some(m) = v
        .get("modelAlias")
        .and_then(|x| x.as_str())
        .or_else(|| v.get("model").and_then(|x| x.as_str()))
    {
        let n = normalize_kimi_model(m);
        if !n.is_empty() {
            *current = Some(n);
        }
    }
}

/// Track Pi model from `model_change` lines (real session logs use `modelId`).
pub(crate) fn note_pi_model_from_line(line: &str, current: &mut Option<String>) {
    if !(line.contains("model_change")
        || line.contains("\"modelId\"")
        || line.contains("\"model\""))
    {
        return;
    }
    let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
        return;
    };
    if v.get("type").and_then(|t| t.as_str()) == Some("model_change") {
        if let Some(m) = v
            .get("modelId")
            .or_else(|| v.get("model"))
            .and_then(|x| x.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            *current = Some(m.to_string());
        }
    }
}

pub(crate) fn bootstrap_pi_model(path: &Path, up_to: u64) -> Option<String> {
    let file = File::open(path).ok()?;
    let mut reader = BufReader::new(file);
    let mut buf = String::new();
    let mut read: u64 = 0;
    let mut model = None;
    while read < up_to {
        buf.clear();
        let n = reader.read_line(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        read += n as u64;
        note_pi_model_from_line(buf.trim(), &mut model);
    }
    model
}

/// When resuming mid-file, recover last seen model from prefix.
pub(crate) fn bootstrap_kimi_model(path: &Path, up_to: u64) -> Option<String> {
    let file = File::open(path).ok()?;
    let mut reader = BufReader::new(file);
    let mut buf = String::new();
    let mut read: u64 = 0;
    let mut model: Option<String> = None;
    loop {
        if read >= up_to {
            break;
        }
        buf.clear();
        let n = reader.read_line(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        let line_bytes = n as u64;
        if read + line_bytes > up_to {
            break;
        }
        read += line_bytes;
        note_kimi_model_from_line(buf.trim(), &mut model);
    }
    model
}

/// Scan `[0, up_to)` of a Codex session file for the latest `turn_context` model.
/// Used when incremental cursor resumes mid-file so subsequent token_count rows
/// still inherit the active model (ccusage carries `current_model` across the whole file).
#[cfg(test)]
pub(crate) fn bootstrap_codex_model(path: &Path, up_to: u64) -> Option<String> {
    bootstrap_codex_prefix(path, up_to).0
}

/// Mid-file resume: recover model + last `total_token_usage` from the already-scanned prefix.
///
/// Without `previous_total`, the first post-cursor `token_count` would look "advanced"
/// and could re-count a duplicate snapshot that ccusage would skip.
pub(crate) fn bootstrap_codex_prefix(
    path: &Path,
    up_to: u64,
) -> (Option<String>, Option<CodexRawTotals>, Option<bool>) {
    let Ok(file) = File::open(path) else {
        return (None, None, None);
    };
    let mut reader = BufReader::new(file);
    let mut buf = String::new();
    let mut read: u64 = 0;
    let mut model: Option<String> = None;
    let mut previous_total: Option<CodexRawTotals> = None;
    let mut service_tier_fast: Option<bool> = None;

    loop {
        if read >= up_to {
            break;
        }
        buf.clear();
        let Ok(n) = reader.read_line(&mut buf) else {
            break;
        };
        if n == 0 {
            break;
        }
        let line_bytes = n as u64;
        // Only fully-contained lines before the resume offset count.
        if read + line_bytes > up_to {
            break;
        }
        read += line_bytes;
        let line = buf.trim();
        if line.is_empty() {
            continue;
        }
        if line.contains("turn_context") || line.contains("thread_settings_applied") {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
                if v.get("type").and_then(|t| t.as_str()) == Some("turn_context") {
                    if let Some(m) = codex_model_from_value(&v) {
                        model = Some(m);
                    }
                }
                if v.get("type").and_then(|t| t.as_str()) == Some("event_msg")
                    && v.pointer("/payload/type").and_then(|t| t.as_str())
                        == Some("thread_settings_applied")
                {
                    if let Some(tier) = v
                        .pointer("/payload/thread_settings/service_tier")
                        .and_then(|t| t.as_str())
                    {
                        service_tier_fast = match tier.trim().to_ascii_lowercase().as_str() {
                            "fast" | "priority" => Some(true),
                            "default" | "standard" => Some(false),
                            _ => None,
                        };
                    }
                }
            }
        }
        if line.contains("token_count") && line.contains("total_token_usage") {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
                if v.get("type").and_then(|t| t.as_str()) == Some("event_msg")
                    && v.pointer("/payload/type").and_then(|t| t.as_str()) == Some("token_count")
                {
                    if let Some(total) = v
                        .pointer("/payload/info/total_token_usage")
                        .filter(|u| u.is_object())
                    {
                        previous_total = Some(codex_raw_totals_from_value(total));
                        if let Some(m) = codex_model_from_value(&v) {
                            model = Some(m);
                        }
                    }
                }
            }
        }
    }
    (model, previous_total, service_tier_fast)
}

/// Real Kimi Code + ccusage wire.jsonl:
/// - new (this machine): type=usage.record + usageScope=turn + model from line
/// - old: StatusUpdate + token_usage; model from config.toml / llm.request inheritance
/// - never hardcodes a product model id when unknown → `"unknown"`
pub(crate) fn extract_kimi(
    line: &str,
    session_id: Option<&str>,
    model_hint: Option<&str>,
) -> std::result::Result<Option<ParsedUsageEvent>, ()> {
    let v: serde_json::Value = serde_json::from_str(line).map_err(|_| ())?;
    let ty = v.get("type").and_then(|t| t.as_str());

    // New Kimi Code format (observed on ~/.kimi-code)
    if ty == Some("usage.record") {
        if v.get("usageScope").and_then(|s| s.as_str()) != Some("turn") {
            return Ok(None); // session-scoped rows are cumulative
        }
        let usage = v.get("usage").filter(|u| u.is_object()).ok_or(())?;
        let input = token_field(usage, &["inputOther", "input_other"]);
        let output = token_field(usage, &["output"]);
        let cache_create = token_field(usage, &["inputCacheCreation", "input_cache_creation"]);
        let cache_read = token_field(usage, &["inputCacheRead", "input_cache_read"]);
        if input == 0 && output == 0 && cache_create == 0 && cache_read == 0 {
            return Ok(None);
        }
        let model = resolve_kimi_model(v.get("model").and_then(|m| m.as_str()), model_hint);
        let ts = v
            .get("time")
            .and_then(|t| t.as_i64())
            .and_then(chrono::DateTime::from_timestamp_millis)
            .map(|d| d.to_rfc3339())
            .or_else(|| find_ts(&v))
            .unwrap_or_else(now_iso);
        let raw_hash = dedupe_hash(AgentId::Kimi, Some(&ts), Some(&model), line);
        return Ok(Some(ParsedUsageEvent {
            agent_id: AgentId::Kimi,
            model,
            input_tokens: input,
            output_tokens: output,
            cache_creation_tokens: cache_create,
            cache_creation_1h_tokens: 0,
            cache_read_tokens: cache_read,
            session_id: session_id.map(|s| s.to_string()),
            ts,
            raw_hash,
            cost_usd: None,
            fast: false,
        }));
    }

    if ty == Some("metadata") || ty == Some("llm.request") {
        return Ok(None);
    }

    // Old wire format: StatusUpdate + token_usage
    let msg = v.get("message").filter(|m| m.is_object());
    let Some(msg) = msg else {
        return Ok(None);
    };
    if msg.get("type").and_then(|t| t.as_str()) != Some("StatusUpdate") {
        return Ok(None);
    }
    let payload = msg.get("payload").filter(|p| p.is_object()).ok_or(())?;
    let usage = payload
        .get("token_usage")
        .filter(|u| u.is_object())
        .ok_or(())?;

    let input = token_field(usage, &["input_other", "inputOther"]);
    let mut output = token_field(usage, &["output"]);
    let cache_create = token_field(usage, &["input_cache_creation", "inputCacheCreation"]);
    let cache_read = token_field(usage, &["input_cache_read", "inputCacheRead"]);
    let total = token_field(usage, &["total"]);
    // ccusage apply_total_token_fallback: if parts empty but total set → put on output
    if input == 0 && output == 0 && cache_create == 0 && cache_read == 0 && total > 0 {
        output = total;
    }
    if input == 0 && output == 0 && cache_create == 0 && cache_read == 0 {
        return Ok(None);
    }

    let message_id = payload
        .get("message_id")
        .and_then(|m| m.as_str())
        .map(str::to_string);
    // Prefer payload/model if present, else inherited hint from config/llm.request
    let model = resolve_kimi_model(
        payload
            .get("model")
            .and_then(|m| m.as_str())
            .or_else(|| v.get("model").and_then(|m| m.as_str())),
        model_hint,
    );
    let ts = v
        .get("timestamp")
        .and_then(|t| t.as_f64())
        .and_then(|secs| chrono::DateTime::from_timestamp_millis((secs * 1000.0) as i64))
        .map(|d| d.to_rfc3339())
        .or_else(|| find_ts(&v))
        .unwrap_or_else(now_iso);
    let raw_hash = dedupe_hash(AgentId::Kimi, message_id.as_deref(), Some(&ts), line);

    Ok(Some(ParsedUsageEvent {
        agent_id: AgentId::Kimi,
        model,
        input_tokens: input,
        output_tokens: output,
        cache_creation_tokens: cache_create,
        cache_creation_1h_tokens: 0,
        cache_read_tokens: cache_read,
        session_id: session_id.map(|s| s.to_string()),
        ts,
        raw_hash,
        cost_usd: None,
        fast: false,
    }))
}

/// Line model → hint → `"unknown"` (never invent kimi-for-coding).
fn resolve_kimi_model(line_model: Option<&str>, hint: Option<&str>) -> String {
    if let Some(m) = line_model
        .map(normalize_kimi_model)
        .filter(|s| !s.is_empty())
    {
        return m;
    }
    if let Some(m) = hint.map(str::trim).filter(|s| !s.is_empty()) {
        return normalize_kimi_model(m);
    }
    "unknown".into()
}

/// ccusage Pi: type=message, role=assistant, usage.{input,output,cacheRead,cacheWrite,cost.total}
/// Model: message.model → optional settings/model_change hint → `"unknown"` (never invent).
pub(crate) fn extract_pi(
    line: &str,
    session_id: Option<&str>,
    model_hint: Option<&str>,
) -> std::result::Result<Option<ParsedUsageEvent>, ()> {
    let v: serde_json::Value = serde_json::from_str(line).map_err(|_| ())?;
    if let Some(ty) = v.get("type").and_then(|t| t.as_str()) {
        if ty != "message" {
            return Ok(None);
        }
    }
    let msg = v.get("message").filter(|m| m.is_object()).ok_or(())?;
    if msg.get("role").and_then(|r| r.as_str()) != Some("assistant") {
        return Ok(None);
    }
    let usage = msg.get("usage").filter(|u| u.is_object()).ok_or(())?;

    let input = token_field(usage, &["input", "input_tokens", "inputTokens"]);
    let mut output = token_field(usage, &["output", "output_tokens", "outputTokens"]);
    let cache_read = token_field(
        usage,
        &["cacheRead", "cache_read", "cache_read_input_tokens"],
    );
    let cache_create = token_field(
        usage,
        &["cacheWrite", "cache_write", "cache_creation_input_tokens"],
    );
    let total = token_field(usage, &["totalTokens", "total_tokens", "total"]);
    if input == 0 && output == 0 && cache_read == 0 && cache_create == 0 && total > 0 {
        output = total;
    }
    if input == 0 && output == 0 && cache_read == 0 && cache_create == 0 {
        return Ok(None);
    }

    let model = msg
        .get("model")
        .or_else(|| msg.get("modelId"))
        .and_then(|m| m.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .or_else(|| {
            model_hint
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
        })
        .unwrap_or_else(|| "unknown".into());
    let ts = find_ts(&v).unwrap_or_else(now_iso);
    // Pi cost: usage.cost.total (USD). Non-object cost is ignored (ccusage).
    let cost_usd = usage
        .get("cost")
        .and_then(|c| c.as_object())
        .and_then(|o| o.get("total"))
        .and_then(|t| t.as_f64())
        .filter(|c| c.is_finite() && *c >= 0.0);

    let raw_hash = dedupe_hash(AgentId::Pi, Some(&ts), Some(&model), line);
    Ok(Some(ParsedUsageEvent {
        agent_id: AgentId::Pi,
        model,
        input_tokens: input,
        output_tokens: output,
        cache_creation_tokens: cache_create,
        cache_creation_1h_tokens: 0,
        cache_read_tokens: cache_read,
        session_id: session_id.map(|s| s.to_string()),
        ts,
        raw_hash,
        cost_usd,
        fast: false,
    }))
}

/// DeepSeek Harness session events: only explicit provider usage.
/// Token Meter heuristics (`surfaceTokens` / `estimated`) are never billed.
pub(crate) fn extract_dsh(
    line: &str,
    session_id: Option<&str>,
    model_hint: Option<&str>,
) -> std::result::Result<Option<ParsedUsageEvent>, ()> {
    let v: serde_json::Value = serde_json::from_str(line).map_err(|_| ())?;
    if is_dsh_heuristic_usage(&v) {
        return Ok(None);
    }
    let ty = v.get("type").and_then(|t| t.as_str()).unwrap_or("");
    if ty == "request/header" || ty.ends_with("/header") || ty == "session" {
        return Ok(None);
    }
    if v.get("seed").and_then(|s| s.as_bool()) == Some(true) || ty.contains("seed") {
        return Ok(None);
    }

    let usage = v
        .get("usage")
        .or_else(|| v.pointer("/message/usage"))
        .or_else(|| v.pointer("/response/usage"))
        .or_else(|| v.pointer("/payload/usage"))
        .filter(|u| u.is_object());
    let Some(usage) = usage else {
        return Ok(None);
    };
    if usage.get("estimated").and_then(|e| e.as_bool()) == Some(true) {
        return Ok(None);
    }
    if usage.get("surfaceTokens").is_some()
        && usage.get("input_tokens").is_none()
        && usage.get("inputTokens").is_none()
        && usage.get("output_tokens").is_none()
        && usage.get("outputTokens").is_none()
    {
        return Ok(None);
    }

    let input = token_field(
        usage,
        &["input_tokens", "inputTokens", "prompt_tokens", "input"],
    );
    let output = token_field(
        usage,
        &[
            "output_tokens",
            "outputTokens",
            "completion_tokens",
            "output",
        ],
    );
    let cache_read = token_field(
        usage,
        &[
            "cache_read_input_tokens",
            "cache_read_tokens",
            "cacheRead",
            "cached_input_tokens",
        ],
    );
    let cache_create = token_field(
        usage,
        &[
            "cache_creation_input_tokens",
            "cache_creation_tokens",
            "cacheWrite",
            "cache_write",
        ],
    );
    if input == 0 && output == 0 && cache_read == 0 && cache_create == 0 {
        return Ok(None);
    }

    let model = find_model(&v)
        .or_else(|| find_model_in(usage))
        .or_else(|| {
            model_hint
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
        })
        .unwrap_or_else(|| "unknown".into());
    let ts = find_ts(&v).unwrap_or_else(now_iso);
    let cost_usd = usage
        .get("costUSD")
        .or_else(|| usage.get("cost_usd"))
        .or_else(|| usage.pointer("/cost/total"))
        .or_else(|| v.get("costUSD"))
        .and_then(|c| c.as_f64())
        .filter(|c| c.is_finite() && *c >= 0.0);
    let sid = v
        .get("sessionId")
        .or_else(|| v.get("session_id"))
        .or_else(|| v.pointer("/header/id"))
        .and_then(|x| x.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .or_else(|| session_id.map(|s| s.to_string()));
    let raw_hash = dedupe_hash(AgentId::Dsh, Some(&ts), Some(&model), line);
    Ok(Some(ParsedUsageEvent {
        agent_id: AgentId::Dsh,
        model,
        input_tokens: input,
        output_tokens: output,
        cache_creation_tokens: cache_create,
        cache_creation_1h_tokens: 0,
        cache_read_tokens: cache_read,
        session_id: sid,
        ts,
        raw_hash,
        cost_usd,
        fast: false,
    }))
}

fn is_dsh_heuristic_usage(v: &serde_json::Value) -> bool {
    let ty = v.get("type").and_then(|t| t.as_str()).unwrap_or("");
    ty.contains("token-meter")
        || ty.contains("tokenMeter")
        || v.get("estimated").and_then(|e| e.as_bool()) == Some(true)
        || (v.get("surfaceTokens").is_some() && v.get("usage").is_none())
}

/// Claude / WorkBuddy generic assistant-log shape (ccusage UsageEntry).
pub(crate) fn extract_claude_like(
    agent: AgentId,
    line: &str,
    session_id: Option<&str>,
) -> std::result::Result<Option<ParsedUsageEvent>, ()> {
    let v: serde_json::Value = serde_json::from_str(line).map_err(|_| ())?;

    // Prefer nested message.usage (ccusage schema)
    let usage = v
        .pointer("/message/usage")
        .or_else(|| v.get("usage"))
        .or_else(|| v.pointer("/response/usage"))
        .filter(|u| u.is_object());

    let Some(usage) = usage else {
        // flat tokens on root
        if v.get("input_tokens").is_none() && v.get("output_tokens").is_none() {
            return Ok(None);
        }
        return extract_from_usage_obj(agent, &v, &v, session_id, line);
    };

    extract_from_usage_obj(agent, &v, usage, session_id, line)
}

fn extract_from_usage_obj(
    agent: AgentId,
    root: &serde_json::Value,
    usage: &serde_json::Value,
    session_id: Option<&str>,
    line: &str,
) -> std::result::Result<Option<ParsedUsageEvent>, ()> {
    let input = token_field(usage, &["input_tokens", "inputTokens", "prompt_tokens"]);
    let output = token_field(
        usage,
        &["output_tokens", "outputTokens", "completion_tokens"],
    );
    let cache_read = token_field(
        usage,
        &[
            "cache_read_input_tokens",
            "cache_read_tokens",
            "cacheReadTokens",
            "cached_input_tokens",
        ],
    );
    let mut cache_create = token_field(
        usage,
        &["cache_creation_input_tokens", "cache_creation_tokens"],
    );
    let mut cache_create_1h = 0;
    // ccusage: prefer cache_creation 5m/1h breakdown when present.
    if let Some(cc) = usage.get("cache_creation").filter(|c| c.is_object()) {
        let a = token_field(cc, &["ephemeral_5m_input_tokens"]);
        let b = token_field(cc, &["ephemeral_1h_input_tokens"]);
        if a + b > 0 {
            cache_create = a;
            cache_create_1h = b;
        }
    }

    if input == 0 && output == 0 && cache_read == 0 && cache_create == 0 && cache_create_1h == 0 {
        return Ok(None);
    }

    let model = find_model(root)
        .or_else(|| find_model_in(usage))
        .filter(|m| m != "<synthetic>")
        .unwrap_or_else(|| "unknown".into());

    let ts = find_ts(root).unwrap_or_else(now_iso);
    let cost_usd = root
        .get("costUSD")
        .or_else(|| root.get("cost_usd"))
        .and_then(|c| c.as_f64())
        .filter(|c| c.is_finite() && *c >= 0.0);

    let message_id = root
        .pointer("/message/id")
        .and_then(|x| x.as_str())
        .map(str::to_string);
    let request_id = root
        .get("requestId")
        .or_else(|| root.get("request_id"))
        .and_then(|x| x.as_str())
        .map(str::to_string);

    // Session id: log field → path stem
    let sid = root
        .get("sessionId")
        .or_else(|| root.get("session_id"))
        .and_then(|x| x.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .or_else(|| session_id.map(|s| s.to_string()));

    let raw_hash = dedupe_hash(agent, message_id.as_deref(), request_id.as_deref(), line);

    Ok(Some(ParsedUsageEvent {
        agent_id: agent,
        model,
        input_tokens: input,
        output_tokens: output,
        cache_creation_tokens: cache_create,
        cache_creation_1h_tokens: cache_create_1h,
        cache_read_tokens: cache_read,
        session_id: sid,
        ts,
        raw_hash,
        cost_usd,
        fast: false,
    }))
}

/// Per-file Codex parse state (ccusage-aligned).
#[derive(Debug, Clone, Default)]
pub(crate) struct CodexParseState {
    pub model: Option<String>,
    /// True when session_meta has `forked_from_id` / `parent_thread_id`.
    pub forkish: bool,
    /// True when forkish and the file opens with a dense rewritten-history burst.
    pub burst_skip_active: bool,
    /// Diagnostics: events emitted / skipped (for end-of-file debug logs).
    pub emitted: u64,
    pub skipped_dup_total: u64,
    pub skipped_burst: u64,
    /// Last seen `total_token_usage` (full OpenAI layout) for cumulative-advance filter.
    previous_total: Option<CodexRawTotals>,
    /// Fork/subagent: skip rewritten parent-history burst at session open.
    replay: CodexReplaySkip,
    /// `thread_settings_applied` Fast/Priority. `None` = not recorded this file.
    service_tier_fast: Option<bool>,
    /// `config.toml service_tier = fast|priority` (ccusage Auto when unrecorded).
    config_fast: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct CodexRawTotals {
    input: i64,
    cached: i64,
    output: i64,
}

#[derive(Debug, Clone, Default)]
enum CodexReplaySkip {
    #[default]
    Done,
    /// Skipping dense open burst; last skipped event timestamp (unix ms).
    SkippingBurst { last_ms: i64 },
}

/// ccusage: longest pause still treated as the same rewritten parent-history burst.
const CODEX_REWRITTEN_BURST_PAUSE_MS: i64 = 1_000;

impl CodexParseState {
    #[cfg(test)]
    pub fn with_model(model: Option<String>) -> Self {
        Self {
            model,
            ..Default::default()
        }
    }

    fn event_is_fast(&self) -> bool {
        self.service_tier_fast.unwrap_or(self.config_fast)
    }

    fn apply_service_tier(&mut self, recorded: &str) {
        self.service_tier_fast = match recorded.trim().to_ascii_lowercase().as_str() {
            "fast" | "priority" => Some(true),
            "default" | "standard" => Some(false),
            _ => None,
        };
    }

    /// Resume mid-file: inherit model + last cumulative total from the scanned prefix.
    pub fn resume_from_prefix(
        model: Option<String>,
        previous_total: Option<CodexRawTotals>,
        service_tier_fast: Option<bool>,
        config_fast: bool,
    ) -> Self {
        Self {
            model,
            previous_total,
            // Past session open — never apply rewritten-history burst on incremental resume.
            forkish: false,
            burst_skip_active: false,
            replay: CodexReplaySkip::Done,
            service_tier_fast,
            config_fast,
            ..Default::default()
        }
    }

    /// Detect fork/subagent session and whether it opens with a rewritten-history burst.
    ///
    /// ccusage: multi-agent children replay parent usage at the fork instant; those
    /// events must not be billed again (see ccusage `replay.rs` / `parser.rs`).
    pub fn init_from_file(path: &Path, model: Option<String>, config_fast: bool) -> Self {
        let forkish = codex_file_is_forkish(path);
        let (replay, burst_skip_active) = if forkish {
            if let Some(first_ms) = codex_detect_rewritten_burst(path) {
                (CodexReplaySkip::SkippingBurst { last_ms: first_ms }, true)
            } else {
                (CodexReplaySkip::Done, false)
            }
        } else {
            (CodexReplaySkip::Done, false)
        };
        Self {
            model,
            forkish,
            burst_skip_active,
            emitted: 0,
            skipped_dup_total: 0,
            skipped_burst: 0,
            previous_total: None,
            replay,
            service_tier_fast: None,
            config_fast,
        }
    }
}

fn codex_file_is_forkish(path: &Path) -> bool {
    let Ok(file) = File::open(path) else {
        return false;
    };
    let mut reader = BufReader::new(file);
    let mut buf = String::new();
    for _ in 0..8 {
        buf.clear();
        if reader.read_line(&mut buf).ok().unwrap_or(0) == 0 {
            break;
        }
        let line = buf.trim();
        if line.is_empty() {
            continue;
        }
        if !(line.contains("forked_from_id") || line.contains("parent_thread_id")) {
            continue;
        }
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
            let p = v.get("payload");
            if p.and_then(|x| x.get("forked_from_id")).is_some()
                || p.and_then(|x| x.get("parent_thread_id")).is_some()
            {
                return true;
            }
        }
    }
    false
}

/// If the first two usage events are written within 1s, treat the open as a
/// rewritten parent-history burst (ccusage `detect_rewritten_burst`).
fn codex_detect_rewritten_burst(path: &Path) -> Option<i64> {
    let file = File::open(path).ok()?;
    let mut reader = BufReader::new(file);
    let mut buf = String::new();
    let mut first_ms: Option<i64> = None;
    loop {
        buf.clear();
        if reader.read_line(&mut buf).ok()? == 0 {
            return None;
        }
        let line = buf.trim();
        if line.is_empty() || !line_might_have_usage_codex(line) {
            continue;
        }
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if v.get("type").and_then(|t| t.as_str()) != Some("event_msg") {
            continue;
        }
        let payload = v.get("payload")?;
        if payload.get("type").and_then(|t| t.as_str()) != Some("token_count") {
            continue;
        }
        let info = payload.get("info")?;
        if info.get("last_token_usage").is_none() && info.get("total_token_usage").is_none() {
            continue;
        }
        let ms = find_ts(&v).as_deref().and_then(parse_iso_ms)?;
        match first_ms {
            None => first_ms = Some(ms),
            Some(first) => {
                return (0..=CODEX_REWRITTEN_BURST_PAUSE_MS)
                    .contains(&(ms - first))
                    .then_some(first);
            }
        }
    }
}

/// Parse ISO-8601 / RFC3339 timestamps to unix milliseconds.
///
/// Strips a trailing `Z` or `±HH:MM` / `±HHMM` offset and treats the remaining
/// wall-clock as UTC. Good enough for **burst gap** comparisons (same offset
/// within a file); not used for calendar-day aggregation.
pub(crate) fn parse_iso_ms(ts: &str) -> Option<i64> {
    let s = ts.trim();
    if s.is_empty() {
        return None;
    }
    // Drop timezone suffix: Z | ±HH:MM | ±HHMM
    let s = if let Some(rest) = s.strip_suffix('Z') {
        rest
    } else if s.len() >= 6 {
        let bytes = s.as_bytes();
        let last = bytes[bytes.len() - 1];
        // ...±HH:MM
        if bytes.len() >= 6
            && (bytes[bytes.len() - 6] == b'+' || bytes[bytes.len() - 6] == b'-')
            && bytes[bytes.len() - 3] == b':'
        {
            &s[..s.len() - 6]
        } else if bytes.len() >= 5
            && (bytes[bytes.len() - 5] == b'+' || bytes[bytes.len() - 5] == b'-')
            && last.is_ascii_digit()
        {
            // ...±HHMM
            &s[..s.len() - 5]
        } else {
            s
        }
    } else {
        s
    };

    let (date, time) = s.split_once('T').or_else(|| s.split_once(' '))?;
    let mut d = date.split('-');
    let y: i64 = d.next()?.parse().ok()?;
    let mo: i64 = d.next()?.parse().ok()?;
    let day: i64 = d.next()?.parse().ok()?;
    // Drop any leftover offset fragments on time (defensive).
    let time = time
        .split_once('+')
        .map(|(t, _)| t)
        .or_else(|| {
            // only split on timezone-style trailing -HH:MM, not negative years
            let b = time.as_bytes();
            if let Some(i) = time.rfind('-') {
                if i > 0 && b.get(i + 1).is_some_and(|c| c.is_ascii_digit()) {
                    return Some(&time[..i]);
                }
            }
            None
        })
        .unwrap_or(time);
    let (hms, frac) = match time.split_once('.') {
        Some((hms, rest)) => {
            let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
            let ms = match digits.len() {
                0 => 0,
                1 => digits.parse::<i64>().ok()? * 100,
                2 => digits.parse::<i64>().ok()? * 10,
                3 => digits.parse().ok()?,
                _ => digits.get(..3)?.parse().ok()?,
            };
            (hms, ms)
        }
        None => (time, 0),
    };
    let mut t = hms.split(':');
    let h: i64 = t.next()?.parse().ok()?;
    let mi: i64 = t.next()?.parse().ok()?;
    let se: i64 = t.next()?.parse().ok()?;
    if !(1..=12).contains(&mo) || !(1..=31).contains(&day) || h > 23 || mi > 59 || se > 60 {
        return None;
    }
    // days from civil date (Howard Hinnant) → unix days
    let y = if mo <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * (if mo > 2 { mo - 3 } else { mo + 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let unix_days = era * 146097 + doe - 719468;
    Some(unix_days * 86_400_000 + h * 3_600_000 + mi * 60_000 + se * 1_000 + frac)
}

fn codex_raw_totals_from_value(usage: &serde_json::Value) -> CodexRawTotals {
    CodexRawTotals {
        input: token_field(usage, &["input_tokens"]).max(0),
        cached: token_field(usage, &["cached_input_tokens", "cache_read_input_tokens"]).max(0),
        output: token_field(usage, &["output_tokens"]).max(0),
    }
}

fn subtract_codex_totals(cur: CodexRawTotals, prev: Option<CodexRawTotals>) -> CodexRawTotals {
    match prev {
        None => cur,
        Some(p) => CodexRawTotals {
            input: (cur.input - p.input).max(0),
            cached: (cur.cached - p.cached).max(0),
            output: (cur.output - p.output).max(0),
        },
    }
}

/// Codex session log (ccusage):
/// - `turn_context` updates `current_model` (no tokens)
/// - `event_msg` / `token_count` emits usage when cumulative totals advance
/// - fork/subagent: skip rewritten parent-history burst at open
pub(crate) fn extract_codex(
    line: &str,
    session_id: Option<&str>,
    state: &mut CodexParseState,
) -> std::result::Result<Option<ParsedUsageEvent>, ()> {
    let v: serde_json::Value = serde_json::from_str(line).map_err(|_| ())?;

    // ccusage: turn_context carries model metadata for subsequent token_count rows
    if v.get("type").and_then(|t| t.as_str()) == Some("turn_context") {
        if let Some(m) = codex_model_from_value(&v) {
            state.model = Some(m);
        }
        return Ok(None);
    }

    if v.get("type").and_then(|t| t.as_str()) == Some("event_msg")
        && v.pointer("/payload/type").and_then(|t| t.as_str()) == Some("thread_settings_applied")
    {
        if let Some(tier) = v
            .pointer("/payload/thread_settings/service_tier")
            .and_then(|t| t.as_str())
        {
            state.apply_service_tier(tier);
        }
        return Ok(None);
    }

    // Only event_msg / token_count carries usage (session path).
    // Do not fall back to extract_claude_like: that path does not peel
    // OpenAI-style cached_input_tokens out of input, so generic `usage`
    // rows would double-count cache in totals and cost.
    let is_token_count = v.get("type").and_then(|t| t.as_str()) == Some("event_msg")
        && v.pointer("/payload/type").and_then(|t| t.as_str()) == Some("token_count");

    if !is_token_count {
        return Ok(None);
    }

    let info = v.pointer("/payload/info");
    let total_usage = info
        .and_then(|i| i.get("total_token_usage"))
        .filter(|u| u.is_object())
        .map(codex_raw_totals_from_value);
    let last_usage = info
        .and_then(|i| i.get("last_token_usage"))
        .filter(|u| u.is_object())
        .map(codex_raw_totals_from_value);

    // ccusage: only count when total_token_usage advanced (skip duplicate snapshots).
    let cumulative_advanced = match (&total_usage, &state.previous_total) {
        (Some(t), Some(p)) => t != p,
        _ => true,
    };

    // Prefer last_token_usage when totals advanced; else delta of totals.
    let raw = if cumulative_advanced {
        last_usage.or_else(|| total_usage.map(|t| subtract_codex_totals(t, state.previous_total)))
    } else {
        state.skipped_dup_total = state.skipped_dup_total.saturating_add(1);
        None
    };

    if let Some(t) = total_usage {
        state.previous_total = Some(t);
    }

    let Some(raw) = raw else {
        return Ok(None);
    };

    // OpenAI/Codex: input_tokens includes cached_input_tokens.
    // Store non-cached (ccusage daily inputTokens).
    let full_input = raw.input.max(0);
    let cache_read = raw.cached.max(0).min(full_input);
    let billable_input = (full_input - cache_read).max(0);
    let output = raw.output.max(0);
    if billable_input == 0 && output == 0 && cache_read == 0 {
        return Ok(None);
    }

    // Fork rewritten-history burst skip (ccusage SkippingRewrittenBurst).
    let ts = find_ts(&v).unwrap_or_else(now_iso);
    if let CodexReplaySkip::SkippingBurst { last_ms } = state.replay {
        match parse_iso_ms(&ts) {
            Some(ms) if (0..=CODEX_REWRITTEN_BURST_PAUSE_MS).contains(&(ms - last_ms)) => {
                state.replay = CodexReplaySkip::SkippingBurst { last_ms: ms };
                state.skipped_burst = state.skipped_burst.saturating_add(1);
                return Ok(None);
            }
            Some(_) => {
                // Pause > threshold: child work begins.
                state.replay = CodexReplaySkip::Done;
                state.burst_skip_active = false;
            }
            None => {
                // Unparseable ts while still in burst: stay conservative, keep skipping.
                state.skipped_burst = state.skipped_burst.saturating_add(1);
                return Ok(None);
            }
        }
    }

    let parsed_model = codex_model_from_value(&v);
    let model = resolve_codex_model(parsed_model, &mut state.model);
    let raw_hash = dedupe_hash(AgentId::Codex, None, None, line);
    state.emitted = state.emitted.saturating_add(1);
    Ok(Some(ParsedUsageEvent {
        agent_id: AgentId::Codex,
        model,
        input_tokens: billable_input,
        output_tokens: output,
        cache_creation_tokens: 0,
        cache_creation_1h_tokens: 0,
        cache_read_tokens: cache_read,
        session_id: session_id.map(|s| s.to_string()),
        ts,
        raw_hash,
        cost_usd: None,
        fast: state.event_is_fast(),
    }))
}

/// Pull model from Codex JSON: payload / info / metadata / model_name (ccusage fields).
fn codex_model_from_value(v: &serde_json::Value) -> Option<String> {
    const PATHS: &[&str] = &[
        "/payload/model",
        "/payload/model_name",
        "/payload/metadata/model",
        "/payload/info/model",
        "/payload/info/model_name",
        "/payload/info/metadata/model",
        "/model",
        "/model_name",
        "/info/model",
    ];
    for p in PATHS {
        if let Some(s) = v
            .pointer(p)
            .and_then(|x| x.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            return Some(s.to_string());
        }
    }
    None
}

/// Prefer parsed line model, else inherited (turn_context / config.toml), else `"unknown"`.
/// Never invent a product model id (ccusage used gpt-5; AgentHub reads real config instead).
fn resolve_codex_model(parsed: Option<String>, current_model: &mut Option<String>) -> String {
    if let Some(m) = parsed
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
    {
        *current_model = Some(m.clone());
        return m;
    }
    if let Some(m) = current_model
        .as_ref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
    {
        return m;
    }
    "unknown".into()
}

/// ccusage-style dedupe: prefer message_id + request_id; else content hash.
fn dedupe_hash(
    agent: AgentId,
    message_id: Option<&str>,
    request_id: Option<&str>,
    line: &str,
) -> String {
    if let Some(mid) = message_id.filter(|s| !s.is_empty()) {
        let mut hasher = Sha256::new();
        hasher.update(agent.as_str().as_bytes());
        hasher.update(b"|");
        hasher.update(mid.as_bytes());
        hasher.update(b"|");
        hasher.update(request_id.unwrap_or("").as_bytes());
        return format!("{:x}", hasher.finalize());
    }
    let mut hasher = Sha256::new();
    hasher.update(agent.as_str().as_bytes());
    hasher.update(b"|");
    hasher.update(line.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn find_model(v: &serde_json::Value) -> Option<String> {
    find_model_in(v).or_else(|| {
        v.pointer("/message/model")
            .and_then(|x| x.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
    })
}

fn find_model_in(v: &serde_json::Value) -> Option<String> {
    for k in ["model", "modelName", "model_name"] {
        if let Some(s) = v.get(k).and_then(|x| x.as_str()) {
            if !s.is_empty() {
                return Some(s.to_string());
            }
        }
    }
    None
}

fn find_ts(v: &serde_json::Value) -> Option<String> {
    for k in ["timestamp", "ts", "created_at", "createdAt", "time"] {
        if let Some(s) = v.get(k).and_then(|x| x.as_str()) {
            if !s.is_empty() {
                return Some(s.to_string());
            }
        }
        if let Some(n) = v.get(k).and_then(|x| x.as_i64()) {
            if let Some(dt) = chrono::DateTime::from_timestamp(n, 0) {
                return Some(dt.to_rfc3339());
            }
            if let Some(dt) = chrono::DateTime::from_timestamp(n / 1000, 0) {
                return Some(dt.to_rfc3339());
            }
        }
    }
    None
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn token_field(v: &serde_json::Value, keys: &[&str]) -> i64 {
    for k in keys {
        if let Some(n) = v.get(*k).and_then(|x| x.as_i64()) {
            return n.max(0);
        }
        if let Some(n) = v.get(*k).and_then(|x| x.as_u64()) {
            return n.min(i64::MAX as u64) as i64;
        }
        if let Some(n) = v.get(*k).and_then(|x| x.as_f64()) {
            return n.max(0.0) as i64;
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::usage::parse_file_for_agent_id;
    use crate::storage::Database;
    use tempfile::tempdir;

    #[test]
    fn extracts_ccusage_claude_fixture_shape() {
        // From ccusage apps/ccusage/test/fixtures/claude/.../chat.jsonl
        let line = r#"{"timestamp":"2026-01-09T10:00:00.000Z","sessionId":"session-alpha","message":{"role":"assistant","id":"msg-alpha-1","model":"claude-sonnet-4-20250514","usage":{"input_tokens":100,"output_tokens":50,"cache_creation_input_tokens":25,"cache_read_input_tokens":10}},"requestId":"req-alpha-1","costUSD":0.12,"version":"1.0.0"}"#;
        let ev = extract_claude_like(AgentId::Claude, line, Some("sess1"))
            .unwrap()
            .unwrap();
        assert_eq!(ev.input_tokens, 100);
        assert_eq!(ev.output_tokens, 50);
        assert_eq!(ev.cache_creation_tokens, 25);
        assert_eq!(ev.cache_read_tokens, 10);
        assert_eq!(ev.model, "claude-sonnet-4-20250514");
        assert_eq!(ev.cost_usd, Some(0.12));
        assert_eq!(ev.session_id.as_deref(), Some("session-alpha"));
        // message_id+request_id dedupe (not full line hash only)
        assert!(!ev.raw_hash.is_empty());
    }

    #[test]
    fn extracts_ccusage_codex_token_count() {
        // When model is present on the token_count info itself
        let line = r#"{"timestamp":"2026-05-13T09:01:00.000Z","type":"event_msg","payload":{"type":"token_count","info":{"model":"gpt-5.2-codex","last_token_usage":{"input_tokens":1000,"cached_input_tokens":250,"output_tokens":125,"reasoning_output_tokens":75,"total_tokens":1200}}}}"#;
        let mut state = CodexParseState::default();
        let ev = extract_codex(line, Some("s1"), &mut state)
            .unwrap()
            .unwrap();
        // Stored input is non-cached (ccusage inputTokens), cache kept separate
        assert_eq!(ev.input_tokens, 750);
        // reasoning is informational; output_tokens already bills it (ccusage)
        assert_eq!(ev.output_tokens, 125);
        assert_eq!(ev.cache_read_tokens, 250);
        assert_eq!(ev.model, "gpt-5.2-codex");
    }

    #[test]
    fn codex_skips_generic_usage_that_would_not_peel_cache() {
        let mut state = CodexParseState::default();
        let line = r#"{"timestamp":"2026-05-13T09:01:00.000Z","type":"response","usage":{"input_tokens":1000,"cached_input_tokens":250,"output_tokens":10}}"#;
        assert!(extract_codex(line, Some("s1"), &mut state)
            .unwrap()
            .is_none());
    }

    #[test]
    fn codex_inherits_model_from_turn_context() {
        // Real Codex logs: model only on turn_context; token_count has none.
        let mut state = CodexParseState::default();
        let ctx = r#"{"timestamp":"2026-08-03T03:12:03.326Z","type":"turn_context","payload":{"model":"gpt-5.6-sol"}}"#;
        assert!(extract_codex(ctx, Some("s1"), &mut state)
            .unwrap()
            .is_none());
        assert_eq!(state.model.as_deref(), Some("gpt-5.6-sol"));

        let tok = r#"{"timestamp":"2026-08-03T03:12:09.556Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":21650,"cached_input_tokens":11008,"output_tokens":217,"reasoning_output_tokens":60,"total_tokens":21867},"last_token_usage":{"input_tokens":21650,"cached_input_tokens":11008,"output_tokens":217,"reasoning_output_tokens":60,"total_tokens":21867},"model_context_window":258400}}}"#;
        let ev = extract_codex(tok, Some("s1"), &mut state).unwrap().unwrap();
        assert_eq!(ev.model, "gpt-5.6-sol");
        assert_eq!(ev.input_tokens, 21650 - 11008);
        assert_eq!(ev.output_tokens, 217);
        assert_eq!(ev.cache_read_tokens, 11008);
    }

    #[test]
    fn codex_falls_back_to_unknown_without_model() {
        // Never invent gpt-5 — real model comes from turn_context or config.toml.
        let mut state = CodexParseState::default();
        let tok = r#"{"timestamp":"2026-05-13T09:01:00.000Z","type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":10,"cached_input_tokens":0,"output_tokens":5,"reasoning_output_tokens":0,"total_tokens":15}}}}"#;
        let ev = extract_codex(tok, None, &mut state).unwrap().unwrap();
        assert_eq!(ev.model, "unknown");
        assert!(state.model.is_none());
    }

    #[test]
    fn codex_uses_config_seeded_model_when_token_count_has_none() {
        let mut state = CodexParseState::with_model(Some("gpt-5.6-luna".into()));
        let tok = r#"{"timestamp":"2026-05-13T09:01:00.000Z","type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":10,"cached_input_tokens":0,"output_tokens":5,"reasoning_output_tokens":0,"total_tokens":15}}}}"#;
        let ev = extract_codex(tok, None, &mut state).unwrap().unwrap();
        assert_eq!(ev.model, "gpt-5.6-luna");
    }

    #[test]
    fn codex_skips_duplicate_total_token_usage() {
        // ccusage: unchanged total_token_usage → drop last_token_usage (issue #884).
        let mut state = CodexParseState::default();
        let a = r#"{"timestamp":"2026-05-13T09:01:00.000Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":100,"cached_input_tokens":0,"output_tokens":10,"total_tokens":110},"last_token_usage":{"input_tokens":100,"cached_input_tokens":0,"output_tokens":10,"total_tokens":110}}}}"#;
        let b = r#"{"timestamp":"2026-05-13T09:01:01.000Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":100,"cached_input_tokens":0,"output_tokens":10,"total_tokens":110},"last_token_usage":{"input_tokens":100,"cached_input_tokens":0,"output_tokens":10,"total_tokens":110}}}}"#;
        assert!(extract_codex(a, None, &mut state).unwrap().is_some());
        assert!(extract_codex(b, None, &mut state).unwrap().is_none());
        assert_eq!(state.skipped_dup_total, 1);
        assert_eq!(state.emitted, 1);
    }

    #[test]
    fn codex_total_delta_when_last_token_usage_missing() {
        let mut state = CodexParseState::default();
        let first = r#"{"timestamp":"2026-05-13T09:01:00.000Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":100,"cached_input_tokens":20,"output_tokens":5,"total_tokens":105}}}}"#;
        let second = r#"{"timestamp":"2026-05-13T09:01:05.000Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":250,"cached_input_tokens":50,"output_tokens":15,"total_tokens":265}}}}"#;
        let e1 = extract_codex(first, None, &mut state).unwrap().unwrap();
        // First total with no last → bill whole total (non-cached).
        assert_eq!(e1.input_tokens, 80);
        assert_eq!(e1.cache_read_tokens, 20);
        let e2 = extract_codex(second, None, &mut state).unwrap().unwrap();
        // Delta: input 150, cached 30 → non-cached 120
        assert_eq!(e2.input_tokens, 120);
        assert_eq!(e2.cache_read_tokens, 30);
        assert_eq!(e2.output_tokens, 10);
    }

    #[test]
    fn codex_fork_rewritten_burst_skipped_then_child_counted() {
        // Dense open (same second) = parent history replay; pause >1s = child work.
        let mut state = CodexParseState {
            forkish: true,
            burst_skip_active: true,
            replay: CodexReplaySkip::SkippingBurst {
                last_ms: parse_iso_ms("2026-08-07T10:09:15.277Z").unwrap(),
            },
            ..Default::default()
        };
        let burst1 = r#"{"timestamp":"2026-08-07T10:09:15.277Z","type":"event_msg","payload":{"type":"token_count","info":{"model":"gpt-5.6-sol","total_token_usage":{"input_tokens":1000,"cached_input_tokens":900,"output_tokens":10,"total_tokens":1010},"last_token_usage":{"input_tokens":1000,"cached_input_tokens":900,"output_tokens":10,"total_tokens":1010}}}}"#;
        let burst2 = r#"{"timestamp":"2026-08-07T10:09:15.500Z","type":"event_msg","payload":{"type":"token_count","info":{"model":"gpt-5.6-sol","total_token_usage":{"input_tokens":2000,"cached_input_tokens":1800,"output_tokens":20,"total_tokens":2020},"last_token_usage":{"input_tokens":1000,"cached_input_tokens":900,"output_tokens":10,"total_tokens":1010}}}}"#;
        let child = r#"{"timestamp":"2026-08-07T10:09:20.000Z","type":"event_msg","payload":{"type":"token_count","info":{"model":"gpt-5.6-sol","total_token_usage":{"input_tokens":2100,"cached_input_tokens":1900,"output_tokens":30,"total_tokens":2130},"last_token_usage":{"input_tokens":100,"cached_input_tokens":50,"output_tokens":10,"total_tokens":110}}}}"#;

        assert!(extract_codex(burst1, Some("fork"), &mut state)
            .unwrap()
            .is_none());
        assert!(extract_codex(burst2, Some("fork"), &mut state)
            .unwrap()
            .is_none());
        assert!(state.skipped_burst >= 2);

        let ev = extract_codex(child, Some("fork"), &mut state)
            .unwrap()
            .unwrap();
        assert_eq!(ev.input_tokens, 50); // 100 - 50
        assert_eq!(ev.cache_read_tokens, 50);
        assert_eq!(ev.output_tokens, 10);
        assert_eq!(state.emitted, 1);
    }

    #[test]
    fn codex_non_fork_counts_dense_open_events() {
        // Without burst skip, dense timestamps are still real turns.
        let mut state = CodexParseState::default();
        let a = r#"{"timestamp":"2026-08-07T10:09:15.277Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":100,"cached_input_tokens":0,"output_tokens":1,"total_tokens":101},"last_token_usage":{"input_tokens":100,"cached_input_tokens":0,"output_tokens":1,"total_tokens":101}}}}"#;
        let b = r#"{"timestamp":"2026-08-07T10:09:15.500Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":250,"cached_input_tokens":0,"output_tokens":3,"total_tokens":253},"last_token_usage":{"input_tokens":150,"cached_input_tokens":0,"output_tokens":2,"total_tokens":152}}}}"#;
        assert_eq!(
            extract_codex(a, None, &mut state)
                .unwrap()
                .unwrap()
                .input_tokens,
            100
        );
        assert_eq!(
            extract_codex(b, None, &mut state)
                .unwrap()
                .unwrap()
                .input_tokens,
            150
        );
        assert_eq!(state.emitted, 2);
        assert_eq!(state.skipped_burst, 0);
    }

    #[test]
    fn codex_init_from_file_detects_fork_and_burst() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("rollout-fork.jsonl");
        let content = concat!(
            r#"{"timestamp":"2026-08-07T10:09:15.000Z","type":"session_meta","payload":{"id":"child","parent_thread_id":"parent","forked_from_id":"parent"}}"#,
            "\n",
            r#"{"timestamp":"2026-08-07T10:09:15.100Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":10,"cached_input_tokens":0,"output_tokens":1,"total_tokens":11},"last_token_usage":{"input_tokens":10,"cached_input_tokens":0,"output_tokens":1,"total_tokens":11}}}}"#,
            "\n",
            r#"{"timestamp":"2026-08-07T10:09:15.200Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":20,"cached_input_tokens":0,"output_tokens":2,"total_tokens":22},"last_token_usage":{"input_tokens":10,"cached_input_tokens":0,"output_tokens":1,"total_tokens":11}}}}"#,
            "\n",
        );
        fs::write(&path, content).unwrap();
        let state = CodexParseState::init_from_file(&path, None, false);
        assert!(state.forkish);
        assert!(state.burst_skip_active);
        assert!(matches!(
            state.replay,
            CodexReplaySkip::SkippingBurst { .. }
        ));
    }

    #[test]
    fn parse_iso_ms_accepts_z_and_offsets() {
        let a = parse_iso_ms("2026-08-07T10:09:15.277Z").unwrap();
        let b = parse_iso_ms("2026-08-07T10:09:15.277+00:00").unwrap();
        assert_eq!(a, b);
        let c = parse_iso_ms("2026-08-07T10:09:16.277Z").unwrap();
        assert_eq!(c - a, 1000);
        assert!(parse_iso_ms("not-a-date").is_none());
        assert!(parse_iso_ms("").is_none());
    }

    #[test]
    fn read_codex_default_model_from_toml() {
        let dir = tempdir().unwrap();
        fs::write(
            dir.path().join("config.toml"),
            "model = \"gpt-5.6-luna\"\nmodel_reasoning_effort = \"xhigh\"\n",
        )
        .unwrap();
        assert_eq!(
            read_codex_default_model(dir.path()).as_deref(),
            Some("gpt-5.6-luna")
        );
    }

    #[test]
    fn read_pi_default_model_from_settings() {
        let dir = tempdir().unwrap();
        fs::write(
            dir.path().join("settings.json"),
            r#"{"defaultProvider":"xai","defaultModel":"grok-4.5"}"#,
        )
        .unwrap();
        assert_eq!(
            read_pi_default_model(dir.path()).as_deref(),
            Some("grok-4.5")
        );
    }

    #[test]
    fn skips_non_usage_lines() {
        let line = r#"{"type":"user","message":{"content":"hi"}}"#;
        assert!(extract_claude_like(AgentId::Claude, line, None)
            .unwrap()
            .is_none());
    }

    #[test]
    fn codex_skips_turn_context_without_tokens() {
        let line = r#"{"timestamp":"2026-05-13T09:00:00.000Z","type":"turn_context","payload":{"model":"gpt-5.2-codex"}}"#;
        let mut state = CodexParseState::default();
        assert!(extract_codex(line, None, &mut state).unwrap().is_none());
    }

    #[test]
    fn bootstrap_codex_model_reads_prefix() {
        let dir = tempdir().unwrap();
        let log = dir.path().join("rollout.jsonl");
        let content = concat!(
            r#"{"timestamp":"2026-08-03T03:12:03.326Z","type":"turn_context","payload":{"model":"gpt-5.6-sol"}}"#,
            "\n",
            r#"{"timestamp":"2026-08-03T03:12:09.556Z","type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":1,"output_tokens":1,"cached_input_tokens":0,"reasoning_output_tokens":0,"total_tokens":2}}}}"#,
            "\n",
        );
        fs::write(&log, content).unwrap();
        let first_line_end = content.find('\n').unwrap() as u64 + 1;
        let m = bootstrap_codex_model(&log, first_line_end);
        assert_eq!(m.as_deref(), Some("gpt-5.6-sol"));
    }

    #[test]
    fn bootstrap_codex_prefix_recovers_previous_total_for_resume() {
        let dir = tempdir().unwrap();
        let log = dir.path().join("rollout.jsonl");
        let l1 = r#"{"timestamp":"2026-08-03T03:12:03.326Z","type":"turn_context","payload":{"model":"gpt-5.6-sol"}}"#;
        let l2 = r#"{"timestamp":"2026-08-03T03:12:09.556Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":1000,"cached_input_tokens":400,"output_tokens":50,"total_tokens":1050},"last_token_usage":{"input_tokens":1000,"cached_input_tokens":400,"output_tokens":50,"total_tokens":1050}}}}"#;
        let l3 = r#"{"timestamp":"2026-08-03T03:12:20.000Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":1000,"cached_input_tokens":400,"output_tokens":50,"total_tokens":1050},"last_token_usage":{"input_tokens":1000,"cached_input_tokens":400,"output_tokens":50,"total_tokens":1050}}}}"#;
        let content = format!("{l1}\n{l2}\n{l3}\n");
        fs::write(&log, &content).unwrap();
        // Resume after first two lines (past the first token_count).
        let up_to = (l1.len() + 1 + l2.len() + 1) as u64;
        let (model, prev, tier) = bootstrap_codex_prefix(&log, up_to);
        assert_eq!(model.as_deref(), Some("gpt-5.6-sol"));
        assert!(tier.is_none());
        let prev = prev.expect("previous total");
        assert_eq!(prev.input, 1000);
        assert_eq!(prev.cached, 400);
        assert_eq!(prev.output, 50);

        // With previous_total seeded, a duplicate total snapshot must be skipped.
        let mut state = CodexParseState::resume_from_prefix(model, Some(prev), None, false);
        assert!(extract_codex(l3, Some("s"), &mut state).unwrap().is_none());
        assert_eq!(state.skipped_dup_total, 1);
    }

    #[test]
    fn extracts_kimi_old_status_update_with_config_hint() {
        let line = r#"{"timestamp":1770983427.123,"message":{"type":"StatusUpdate","payload":{"message_id":"m1","token_usage":{"input_other":100,"output":50,"input_cache_creation":10,"input_cache_read":20,"total":180}}}}"#;
        // Without hint → unknown (never invent kimi-for-coding)
        let bare = extract_kimi(line, Some("sess-a"), None).unwrap().unwrap();
        assert_eq!(bare.model, "unknown");
        // With config/llm hint
        let ev = extract_kimi(line, Some("sess-a"), Some("kimi-code/k3"))
            .unwrap()
            .unwrap();
        assert_eq!(ev.input_tokens, 100);
        assert_eq!(ev.output_tokens, 50);
        assert_eq!(ev.cache_creation_tokens, 10);
        assert_eq!(ev.cache_read_tokens, 20);
        assert_eq!(ev.model, "k3");
    }

    #[test]
    fn extracts_kimi_new_usage_record_from_real_shape() {
        // Real ~/.kimi-code wire.jsonl sample
        let turn = r#"{"type":"usage.record","model":"kimi-code/k3","usage":{"inputOther":8808,"output":2479,"inputCacheRead":19200,"inputCacheCreation":0},"usageScope":"turn","time":1784277702176}"#;
        let ev = extract_kimi(turn, Some("sess-b"), None).unwrap().unwrap();
        assert_eq!(ev.input_tokens, 8808);
        assert_eq!(ev.output_tokens, 2479);
        assert_eq!(ev.cache_creation_tokens, 0);
        assert_eq!(ev.cache_read_tokens, 19200);
        assert_eq!(ev.model, "k3");

        let session = r#"{"type":"usage.record","usageScope":"session","usage":{"inputOther":999,"output":999}}"#;
        assert!(extract_kimi(session, None, None).unwrap().is_none());
    }

    #[test]
    fn kimi_inherits_model_from_llm_request() {
        let mut model = None;
        let req = r#"{"type":"llm.request","kind":"loop","provider":"kimi","model":"k3","modelAlias":"kimi-code/k3","time":1784276981665}"#;
        note_kimi_model_from_line(req, &mut model);
        assert_eq!(model.as_deref(), Some("k3"));
        let usage = r#"{"type":"usage.record","usageScope":"turn","usage":{"inputOther":1,"output":2,"inputCacheRead":0,"inputCacheCreation":0},"time":1784276982000}"#;
        // no model on usage line → use inheritance
        let ev = extract_kimi(usage, None, model.as_deref())
            .unwrap()
            .unwrap();
        assert_eq!(ev.model, "k3");
    }

    #[test]
    fn read_kimi_default_model_from_toml() {
        let dir = tempdir().unwrap();
        fs::write(
            dir.path().join("config.toml"),
            "default_model = \"kimi-code/k3\"\n\n[thinking]\nenabled = true\n",
        )
        .unwrap();
        assert_eq!(read_kimi_default_model(dir.path()).as_deref(), Some("k3"));
    }

    #[test]
    fn kimi_total_fallback_when_parts_missing() {
        let line = r#"{"timestamp":1770983427.0,"message":{"type":"StatusUpdate","payload":{"token_usage":{"total":432}}}}"#;
        let ev = extract_kimi(line, None, Some("kimi-for-coding"))
            .unwrap()
            .unwrap();
        assert_eq!(ev.output_tokens, 432);
        assert_eq!(ev.input_tokens, 0);
        assert_eq!(ev.model, "kimi-for-coding");
    }

    #[test]
    fn kimi_root_from_new_and_old_wire_paths() {
        let new =
            PathBuf::from("/home/u/.kimi-code/sessions/wd_x/session_y/agents/main/wire.jsonl");
        let root = kimi_root_from_wire_path(&new).unwrap();
        assert!(root.ends_with(".kimi-code"));
        let old = PathBuf::from("/home/u/.kimi/sessions/group/session/wire.jsonl");
        let root2 = kimi_root_from_wire_path(&old).unwrap();
        assert!(root2.ends_with(".kimi"));
    }

    #[test]
    fn extracts_pi_assistant_usage() {
        // Real ~/.pi shape (observed on this machine)
        let line = r#"{"type":"message","timestamp":"2026-08-02T09:00:00.998Z","message":{"role":"assistant","model":"grok-4.5","usage":{"input":100,"output":200,"cacheRead":10,"cacheWrite":5,"cost":{"total":0.05}}}}"#;
        let ev = extract_pi(line, Some("session-a"), None).unwrap().unwrap();
        assert_eq!(ev.input_tokens, 100);
        assert_eq!(ev.output_tokens, 200);
        assert_eq!(ev.cache_read_tokens, 10);
        assert_eq!(ev.cache_creation_tokens, 5);
        assert_eq!(ev.model, "grok-4.5");
        assert_eq!(ev.cost_usd, Some(0.05));
    }

    #[test]
    fn pi_uses_model_hint_when_message_lacks_model() {
        let line = r#"{"type":"message","timestamp":"2026-01-02T00:00:00.000Z","message":{"role":"assistant","usage":{"input":1,"output":2}}}"#;
        let bare = extract_pi(line, None, None).unwrap().unwrap();
        assert_eq!(bare.model, "unknown");
        let hinted = extract_pi(line, None, Some("grok-4.5")).unwrap().unwrap();
        assert_eq!(hinted.model, "grok-4.5");
    }

    #[test]
    fn pi_total_tokens_fallback() {
        let line = r#"{"type":"message","timestamp":"2026-01-02T00:00:00.000Z","message":{"role":"assistant","model":"grok-4.5","usage":{"totalTokens":333}}}"#;
        let ev = extract_pi(line, None, None).unwrap().unwrap();
        assert_eq!(ev.output_tokens, 333);
    }

    #[test]
    fn pi_skips_non_assistant() {
        let line = r#"{"type":"message","timestamp":"2026-01-02T00:00:00.000Z","message":{"role":"user","usage":{"input":1}}}"#;
        assert!(extract_pi(line, None, None).unwrap().is_none());
    }

    #[test]
    fn workbuddy_uses_claude_shape() {
        let line = r#"{"timestamp":"2026-01-09T10:00:00.000Z","sessionId":"wb1","message":{"id":"m1","model":"claude-sonnet-4","usage":{"input_tokens":10,"output_tokens":5,"cache_read_input_tokens":1}}}"#;
        let ev = extract_claude_like(AgentId::WorkBuddy, line, None)
            .unwrap()
            .unwrap();
        assert_eq!(ev.input_tokens, 10);
        assert_eq!(ev.agent_id, AgentId::WorkBuddy);
    }

    #[test]
    fn is_kimi_wire_layout_accepts_old_and_new() {
        let sessions = PathBuf::from("/home/u/.kimi/sessions");
        let old = sessions.join("group").join("session-a").join("wire.jsonl");
        let new = sessions
            .join("ws")
            .join("session-b")
            .join("agents")
            .join("agent-1")
            .join("wire.jsonl");
        let bad = sessions
            .join("nested")
            .join("path")
            .join("session")
            .join("wire.jsonl");
        assert!(is_kimi_wire_layout(&sessions, &old));
        assert!(is_kimi_wire_layout(&sessions, &new));
        assert!(!is_kimi_wire_layout(&sessions, &bad));
    }

    #[test]
    fn incremental_cursor_advances() {
        let dir = tempdir().unwrap();
        let db = Database::open(&dir.path().join("t.db")).unwrap();
        let repo = UsageRepo::new(db);
        let log = dir.path().join("a.jsonl");
        fs::write(
            &log,
            r#"{"message":{"model":"m","usage":{"input_tokens":1,"output_tokens":2}}}
"#,
        )
        .unwrap();
        let batch = parse_file_for_agent_id(AgentId::Claude, &log, &repo).unwrap();
        assert_eq!(batch.events.len(), 1);
        assert!(repo.get_cursor(&log.to_string_lossy()).unwrap().is_none());
        repo.insert_batch_and_cursors(&[], std::slice::from_ref(&batch.cursor))
            .unwrap();
        let cur = repo.get_cursor(&log.to_string_lossy()).unwrap().unwrap();
        assert!(cur.byte_offset > 0);

        let batch2 = parse_file_for_agent_id(AgentId::Claude, &log, &repo).unwrap();
        assert!(batch2.events.is_empty());
    }

    /// Smoke against real ~/.kimi-code when present (this developer machine).
    #[test]
    fn live_kimi_code_discover_and_parse_sample() {
        let Ok(home) = crate::utils::paths::home_dir() else {
            return;
        };
        let root = home.join(".kimi-code");
        if !root.join("sessions").is_dir() {
            return;
        }
        // config.toml default_model
        let dm = read_kimi_default_model(&root);
        assert!(dm.is_some(), "expected default_model from real config.toml");
        assert_ne!(dm.as_deref(), Some("kimi-for-coding")); // real machine uses k3

        let files = discover_kimi_wire_files().expect("discover");
        assert!(
            !files.is_empty(),
            "expected wire.jsonl under ~/.kimi-code/sessions"
        );

        // Parse one non-empty wire file
        let sample = files
            .iter()
            .find(|p| fs::metadata(p).map(|m| m.len() > 1000).unwrap_or(false))
            .expect("non-empty wire");
        let dir = tempdir().unwrap();
        let db = Database::open(&dir.path().join("t.db")).unwrap();
        let repo = UsageRepo::new(db);
        let batch = parse_file_for_agent_id(AgentId::Kimi, sample, &repo).expect("parse");
        assert!(
            !batch.events.is_empty(),
            "expected usage.record turns from {}",
            sample.display()
        );
        // Models must come from wire lines / config — never invent product defaults
        assert!(
            batch.events.iter().all(|e| e.model != "unknown"),
            "unexpected unknown model when logs carry model fields"
        );
        let models: std::collections::BTreeSet<_> =
            batch.events.iter().map(|e| e.model.as_str()).collect();
        assert!(
            models.iter().any(|m| *m == "k3" || m.contains("kimi")),
            "unexpected models from live wire: {models:?}"
        );
    }

    /// Live ~/.codex: config model + real rollout token_count inheritance.
    #[test]
    fn live_codex_config_and_session_models() {
        let Ok(home) = crate::utils::paths::home_dir() else {
            return;
        };
        let root = home.join(".codex");
        if !root.join("sessions").is_dir() {
            return;
        }
        let dm = read_codex_default_model(&root);
        assert!(
            dm.is_some(),
            "expected model= from real ~/.codex/config.toml"
        );
        assert_ne!(dm.as_deref(), Some("gpt-5"), "must not invent legacy gpt-5");

        let files = discover_usage_files(AgentId::Codex).expect("discover");
        if files.is_empty() {
            return;
        }
        let sample = files
            .iter()
            .rev()
            .find(|p| fs::metadata(p).map(|m| m.len() > 2000).unwrap_or(false))
            .expect("non-empty codex rollout");
        let dir = tempdir().unwrap();
        let db = Database::open(&dir.path().join("t.db")).unwrap();
        let repo = UsageRepo::new(db);
        let batch = parse_file_for_agent_id(AgentId::Codex, sample, &repo).expect("parse");
        if batch.events.is_empty() {
            return; // short session without token_count
        }
        assert!(
            batch
                .events
                .iter()
                .all(|e| e.model != "gpt-5" || dm.as_deref() == Some("gpt-5")),
            "must not invent gpt-5 unless config really says so; got {:?}",
            batch
                .events
                .iter()
                .map(|e| e.model.as_str())
                .collect::<Vec<_>>()
        );
        // Prefer real models observed on this machine (turn_context / config)
        let models: std::collections::BTreeSet<_> =
            batch.events.iter().map(|e| e.model.as_str()).collect();
        assert!(
            models.iter().all(|m| *m != "unknown" || dm.is_none()),
            "unexpected unknown when config/logs available: {models:?} cfg={dm:?}"
        );
    }

    /// Live ~/.claude/projects: models come from log lines, never hardcoded.
    #[test]
    fn live_claude_projects_models_from_logs() {
        let Ok(home) = crate::utils::paths::home_dir() else {
            return;
        };
        if !home.join(".claude").join("projects").is_dir() {
            return;
        }
        let files = discover_usage_files(AgentId::Claude).expect("discover");
        if files.is_empty() {
            return;
        }
        let sample = files
            .iter()
            .rev()
            .find(|p| fs::metadata(p).map(|m| m.len() > 5000).unwrap_or(false));
        let Some(sample) = sample else {
            return;
        };
        let dir = tempdir().unwrap();
        let db = Database::open(&dir.path().join("t.db")).unwrap();
        let repo = UsageRepo::new(db);
        let batch = parse_file_for_agent_id(AgentId::Claude, sample, &repo).expect("parse");
        if batch.events.is_empty() {
            return;
        }
        let models: std::collections::BTreeSet<_> =
            batch.events.iter().map(|e| e.model.as_str()).collect();
        // Live Claude project logs may contain third-party models (custom
        // providers). Only assert parser hygiene: no synthetic placeholders and
        // at least one non-empty model id.
        assert!(
            models.iter().all(|m| *m != "<synthetic>"),
            "synthetic models should be filtered: {models:?}"
        );
        assert!(
            models.iter().any(|m| !m.trim().is_empty()),
            "live claude parse produced empty model ids: {models:?}"
        );
    }

    /// Live ~/.pi/agent: settings defaultModel + session usage.
    #[test]
    fn live_pi_settings_and_session() {
        let Ok(home) = crate::utils::paths::home_dir() else {
            return;
        };
        let agent_dir = home.join(".pi").join("agent");
        if !agent_dir.is_dir() {
            return;
        }
        let dm = read_pi_default_model(&agent_dir);
        // Live settings may or may not pin a default model; the reader must
        // follow the file instead of assuming this machine's content.
        let pins_model = fs::read_to_string(agent_dir.join("settings.json"))
            .map(|text| {
                serde_json::from_str::<serde_json::Value>(&text)
                    .ok()
                    .map(|v| {
                        v.get("defaultModel")
                            .or_else(|| v.get("default_model"))
                            .and_then(|x| x.as_str())
                            .map(str::trim)
                            .filter(|s| !s.is_empty())
                            .is_some()
                    })
                    .unwrap_or(false)
            })
            .unwrap_or(false);
        assert_eq!(
            dm.is_some(),
            pins_model,
            "read_pi_default_model must follow live settings.json"
        );
        let files = discover_usage_files(AgentId::Pi).expect("discover");
        if files.is_empty() {
            return;
        }
        let sample = &files[0];
        let dir = tempdir().unwrap();
        let db = Database::open(&dir.path().join("t.db")).unwrap();
        let repo = UsageRepo::new(db);
        let batch = parse_file_for_agent_id(AgentId::Pi, sample, &repo).expect("parse");
        if batch.events.is_empty() {
            return;
        }
        assert!(
            batch.events.iter().all(|e| e.model != "unknown"),
            "pi messages carry model or settings hint"
        );
    }

    #[test]
    fn extract_dsh_takes_provider_usage_and_skips_token_meter() {
        let header = r#"{"type":"request/header","model":"deepseek-v4-flash"}"#;
        let billed = r#"{"type":"assistant/message","usage":{"input_tokens":12,"output_tokens":34,"cache_read_input_tokens":2}}"#;
        let meter = r#"{"type":"token-meter","surfaceTokens":999,"estimated":true}"#;
        let estimated =
            r#"{"type":"assistant/message","usage":{"surfaceTokens":80,"estimated":true}}"#;
        let seed = r#"{"type":"assistant/message","seed":true,"usage":{"input_tokens":100,"output_tokens":100}}"#;
        let mut model = None;
        note_dsh_model_from_line(header, &mut model);
        assert_eq!(model.as_deref(), Some("deepseek-v4-flash"));
        let ev = extract_dsh(billed, Some("sess-dsh-demo"), model.as_deref())
            .unwrap()
            .expect("provider usage");
        assert_eq!(ev.agent_id, AgentId::Dsh);
        assert_eq!(ev.model, "deepseek-v4-flash");
        assert_eq!(ev.input_tokens, 12);
        assert_eq!(ev.output_tokens, 34);
        assert_eq!(ev.cache_read_tokens, 2);
        assert!(extract_dsh(meter, None, None).unwrap().is_none());
        assert!(extract_dsh(estimated, None, None).unwrap().is_none());
        assert!(extract_dsh(seed, None, None).unwrap().is_none());
    }

    #[test]
    fn parse_dsh_fixture_collects_two_events() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("sess-dsh-demo.jsonl");
        std::fs::write(path.clone(), include_str!("fixtures/dsh_session.jsonl")).unwrap();
        let db = Database::open(&dir.path().join("t.db")).unwrap();
        let repo = UsageRepo::new(db);
        let batch = parse_file_for_agent_id(AgentId::Dsh, &path, &repo).expect("parse");
        assert_eq!(batch.events.len(), 2);
        assert!(batch.events.iter().all(|e| e.agent_id == AgentId::Dsh));
        assert_eq!(batch.events[0].model, "deepseek-v4-flash");
        assert_eq!(batch.events[1].output_tokens, 5);
        assert_eq!(batch.events[1].cost_usd, Some(0.001));
        assert!(batch.events.iter().all(|e| e.input_tokens < 50));
    }

    #[test]
    fn discover_dsh_files_only_known_roots() {
        use std::sync::Mutex;
        static ENV_LOCK: Mutex<()> = Mutex::new(());
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = tempdir().unwrap();
        let home = dir.path().join("dsh-home");
        let extra = dir.path().join("session-root");
        let cwd_sessions = dir.path().join(".sessions");
        std::fs::create_dir_all(home.join("sessions/node_modules")).unwrap();
        std::fs::create_dir_all(home.join("sessions/cache.db")).unwrap();
        std::fs::create_dir_all(home.join("profiles/headless/sessions")).unwrap();
        std::fs::create_dir_all(&extra).unwrap();
        std::fs::create_dir_all(&cwd_sessions).unwrap();
        std::fs::write(home.join("sessions/keep.jsonl"), "{}\n").unwrap();
        std::fs::write(
            home.join("profiles/headless/sessions/profile.jsonl"),
            "{}\n",
        )
        .unwrap();
        std::fs::write(home.join("sessions/node_modules/skip.jsonl"), "{}\n").unwrap();
        std::fs::write(home.join("sessions/cache.db/skip.jsonl"), "{}\n").unwrap();
        std::fs::write(extra.join("extra.jsonl"), "{}\n").unwrap();
        std::fs::write(cwd_sessions.join("random.jsonl"), "{}\n").unwrap();

        let prev_home = std::env::var_os("DSH_HOME");
        let prev_root = std::env::var_os("DSH_SESSION_ROOT");
        std::env::set_var("DSH_HOME", &home);
        std::env::set_var("DSH_SESSION_ROOT", &extra);
        let files = discover_dsh_files().expect("discover");
        match prev_home {
            Some(v) => std::env::set_var("DSH_HOME", v),
            None => std::env::remove_var("DSH_HOME"),
        }
        match prev_root {
            Some(v) => std::env::set_var("DSH_SESSION_ROOT", v),
            None => std::env::remove_var("DSH_SESSION_ROOT"),
        }

        let names: Vec<String> = files
            .iter()
            .filter_map(|p| p.file_name().and_then(|n| n.to_str()).map(str::to_string))
            .collect();
        assert!(names.contains(&"keep.jsonl".into()), "{names:?}");
        assert!(names.contains(&"profile.jsonl".into()), "{names:?}");
        assert!(names.contains(&"extra.jsonl".into()), "{names:?}");
        assert!(!names.contains(&"skip.jsonl".into()), "{names:?}");
        assert!(!names.contains(&"random.jsonl".into()), "{names:?}");
        assert!(files.iter().all(|p| {
            let s = p.to_string_lossy();
            !s.contains("node_modules") && !s.contains(".db") && !s.contains(".sessions")
        }));
    }

    #[test]
    fn claude_cache_creation_1h_stays_split() {
        let line = r#"{"timestamp":"2026-01-09T10:00:00.000Z","message":{"role":"assistant","id":"m1","model":"claude-sonnet-4-5","usage":{"input_tokens":10,"output_tokens":1,"cache_creation_input_tokens":300,"cache_creation":{"ephemeral_5m_input_tokens":100,"ephemeral_1h_input_tokens":200},"cache_read_input_tokens":4}},"requestId":"r1"}"#;
        let ev = extract_claude_like(AgentId::Claude, line, Some("s"))
            .unwrap()
            .unwrap();
        assert_eq!(ev.cache_creation_tokens, 100);
        assert_eq!(ev.cache_creation_1h_tokens, 200);
        assert_eq!(ev.cache_read_tokens, 4);
        assert_eq!(ev.cache_tokens_total(), 304);
    }

    #[test]
    fn codex_fast_tier_from_thread_settings() {
        let settings = r#"{"timestamp":"2026-07-09T08:00:00.000Z","type":"event_msg","payload":{"type":"thread_settings_applied","thread_settings":{"service_tier":"priority"}}}"#;
        let tok = r#"{"timestamp":"2026-07-09T08:01:00.000Z","type":"event_msg","payload":{"type":"token_count","info":{"model":"gpt-5.6-sol","last_token_usage":{"input_tokens":100,"cached_input_tokens":0,"output_tokens":1,"total_tokens":101}}}}"#;
        let mut state = CodexParseState::default();
        assert!(extract_codex(settings, Some("s"), &mut state)
            .unwrap()
            .is_none());
        let ev = extract_codex(tok, Some("s"), &mut state).unwrap().unwrap();
        assert!(ev.fast);
        assert_eq!(ev.input_tokens, 100);
    }

    #[test]
    fn discover_codex_prefers_live_sessions_over_archived_same_path() {
        let dir = tempdir().unwrap();
        let sessions = dir.path().join("sessions");
        let archived = dir.path().join("archived_sessions");
        fs::create_dir_all(sessions.join("dup")).unwrap();
        fs::create_dir_all(archived.join("dup")).unwrap();
        fs::create_dir_all(archived.join("only")).unwrap();
        fs::write(sessions.join("dup/rollout.jsonl"), "{}\n").unwrap();
        fs::write(archived.join("dup/rollout.jsonl"), "{archived}\n").unwrap();
        fs::write(archived.join("only/rollout.jsonl"), "{only}\n").unwrap();
        let files = discover_codex_files_in(dir.path());
        let names: Vec<_> = files
            .iter()
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .collect();
        assert!(
            names
                .iter()
                .any(|n| n.contains("/sessions/dup/rollout.jsonl")),
            "{names:?}"
        );
        assert!(
            names
                .iter()
                .any(|n| n.contains("/archived_sessions/only/rollout.jsonl")),
            "{names:?}"
        );
        assert!(
            !names
                .iter()
                .any(|n| n.contains("/archived_sessions/dup/rollout.jsonl")),
            "archived duplicate should yield to live sessions: {names:?}"
        );
    }

    #[test]
    fn read_codex_fast_service_tier_from_toml() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("config.toml"), "service_tier = \"fast\"\n").unwrap();
        assert!(read_codex_fast_service_tier(dir.path()));
        fs::write(
            dir.path().join("config.toml"),
            "service_tier = \"standard\"\n",
        )
        .unwrap();
        assert!(!read_codex_fast_service_tier(dir.path()));
        fs::write(
            dir.path().join("config.toml"),
            "[profiles.work]\nservice_tier = \"priority\"\n",
        )
        .unwrap();
        assert!(
            !read_codex_fast_service_tier(dir.path()),
            "profile-only Fast must not mark every session Fast"
        );
    }
}
