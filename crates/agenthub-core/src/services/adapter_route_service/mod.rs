//! Read-only compatibility analysis for explicitly tagged connection records.
//!
//! Split for maintainability only — public path stays
//! [`crate::services::AdapterRouteService`].

mod actions;
mod classify;
mod plan;

#[cfg(test)]
mod tests;

use crate::error::{AppError, Result};
use crate::models::{
    adapter_maturity_from_decision, decide_adapter_capability, AccountKind, AdapterAction,
    AdapterApplyPlan, AdapterCapabilityDecision, AdapterCredentialClass, AdapterEvidence,
    AdapterPlanChange, AdapterReusePath, AdapterRoute, AdapterRouteAnalysis, AdapterRouteRequest,
    AdapterServiceImpact, AdapterSourceKind, AdapterSourceProduct, AdapterSupport, AgentId,
};
use crate::services::adapter_route_constants::*;
use crate::storage::{AccountRepo, Database, ProviderRepo};
use serde_json::Value;

use actions::RouteSourceLabel;

pub(super) use actions::*;

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

