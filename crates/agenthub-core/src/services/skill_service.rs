//! Skill source scan + per-agent projection matrix, with safe projection writes.
//!
//! Construction takes an explicit `source_root` and [`AdapterRegistry`]; it does
//! **not** scan the filesystem. Writes (`sync` / `disable`) project or remove a
//! single validated skill directory under an adapter's `skills_dir`.

use std::collections::hash_map::DefaultHasher;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use crate::adapters::AdapterRegistry;
use crate::catalog::limits::SKILL_MARKDOWN_PREVIEW_CHARS;
use crate::error::{AppError, Result};
use crate::logging::targets;
use crate::models::{
    AgentId, Capability, InstalledSkill, Skill, SkillAction, SkillFailure, SkillLinkKind,
    SkillMapStatus, SkillMarkdownPreview, SkillProjectMode, SkillProjectResult, SkillProjection,
    SkillSourceRecord, SkillSyncReport, SkillSyncState,
};
use crate::platform::skills::{
    bootstrap_skill_assignments, clear_managed_target_for_reproject, collect_regular_files,
    commit_skill_package, detect_link_kind, ensure_no_symlink_in_ancestors,
    ensure_no_symlink_in_existing_prefix, ensure_skill_md, finalize_link_projection_ownership,
    inspect_projection_target, is_exact_child, is_link_or_reparse, link_resolves_to_source,
    materialize_projection, normalize_rel_path, package_revision, paths_equal_lexical,
    prepare_git_skill_staging, project_copy_with_ownership, record_copy_ownership,
    reject_source_target_overlap, remove_projection_link, resolve_link_path, skill_lock_load,
    skill_lock_remove, skill_lock_upsert, unproject_with_ownership, validate_and_collect_source,
    validate_skill_id, validate_skills_root, validate_tree_entries_safe, write_skill_tree,
    PreparedSkillTree, SkillAssignmentService, SkillBootstrapReport, SkillCommitFaults,
    SkillPackageService, SkillReconciler, SkillSourceService, SkillTargetRegistry, TargetPresence,
};
use crate::platform::AgentKey;
use crate::storage::{Database, SkillRepo};
use crate::utils::agent_lock::AgentWriteLock;

fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

fn log_skill_write_result(
    op: &str,
    skill_id: &str,
    agent: Option<&str>,
    started: Instant,
    ok: bool,
    err: Option<&AppError>,
) {
    if ok {
        tracing::info!(
            module = targets::SKILL,
            op = op,
            skill_id = skill_id,
            agent = agent.unwrap_or("-"),
            elapsed_ms = elapsed_ms(started),
            "ok"
        );
    } else if let Some(e) = err {
        let msg = crate::utils::redact::redact_text(&e.to_string());
        if let Some(a) = agent {
            tracing::error!(
                module = targets::SKILL,
                code = e.code(),
                op = op,
                skill_id = skill_id,
                agent = a,
                elapsed_ms = elapsed_ms(started),
                "{msg}"
            );
        } else {
            tracing::error!(
                module = targets::SKILL,
                code = e.code(),
                op = op,
                skill_id = skill_id,
                elapsed_ms = elapsed_ms(started),
                "{msg}"
            );
        }
    }
}

/// Cached `list()` snapshot keyed by a cheap filesystem fingerprint.
struct SkillListCache {
    fingerprint: u64,
    skills: Arc<Vec<Skill>>,
}

/// One on-disk skill directory discovered under an agent's skills root.
struct AgentSkillDirEntry {
    agent: AgentId,
    skill_id: String,
    path: PathBuf,
    skills_root: PathBuf,
    display: String,
    description: String,
}

/// Scans the shared skill source root and projects status onto each agent.
pub struct SkillService {
    source_root: PathBuf,
    registry: AdapterRegistry,
    targets: SkillTargetRegistry,
    /// Process-local list matrix cache (invalidated on writes / FS fingerprint change).
    list_cache: Mutex<Option<SkillListCache>>,
    /// Optional DB for assignment + reconcile (P12). When `None`, sync/disable
    /// stay filesystem-only (unit tests that only exercise FS).
    db: Option<Database>,
}

impl SkillService {
    /// Explicit dependencies — no home resolution and no automatic scan.
    ///
    /// Filesystem-only path (no assignment table). Prefer [`with_db`] in production.
    pub fn new(source_root: PathBuf, registry: AdapterRegistry) -> Self {
        // Compatibility: derive targets from the provided adapter registry so
        // unit tests with fake adapters keep working. Production composition
        // uses [`Self::with_db_and_target_registry`] + builtin StaticSkillTarget.
        let targets = SkillTargetRegistry::from_adapter_registry(&registry)
            .expect("adapter registry must contain unique skill target keys");
        Self::with_target_registry(source_root, registry, targets)
    }

    /// Filesystem-only constructor with an explicitly composed target registry.
    pub fn with_target_registry(
        source_root: PathBuf,
        registry: AdapterRegistry,
        targets: SkillTargetRegistry,
    ) -> Self {
        Self {
            source_root,
            registry,
            targets,
            list_cache: Mutex::new(None),
            db: None,
        }
    }

    /// Production constructor: shared source root + adapter registry + assignment DB.
    pub fn with_db(source_root: PathBuf, registry: AdapterRegistry, db: Database) -> Self {
        // Compatibility path (tests / callers that still pass a full adapter set).
        // Production AgentHub uses builtin StaticSkillTarget via
        // [`Self::with_db_and_target_registry`].
        let targets = SkillTargetRegistry::from_adapter_registry(&registry)
            .expect("adapter registry must contain unique skill target keys");
        Self::with_db_and_target_registry(source_root, registry, db, targets)
    }

    /// Production/test composition with one shared key-native target registry.
    pub fn with_db_and_target_registry(
        source_root: PathBuf,
        registry: AdapterRegistry,
        db: Database,
        targets: SkillTargetRegistry,
    ) -> Self {
        Self {
            source_root,
            registry,
            targets,
            list_cache: Mutex::new(None),
            db: Some(db),
        }
    }

    pub fn source_root(&self) -> &Path {
        &self.source_root
    }

    pub fn registry(&self) -> &AdapterRegistry {
        &self.registry
    }

    pub fn target_registry(&self) -> &SkillTargetRegistry {
        &self.targets
    }

    pub fn db(&self) -> Option<&Database> {
        self.db.as_ref()
    }

    /// Idempotent import of packages + managed assignments from lock / FS.
    ///
    /// No-op when this service has no database.
    pub fn bootstrap_assignments(&self) -> Result<SkillBootstrapReport> {
        let Some(db) = self.db.as_ref() else {
            return Ok(SkillBootstrapReport::default());
        };
        let repo = SkillRepo::new(db.clone());
        bootstrap_skill_assignments(&self.source_root, &self.targets, &repo, &chrono_now())
    }

    /// Drop cached `list()` results (writes, external FS watcher).
    pub fn invalidate_list_cache(&self) {
        if let Ok(mut guard) = self.list_cache.lock() {
            *guard = None;
        }
    }

    /// Immediate child skill ids under the shared source root (no projection matrix).
    ///
    /// Used by market installed-flag marking so empty/market queries do not pay for
    /// a full N×agent classification when the list cache is cold.
    pub fn list_shared_ids(&self) -> Result<HashSet<String>> {
        // Prefer warm list cache — already paid for the scan.
        if let Ok(guard) = self.list_cache.lock() {
            if let Some(cache) = guard.as_ref() {
                if cache.fingerprint == self.list_fingerprint() {
                    return Ok(cache.skills.iter().map(|s| s.id.clone()).collect());
                }
            }
        }

        let root = &self.source_root;
        if !root.exists() {
            return Ok(HashSet::new());
        }
        let meta = fs::metadata(root)?;
        if !meta.is_dir() {
            return Err(AppError::InvalidArg(format!(
                "skill source root is not a directory: {}",
                root.display()
            )));
        }

        let mut ids = HashSet::new();
        for ent in fs::read_dir(root)? {
            let ent = ent?;
            let name = ent.file_name();
            let name_str = name.to_string_lossy();
            if name_str.starts_with('.') {
                continue;
            }
            let file_type = match ent.file_type() {
                Ok(ft) => ft,
                Err(_) => continue,
            };
            if file_type.is_symlink() || !file_type.is_dir() {
                continue;
            }
            let id = name_str.into_owned();
            if !id.is_empty() {
                ids.insert(id);
            }
        }
        Ok(ids)
    }

    /// Read `SKILL.md` for GUI markdown preview.
    ///
    /// - `private_agent == None` → shared source root (`~/.agents/skills/<id>`).
    /// - `private_agent == Some(agent)` → that agent's private skills dir.
    /// - Rejects path traversal / unsafe ids; never follows skill-root symlinks.
    /// - Body is capped at [`SKILL_MARKDOWN_PREVIEW_CHARS`].
    pub fn read_skill_markdown(
        &self,
        skill_id: &str,
        private_agent: Option<AgentId>,
    ) -> Result<SkillMarkdownPreview> {
        let skill_id = validate_skill_id(skill_id)?;
        let skill_dir = match private_agent {
            None => {
                let dir = self.source_root.join(skill_id);
                if !is_exact_child(&dir, &self.source_root, skill_id) {
                    return Err(AppError::InvalidArg(format!(
                        "skill source path escapes source root: {}",
                        dir.display()
                    )));
                }
                dir
            }
            Some(agent) => {
                let adapter = self.registry.require(agent, Capability::Skills)?;
                let skills_root = adapter.skills_dir().ok_or_else(|| {
                    AppError::Unsupported(format!(
                        "agent {} has no skills directory",
                        agent.as_str()
                    ))
                })?;
                validate_skills_root(&skills_root)?;
                let dir = skills_root.join(skill_id);
                if !is_exact_child(&dir, &skills_root, skill_id) {
                    return Err(AppError::InvalidArg(format!(
                        "skill target path escapes skills root: {}",
                        dir.display()
                    )));
                }
                dir
            }
        };

        // Skill directory itself must be a real directory (not a symlink root).
        if !skill_dir.is_dir() {
            return Err(AppError::NotFound(format!(
                "skill directory not found: {}",
                skill_dir.display()
            )));
        }
        let dir_meta = fs::symlink_metadata(&skill_dir)?;
        if dir_meta.file_type().is_symlink() {
            return Err(AppError::InvalidArg(format!(
                "refusing to read skill via symlink directory: {}",
                skill_dir.display()
            )));
        }

        let skill_md = skill_dir.join("SKILL.md");
        if skill_md.is_file() {
            return read_skill_md_file(skill_id, &skill_dir, &skill_md);
        }
        // Case-insensitive fallback common on Windows installs that wrote skill.md.
        let alt = skill_dir.join("skill.md");
        if alt.is_file() {
            return read_skill_md_file(skill_id, &skill_dir, &alt);
        }
        Err(AppError::NotFound(format!(
            "SKILL.md not found in {}",
            skill_dir.display()
        )))
    }

    /// List skills under `source_root` with a per-agent projection matrix.
    ///
    /// - Missing source root → empty list (not an error).
    /// - Source path exists but is not a directory → [`AppError::InvalidArg`].
    /// - Immediate children only: directories kept; regular files, dot-prefixed
    ///   names, and bookkeeping such as `.skill-lock.json` ignored.
    /// - Order is deterministic by stable skill id (directory name).
    /// - Results are process-cached until fingerprint changes or
    ///   [`invalidate_list_cache`] is called.
    pub fn list(&self) -> Result<Vec<Skill>> {
        let fingerprint = self.list_fingerprint();
        if let Ok(guard) = self.list_cache.lock() {
            if let Some(cache) = guard.as_ref() {
                if cache.fingerprint == fingerprint {
                    return Ok((*cache.skills).clone());
                }
            }
        }

        let skills = self.list_uncached()?;
        if let Ok(mut guard) = self.list_cache.lock() {
            *guard = Some(SkillListCache {
                fingerprint,
                skills: Arc::new(skills.clone()),
            });
        }
        Ok(skills)
    }

    fn list_uncached(&self) -> Result<Vec<Skill>> {
        let root = &self.source_root;
        if !root.exists() {
            return Ok(Vec::new());
        }
        let meta = fs::metadata(root)?;
        if !meta.is_dir() {
            return Err(AppError::InvalidArg(format!(
                "skill source root is not a directory: {}",
                root.display()
            )));
        }

        let mut entries: Vec<PathBuf> = Vec::new();
        for ent in fs::read_dir(root)? {
            let ent = ent?;
            let name = ent.file_name();
            let name_str = name.to_string_lossy();
            // Ignore bookkeeping / hidden / non-skill noise at the top level.
            if name_str.starts_with('.') {
                continue;
            }
            let path = ent.path();
            let file_type = match ent.file_type() {
                Ok(ft) => ft,
                Err(_) => continue,
            };
            // Only immediate child directories; never follow links as skill roots.
            if file_type.is_symlink() || !file_type.is_dir() {
                continue;
            }
            entries.push(path);
        }

        entries.sort_by(|a, b| {
            let an = a.file_name().map(|s| s.to_os_string());
            let bn = b.file_name().map(|s| s.to_os_string());
            an.cmp(&bn)
        });

        let mut skills = Vec::with_capacity(entries.len());
        for dir in entries {
            let id = dir
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            if id.is_empty() {
                continue;
            }
            let (name, description) = read_skill_metadata(&dir, &id);
            let projections = self.project_matrix(&id, &dir);
            skills.push(Skill {
                id,
                name,
                description,
                source_dir: dir,
                projections,
            });
        }
        Ok(skills)
    }

    /// Cheap fingerprint of shared + agent skill roots (shallow entries + SKILL.md).
    fn list_fingerprint(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.source_root.hash(&mut hasher);
        hash_skill_root_shallow(&self.source_root, &mut hasher);
        for agent in AgentId::ALL {
            agent.as_str().hash(&mut hasher);
            let Some(adapter) = self.registry.get(agent) else {
                continue;
            };
            let Some(dir) = adapter.skills_dir() else {
                continue;
            };
            dir.hash(&mut hasher);
            hash_skill_root_shallow(&dir, &mut hasher);
        }
        hasher.finish()
    }

    /// Project one source skill onto a single agent's `skills_dir/<skill_id>` as a copy.
    ///
    /// - `force == false`: create when absent; no-op when already linked to source
    ///   or byte-identical copy; reject foreign/conflict without mutation.
    /// - `force == true`: build a sibling staging directory, then replace the
    ///   exact target (including removing a prior projection link without following
    ///   it). Old target is preserved until staging is complete; on replace failure
    ///   staging is cleaned and the old target restored best-effort. Sibling skills
    ///   are never mutated.
    /// - Existing real-directory targets with nested symlink / special / unreadable
    ///   entries are always rejected with [`AppError::InvalidArg`] (including force).
    ///
    /// When a database is attached ([`with_db`]), desired assignment is set first
    /// and projection runs through [`SkillReconciler`] (failure keeps desired +
    /// `last_error`, never claims applied).
    pub fn sync(&self, skill_id: &str, agent: AgentId, force: bool) -> Result<()> {
        let started = Instant::now();
        let result = (|| {
            let skill_id = validate_skill_id(skill_id)?;
            if self.db.is_some() {
                self.sync_via_assignment(skill_id, agent, force)
            } else {
                self.sync_projection(skill_id, agent, force)
            }
        })();
        match &result {
            Ok(()) => {
                self.invalidate_list_cache();
                log_skill_write_result("sync", skill_id, Some(agent.as_str()), started, true, None)
            }
            Err(e) => log_skill_write_result(
                "sync",
                skill_id,
                Some(agent.as_str()),
                started,
                false,
                Some(e),
            ),
        }
        result
    }

    /// Sync every listed skill onto `targets`.
    ///
    /// Individual `sync` errors are collected into [`SkillSyncReport::failed`]
    /// rather than aborting the batch. `list()` failures still propagate.
    ///
    /// When `skip_unsupported` is true, a skill whose projection state for that
    /// agent is [`SkillSyncState::Unsupported`] is recorded as skipped instead
    /// of calling `sync`. CLI `--all` and GUI "all agents" pass `true`; a
    /// single-agent target passes `false` so unsupported still surfaces as
    /// `failed`.
    pub fn sync_targets(
        &self,
        targets: &[AgentId],
        force: bool,
        skip_unsupported: bool,
    ) -> Result<SkillSyncReport> {
        let skills = self.list()?;
        let mut report = SkillSyncReport::default();

        for skill in &skills {
            for &agent in targets {
                let action = SkillAction {
                    skill: skill.id.clone(),
                    agent,
                };
                if skip_unsupported && skill.state_for(agent) == Some(SkillSyncState::Unsupported) {
                    report.skipped.push(action);
                    continue;
                }
                match self.sync(&skill.id, agent, force) {
                    Ok(()) => report.synced.push(action),
                    Err(error) => report.failed.push(SkillFailure {
                        skill: action.skill,
                        agent,
                        code: error.code().to_string(),
                        error: error.to_string(),
                    }),
                }
            }
        }
        Ok(report)
    }

    /// Remove the projected skill directory for one agent.
    ///
    /// Only the exact validated `skills_dir/<skill_id>` target is removed.
    /// Missing target is an idempotent success.
    ///
    /// **Link targets** (symlink / junction): remove the link entry itself via
    /// [`remove_projection_link`] — never follow into the source tree.
    ///
    /// Real directories with nested unsafe entries, special/non-dir targets, and
    /// containment violations are rejected without mutation. Source tree is not
    /// modified.
    ///
    /// With a database, sets `desired_enabled=false` then reconciles.
    pub fn disable(&self, skill_id: &str, agent: AgentId) -> Result<()> {
        self.disable_key(skill_id, &AgentKey::from_agent_id(agent))
    }

    /// Key-native disable path used by shared uninstall and future Agent targets.
    pub fn disable_key(&self, skill_id: &str, agent_key: &AgentKey) -> Result<()> {
        let started = Instant::now();
        let result = (|| {
            let skill_id = validate_skill_id(skill_id)?;
            if self.db.is_some() {
                self.disable_via_assignment_key(skill_id, agent_key)
            } else {
                self.disable_projection_key(skill_id, agent_key)
            }
        })();
        match &result {
            Ok(()) => {
                self.invalidate_list_cache();
                log_skill_write_result(
                    "disable",
                    skill_id,
                    Some(agent_key.as_str()),
                    started,
                    true,
                    None,
                )
            }
            Err(e) => log_skill_write_result(
                "disable",
                skill_id,
                Some(agent_key.as_str()),
                started,
                false,
                Some(e),
            ),
        }
        result
    }

    /// FS-only project (used when no DB). Ownership rules match
    /// [`SkillReconciler::project_copy`] via shared `project_copy_with_ownership`.
    fn sync_projection(&self, skill_id: &str, agent: AgentId, force: bool) -> Result<()> {
        let (source_dir, skills_root, target_dir) =
            self.resolve_projection_paths(skill_id, agent)?;
        let agent_key = AgentKey::from_agent_id(agent);

        // Validate source tree up front (no staging until source is safe).
        let source_files = validate_and_collect_source(&source_dir, skill_id)?;
        let revision = self.projection_revision(skill_id);

        project_copy_with_ownership(
            &skills_root,
            skill_id,
            &source_dir,
            &target_dir,
            &source_files,
            force,
            &revision,
            &agent_key,
        )
    }

    /// Resolve ownership revision from lock record, then package row, else `"1"`.
    fn projection_revision(&self, skill_id: &str) -> String {
        if let Ok(lock) = skill_lock_load(&self.source_root) {
            if let Some(rec) = lock.get(skill_id) {
                return package_revision(rec);
            }
        }
        if let Some(db) = self.db.as_ref() {
            if let Ok(Some(pkg)) = SkillRepo::new(db.clone()).get_package(skill_id) {
                if !pkg.revision.trim().is_empty() {
                    return pkg.revision;
                }
            }
        }
        "1".into()
    }

    /// After a successful package commit: reconcile desired assignments.
    ///
    /// Single-target failures keep the shared package and rely on the reconciler
    /// observed error; infrastructure failures return a partial-failure error
    /// (package is **not** rolled back).
    fn reconcile_after_package_commit(&self, skill_id: &str) -> Result<()> {
        let Some(_) = self.db.as_ref() else {
            return Ok(());
        };
        let now = chrono_now();
        let partial = |e: &AppError| {
            let msg = crate::utils::redact::redact_text(&e.to_string());
            AppError::message(
                "skill.reconcile_partial",
                format!(
                    "shared package for '{skill_id}' was updated but projection reconcile failed: {msg}"
                ),
            )
        };
        let (_assign, reconciler) = self.assignment_stack().map_err(|e| partial(&e))?;
        match reconciler.reconcile_skill(skill_id, true, &now) {
            Ok(outcomes) => {
                let mut failed = 0u32;
                for (agent, r) in &outcomes {
                    if let Err(e) = r {
                        failed += 1;
                        let msg = crate::utils::redact::redact_text(&e.to_string());
                        tracing::warn!(
                            module = targets::SKILL,
                            op = "reconcile_after_commit",
                            skill_id = skill_id,
                            agent = agent.as_str(),
                            code = e.code(),
                            "shared package committed; target reconcile failed: {msg}"
                        );
                    }
                }
                if failed > 0 {
                    tracing::warn!(
                        module = targets::SKILL,
                        op = "reconcile_after_commit",
                        skill_id = skill_id,
                        failed_targets = failed,
                        total_targets = outcomes.len(),
                        "shared package committed; {failed}/{} target reconcile(s) failed",
                        outcomes.len()
                    );
                }
                Ok(())
            }
            // Repo / update_observed infrastructure errors → skill.reconcile_partial.
            Err(e) => Err(partial(&e)),
        }
    }

    /// FS-only unproject — ownership proof required (same as reconciler).
    fn disable_projection_key(&self, skill_id: &str, agent_key: &AgentKey) -> Result<()> {
        let (source_dir, skills_root, target_dir) =
            self.resolve_projection_paths_key(skill_id, agent_key)?;

        unproject_with_ownership(&skills_root, skill_id, &source_dir, &target_dir, agent_key)
    }

    fn assignment_stack(&self) -> Result<(SkillAssignmentService, SkillReconciler)> {
        let db = self.db.as_ref().ok_or_else(|| {
            AppError::message("skill.db", "skill assignment database is not configured")
        })?;
        let repo = SkillRepo::new(db.clone());
        let assign = SkillAssignmentService::new(repo.clone());
        let reconciler = SkillReconciler::new(self.source_root.clone(), self.targets.clone(), repo);
        Ok((assign, reconciler))
    }

    fn sync_via_assignment(&self, skill_id: &str, agent: AgentId, force: bool) -> Result<()> {
        let now = chrono_now();
        let (assign, reconciler) = self.assignment_stack()?;
        let lock = skill_lock_load(&self.source_root)?;
        let record = lock.get(skill_id);
        let agent_key = AgentKey::from_agent_id(agent);
        assign.ensure_package(skill_id, record, &now)?;
        assign.set_desired_enabled(skill_id, &agent_key, true, Some("copy"), &now)?;
        reconciler.reconcile_one(skill_id, &agent_key, force, &now)
    }

    fn disable_via_assignment_key(&self, skill_id: &str, agent_key: &AgentKey) -> Result<()> {
        let now = chrono_now();
        let (assign, reconciler) = self.assignment_stack()?;
        let lock = skill_lock_load(&self.source_root)?;
        let record = lock.get(skill_id);
        // Package may already exist; ensure so FK for assignment is satisfied.
        assign.ensure_package(skill_id, record, &now)?;
        assign.set_desired_enabled(skill_id, agent_key, false, None, &now)?;
        reconciler.reconcile_one(skill_id, agent_key, false, &now)
    }

    /// Build projections for every agent in [`AgentId::ALL`] order.
    ///
    /// Source tree index is collected once and reused across agents that support
    /// skills (avoids re-reading the same source for Claude/Codex/Grok).
    fn project_matrix(&self, skill_id: &str, source_dir: &Path) -> Vec<SkillProjection> {
        // Index only — path → size. Full bytes are never held for list().
        let source_index = collect_file_index(source_dir).ok();
        // Lazy content hashes when path+size matches (streamed, discarded after).
        let mut source_hashes: Option<BTreeMap<String, u64>> = None;

        AgentId::ALL
            .iter()
            .copied()
            .map(|agent| {
                self.project_one(
                    agent,
                    skill_id,
                    source_dir,
                    source_index.as_ref(),
                    &mut source_hashes,
                )
            })
            .collect()
    }

    fn project_one(
        &self,
        agent: AgentId,
        skill_id: &str,
        source_dir: &Path,
        source_index: Option<&BTreeMap<String, u64>>,
        source_hashes: &mut Option<BTreeMap<String, u64>>,
    ) -> SkillProjection {
        let blocked = |map_status: SkillMapStatus| SkillProjection {
            agent,
            state: SkillSyncState::Unsupported,
            link_kind: SkillLinkKind::None,
            target_dir: None,
            resolved_target: None,
            map_status,
        };

        let Some(adapter) = self.registry.get(agent) else {
            return blocked(SkillMapStatus::AgentUnsupported);
        };

        if adapter.capability(Capability::Skills).is_blocked() {
            return blocked(SkillMapStatus::AgentUnsupported);
        }

        let Some(skills_root) = adapter.skills_dir() else {
            // Adapter claims skills support but provides no root.
            return blocked(SkillMapStatus::TargetUnavailable);
        };

        let target_dir = skills_root.join(skill_id);
        let (state, link_kind, resolved_target) =
            classify_projection(source_dir, source_index, source_hashes, &target_dir);
        let map_status = match state {
            SkillSyncState::Unsupported => SkillMapStatus::TargetUnavailable,
            SkillSyncState::Foreign | SkillSyncState::Conflict => SkillMapStatus::Conflict,
            SkillSyncState::Linked | SkillSyncState::Copied | SkillSyncState::Absent => {
                SkillMapStatus::Available
            }
        };
        SkillProjection {
            agent,
            state,
            link_kind,
            target_dir: Some(target_dir),
            resolved_target,
            map_status,
        }
    }

    /// Resolve source + skills_root + target for a validated skill id / agent.
    fn resolve_projection_paths(
        &self,
        skill_id: &str,
        agent: AgentId,
    ) -> Result<(PathBuf, PathBuf, PathBuf)> {
        let agent_key = AgentKey::from_agent_id(agent);
        if self.targets.get(&agent_key).is_none() && self.registry.get(agent).is_none() {
            return Err(AppError::NotFound(format!(
                "agent {} has no adapter",
                agent.as_str()
            )));
        }
        self.resolve_projection_paths_key(skill_id, &agent_key)
    }

    fn resolve_projection_paths_key(
        &self,
        skill_id: &str,
        agent_key: &AgentKey,
    ) -> Result<(PathBuf, PathBuf, PathBuf)> {
        let target = self.targets.get(agent_key).ok_or_else(|| {
            AppError::Unsupported(format!("agent {} has no skill target", agent_key.as_str()))
        })?;
        if !target.supports_skills() {
            return Err(AppError::Unsupported(format!(
                "agent {} does not support skills",
                agent_key.as_str()
            )));
        }
        let skills_root = target.skills_root().ok_or_else(|| {
            AppError::Unsupported(format!(
                "agent {} has no skills directory",
                agent_key.as_str()
            ))
        })?;

        // skills_root must not be a symlink or non-directory; ancestors must not
        // be symlinks either (prevents target resolution via adapter-supplied links).
        validate_skills_root(&skills_root)?;

        let source_dir = self.source_root.join(skill_id);
        let target_dir = skills_root.join(skill_id);

        if !is_exact_child(&source_dir, &self.source_root, skill_id) {
            return Err(AppError::InvalidArg(format!(
                "skill source path escapes source root: {}",
                source_dir.display()
            )));
        }
        if !is_exact_child(&target_dir, &skills_root, skill_id) {
            return Err(AppError::InvalidArg(format!(
                "skill target path escapes skills root: {}",
                target_dir.display()
            )));
        }

        // Source path (including leaf) must not traverse links — truth is a real dir.
        ensure_no_symlink_in_existing_prefix(&source_dir)?;
        // Target skill directory itself may be a projection link/reparse point;
        // only ancestors of the leaf are forbidden to be links.
        ensure_no_symlink_in_ancestors(&target_dir)?;

        reject_source_target_overlap(&self.source_root, &source_dir, &skills_root, &target_dir)?;

        Ok((source_dir, skills_root, target_dir))
    }

    // -----------------------------------------------------------------------
    // Install / uninstall / update / project (Phase B)
    // -----------------------------------------------------------------------

    /// Install a skill into the shared source root from a local path, zip, or git URL.
    ///
    /// Atomic: materialize + validate under staging, then commit live + lock +
    /// package revision together (backup retained until metadata succeeds).
    /// Requires `SKILL.md` in the package root. Same-name install without
    /// `overwrite` is rejected.
    pub fn install_skill(&self, source: &str, overwrite: bool) -> Result<Skill> {
        let started = Instant::now();
        let result = (|| {
            let source = source.trim();
            if source.is_empty() {
                return Err(AppError::InvalidArg(
                    "install source must not be empty".into(),
                ));
            }
            let _global = acquire_skill_root_lock(&self.source_root)?;

            let sources = SkillSourceService::new();
            let packages = SkillPackageService::new();
            // 1–2: materialize + validate **before** touching live.
            let (package_dir, cleanup, source_kind, locator) = sources.materialize(source)?;
            let install_result = (|| {
                sources.ensure_skill_md(&package_dir)?;
                let skill_id = sources.infer_skill_id(&package_dir, source)?;
                let skill_id = validate_skill_id(&skill_id)?.to_string();
                let _skill_lock = acquire_skill_lock(&self.source_root, &skill_id)?;

                let dest = self.source_root.join(&skill_id);
                if dest.exists() && !overwrite {
                    return Err(AppError::InvalidArg(format!(
                        "skill '{skill_id}' already exists (pass overwrite to replace)"
                    )));
                }

                if !self.source_root.exists() {
                    fs::create_dir_all(&self.source_root)?;
                }
                ensure_no_symlink_in_existing_prefix(&self.source_root)?;

                if dest.exists() {
                    // Replace existing real directory only (never follow a link root).
                    let meta = fs::symlink_metadata(&dest)?;
                    if is_link_or_reparse(&meta) {
                        return Err(AppError::InvalidArg(format!(
                            "refusing to overwrite link at skill source: {}",
                            dest.display()
                        )));
                    }
                }

                let files = packages.validate_and_collect(&package_dir, &skill_id)?;
                let now = chrono_now();
                let record = SkillSourceRecord {
                    kind: source_kind,
                    locator: locator.clone(),
                    version: None,
                    installed_at: now.clone(),
                    updated_at: None,
                };

                let repo = self.db.as_ref().map(|db| SkillRepo::new(db.clone()));
                let committed = commit_skill_package(
                    &self.source_root,
                    &skill_id,
                    PreparedSkillTree::Files(&files),
                    record,
                    repo.as_ref(),
                    &now,
                    SkillCommitFaults::default(),
                )?;

                // Reconcile after main commit (no package rollback on target errors).
                self.reconcile_after_package_commit(&skill_id)?;

                let (name, description) = read_skill_metadata(&committed.dest, &committed.skill_id);
                let projections = self.project_matrix(&committed.skill_id, &committed.dest);
                Ok(Skill {
                    id: committed.skill_id,
                    name,
                    description,
                    source_dir: committed.dest,
                    projections,
                })
            })();

            if let Some(dir) = cleanup {
                let _ = fs::remove_dir_all(dir);
            }
            install_result
        })();

        match &result {
            Ok(skill) => {
                self.invalidate_list_cache();
                log_skill_write_result("install", &skill.id, None, started, true, None)
            }
            // Avoid logging raw source (may contain path/URL credentials); AppError is redacted.
            Err(e) => log_skill_write_result("install", "-", None, started, false, Some(e)),
        }
        result
    }

    /// Uninstall a shared source skill: snapshot, remove all agent projections,
    /// delete the source directory, drop lock entry.
    ///
    /// Agent-private skills (not under source root) are removed only from their
    /// own root when `skill_id` is found via [`list_installed`] with origin.
    pub fn uninstall_skill(
        &self,
        skill_id: &str,
        backup: Option<&crate::services::BackupService>,
    ) -> Result<()> {
        let started = Instant::now();
        let result = (|| {
            let skill_id = validate_skill_id(skill_id)?.to_string();
            let _global = acquire_skill_root_lock(&self.source_root)?;
            let _skill_lock = acquire_skill_lock(&self.source_root, &skill_id)?;

            let source_dir = self.source_root.join(&skill_id);
            if !source_dir.exists() {
                return Err(AppError::NotFound(format!(
                    "skill '{skill_id}' is not installed in the shared source root"
                )));
            }

            // Validate lock is readable *before* any projection/source delete.
            // A corrupt `.skill-lock.json` must not cause uninstall side effects.
            let _lock = skill_lock_load(&self.source_root)?;

            // Best-effort pre-uninstall snapshot of agent live files (not the skill
            // tree itself — backup_service snapshots adapter live paths). Callers
            // that hold BackupService should pass it; we still proceed without.
            if let Some(backups) = backup {
                for agent in AgentId::ALL {
                    if self
                        .registry
                        .get(agent)
                        .is_some_and(|a| a.capability(Capability::Skills).is_usable())
                    {
                        let _ = backups.snapshot(
                            agent,
                            crate::models::BackupKind::PreSkillUninstall,
                            Some(&format!("pre-skill-uninstall:{skill_id}")),
                        );
                    }
                }
            }

            // Build the complete key-native target set before mutating anything.
            // Assignments for a missing/disabled contribution fail closed: the
            // source must remain until that target can be safely reconciled.
            let target_keys: Vec<AgentKey> = self
                .targets
                .all()
                .filter(|target| target.supports_skills() && target.skills_root().is_some())
                .map(|target| target.agent_key())
                .collect();
            if let Some(db) = self.db.as_ref() {
                let repo = SkillRepo::new(db.clone());
                for assignment in repo.list_assignments_for_skill(&skill_id)? {
                    let key = AgentKey::parse(assignment.agent_key.clone()).map_err(|err| {
                        AppError::message(
                            "skill.assignment_data",
                            format!(
                                "invalid agent_key '{}' in skill assignment for '{}': {err}",
                                assignment.agent_key, skill_id
                            ),
                        )
                    })?;
                    if !target_keys.contains(&key) {
                        return Err(AppError::message(
                            "skill.target_unavailable",
                            format!(
                                "cannot uninstall skill '{skill_id}': target for assigned agent '{}' is not registered and usable",
                                key.as_str()
                            ),
                        ));
                    }
                }
            }

            // Remove projections first. Every lock, ownership, marker and target
            // failure propagates; source/lock deletion is strictly after all keys.
            let lock_dir = self.source_root.join(".locks");
            for key in target_keys {
                let _agent_lock = AgentWriteLock::acquire_key(&lock_dir, &key)?;
                self.disable_key(&skill_id, &key)?;
            }

            // Remove source directory (must be a real dir).
            let meta = fs::symlink_metadata(&source_dir)?;
            if is_link_or_reparse(&meta) {
                return Err(AppError::InvalidArg(format!(
                    "refusing to uninstall: source is a link: {}",
                    source_dir.display()
                )));
            }
            validate_tree_entries_safe(&source_dir, "skill source")?;
            fs::remove_dir_all(&source_dir)?;
            skill_lock_remove(&self.source_root, &skill_id)?;
            Ok(())
        })();
        match &result {
            Ok(()) => {
                self.invalidate_list_cache();
                log_skill_write_result("uninstall", skill_id, None, started, true, None)
            }
            Err(e) => log_skill_write_result("uninstall", skill_id, None, started, false, Some(e)),
        }
        result
    }

    /// Uninstall a skill from an agent-private root (not projectable).
    pub fn uninstall_private_skill(&self, skill_id: &str, agent: AgentId) -> Result<()> {
        let started = Instant::now();
        let result = (|| {
            let skill_id = validate_skill_id(skill_id)?.to_string();
            let adapter = self.registry.get(agent).ok_or_else(|| {
                AppError::Unsupported(format!("agent {} has no adapter", agent.as_str()))
            })?;
            let skills_root = adapter.skills_dir().ok_or_else(|| {
                AppError::Unsupported(format!("agent {} has no skills directory", agent.as_str()))
            })?;
            let target = skills_root.join(&skill_id);
            // Must not be the shared source.
            if paths_equal_lexical(&skills_root, &self.source_root) {
                return self.uninstall_skill(&skill_id, None);
            }
            let lock_dir = self.source_root.join(".locks");
            let _agent_lock = AgentWriteLock::acquire(&lock_dir, agent)?;
            match inspect_projection_target(&target)? {
                TargetPresence::Missing => Ok(()),
                TargetPresence::Link { .. } => remove_projection_link(&target),
                TargetPresence::Directory => {
                    validate_tree_entries_safe(&target, "private skill")?;
                    fs::remove_dir_all(&target)?;
                    Ok(())
                }
                TargetPresence::Dangerous { kind } => Err(AppError::InvalidArg(format!(
                    "refusing to uninstall private skill ({kind}): {}",
                    target.display()
                ))),
            }
        })();
        // Shared-root path already logs inside uninstall_skill; still record agent context.
        match &result {
            Ok(()) => {
                self.invalidate_list_cache();
                log_skill_write_result(
                    "uninstall_private",
                    skill_id,
                    Some(agent.as_str()),
                    started,
                    true,
                    None,
                )
            }
            Err(e) => log_skill_write_result(
                "uninstall_private",
                skill_id,
                Some(agent.as_str()),
                started,
                false,
                Some(e),
            ),
        }
        result
    }

    /// Update a skill from its recorded source (git: staging clone + commit;
    /// local/zip/market: re-fetch + commit). Never `git pull` on the live tree.
    pub fn update_skill(&self, skill_id: &str) -> Result<Skill> {
        let started = Instant::now();
        let result = (|| {
            let skill_id = validate_skill_id(skill_id)?.to_string();
            let _global = acquire_skill_root_lock(&self.source_root)?;
            let _skill_lock = acquire_skill_lock(&self.source_root, &skill_id)?;

            let dest = self.source_root.join(&skill_id);
            // Refuse link/junction roots (do not follow into foreign trees).
            match fs::symlink_metadata(&dest) {
                Err(e) if e.kind() == io::ErrorKind::NotFound => {
                    return Err(AppError::NotFound(format!(
                        "skill '{skill_id}' not found in source root"
                    )));
                }
                Err(e) => return Err(AppError::from(e)),
                Ok(meta) if is_link_or_reparse(&meta) => {
                    return Err(AppError::InvalidArg(format!(
                        "refusing to update link at skill source: {}",
                        dest.display()
                    )));
                }
                Ok(meta) if !meta.is_dir() => {
                    return Err(AppError::NotFound(format!(
                        "skill '{skill_id}' not found in source root"
                    )));
                }
                Ok(_) => {}
            }
            // Main commit must surface lock read/parse errors (not empty map).
            let lock = skill_lock_load(&self.source_root)?;
            let record = lock.get(&skill_id).cloned().ok_or_else(|| {
                AppError::InvalidArg(format!(
                    "skill '{skill_id}' has no recorded source in .skill-lock.json; reinstall to track origin"
                ))
            })?;

            let now = chrono_now();
            let mut updated = record.clone();
            updated.updated_at = Some(now.clone());

            // 1–2: materialize + validate fully before live swap.
            let prepared = match record.kind.as_str() {
                "git" => {
                    // Clone to staging only — no live pull.
                    let staging =
                        prepare_git_skill_staging(&self.source_root, &skill_id, &record.locator)?;
                    PreparedSkillTree::StagingDir(staging)
                }
                "local" | "zip" | "market" => {
                    let sources = SkillSourceService::new();
                    let packages = SkillPackageService::new();
                    let (package_dir, cleanup, _, _) = sources.materialize(&record.locator)?;
                    let files_result = (|| {
                        sources.ensure_skill_md(&package_dir)?;
                        packages.validate_and_collect(&package_dir, &skill_id)
                    })();
                    if let Some(dir) = cleanup {
                        let _ = fs::remove_dir_all(dir);
                    }
                    let files = files_result?;
                    // Leak files into owned map for commit (PreparedSkillTree::Files borrows).
                    // Commit immediately in this arm instead.
                    let repo = self.db.as_ref().map(|db| SkillRepo::new(db.clone()));
                    let committed = commit_skill_package(
                        &self.source_root,
                        &skill_id,
                        PreparedSkillTree::Files(&files),
                        updated.clone(),
                        repo.as_ref(),
                        &now,
                        SkillCommitFaults::default(),
                    )?;
                    self.reconcile_after_package_commit(&skill_id)?;
                    let (name, description) =
                        read_skill_metadata(&committed.dest, &committed.skill_id);
                    return Ok(Skill {
                        id: committed.skill_id,
                        name,
                        description,
                        source_dir: committed.dest,
                        projections: self.project_matrix(&skill_id, &dest),
                    });
                }
                other => {
                    return Err(AppError::InvalidArg(format!(
                        "cannot update skill with source kind '{other}'"
                    )));
                }
            };

            let repo = self.db.as_ref().map(|db| SkillRepo::new(db.clone()));
            let committed = commit_skill_package(
                &self.source_root,
                &skill_id,
                prepared,
                updated,
                repo.as_ref(),
                &now,
                SkillCommitFaults::default(),
            )?;

            // assignment_stack / ensure_package errors are part of main commit
            // (via commit_skill_package). Reconcile is post-commit only.
            self.reconcile_after_package_commit(&skill_id)?;

            let (name, description) = read_skill_metadata(&committed.dest, &committed.skill_id);
            Ok(Skill {
                id: committed.skill_id,
                name,
                description,
                source_dir: committed.dest,
                projections: self.project_matrix(&skill_id, &dest),
            })
        })();
        match &result {
            Ok(skill) => {
                self.invalidate_list_cache();
                log_skill_write_result("update", &skill.id, None, started, true, None)
            }
            Err(e) => log_skill_write_result("update", skill_id, None, started, false, Some(e)),
        }
        result
    }

    /// Project a source skill onto an agent as link or copy.
    ///
    /// Windows link mode: junction → symlink → copy.
    /// Unix link mode: symlink → copy.
    pub fn project_skill(
        &self,
        skill_id: &str,
        agent: AgentId,
        mode: SkillProjectMode,
    ) -> Result<SkillProjectResult> {
        let started = Instant::now();
        let result = (|| {
            let skill_id = validate_skill_id(skill_id)?.to_string();
            let lock_dir = self.source_root.join(".locks");
            let _agent_lock = AgentWriteLock::acquire(&lock_dir, agent)?;
            let _skill_lock = acquire_skill_lock(&self.source_root, &skill_id)?;

            let (source_dir, skills_root, target_dir) =
                self.resolve_projection_paths(&skill_id, agent)?;
            let agent_key = AgentKey::from_agent_id(agent);
            ensure_skill_md(&source_dir).map_err(|_| {
                AppError::NotFound(format!(
                    "skill source not found or missing SKILL.md: {skill_id}"
                ))
            })?;

            // Only clear targets we can prove are platform-managed (R03).
            // Unmanaged user directories / foreign links → skill.conflict.
            clear_managed_target_for_reproject(
                &skills_root,
                &skill_id,
                &source_dir,
                &target_dir,
                &agent_key,
            )?;

            if !skills_root.exists() {
                ensure_no_symlink_in_existing_prefix(&skills_root)?;
                fs::create_dir_all(&skills_root)?;
            }
            validate_skills_root(&skills_root)?;

            let revision = self.projection_revision(&skill_id);
            let (applied, fell_back) = match mode {
                SkillProjectMode::Copy => {
                    let files = validate_and_collect_source(&source_dir, &skill_id)?;
                    materialize_projection(&skills_root, &skill_id, &target_dir, &files, None)?;
                    // Marker write failure fails the op; assignment path would
                    // not claim applied. Here project_skill returns Err.
                    record_copy_ownership(&skills_root, &skill_id, &target_dir, &revision)?;
                    (SkillLinkKind::None, false)
                }
                SkillProjectMode::Link => {
                    let (applied, fell_back) = create_projection_link(&source_dir, &target_dir)?;
                    // True link: clear stale marker (errors must not be swallowed).
                    // SkillLinkKind::None = copy fallback → record ownership marker.
                    finalize_link_projection_ownership(
                        &skills_root,
                        &skill_id,
                        &target_dir,
                        applied == SkillLinkKind::None,
                        &revision,
                    )?;
                    (applied, fell_back)
                }
            };

            Ok(SkillProjectResult {
                skill_id,
                agent,
                requested_mode: mode,
                applied_link_kind: applied,
                fell_back,
                target_dir,
            })
        })();
        match &result {
            Ok(r) => {
                self.invalidate_list_cache();
                tracing::info!(
                    module = targets::SKILL,
                    op = "project",
                    skill_id = %r.skill_id,
                    agent = agent.as_str(),
                    mode = mode.as_str(),
                    applied = r.applied_link_kind.as_str(),
                    fell_back = r.fell_back,
                    elapsed_ms = elapsed_ms(started),
                    "ok"
                );
            }
            Err(e) => log_skill_write_result(
                "project",
                skill_id,
                Some(agent.as_str()),
                started,
                false,
                Some(e),
            ),
        }
        result
    }

    /// List skills from the shared source **and** agent-private roots.
    pub fn list_installed(&self) -> Result<Vec<InstalledSkill>> {
        let mut out: Vec<InstalledSkill> = Vec::new();
        let lock = skill_lock_load(&self.source_root)?;

        // Shared source skills (projectable).
        for skill in self.list()? {
            let source = lock.get(&skill.id).cloned();
            out.push(installed_skill_from_shared(
                &self.source_root,
                skill,
                source,
            ));
        }

        let shared_ids: HashSet<String> = out.iter().map(|s| s.id.clone()).collect();

        // Per shared skill: (file index, lazy content hashes) reused across agents.
        let mut shared_cmp_cache: HashMap<
            String,
            (Option<BTreeMap<String, u64>>, Option<BTreeMap<String, u64>>),
        > = HashMap::new();

        // Agent workspace roots: every on-disk skill under each agent, including
        // projections/copies of shared ids.
        // - Not in shared → PrivateSource (可加入共享库)
        // - Same id + content equal / link to shared → Available (已在共享库)
        // - Same id + content differs → Conflict (有冲突，可覆盖加入)
        let source_root = &self.source_root;
        for_each_agent_skill_dir(&self.registry, |entry| {
            let map_status = if shared_ids.contains(&entry.skill_id) {
                let shared_dir = source_root.join(&entry.skill_id);
                map_status_agent_vs_shared(
                    &entry.skill_id,
                    &shared_dir,
                    &entry.path,
                    &mut shared_cmp_cache,
                )
            } else {
                SkillMapStatus::PrivateSource
            };
            out.push(installed_skill_from_agent(entry, map_status));
        });

        out.sort_by(|a, b| a.id.cmp(&b.id).then(a.origin.cmp(&b.origin)));
        Ok(out)
    }

    /// Catalog view: shared library rows plus agent-private skills that are **not**
    /// already in the shared library (id match only — no content-hash compare).
    ///
    /// - Shared rows match [`Self::list`] content: `origin=shared`, projectable, full projections.
    /// - Agent-only ids emit one row per agent directory (`projections` is empty).
    /// - Same id under two agents (and not in shared) stays two rows.
    /// - Agent copies / conflicts of a shared id are omitted.
    pub fn list_catalog(&self) -> Result<Vec<InstalledSkill>> {
        let mut out: Vec<InstalledSkill> = Vec::new();
        let lock = skill_lock_load(&self.source_root)?;

        for skill in self.list()? {
            let source = lock.get(&skill.id).cloned();
            out.push(installed_skill_from_shared(
                &self.source_root,
                skill,
                source,
            ));
        }

        let shared_ids: HashSet<String> = out.iter().map(|s| s.id.clone()).collect();

        for_each_agent_skill_dir(&self.registry, |entry| {
            if shared_ids.contains(&entry.skill_id) {
                return;
            }
            out.push(installed_skill_from_agent(
                entry,
                SkillMapStatus::PrivateSource,
            ));
        });

        out.sort_by(|a, b| a.id.cmp(&b.id).then(a.origin.cmp(&b.origin)));
        Ok(out)
    }

    /// Copy an agent-private skill into the shared source root.
    ///
    /// - Default is **copy** (never moves / deletes the private skill).
    /// - Validates skill id, path safety, and `SKILL.md`.
    /// - Without `overwrite`, refuses if the shared root already has the same id.
    /// - Reuses the same locks / staging / tree validation as [`install_skill`].
    pub fn import_private_to_shared(
        &self,
        skill_id: &str,
        agent: AgentId,
        overwrite: bool,
    ) -> Result<Skill> {
        let started = Instant::now();
        let result = (|| {
            let skill_id = validate_skill_id(skill_id)?.to_string();
            let adapter = self.registry.require(agent, Capability::Skills)?;
            let skills_root = adapter.skills_dir().ok_or_else(|| {
                AppError::Unsupported(format!("agent {} has no skills directory", agent.as_str()))
            })?;

            // Private root must not be the shared source.
            if paths_equal_lexical(&skills_root, &self.source_root) {
                return Err(AppError::InvalidArg(format!(
                    "skill '{skill_id}' is already under the shared source root"
                )));
            }

            let private_dir = skills_root.join(&skill_id);
            if !is_exact_child(&private_dir, &skills_root, &skill_id) {
                return Err(AppError::InvalidArg(format!(
                    "private skill path escapes agent skills root: {}",
                    private_dir.display()
                )));
            }

            // Resolve real package directory (follow link only for reading content).
            let package_dir = match inspect_projection_target(&private_dir)? {
                TargetPresence::Missing => {
                    return Err(AppError::NotFound(format!(
                        "private skill '{skill_id}' not found for {}",
                        agent.as_str()
                    )));
                }
                TargetPresence::Link { .. } => {
                    resolve_link_path(&private_dir).ok_or_else(|| {
                        AppError::InvalidArg(format!(
                            "private skill link is unresolvable: {}",
                            private_dir.display()
                        ))
                    })?
                }
                TargetPresence::Directory => private_dir.clone(),
                TargetPresence::Dangerous { kind } => {
                    return Err(AppError::InvalidArg(format!(
                        "refusing to import unsafe private skill ({kind}): {}",
                        private_dir.display()
                    )));
                }
            };

            // Do not allow importing the shared root itself via a private alias.
            if paths_equal_lexical(&package_dir, &self.source_root.join(&skill_id)) {
                return Err(AppError::InvalidArg(format!(
                    "private skill resolves to the shared source entry for '{skill_id}'"
                )));
            }

            ensure_skill_md(&package_dir)?;
            let files = validate_and_collect_source(&package_dir, &skill_id)?;

            let _global = acquire_skill_root_lock(&self.source_root)?;
            let _skill_lock = acquire_skill_lock(&self.source_root, &skill_id)?;

            let dest = self.source_root.join(&skill_id);
            if dest.exists() && !overwrite {
                return Err(AppError::message(
                    "skill.conflict",
                    format!(
                        "skill '{skill_id}' already exists in shared source (pass overwrite to replace)"
                    ),
                ));
            }

            if !self.source_root.exists() {
                fs::create_dir_all(&self.source_root)?;
            }
            ensure_no_symlink_in_existing_prefix(&self.source_root)?;

            if dest.exists() {
                let meta = fs::symlink_metadata(&dest)?;
                if is_link_or_reparse(&meta) {
                    return Err(AppError::InvalidArg(format!(
                        "refusing to overwrite link at skill source: {}",
                        dest.display()
                    )));
                }
                materialize_projection(&self.source_root, &skill_id, &dest, &files, Some(&dest))?;
            } else {
                materialize_projection(&self.source_root, &skill_id, &dest, &files, None)?;
            }

            let now = chrono_now();
            let record = SkillSourceRecord {
                kind: "local".into(),
                locator: private_dir.to_string_lossy().into_owned(),
                version: None,
                installed_at: now,
                updated_at: None,
            };
            skill_lock_upsert(&self.source_root, &skill_id, record)?;

            // Private original must remain after copy.
            if !private_dir.exists() {
                return Err(AppError::message(
                    "skill.import",
                    format!(
                        "import unexpectedly removed private skill at {}",
                        private_dir.display()
                    ),
                ));
            }

            let (name, description) = read_skill_metadata(&dest, &skill_id);
            let projections = self.project_matrix(&skill_id, &dest);
            Ok(Skill {
                id: skill_id,
                name,
                description,
                source_dir: dest,
                projections,
            })
        })();

        match &result {
            Ok(skill) => {
                self.invalidate_list_cache();
                log_skill_write_result(
                    "import_private",
                    &skill.id,
                    Some(agent.as_str()),
                    started,
                    true,
                    None,
                )
            }
            Err(e) => log_skill_write_result(
                "import_private",
                skill_id,
                Some(agent.as_str()),
                started,
                false,
                Some(e),
            ),
        }
        result
    }

    pub fn skill_lock_path(&self) -> PathBuf {
        self.source_root.join(".skill-lock.json")
    }
}

// ---------------------------------------------------------------------------
// List cache fingerprint
// ---------------------------------------------------------------------------

/// Shallow directory fingerprint: entry name + kind + mtime/size, plus `SKILL.md`
/// when present. Deep content is intentionally not hashed (writes + watcher
/// invalidate; nested edits usually bump parent/SKILL.md mtime on Windows).
///
/// Hash order is **sorted by entry name** so fingerprint is stable across `read_dir`.
fn hash_skill_root_shallow(root: &Path, hasher: &mut DefaultHasher) {
    let Ok(rd) = fs::read_dir(root) else {
        0u8.hash(hasher);
        return;
    };
    // (name, kind, mtime, len, optional SKILL.md mtime/len)
    let mut rows: Vec<(String, u8, u64, u64, Option<(u64, u64)>)> = Vec::new();
    for ent in rd.flatten() {
        let name = ent.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let path = ent.path();
        let Ok(meta) = fs::symlink_metadata(&path) else {
            continue;
        };
        let kind: u8 = if is_link_or_reparse(&meta) {
            1
        } else if meta.is_dir() {
            2
        } else if meta.is_file() {
            3
        } else {
            4
        };
        let mtime = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let len = meta.len();
        let skill_md_fp = if kind == 2 {
            let skill_md = path.join("SKILL.md");
            fs::symlink_metadata(&skill_md).ok().map(|sm| {
                let sm_mtime = sm
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                (sm_mtime, sm.len())
            })
        } else {
            None
        };
        rows.push((name, kind, mtime, len, skill_md_fp));
    }
    rows.sort_by(|a, b| a.0.cmp(&b.0));
    for (name, kind, mtime, len, skill_md_fp) in rows {
        name.hash(hasher);
        kind.hash(hasher);
        mtime.hash(hasher);
        len.hash(hasher);
        match skill_md_fp {
            Some((sm_mtime, sm_len)) => {
                1u8.hash(hasher);
                sm_mtime.hash(hasher);
                sm_len.hash(hasher);
            }
            None => 0u8.hash(hasher),
        }
    }
}

// ---------------------------------------------------------------------------
// Validation / path safety
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Metadata / comparison (read path)
// ---------------------------------------------------------------------------

/// Read optional `SKILL.md` frontmatter; fall back to directory name / empty desc.
fn read_skill_metadata(skill_dir: &Path, fallback_name: &str) -> (String, String) {
    let skill_md = skill_dir.join("SKILL.md");
    match fs::read_to_string(&skill_md) {
        Ok(content) => parse_skill_frontmatter(&content, fallback_name),
        Err(_) => (fallback_name.to_string(), String::new()),
    }
}

/// Load a skill markdown file for GUI preview (metadata name + capped body).
fn read_skill_md_file(
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
fn is_yaml_block_scalar_marker(value: &str) -> bool {
    let v = value.trim();
    let mut chars = v.chars();
    match chars.next() {
        Some('|') | Some('>') => chars.all(|c| c.is_ascii_digit() || c == '-' || c == '+'),
        _ => false,
    }
}

/// Collect indented lines of a YAML block scalar, collapse to one display line.
fn collect_yaml_block_scalar(lines: &[&str], idx: &mut usize, key_indent: usize) -> String {
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

fn strip_simple_quotes(value: &str) -> &str {
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

/// Walk every agent skills root with the same discovery rules as
/// [`SkillService::list_installed`] / [`SkillService::list_catalog`].
fn for_each_agent_skill_dir(registry: &AdapterRegistry, mut visit: impl FnMut(AgentSkillDirEntry)) {
    for agent in AgentId::ALL {
        let Some(adapter) = registry.get(agent) else {
            continue;
        };
        if adapter.capability(Capability::Skills).is_blocked() {
            continue;
        }
        let Some(skills_root) = adapter.skills_dir() else {
            continue;
        };
        if !skills_root.is_dir() {
            continue;
        }
        let entries = match fs::read_dir(&skills_root) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for ent in entries {
            let ent = match ent {
                Ok(e) => e,
                Err(_) => continue,
            };
            let name = ent.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            let path = ent.path();
            let meta = match fs::symlink_metadata(&path) {
                Ok(m) => m,
                Err(_) => continue,
            };
            // Accept real dirs and projection links as "installed" for the agent.
            let is_dirish =
                meta.is_dir() || is_link_or_reparse(&meta) || meta.file_type().is_symlink();
            if !is_dirish {
                continue;
            }
            // Prefer entries that look like skills (SKILL.md) when resolvable.
            let skill_md_ok = path.join("SKILL.md").is_file()
                || resolve_link_path(&path)
                    .map(|r| r.join("SKILL.md").is_file())
                    .unwrap_or(false);
            if !skill_md_ok {
                continue;
            }
            let (display, description) = read_skill_metadata(&path, &name);
            visit(AgentSkillDirEntry {
                agent,
                skill_id: name,
                path,
                skills_root: skills_root.clone(),
                display,
                description,
            });
        }
    }
}

fn installed_skill_from_shared(
    source_root: &Path,
    skill: Skill,
    source: Option<SkillSourceRecord>,
) -> InstalledSkill {
    InstalledSkill {
        id: skill.id.clone(),
        name: skill.name,
        description: skill.description,
        source_dir: skill.source_dir.clone(),
        root_label: short_root_label(source_root),
        root_dir: source_root.to_path_buf(),
        origin: "shared".into(),
        projectable: true,
        map_status: SkillMapStatus::Available,
        source,
        projections: skill.projections,
    }
}

fn installed_skill_from_agent(
    entry: AgentSkillDirEntry,
    map_status: SkillMapStatus,
) -> InstalledSkill {
    InstalledSkill {
        id: entry.skill_id,
        name: entry.display,
        description: entry.description,
        source_dir: entry.path,
        root_label: short_root_label(&entry.skills_root),
        root_dir: entry.skills_root,
        origin: entry.agent.as_str().to_string(),
        projectable: false,
        map_status,
        source: None,
        projections: vec![],
    }
}

/// Compare an agent-root skill directory against the shared library copy.
///
/// Uses the same path/size + stream-hash pipeline as the projection matrix.
fn map_status_agent_vs_shared(
    skill_id: &str,
    shared_dir: &Path,
    agent_path: &Path,
    cache: &mut HashMap<String, (Option<BTreeMap<String, u64>>, Option<BTreeMap<String, u64>>)>,
) -> SkillMapStatus {
    let entry = cache.entry(skill_id.to_string()).or_insert_with(|| {
        let index = collect_file_index(shared_dir).ok();
        (index, None)
    });
    let source_index = entry.0.as_ref();
    let (state, _, _) = classify_projection(shared_dir, source_index, &mut entry.1, agent_path);
    match state {
        SkillSyncState::Linked | SkillSyncState::Copied => SkillMapStatus::Available,
        SkillSyncState::Foreign | SkillSyncState::Conflict => SkillMapStatus::Conflict,
        // Shared id listed but shared dir missing/unreadable — treat as local-only.
        SkillSyncState::Absent => SkillMapStatus::PrivateSource,
        SkillSyncState::Unsupported => SkillMapStatus::Conflict,
    }
}

/// Classify source vs target for the projection matrix (list path).
///
/// Returns `(state, link_kind, resolved_target)`.
///
/// - Missing → [`SkillSyncState::Absent`]
/// - Link resolving to source → [`SkillSyncState::Linked`] (+ kind + resolved)
/// - Link resolving elsewhere / unresolvable → [`SkillSyncState::Foreign`]
/// - Real directory, regular-file trees identical → [`SkillSyncState::Copied`]
/// - Real directory, content differs → [`SkillSyncState::Foreign`]
/// - File / special / unreadable / nested unsafe → [`SkillSyncState::Conflict`]
fn classify_projection(
    source: &Path,
    source_index: Option<&BTreeMap<String, u64>>,
    source_hashes: &mut Option<BTreeMap<String, u64>>,
    target: &Path,
) -> (SkillSyncState, SkillLinkKind, Option<PathBuf>) {
    let meta = match fs::symlink_metadata(target) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return (SkillSyncState::Absent, SkillLinkKind::None, None);
        }
        Err(_) => return (SkillSyncState::Conflict, SkillLinkKind::None, None),
        Ok(m) => m,
    };

    // Projection link: the skill directory itself may be a junction/symlink.
    if is_link_or_reparse(&meta) {
        let kind = detect_link_kind(target, &meta);
        let resolved = resolve_link_path(target);
        // Containment: resolved path must not escape... we report foreign if it
        // doesn't match source; callers that write still validate ancestors.
        if let Some(ref resolved_path) = resolved {
            if link_resolves_to_source(target, source) {
                return (SkillSyncState::Linked, kind, Some(resolved_path.clone()));
            }
            return (SkillSyncState::Foreign, kind, Some(resolved_path.clone()));
        }
        return (SkillSyncState::Foreign, kind, None);
    }

    if !meta.is_dir() {
        // Regular file or special where a skill dir is expected.
        return (SkillSyncState::Conflict, SkillLinkKind::None, None);
    }

    let Some(source_index) = source_index else {
        return (SkillSyncState::Conflict, SkillLinkKind::None, None);
    };

    let target_index = match collect_file_index(target) {
        Ok(t) => t,
        // Nested symlink / special / unreadable inside a real dir → conflict.
        Err(_) => return (SkillSyncState::Conflict, SkillLinkKind::None, None),
    };

    // Path set + size: content mismatches → foreign (not conflict).
    let mut size_match = source_index.len() == target_index.len();
    if size_match {
        for (path, src_size) in source_index {
            match target_index.get(path) {
                Some(tgt_size) if tgt_size == src_size => {}
                _ => {
                    size_match = false;
                    break;
                }
            }
        }
    }
    if !size_match {
        return (SkillSyncState::Foreign, SkillLinkKind::None, None);
    }

    // Same paths and sizes — stream-hash content (no full-byte retention).
    if source_hashes.is_none() {
        match hash_tree_files(source, source_index) {
            Ok(h) => *source_hashes = Some(h),
            Err(()) => return (SkillSyncState::Conflict, SkillLinkKind::None, None),
        }
    }
    let Some(src_hashes) = source_hashes.as_ref() else {
        return (SkillSyncState::Conflict, SkillLinkKind::None, None);
    };
    let tgt_hashes = match hash_tree_files(target, &target_index) {
        Ok(h) => h,
        Err(()) => return (SkillSyncState::Conflict, SkillLinkKind::None, None),
    };

    if src_hashes == &tgt_hashes {
        (SkillSyncState::Copied, SkillLinkKind::None, None)
    } else {
        (SkillSyncState::Foreign, SkillLinkKind::None, None)
    }
}

/// Collect normalized relative path → file size for all regular files under root.
///
/// Same safety rules as [`collect_regular_files`]: no symlink follow, non-regular
/// → error, case-fold portable-name collisions → error. Does **not** read bytes.
fn collect_file_index(root: &Path) -> std::result::Result<BTreeMap<String, u64>, ()> {
    let root_meta = fs::symlink_metadata(root).map_err(|_| ())?;
    if is_link_or_reparse(&root_meta) || !root_meta.is_dir() {
        return Err(());
    }

    let mut out = BTreeMap::new();
    collect_file_index_rec(root, root, &mut out)?;

    let mut portable_keys = BTreeMap::new();
    for key in out.keys() {
        let folded = key.to_lowercase();
        if portable_keys.insert(folded, key).is_some() {
            return Err(());
        }
    }

    Ok(out)
}

fn collect_file_index_rec(
    root: &Path,
    dir: &Path,
    out: &mut BTreeMap<String, u64>,
) -> std::result::Result<(), ()> {
    let entries = fs::read_dir(dir).map_err(|_| ())?;
    for ent in entries {
        let ent = ent.map_err(|_| ())?;
        let path = ent.path();

        let meta = fs::symlink_metadata(&path).map_err(|_| ())?;
        let ft = meta.file_type();

        if is_link_or_reparse(&meta) {
            return Err(());
        }
        if ft.is_dir() {
            collect_file_index_rec(root, &path, out)?;
            continue;
        }
        if !ft.is_file() {
            return Err(());
        }

        let rel = path.strip_prefix(root).map_err(|_| ())?;
        let key = normalize_rel_path(rel)?;
        let size = meta.len();
        if out.insert(key, size).is_some() {
            return Err(());
        }
    }
    Ok(())
}

/// Stream-hash each file listed in `index` under `root`. Buffers are discarded
/// after hashing — only the u64 digest is kept.
fn hash_tree_files(
    root: &Path,
    index: &BTreeMap<String, u64>,
) -> std::result::Result<BTreeMap<String, u64>, ()> {
    let mut out = BTreeMap::new();
    for rel in index.keys() {
        let path = join_normalized(root, rel)?;
        let hash = stream_file_hash(&path)?;
        out.insert(rel.clone(), hash);
    }
    Ok(out)
}

fn join_normalized(root: &Path, rel: &str) -> std::result::Result<PathBuf, ()> {
    let mut path = root.to_path_buf();
    for part in rel.split('/') {
        if part.is_empty() || part == "." || part == ".." {
            return Err(());
        }
        path.push(part);
    }
    Ok(path)
}

fn stream_file_hash(path: &Path) -> std::result::Result<u64, ()> {
    let mut file = fs::File::open(path).map_err(|_| ())?;
    let mut hasher = DefaultHasher::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf).map_err(|_| ())?;
        if n == 0 {
            break;
        }
        hasher.write(&buf[..n]);
    }
    Ok(hasher.finish())
}

// ---------------------------------------------------------------------------
// Phase B helpers retained on façade: timestamps, projection links, process locks
// ---------------------------------------------------------------------------

fn chrono_now() -> String {
    // Sub-second precision so install→update in the same wall-clock second still
    // yields a distinct package revision (revision falls back to updated_at).
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn short_root_label(root: &Path) -> String {
    if let Ok(home) = crate::utils::paths::home_dir() {
        if let Ok(rel) = root.strip_prefix(&home) {
            return format!("~/{}", rel.to_string_lossy().replace('\\', "/"));
        }
    }
    root.display().to_string()
}
/// Create a projection link with platform fallbacks.
/// Returns (applied_kind, fell_back).
fn create_projection_link(source: &Path, target: &Path) -> Result<(SkillLinkKind, bool)> {
    let source = fs::canonicalize(source).map_err(|e| {
        AppError::InvalidArg(format!(
            "cannot canonicalize skill source {}: {e}",
            source.display()
        ))
    })?;

    #[cfg(windows)]
    {
        // 1) Junction (no admin)
        if create_windows_junction_runtime(&target, &source).is_ok() {
            return Ok((SkillLinkKind::Junction, false));
        }
        // 2) Directory symlink
        if std::os::windows::fs::symlink_dir(&source, target).is_ok() {
            return Ok((SkillLinkKind::Symlink, true));
        }
        // 3) Copy fallback
        let files = collect_regular_files(&source).map_err(|()| {
            AppError::InvalidArg("skill source tree is unsafe for copy fallback".into())
        })?;
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        write_skill_tree(target, &files)?;
        Ok((SkillLinkKind::None, true))
    }

    #[cfg(not(windows))]
    {
        if std::os::unix::fs::symlink(&source, target).is_ok() {
            return Ok((SkillLinkKind::Symlink, false));
        }
        let files = collect_regular_files(&source).map_err(|()| {
            AppError::InvalidArg("skill source tree is unsafe for copy fallback".into())
        })?;
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        write_skill_tree(target, &files)?;
        Ok((SkillLinkKind::None, true))
    }
}

#[cfg(windows)]
fn create_windows_junction_runtime(link: &Path, target: &Path) -> std::io::Result<()> {
    use std::process::Command;
    if let Some(parent) = link.parent() {
        fs::create_dir_all(parent)?;
    }
    let target_s = target.to_string_lossy().to_string();
    let link_arg = link.to_string_lossy().to_string();
    let status = Command::new("cmd")
        .args(["/C", "mklink", "/J", &link_arg, &target_s])
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "mklink /J failed",
        ))
    }
}

/// Per-skill exclusive lock under `<source_root>/.locks/skill-<id>.lock`.
fn acquire_skill_lock(source_root: &Path, skill_id: &str) -> Result<SkillScopedLock> {
    let lock_dir = source_root.join(".locks");
    fs::create_dir_all(&lock_dir)?;
    SkillScopedLock::acquire(&lock_dir, skill_id)
}

fn acquire_skill_root_lock(source_root: &Path) -> Result<SkillScopedLock> {
    let lock_dir = source_root.join(".locks");
    fs::create_dir_all(&lock_dir)?;
    SkillScopedLock::acquire(&lock_dir, "__root__")
}

/// Lightweight exclusive lock (same format as AgentWriteLock) for skill ids.
struct SkillScopedLock {
    path: PathBuf,
    file: Option<std::fs::File>,
    token: String,
}

impl SkillScopedLock {
    fn acquire(lock_dir: &Path, key: &str) -> Result<Self> {
        use std::fs::OpenOptions;
        use std::io::Write;
        use uuid::Uuid;

        fs::create_dir_all(lock_dir)?;
        // Sanitize key for filename.
        let safe: String = key
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        let path = lock_dir.join(format!("skill-{safe}.lock"));
        let token = Uuid::new_v4().to_string();
        let body = format!(
            "pid={}\ncreated_unix_ms={}\ntoken={token}\n",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0)
        );

        for _ in 0..3 {
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(mut file) => {
                    file.write_all(body.as_bytes())?;
                    let _ = file.sync_all();
                    return Ok(Self {
                        path,
                        file: Some(file),
                        token,
                    });
                }
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                    // Stale reclaim: if owner pid dead or file unreadable, remove.
                    if let Ok(raw) = fs::read_to_string(&path) {
                        let mut pid = None;
                        for line in raw.lines() {
                            if let Some(v) = line.strip_prefix("pid=") {
                                pid = v.trim().parse::<u32>().ok();
                            }
                        }
                        let dead = pid.is_some_and(|p| !process_is_alive_skill(p));
                        if dead {
                            let _ = fs::remove_file(&path);
                            continue;
                        }
                    } else {
                        let _ = fs::remove_file(&path);
                        continue;
                    }
                    return Err(AppError::message(
                        "skill.lock",
                        format!("another skill write is already running for '{key}'"),
                    ));
                }
                Err(e) => return Err(AppError::from(e)),
            }
        }
        Err(AppError::message(
            "skill.lock",
            format!("could not acquire skill lock for '{key}'"),
        ))
    }
}

impl Drop for SkillScopedLock {
    fn drop(&mut self) {
        drop(self.file.take());
        if let Ok(raw) = fs::read_to_string(&self.path) {
            if raw.contains(&self.token) {
                let _ = fs::remove_file(&self.path);
            }
        }
    }
}

fn process_is_alive_skill(pid: u32) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        std::process::Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/NH"])
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .map(|o| {
                let s = String::from_utf8_lossy(&o.stdout);
                s.contains(&pid.to_string())
            })
            .unwrap_or(true)
    }
    #[cfg(not(windows))]
    {
        std::path::Path::new(&format!("/proc/{pid}")).exists()
    }
}

#[cfg(test)]
mod tests;
