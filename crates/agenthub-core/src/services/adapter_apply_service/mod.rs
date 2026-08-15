//! Write-side adapter apply.
//!
//! Split for maintainability only — public path stays
//! [`crate::services::AdapterApplyService`].

mod saga;
mod specs;

#[cfg(test)]
mod tests;

use std::path::PathBuf;

use crate::adapters::AdapterRegistry;
use crate::services::{
    AdapterRouteService, AdapterSecretResolver, ProviderLiveConfigSnapshot, ProviderService,
};
use crate::storage::{AdapterProfileRepo, Database};
use crate::models::{AdapterProfile, AgentId, Provider, ProviderInput};

/// Pre-switch snapshot used to reverse a successful live switch when profile
/// finalization (or the switch itself) fails. Deliberately private and
/// non-serializable: the live config may contain materialized credentials.
pub(super) struct ApplySnapshot {
    /// Generated provider row before create/update in this apply, if any.
    pub(super) generated_before: Option<Provider>,
    /// Target agent current provider before switch (may equal `generated_before`).
    pub(super) previous_current: Option<Provider>,
    pub(super) live_config: ProviderLiveConfigSnapshot,
    pub(super) created: bool,
}

pub(super) struct GeneratedApplySpec {
    pub(super) target_agent: AgentId,
    pub(super) provider_id: String,
    pub(super) proposed: AdapterProfile,
    pub(super) provider: ProviderInput,
}


/// Applies supported write-side routes and owns their generated profiles.
#[derive(Clone)]
pub struct AdapterApplyService {
    pub(super) routes: AdapterRouteService,
    pub(super) profiles: AdapterProfileRepo,
    pub(super) providers: ProviderService,
    pub(super) secrets: AdapterSecretResolver,
}

impl AdapterApplyService {
    pub fn new(db: Database, registry: AdapterRegistry, backups_root: PathBuf) -> Self {
        Self::from_parts(
            AdapterRouteService::new(db.clone()),
            AdapterProfileRepo::new(db.clone()),
            ProviderService::with_live(db.clone(), registry, backups_root),
            AdapterSecretResolver::new(db),
        )
    }

    /// Assemble from hub-owned parts so [`crate::AgentHub::open`] shares one
    /// [`ProviderService`] instead of constructing a second `with_live`.
    pub fn from_parts(
        routes: AdapterRouteService,
        profiles: AdapterProfileRepo,
        providers: ProviderService,
        secrets: AdapterSecretResolver,
    ) -> Self {
        Self {
            routes,
            profiles,
            providers,
            secrets,
        }
    }
}
