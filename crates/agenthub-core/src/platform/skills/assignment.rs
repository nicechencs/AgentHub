//! Desired skill assignment service (P12).
//!
//! Updates package rows and desired_enabled before reconcile. Never claims
//! applied state — that is owned by [`super::SkillReconciler`].

use crate::error::Result;
use crate::models::{AgentId, SkillSourceRecord};
use crate::platform::AgentKey;
use crate::storage::{SkillAssignmentRow, SkillPackageRow, SkillRepo};

/// Owns package upsert + desired assignment flips.
#[derive(Clone)]
pub struct SkillAssignmentService {
    repo: SkillRepo,
}

impl SkillAssignmentService {
    pub fn new(repo: SkillRepo) -> Self {
        Self { repo }
    }

    pub fn repo(&self) -> &SkillRepo {
        &self.repo
    }

    /// Ensure a `skill_packages` row exists from an optional lock record.
    ///
    /// When `record` is `None`, inserts a minimal `unknown` package if missing
    /// (does not overwrite an existing richer row).
    pub fn ensure_package(
        &self,
        skill_id: &str,
        record: Option<&SkillSourceRecord>,
        now: &str,
    ) -> Result<SkillPackageRow> {
        if let Some(existing) = self.repo.get_package(skill_id)? {
            if let Some(rec) = record {
                let revision = package_revision(rec);
                let manifest = serde_json::to_string(rec).unwrap_or_else(|_| "{}".into());
                if existing.source_kind != rec.kind
                    || existing.locator != rec.locator
                    || existing.revision != revision
                {
                    return self.repo.upsert_package(&SkillPackageRow {
                        id: skill_id.to_string(),
                        source_kind: rec.kind.clone(),
                        locator: rec.locator.clone(),
                        revision,
                        manifest_json: manifest,
                        created_at: existing.created_at,
                        updated_at: now.to_string(),
                    });
                }
            }
            return Ok(existing);
        }

        let row = match record {
            Some(rec) => SkillPackageRow {
                id: skill_id.to_string(),
                source_kind: rec.kind.clone(),
                locator: rec.locator.clone(),
                revision: package_revision(rec),
                manifest_json: serde_json::to_string(rec).unwrap_or_else(|_| "{}".into()),
                created_at: now.to_string(),
                updated_at: now.to_string(),
            },
            None => SkillPackageRow {
                id: skill_id.to_string(),
                source_kind: "unknown".into(),
                locator: String::new(),
                revision: "1".into(),
                manifest_json: "{}".into(),
                created_at: now.to_string(),
                updated_at: now.to_string(),
            },
        };
        self.repo.upsert_package(&row)
    }

    pub fn get_assignment(
        &self,
        skill_id: &str,
        agent_key: &AgentKey,
    ) -> Result<Option<SkillAssignmentRow>> {
        self.repo.get_assignment(skill_id, agent_key.as_str())
    }

    /// Compatibility façade for callers that still use the built-in enum.
    pub fn get_assignment_for_agent(
        &self,
        skill_id: &str,
        agent: AgentId,
    ) -> Result<Option<SkillAssignmentRow>> {
        let agent_key = AgentKey::from_agent_id(agent);
        self.get_assignment(skill_id, &agent_key)
    }

    /// Set desired_enabled (and optional projection mode). Does not project FS.
    ///
    /// Package row must already exist (call [`ensure_package`] first).
    pub fn set_desired_enabled(
        &self,
        skill_id: &str,
        agent_key: &AgentKey,
        desired_enabled: bool,
        projection_mode: Option<&str>,
        now: &str,
    ) -> Result<SkillAssignmentRow> {
        let existing = self.repo.get_assignment(skill_id, agent_key.as_str())?;
        // None keeps the existing mode so disable/sync do not reset link → copy.
        // A new row with no explicit mode still defaults to copy.
        let mode = projection_mode
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToString::to_string)
            .or_else(|| existing.as_ref().map(|prev| prev.projection_mode.clone()))
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "copy".into());
        let row = match existing {
            Some(prev) => SkillAssignmentRow {
                skill_package_id: skill_id.to_string(),
                agent_key: agent_key.to_string(),
                desired_enabled,
                projection_mode: mode,
                // Keep applied_revision until reconcile succeeds/fails with a new value.
                applied_revision: prev.applied_revision,
                // Reset observed to pending so reconciler is expected to run.
                observed_status: "pending".into(),
                last_error: None,
                updated_at: now.to_string(),
            },
            None => SkillAssignmentRow {
                skill_package_id: skill_id.to_string(),
                agent_key: agent_key.to_string(),
                desired_enabled,
                projection_mode: mode,
                applied_revision: None,
                observed_status: "pending".into(),
                last_error: None,
                updated_at: now.to_string(),
            },
        };
        self.repo.upsert_assignment(&row)
    }

    /// Compatibility façade for callers that still use the built-in enum.
    pub fn set_desired_enabled_for_agent(
        &self,
        skill_id: &str,
        agent: AgentId,
        desired_enabled: bool,
        projection_mode: Option<&str>,
        now: &str,
    ) -> Result<SkillAssignmentRow> {
        let agent_key = AgentKey::from_agent_id(agent);
        self.set_desired_enabled(skill_id, &agent_key, desired_enabled, projection_mode, now)
    }
}

/// Stable revision string from a lock record.
pub fn package_revision(record: &SkillSourceRecord) -> String {
    if let Some(v) = record
        .version
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        return v.to_string();
    }
    if let Some(u) = record
        .updated_at
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        return u.to_string();
    }
    if !record.installed_at.trim().is_empty() {
        return record.installed_at.clone();
    }
    "1".into()
}
