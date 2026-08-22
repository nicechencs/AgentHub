//! Ticket bind / unbind write API (connection-binding-model §4 / §6.3).
//!
//! Storage stays profile + `is_current` + ActiveBinding. This service is the
//! only write entry: it plans, dispatches the existing reshape sagas, and
//! derives the Agent's active [`TicketBinding`]. Codex `local_bridge` bind is
//! hosted by the desktop controller; core refuses to own the listener.

use std::path::PathBuf;

use crate::adapters::AdapterRegistry;
use crate::error::{AppError, Result};
use crate::models::{
    parse_ticket_id, AdapterApplyRequest, AdapterApplyResult, AdapterProfile, AdapterRoute,
    AdapterSourceKind, AgentId, TicketBinding, TicketBindingRoute, TicketBridgeRuntime,
    TicketPlanRequest, TicketUnbindRequest,
};
use crate::models::CODEX_SUBSCRIPTION_TO_CODEX_RULE_ID;
use crate::services::{
    AccountService, AdapterApplyService, ProviderService, TicketReadService,
};
use crate::storage::{AdapterProfileRepo, Database};

const HOSTED_BRIDGE_BIND: &str = "ticket.bind_hosted_bridge";
const PREVIOUS_CURRENT_ID: &str = "previousCurrentId";
const PREVIOUS_BACKUP_ID: &str = "previousBackupId";

/// Binds a ticket to an Agent and unbinds the generated projection.
pub struct TicketBindService {
    tickets: TicketReadService,
    apply: AdapterApplyService,
    profiles: AdapterProfileRepo,
    providers: ProviderService,
    accounts: AccountService,
}

impl TicketBindService {
    pub fn new(db: Database, registry: AdapterRegistry, backups_root: PathBuf) -> Self {
        Self::from_parts(
            TicketReadService::new(db.clone()),
            AdapterApplyService::new(db.clone(), registry.clone(), backups_root.clone()),
            AdapterProfileRepo::new(db.clone()),
            ProviderService::with_live(db.clone(), registry.clone(), backups_root.clone()),
            AccountService::with_live(db, registry, backups_root),
        )
    }

    /// Assemble from hub-owned parts so [`crate::AgentHub::open`] shares one
    /// [`ProviderService`] and one [`AccountService`] instead of constructing
    /// a second `with_live` for ticket bind.
    pub fn from_parts(
        tickets: TicketReadService,
        apply: AdapterApplyService,
        profiles: AdapterProfileRepo,
        providers: ProviderService,
        accounts: AccountService,
    ) -> Self {
        Self {
            tickets,
            apply,
            profiles,
            providers,
            accounts,
        }
    }

    /// Bind `ticketId` to `targetAgentId`. Returns that Agent's active binding.
    ///
    /// Account sources go through apply as-is; they are not copied into a
    /// Provider ticket first. Projection tickets are rejected with the same
    /// [`crate::models::PROJECTION_NOT_A_TICKET`] reason as `plan`.
    pub fn bind(&self, request: &TicketPlanRequest) -> Result<TicketBinding> {
        let (source_kind, source_id) = self.tickets.parse_bindable_ticket(&request.ticket_id)?;
        let plan = self.tickets.plan(request)?;
        if !plan.can_apply {
            return Err(AppError::Unsupported(plan.reason));
        }
        if plan.analysis.route == AdapterRoute::LocalBridge {
            return Err(AppError::message(
                HOSTED_BRIDGE_BIND,
                "local_bridge bind must run in the desktop host saga",
            ));
        }
        if is_codex_official_self_bind(
            source_kind,
            request.target_agent_id,
            plan.analysis.rule_id.as_deref(),
        ) {
            self.accounts.switch(&source_id, AgentId::Codex)?;
            return Ok(TicketBinding {
                ticket_id: request.ticket_id.clone(),
                agent_id: AgentId::Codex,
                route: TicketBindingRoute::Native,
                active: true,
                profile_id: None,
                bridge: None,
            });
        }
        let result = self.apply.apply(&AdapterApplyRequest {
            source_kind,
            source_id,
            target_agent_id: request.target_agent_id,
        })?;
        Ok(ticket_binding_from_apply(&request.ticket_id, &result))
    }

    /// Stop is the desktop host's job for `route=bridge`. Core then restores
    /// the Agent's previous live (when the generated row is current) and
    /// deletes the projection + profile. The source ticket remains.
    pub fn unbind(&self, request: &TicketUnbindRequest) -> Result<()> {
        let (source_kind, source_id) =
            parse_ticket_id(&request.ticket_id).map_err(AppError::InvalidArg)?;
        let profile = self
            .find_profile(source_kind, &source_id, request.agent_id)?
            .ok_or_else(|| {
                AppError::NotFound(format!(
                    "binding not found: {} → {}",
                    request.ticket_id,
                    request.agent_id.as_str()
                ))
            })?;
        if matches!(
            (profile.target_agent_id, profile.route),
            (AgentId::Claude, AdapterRoute::NativeEndpoint)
                | (AgentId::Pi, AdapterRoute::ConfigSync)
        ) {
            return self.apply.remove(&profile.id);
        }
        self.unbind_generated_profile(&profile)
    }

    fn find_profile(
        &self,
        source_kind: AdapterSourceKind,
        source_id: &str,
        agent_id: AgentId,
    ) -> Result<Option<AdapterProfile>> {
        let mut profiles = self
            .apply
            .list(Some(source_kind), Some(source_id), Some(agent_id))?;
        Ok(profiles.pop())
    }

    fn unbind_generated_profile(&self, profile: &AdapterProfile) -> Result<()> {
        let saga_guard = self.providers.begin_live_saga(profile.target_agent_id)?;
        if let Some(provider_id) = profile.generated_provider_id.as_deref() {
            let provider = match self
                .providers
                .get(provider_id, Some(profile.target_agent_id))
            {
                Ok(provider) => Some(provider),
                Err(AppError::NotFound(_)) => None,
                Err(error) => return Err(error),
            };
            if let Some(provider) = provider {
                if provider.is_current {
                    restore_previous_binding(
                        &self.providers,
                        &saga_guard,
                        &provider,
                        profile.target_agent_id,
                    )?;
                }
                self.providers.delete_with_guard(
                    &saga_guard,
                    provider_id,
                    profile.target_agent_id,
                )?;
            }
        }
        self.profiles.delete(&profile.id)
    }
}

/// Map a successful apply/host-saga result onto the Agent's active binding.
pub fn ticket_binding_from_apply(ticket_id: &str, result: &AdapterApplyResult) -> TicketBinding {
    let route = match result.profile.route {
        AdapterRoute::ConfigSync | AdapterRoute::NativeEndpoint => TicketBindingRoute::Reshape,
        AdapterRoute::LocalBridge => TicketBindingRoute::Bridge,
        AdapterRoute::Unsupported => TicketBindingRoute::Reshape,
    };
    let bridge = if route == TicketBindingRoute::Bridge {
        Some(TicketBridgeRuntime {
            port: result.profile.local_port,
            running: false,
        })
    } else {
        None
    };
    TicketBinding {
        ticket_id: ticket_id.to_owned(),
        agent_id: result.profile.target_agent_id,
        route,
        active: result.provider.is_current,
        profile_id: Some(result.profile.id.clone()),
        bridge,
    }
}

fn is_codex_official_self_bind(
    source_kind: AdapterSourceKind,
    target: AgentId,
    rule_id: Option<&str>,
) -> bool {
    source_kind == AdapterSourceKind::Account
        && target == AgentId::Codex
        && rule_id == Some(CODEX_SUBSCRIPTION_TO_CODEX_RULE_ID)
}

fn restore_previous_binding(
    providers: &ProviderService,
    saga_guard: &crate::services::ProviderLiveSagaGuard<'_>,
    generated: &crate::models::Provider,
    target_agent: AgentId,
) -> Result<()> {
    let previous_id = generated
        .meta
        .get(PREVIOUS_CURRENT_ID)
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|id| !id.is_empty() && *id != generated.id);
    if let Some(previous_id) = previous_id {
        match providers.get(previous_id, Some(target_agent)) {
            Ok(previous) => {
                if target_agent == AgentId::Codex
                    && crate::integrations::agents::codex::leftover::provider_is_bridge_leftover(
                        &previous,
                    )
                {
                    crate::integrations::agents::codex::leftover::strip_live_bridge_leftovers()?;
                    return Ok(());
                }
                providers.switch_with_guard(saga_guard, previous_id, target_agent)?;
                if target_agent == AgentId::Codex {
                    crate::integrations::agents::codex::leftover::strip_live_bridge_leftovers()?;
                }
                return Ok(());
            }
            Err(AppError::NotFound(_)) => {}
            Err(error) => return Err(error),
        }
    }
    if let Some(backup_id) = generated
        .meta
        .get(PREVIOUS_BACKUP_ID)
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|id| !id.is_empty())
    {
        providers.restore_named_backup_or_clean_codex(saga_guard, backup_id, target_agent)?;
    } else if target_agent == AgentId::Codex {
        crate::integrations::agents::codex::leftover::strip_live_bridge_leftovers()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
