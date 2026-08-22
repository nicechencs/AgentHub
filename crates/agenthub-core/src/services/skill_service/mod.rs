//! Skill source scan + per-agent projection matrix, with safe projection writes.
//!
//! Construction takes an explicit `source_root` and [`AdapterRegistry`]; it does
//! **not** scan the filesystem. Writes (`sync` / `disable`) project or remove a
//! single validated skill directory under an adapter's `skills_dir`.
//!
//! Pure YAML / hash / classify / lock helpers live in [`crate::platform::skills`].
//! This module is the orchestration façade (P2-6).

mod catalog;
mod install;
mod list;
mod sync;

#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::adapters::AdapterRegistry;
use crate::error::{AppError, Result};
use crate::logging::targets;
use crate::models::Skill;
use crate::platform::skills::{
    acquire_skill_root_lock, bootstrap_skill_assignments, recover_skill_commit_journal,
    SkillBootstrapReport, SkillTargetRegistry,
};
use crate::storage::Database;

/// Re-export for market + tests (`use super::*` / `skill_service::parse_skill_frontmatter`).
pub(crate) use crate::platform::skills::parse_skill_frontmatter;

// Test prelude: `tests.rs` relies on `use super::*` seeing the former monolith imports.
#[cfg(test)]
#[allow(unused_imports)]
use crate::models::{
    AgentId, Capability, InstalledSkill, SkillAction, SkillFailure, SkillLinkKind, SkillMapStatus,
    SkillMarkdownPreview, SkillProjectMode, SkillProjectResult, SkillProjection, SkillSourceRecord,
    SkillSyncReport, SkillSyncState,
};
#[cfg(test)]
#[allow(unused_imports)]
use crate::platform::skills::{
    collect_regular_files, create_projection_link, detect_link_kind, ensure_skill_md,
    is_link_or_reparse, materialize_projection, normalize_rel_path, paths_equal_lexical,
    remove_projection_link, resolve_link_path, skill_lock_load, skill_lock_upsert,
    validate_and_collect_source, validate_skill_id, validate_skills_root, write_skill_tree,
    PreparedSkillTree, SkillPackageService, SkillSourceService, TargetPresence,
};
#[cfg(test)]
#[allow(unused_imports)]
use crate::platform::AgentKey;
#[cfg(test)]
#[allow(unused_imports)]
use crate::storage::SkillRepo;
#[cfg(test)]
#[allow(unused_imports)]
use std::collections::{BTreeMap, HashMap, HashSet};
#[cfg(test)]
#[allow(unused_imports)]
use std::fs;
#[cfg(test)]
#[allow(unused_imports)]
use std::io::{self, Write};

/// Cached `list()` snapshot keyed by a cheap filesystem fingerprint.
pub(super) struct SkillListCache {
    fingerprint: u64,
    skills: Arc<Vec<Skill>>,
}

pub struct SkillService {
    pub(super) source_root: PathBuf,
    pub(super) registry: AdapterRegistry,
    pub(super) targets: SkillTargetRegistry,
    /// Process-local list matrix cache (invalidated on writes / FS fingerprint change).
    pub(super) list_cache: Mutex<Option<SkillListCache>>,
    /// Optional DB for assignment + reconcile (P12). When `None`, sync/disable
    /// stay filesystem-only (unit tests that only exercise FS).
    pub(super) db: Option<Database>,
}

pub(super) fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

pub(super) fn log_skill_write_result(
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
        let repo = crate::storage::SkillRepo::new(db.clone());
        bootstrap_skill_assignments(
            &self.source_root,
            &self.targets,
            &repo,
            &crate::platform::skills::chrono_now(),
        )
    }

    /// Recover an interrupted shared-skill package commit under the root lock.
    ///
    /// This intentionally does not import assignments or reconcile projections;
    /// startup must only restore the commit's live/lock/package state.  The
    /// normal bootstrap path may be called separately when assignment import is
    /// desired.
    pub fn recover_pending_commit(&self) -> Result<()> {
        let _root_lock = acquire_skill_root_lock(&self.source_root)?;
        let repo = self
            .db
            .as_ref()
            .map(|db| crate::storage::SkillRepo::new(db.clone()));
        recover_skill_commit_journal(&self.source_root, repo.as_ref()).map(|_| ())
    }

    /// Drop cached `list()` results (writes, external FS watcher).
    pub fn invalidate_list_cache(&self) {
        if let Ok(mut guard) = self.list_cache.lock() {
            *guard = None;
        }
    }
}
