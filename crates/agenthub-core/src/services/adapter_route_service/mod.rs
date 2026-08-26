//! Read-only compatibility analysis for explicitly tagged connection records.
//!
//! Split for maintainability only — public path stays
//! [`crate::services::AdapterRouteService`].

mod actions;
mod classify;
mod plan;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod classify_contract;

// The following imports/re-exports are kept for the child `tests` module.
#[allow(unused_imports)]
use crate::error::{AppError, Result};
use crate::models::{AdapterCapabilityDecision, AdapterCredentialClass};
use crate::storage::{AccountRepo, Database, ProviderRepo};
#[allow(unused_imports)]
pub(super) use actions::*;

use actions::RouteSourceLabel;

#[cfg(test)]
pub(super) use actions::bind_implementation_open;

/// Determines whether one saved connection has a supported preview route to an agent.
///
/// This service deliberately uses only explicit persisted fields. It does not inspect,
/// infer, return, or copy credentials and it never writes a config or starts a bridge.
#[derive(Clone)]
pub struct AdapterRouteService {
    pub(super) accounts: AccountRepo,
    pub(super) providers: ProviderRepo,
}

impl AdapterRouteService {
    pub fn new(db: Database) -> Self {
        Self {
            accounts: AccountRepo::new(db.clone()),
            providers: ProviderRepo::new(db),
        }
    }
}

pub(super) struct ClassifiedRoute {
    pub(super) source: RouteSourceLabel,
    pub(super) credential: AdapterCredentialClass,
    pub(super) decision: AdapterCapabilityDecision,
}
