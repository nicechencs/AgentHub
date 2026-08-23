//! List / delete / excerpt agent-native project containers & session logs.
//!
//! Platform merge/sort/metadata/delete live here. Per-agent discovery is
//! contributed via [`crate::platform::projects::ProjectSource`] (builtin
//! registry: [`crate::platform::projects::builtin_project_registry`]).
//! Filesystem scanners live in [`scan`] (still owned here until further split).

mod scan;
mod session_index;

// Re-export scanners for unit tests (`use super::*`) and platform ProjectSource impls.
#[allow(unused_imports)] // used via `use super::*` in tests.rs and platform::projects::sources
pub(crate) use scan::{
    aggregate_projects, extract_any_text, extract_userish_text, grok_session_dir_for_delete,
    kimi_session_dir_for_delete, list_claude_workbuddy_projects, list_claude_workbuddy_sessions,
    list_codex_sessions, list_cursor_projects, list_dsh_sessions, list_grok_projects,
    list_grok_sessions, list_kimi_projects, list_kimi_sessions, list_pi_projects, list_pi_sessions,
    load_excerpt,
};

#[cfg(test)]
#[allow(unused_imports)]
pub(crate) use scan::is_session_file;

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime};

use chrono::{DateTime, Utc};

use crate::adapters::AdapterRegistry;
use crate::error::{AppError, Result};
use crate::logging::{self, targets};
use crate::models::{
    AgentId, AgentProject, AgentProjectExcerpt, AgentSession, Capability, ProjectMetadataFile,
    ProjectUserMeta,
};
use crate::platform::projects::{
    builtin_project_registry, ProjectScanContext, ProjectSourceRegistry,
};
use crate::platform::AgentKey;
use crate::utils::atomic::atomic_write;
use crate::utils::paths::agent_home;
use crate::utils::redact::redact_text;

const METADATA_FILE: &str = "project_metadata.json";

fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

pub struct ProjectService {
    registry: AdapterRegistry,
    data_dir: PathBuf,
    project_sources: ProjectSourceRegistry,
}

impl ProjectService {
    pub fn new(registry: AdapterRegistry, data_dir: PathBuf) -> Self {
        Self::with_project_sources(registry, data_dir, builtin_project_registry().clone())
    }

    /// Construct a project service with an explicit source registry.
    ///
    /// Production uses the built-in registry via [`Self::new`]; tests and future
    /// agent integrations can inject additional sources without changing this service.
    pub fn with_project_sources(
        registry: AdapterRegistry,
        data_dir: PathBuf,
        project_sources: ProjectSourceRegistry,
    ) -> Self {
        Self {
            registry,
            data_dir,
            project_sources,
        }
    }

    fn metadata_path(&self) -> PathBuf {
        self.data_dir.join(METADATA_FILE)
    }

    /// Load AgentHub-side project metadata (hidden / alias / showHidden).
    pub fn get_metadata(&self) -> Result<ProjectMetadataFile> {
        load_metadata(&self.metadata_path())
    }

    /// Persist full metadata document.
    pub fn save_metadata(&self, doc: &ProjectMetadataFile) -> Result<()> {
        save_metadata(&self.metadata_path(), doc)
    }

    /// Update one project's user meta. Empty meta removes the key.
    pub fn upsert_project_meta(&self, project_id: &str, meta: ProjectUserMeta) -> Result<()> {
        if project_id.is_empty() || !project_id.contains(":proj:") {
            return Err(AppError::InvalidArg(format!(
                "invalid project id for metadata: {project_id}"
            )));
        }
        let mut doc = self.get_metadata()?;
        let cleaned = ProjectUserMeta {
            hidden: meta.hidden,
            alias: meta
                .alias
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
        };
        if cleaned.is_empty() {
            doc.projects.remove(project_id);
        } else {
            doc.projects.insert(project_id.to_string(), cleaned);
        }
        self.save_metadata(&doc)
    }

    pub fn set_show_hidden_projects(&self, show: bool) -> Result<()> {
        let mut doc = self.get_metadata()?;
        doc.show_hidden_projects = show;
        self.save_metadata(&doc)
    }

    fn project_sources(&self) -> &ProjectSourceRegistry {
        &self.project_sources
    }

    fn scan_projects_for_agent_key(
        &self,
        agent_key: &AgentKey,
        agent_home: &Path,
    ) -> Result<Vec<AgentProject>> {
        let ctx = ProjectScanContext {
            home: agent_home,
            data_dir: Some(self.data_dir.as_path()),
        };
        match self.project_sources().get(agent_key) {
            Some(source) => source.list_projects(&ctx),
            None => Ok(Vec::new()),
        }
    }

    fn scan_sessions_for_agent_key(
        &self,
        agent_key: &AgentKey,
        agent_home: &Path,
    ) -> Result<Vec<AgentSession>> {
        let ctx = ProjectScanContext {
            home: agent_home,
            data_dir: Some(self.data_dir.as_path()),
        };
        match self.project_sources().get(agent_key) {
            Some(source) => source.list_sessions(&ctx),
            None => Ok(Vec::new()),
        }
    }

    fn scan_project_sessions_for_agent_key(
        &self,
        agent_key: &AgentKey,
        agent_home: &Path,
        project_id: &str,
        project_key: &str,
    ) -> Result<Vec<AgentSession>> {
        let source = self.project_sources().get(agent_key).ok_or_else(|| {
            AppError::NotFound(format!(
                "no project source registered for agent: {agent_key}"
            ))
        })?;
        let ctx = ProjectScanContext {
            home: agent_home,
            data_dir: Some(self.data_dir.as_path()),
        };
        source.list_sessions_in_project(&ctx, project_id, project_key)
    }

    /// List project containers for an open [`AgentKey`] and an explicit home.
    pub fn list_projects_for_agent_key(
        &self,
        agent_key: &AgentKey,
        agent_home: &Path,
        include_hidden: bool,
    ) -> Result<Vec<AgentProject>> {
        let mut rows = self.scan_projects_for_agent_key(agent_key, agent_home)?;
        let meta = self.get_metadata().unwrap_or_default();
        apply_metadata(&mut rows, &meta);
        if !include_hidden {
            rows.retain(|project| !project.hidden);
        }
        rows.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(rows)
    }

    /// List flattened sessions for an open [`AgentKey`] and an explicit home.
    pub fn list_for_agent_key(
        &self,
        agent_key: &AgentKey,
        agent_home: &Path,
    ) -> Result<Vec<AgentSession>> {
        let mut rows = self.scan_sessions_for_agent_key(agent_key, agent_home)?;
        rows.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(rows)
    }

    /// List sessions in one project for an open [`AgentKey`] and explicit home.
    pub fn list_project_sessions_for_agent_key(
        &self,
        agent_key: &AgentKey,
        agent_home: &Path,
        project_id: &str,
        project_key: &str,
    ) -> Result<Vec<AgentSession>> {
        let mut rows = self.scan_project_sessions_for_agent_key(
            agent_key,
            agent_home,
            project_id,
            project_key,
        )?;
        rows.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(rows)
    }

    /// List project **containers** for one agent (or all when `None`).
    ///
    /// When `include_hidden` is false, projects marked hidden in metadata are omitted
    /// (unless `show_hidden_projects` is true in the metadata file — then they are included
    /// with `hidden=true` for UI). Prefer explicit `include_hidden` from the UI toggle.
    pub fn list_projects(
        &self,
        agent: Option<AgentId>,
        include_hidden: bool,
    ) -> Result<Vec<AgentProject>> {
        let started = Instant::now();
        let filter = agent.map(|a| a.as_str()).unwrap_or("all");
        let meta = self.get_metadata().unwrap_or_default();
        let mut out = Vec::new();
        let iter: Vec<_> = agent
            .map_or_else(|| AgentId::ALL.to_vec(), |a| vec![a])
            .into_iter()
            .filter_map(|agent_id| {
                let agent_key = AgentKey::from_agent_id(agent_id);
                self.project_sources()
                    .contains_key(&agent_key)
                    .then_some((agent_id, agent_key))
            })
            .collect();
        for (a, agent_key) in iter {
            let home = match agent_home(a) {
                Ok(h) => h,
                Err(e) => {
                    tracing::warn!(
                        module = targets::PROJECT,
                        op = "list_projects",
                        agent = a.as_str(),
                        code = e.code(),
                        error = %redact_text(&e.to_string()),
                        "project list skipped agent (home)"
                    );
                    continue;
                }
            };
            match self.scan_projects_for_agent_key(&agent_key, &home) {
                Ok(mut rows) => out.append(&mut rows),
                Err(e) => {
                    let err_msg = redact_text(&e.to_string());
                    tracing::warn!(
                        module = targets::PROJECT,
                        op = "list_projects",
                        agent = a.as_str(),
                        code = e.code(),
                        error = %err_msg,
                        "project list skipped agent"
                    );
                }
            }
        }
        apply_metadata(&mut out, &meta);
        if !include_hidden {
            out.retain(|p| !p.hidden);
        }
        out.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        tracing::info!(
            module = targets::PROJECT,
            op = "list_projects",
            agent = filter,
            include_hidden,
            count = out.len(),
            elapsed_ms = elapsed_ms(started),
            "list_projects ok"
        );
        Ok(out)
    }

    /// List sessions under one project container (agent-scoped subset scan).
    pub fn list_sessions(&self, project_id: &str) -> Result<Vec<AgentSession>> {
        let started = Instant::now();
        let (agent, key) = parse_project_id(project_id)?;
        let home = agent_home(agent)?;
        let agent_key = AgentKey::from_agent_id(agent);
        let rows = self.list_project_sessions_for_agent_key(&agent_key, &home, project_id, &key)?;
        tracing::info!(
            module = targets::PROJECT,
            op = "list_sessions",
            project_id,
            count = rows.len(),
            elapsed_ms = elapsed_ms(started),
            "list_sessions ok"
        );
        Ok(rows)
    }

    /// Flattened session list (delete / excerpt / transition helpers).
    /// Cursor has no session transcripts → empty for that agent.
    pub fn list(&self, agent: Option<AgentId>) -> Result<Vec<AgentSession>> {
        let started = Instant::now();
        let filter = agent.map(|a| a.as_str()).unwrap_or("all");
        let mut out = Vec::new();
        let iter: Vec<_> = agent
            .map_or_else(|| AgentId::ALL.to_vec(), |a| vec![a])
            .into_iter()
            .filter_map(|agent_id| {
                let agent_key = AgentKey::from_agent_id(agent_id);
                self.project_sources()
                    .contains_key(&agent_key)
                    .then_some((agent_id, agent_key))
            })
            .collect();
        for (a, agent_key) in iter {
            let home = match agent_home(a) {
                Ok(h) => h,
                Err(e) => {
                    tracing::warn!(
                        module = targets::PROJECT,
                        op = "list",
                        agent = a.as_str(),
                        code = e.code(),
                        error = %redact_text(&e.to_string()),
                        "session list skipped agent (home)"
                    );
                    continue;
                }
            };
            match self.scan_sessions_for_agent_key(&agent_key, &home) {
                Ok(mut rows) => out.append(&mut rows),
                Err(e) => {
                    let err_msg = redact_text(&e.to_string());
                    tracing::warn!(
                        module = targets::PROJECT,
                        op = "list",
                        agent = a.as_str(),
                        code = e.code(),
                        error = %err_msg,
                        "session list skipped agent"
                    );
                }
            }
        }
        out.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        tracing::info!(
            module = targets::PROJECT,
            op = "list",
            agent = filter,
            count = out.len(),
            elapsed_ms = elapsed_ms(started),
            "list ok"
        );
        Ok(out)
    }

    /// Delete one session file. Path must resolve under the agent home.
    pub fn delete(&self, id: &str) -> Result<()> {
        let started = Instant::now();
        let result = self.delete_inner(id, None);
        match &result {
            Ok(()) => {
                tracing::info!(
                    module = targets::PROJECT,
                    op = "delete",
                    id,
                    elapsed_ms = elapsed_ms(started),
                    "delete ok"
                );
            }
            Err(e) => {
                logging::log_app_error(targets::PROJECT, "delete", e);
            }
        }
        result
    }

    /// Batch delete; returns how many succeeded.
    pub fn delete_many(&self, ids: &[String]) -> Result<u32> {
        let started = Instant::now();
        let mut ok = 0u32;
        let mut failed = 0u32;
        for id in ids {
            match self.delete_inner(id, None) {
                Ok(()) => {
                    ok += 1;
                    tracing::info!(
                        module = targets::PROJECT,
                        op = "delete",
                        id = %id,
                        "delete ok"
                    );
                }
                Err(e) => {
                    failed += 1;
                    let err_msg = redact_text(&e.to_string());
                    tracing::warn!(
                        module = targets::PROJECT,
                        op = "delete_many",
                        id = %id,
                        code = e.code(),
                        error = %err_msg,
                        "project delete failed"
                    );
                }
            }
        }
        tracing::info!(
            module = targets::PROJECT,
            op = "delete_many",
            requested = ids.len(),
            ok,
            failed,
            elapsed_ms = elapsed_ms(started),
            "delete_many done"
        );
        Ok(ok)
    }

    /// Load truncated conversation text for summarize / continue context.
    pub fn excerpts(&self, ids: &[String]) -> Result<Vec<AgentProjectExcerpt>> {
        let started = Instant::now();
        let mut out = Vec::with_capacity(ids.len());
        let mut failed = 0u32;
        for id in ids {
            match load_excerpt(id, None) {
                Ok(ex) => out.push(ex),
                Err(e) => {
                    failed += 1;
                    let err_msg = redact_text(&e.to_string());
                    tracing::warn!(
                        module = targets::PROJECT,
                        op = "excerpt",
                        id = %id,
                        code = e.code(),
                        error = %err_msg,
                        "project excerpt failed"
                    );
                }
            }
        }
        tracing::info!(
            module = targets::PROJECT,
            op = "excerpt",
            requested = ids.len(),
            count = out.len(),
            failed,
            elapsed_ms = elapsed_ms(started),
            "excerpt ok"
        );
        Ok(out)
    }

    /// Test/helper entry: delete with an explicit agent home root.
    #[cfg(test)]
    fn delete_with_home(&self, id: &str, home: &Path) -> Result<()> {
        self.delete_inner(id, Some(home))
    }

    fn delete_inner(&self, id: &str, home_override: Option<&Path>) -> Result<()> {
        let (agent, rel) = parse_session_id(id)?;
        self.registry.require(agent, Capability::ProjectDelete)?;
        let abs = resolve_under_home(agent, &rel, home_override)?;
        if !abs.exists() {
            return Err(AppError::NotFound(format!("project not found: {id}")));
        }
        tracing::debug!(
            module = targets::PROJECT,
            op = "delete",
            agent = agent.as_str(),
            id,
            path = %abs.display(),
            "delete start"
        );
        // Agent-specific delete root (e.g. Grok/Kimi whole session dir) via ProjectSource.
        let agent_key = AgentKey::from_agent_id(agent);
        if let Some(source) = self.project_sources().get(&agent_key) {
            if let Some(session_dir) = source.delete_root_for_session_file(&abs) {
                fs::remove_dir_all(&session_dir)?;
                if let Some(parent) = session_dir.parent() {
                    maybe_remove_empty_dir(parent, agent, home_override)?;
                }
                return Ok(());
            }
        }
        if abs.is_dir() {
            fs::remove_dir_all(&abs)?;
        } else {
            fs::remove_file(&abs)?;
            if let Some(parent) = abs.parent() {
                maybe_remove_empty_dir(parent, agent, home_override)?;
            }
        }
        Ok(())
    }
}

impl Default for ProjectService {
    fn default() -> Self {
        Self::new(AdapterRegistry::default(), PathBuf::from("."))
    }
}

/// Test helpers: list projects/sessions for an explicit home root (not production call path).
#[cfg(test)]
pub(crate) fn list_projects_for_agent_home(
    agent: AgentId,
    home: &Path,
    data_dir: Option<&Path>,
) -> Result<Vec<AgentProject>> {
    let key = AgentKey::from_agent_id(agent);
    list_projects_for_key_home(builtin_project_registry(), &key, home, data_dir)
}

#[cfg(test)]
pub(crate) fn list_projects_for_key_home(
    registry: &ProjectSourceRegistry,
    key: &AgentKey,
    home: &Path,
    data_dir: Option<&Path>,
) -> Result<Vec<AgentProject>> {
    let ctx = ProjectScanContext { home, data_dir };
    match registry.get(key) {
        Some(source) => source.list_projects(&ctx),
        None => Ok(vec![]),
    }
}

#[cfg(test)]
pub(crate) fn list_sessions_for_agent_home(
    agent: AgentId,
    home: &Path,
    data_dir: Option<&Path>,
) -> Result<Vec<AgentSession>> {
    let key = AgentKey::from_agent_id(agent);
    list_sessions_for_key_home(builtin_project_registry(), &key, home, data_dir)
}

#[cfg(test)]
pub(crate) fn list_sessions_for_key_home(
    registry: &ProjectSourceRegistry,
    key: &AgentKey,
    home: &Path,
    data_dir: Option<&Path>,
) -> Result<Vec<AgentSession>> {
    let ctx = ProjectScanContext { home, data_dir };
    match registry.get(key) {
        Some(source) => source.list_sessions(&ctx),
        None => Ok(vec![]),
    }
}

#[cfg(test)]
pub(crate) fn list_sessions_for_project_home(
    agent: AgentId,
    home: &Path,
    project_id: &str,
    key: &str,
    data_dir: Option<&Path>,
) -> Result<Vec<AgentSession>> {
    let agent_key = AgentKey::from_agent_id(agent);
    list_sessions_for_project_key_home(
        builtin_project_registry(),
        &agent_key,
        home,
        project_id,
        key,
        data_dir,
    )
}

#[cfg(test)]
pub(crate) fn list_sessions_for_project_key_home(
    registry: &ProjectSourceRegistry,
    agent_key: &AgentKey,
    home: &Path,
    project_id: &str,
    key: &str,
    data_dir: Option<&Path>,
) -> Result<Vec<AgentSession>> {
    let ctx = ProjectScanContext { home, data_dir };
    match registry.get(agent_key) {
        Some(source) => source.list_sessions_in_project(&ctx, project_id, key),
        None => Ok(vec![]),
    }
}

/// Claude / WorkBuddy: one container per `projects/<encodedDir>/`, top-level session files only.
///
/// When `only_encoded` is set, only that project directory is scanned.
pub(crate) fn parse_project_id(id: &str) -> Result<(AgentId, String)> {
    let mut parts = id.splitn(3, ':');
    let agent_s = parts
        .next()
        .ok_or_else(|| AppError::InvalidArg(format!("invalid project id: {id}")))?;
    let marker = parts
        .next()
        .ok_or_else(|| AppError::InvalidArg(format!("invalid project id: {id}")))?;
    let key = parts
        .next()
        .ok_or_else(|| AppError::InvalidArg(format!("invalid project id: {id}")))?;
    if marker != "proj" {
        return Err(AppError::InvalidArg(format!("invalid project id: {id}")));
    }
    let agent = AgentId::parse(agent_s)
        .ok_or_else(|| AppError::InvalidArg(format!("invalid agent in project id: {id}")))?;
    if key.is_empty() || key.contains("..") {
        return Err(AppError::InvalidArg(format!("unsafe project id: {id}")));
    }
    Ok((agent, key.to_string()))
}

pub(crate) fn parse_session_id(id: &str) -> Result<(AgentId, String)> {
    let (agent_s, rel) = id
        .split_once(':')
        .ok_or_else(|| AppError::InvalidArg(format!("invalid project id: {id}")))?;
    // Reject container ids accidentally passed to delete/excerpt.
    if rel.starts_with("proj:") {
        return Err(AppError::InvalidArg(format!(
            "expected session id, got project container: {id}"
        )));
    }
    let agent = AgentId::parse(agent_s)
        .ok_or_else(|| AppError::InvalidArg(format!("invalid agent in project id: {id}")))?;
    if rel.is_empty() || rel.contains("..") {
        return Err(AppError::InvalidArg(format!("unsafe project id: {id}")));
    }
    Ok((agent, rel.to_string()))
}

pub(crate) fn resolve_under_home(
    agent: AgentId,
    rel: &str,
    home_override: Option<&Path>,
) -> Result<PathBuf> {
    let home = match home_override {
        Some(h) => h.to_path_buf(),
        None => agent_home(agent)?,
    };
    let home_canon = fs::canonicalize(&home).unwrap_or(home.clone());
    let mut abs = home.clone();
    for part in rel.split('/') {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." {
            return Err(AppError::InvalidArg("path traversal rejected".into()));
        }
        abs.push(part);
    }
    if abs.exists() {
        let canon = fs::canonicalize(&abs)?;
        if !canon.starts_with(&home_canon) {
            return Err(AppError::InvalidArg("path escapes agent home".into()));
        }
        return Ok(canon);
    }
    if !abs.starts_with(&home) && !abs.starts_with(&home_canon) {
        return Err(AppError::InvalidArg("path escapes agent home".into()));
    }
    Ok(abs)
}

fn maybe_remove_empty_dir(dir: &Path, agent: AgentId, home_override: Option<&Path>) -> Result<()> {
    let home = match home_override {
        Some(h) => h.to_path_buf(),
        None => agent_home(agent)?,
    };
    let protected = [
        home.clone(),
        home.join("projects"),
        home.join("sessions"),
        home.join("agent"),
        home.join("agent").join("sessions"),
    ];
    let mut cur = dir.to_path_buf();
    for _ in 0..4 {
        if protected.iter().any(|p| p == &cur) {
            break;
        }
        if !cur.starts_with(&home) {
            break;
        }
        let empty = fs::read_dir(&cur)
            .map(|mut d| d.next().is_none())
            .unwrap_or(false);
        if !empty {
            break;
        }
        let _ = fs::remove_dir(&cur);
        if let Some(parent) = cur.parent() {
            cur = parent.to_path_buf();
        } else {
            break;
        }
    }
    Ok(())
}

pub(crate) fn system_time_to_rfc3339(t: SystemTime) -> String {
    let dt: DateTime<Utc> = t.into();
    dt.to_rfc3339()
}

fn load_metadata(path: &Path) -> Result<ProjectMetadataFile> {
    if !path.exists() {
        return Ok(ProjectMetadataFile::default());
    }
    let raw = fs::read_to_string(path)?;
    if raw.trim().is_empty() {
        return Ok(ProjectMetadataFile::default());
    }
    let doc: ProjectMetadataFile = serde_json::from_str(&raw)
        .map_err(|e| AppError::InvalidArg(format!("invalid project_metadata.json: {e}")))?;
    Ok(doc)
}

fn save_metadata(path: &Path, doc: &ProjectMetadataFile) -> Result<()> {
    let mut out = doc.clone();
    out.version = 1;
    // Drop empty entries
    out.projects.retain(|_, m| !m.is_empty());
    let bytes = serde_json::to_vec_pretty(&out)
        .map_err(|e| AppError::InvalidArg(format!("serialize project_metadata.json: {e}")))?;
    atomic_write(path, &bytes)?;
    Ok(())
}

fn apply_metadata(projects: &mut [AgentProject], meta: &ProjectMetadataFile) {
    for p in projects.iter_mut() {
        if let Some(m) = meta.projects.get(&p.id) {
            p.hidden = m.hidden;
            p.alias = m.alias.clone();
        }
    }
}

#[cfg(test)]
mod tests;
