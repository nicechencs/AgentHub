//! Read-only Ticket / Binding wallet aggregation (connection-binding-model §6 step 1).
//!
//! Builds a wallet from accounts + providers + adapter profiles. Never writes.

use std::collections::{HashMap, HashSet};

use crate::error::{AppError, Result};
use crate::models::{
    parse_ticket_id, ticket_id, Account, AccountKind, AdapterApplyPlan, AdapterProfile,
    AdapterRoute, AdapterRouteRequest, AdapterSourceKind, AgentId, Provider, Ticket,
    TicketBinding, TicketBindingRoute, TicketBridgeRuntime, TicketCredentialClass,
    TicketPlanRequest, TicketSurface, TicketWallet,
};
use crate::services::AdapterRouteService;
use crate::storage::{AccountRepo, AdapterProfileRepo, Database, ProviderRepo};

/// Aggregates Ticket / Binding read models and thin `plan(ticket, agent)` wrapping.
pub struct TicketReadService {
    accounts: AccountRepo,
    providers: ProviderRepo,
    profiles: AdapterProfileRepo,
    routes: AdapterRouteService,
}

impl TicketReadService {
    pub fn new(db: Database) -> Self {
        Self {
            accounts: AccountRepo::new(db.clone()),
            providers: ProviderRepo::new(db.clone()),
            profiles: AdapterProfileRepo::new(db.clone()),
            routes: AdapterRouteService::new(db),
        }
    }

    /// List all true tickets and derived bindings. Generated projection providers
    /// are excluded from the ticket list.
    pub fn list_wallet(&self) -> Result<TicketWallet> {
        let accounts = self.accounts.list(None)?;
        let providers = self.providers.list(None)?;
        let profiles = self.profiles.list_filtered(&Default::default())?;

        let generated_provider_ids: HashSet<String> = profiles
            .iter()
            .filter_map(|profile| profile.generated_provider_id.clone())
            .collect();

        let mut tickets = Vec::with_capacity(accounts.len() + providers.len());
        for account in &accounts {
            tickets.push(self.ticket_from_account(account)?);
        }
        for provider in &providers {
            if generated_provider_ids.contains(&provider.id) {
                continue;
            }
            tickets.push(self.ticket_from_provider(provider)?);
        }
        tickets.sort_by(|a, b| a.id.cmp(&b.id));

        let ticket_ids: HashSet<String> = tickets.iter().map(|t| t.id.clone()).collect();
        let bindings = derive_bindings(&accounts, &providers, &profiles, &ticket_ids);

        Ok(TicketWallet { tickets, bindings })
    }

    /// Resolve `ticketId` and delegate to [`AdapterRouteService::plan`].
    pub fn plan(&self, request: &TicketPlanRequest) -> Result<AdapterApplyPlan> {
        let (source_kind, source_id) = parse_ticket_id(&request.ticket_id)
            .map_err(AppError::InvalidArg)?;
        self.routes.plan(&AdapterRouteRequest {
            source_kind,
            source_id,
            target_agent_id: request.target_agent_id,
        })
    }

    fn ticket_from_account(&self, account: &Account) -> Result<Ticket> {
        let product = self
            .routes
            .classify_source_product(AdapterSourceKind::Account, &account.id)?;
        let surface = TicketSurface::from_product(product);
        Ok(Ticket {
            id: ticket_id(AdapterSourceKind::Account, &account.id),
            source_kind: AdapterSourceKind::Account,
            source_id: account.id.clone(),
            agent_id: account.agent_id,
            label: account.label.clone(),
            surface,
            credential_class: match account.kind {
                AccountKind::Oauth => TicketCredentialClass::Oauth,
                AccountKind::ApiKey => TicketCredentialClass::ApiKey,
            },
            speaks: surface.speaks().to_vec(),
            imported_from: Some(account.agent_id),
        })
    }

    fn ticket_from_provider(&self, provider: &Provider) -> Result<Ticket> {
        let product = self
            .routes
            .classify_source_product(AdapterSourceKind::Provider, &provider.id)?;
        let surface = TicketSurface::from_product(product);
        Ok(Ticket {
            id: ticket_id(AdapterSourceKind::Provider, &provider.id),
            source_kind: AdapterSourceKind::Provider,
            source_id: provider.id.clone(),
            agent_id: provider.agent_id,
            label: provider.name.clone(),
            surface,
            credential_class: TicketCredentialClass::ApiKey,
            speaks: surface.speaks().to_vec(),
            imported_from: Some(provider.agent_id),
        })
    }
}

fn derive_bindings(
    accounts: &[Account],
    providers: &[Provider],
    profiles: &[AdapterProfile],
    ticket_ids: &HashSet<String>,
) -> Vec<TicketBinding> {
    let provider_by_id: HashMap<&str, &Provider> =
        providers.iter().map(|p| (p.id.as_str(), p)).collect();
    let profile_by_generated: HashMap<&str, &AdapterProfile> = profiles
        .iter()
        .filter_map(|p| {
            p.generated_provider_id
                .as_deref()
                .map(|generated| (generated, p))
        })
        .collect();

    // agent → winning active candidate (provider current beats account current).
    let mut active_by_agent: HashMap<AgentId, TicketBinding> = HashMap::new();
    let mut active_profile_ids: HashSet<String> = HashSet::new();

    // (a) current accounts → native active candidates (loses to provider current).
    for account in accounts.iter().filter(|a| a.is_current) {
        let ticket = ticket_id(AdapterSourceKind::Account, &account.id);
        if !ticket_ids.contains(&ticket) {
            continue;
        }
        active_by_agent
            .entry(account.agent_id)
            .or_insert_with(|| TicketBinding {
                ticket_id: ticket,
                agent_id: account.agent_id,
                route: TicketBindingRoute::Native,
                active: true,
                profile_id: None,
                bridge: None,
            });
    }

    // (b) current providers — provider wins over any account candidate.
    for provider in providers.iter().filter(|p| p.is_current) {
        if let Some(profile) = profile_by_generated.get(provider.id.as_str()) {
            if let Some(binding) = binding_from_profile(profile, true, ticket_ids) {
                if let Some(profile_id) = binding.profile_id.clone() {
                    active_profile_ids.insert(profile_id);
                }
                active_by_agent.insert(binding.agent_id, binding);
            }
            continue;
        }
        let ticket = ticket_id(AdapterSourceKind::Provider, &provider.id);
        if !ticket_ids.contains(&ticket) {
            continue;
        }
        active_by_agent.insert(
            provider.agent_id,
            TicketBinding {
                ticket_id: ticket,
                agent_id: provider.agent_id,
                route: TicketBindingRoute::Native,
                active: true,
                profile_id: None,
                bridge: None,
            },
        );
    }

    let mut bindings: Vec<TicketBinding> = active_by_agent.into_values().collect();

    // (c) remaining profiles that are not the active current projection.
    for profile in profiles {
        if active_profile_ids.contains(&profile.id) {
            continue;
        }
        // Skip when the generated provider is current but we already emitted it,
        // or when source is missing (binding_from_profile returns None).
        if let Some(generated) = profile.generated_provider_id.as_deref() {
            if provider_by_id
                .get(generated)
                .is_some_and(|provider| provider.is_current)
            {
                // Current projection already handled in (b); if it was skipped
                // (missing source), do not synthesize a ghost inactive binding.
                continue;
            }
        }
        if let Some(binding) = binding_from_profile(profile, false, ticket_ids) {
            bindings.push(binding);
        }
    }

    bindings.sort_by(|a, b| {
        a.agent_id
            .as_str()
            .cmp(b.agent_id.as_str())
            .then_with(|| a.ticket_id.cmp(&b.ticket_id))
            .then_with(|| a.profile_id.cmp(&b.profile_id))
            .then_with(|| a.active.cmp(&b.active).reverse())
    });
    bindings
}

fn binding_from_profile(
    profile: &AdapterProfile,
    active: bool,
    ticket_ids: &HashSet<String>,
) -> Option<TicketBinding> {
    let route = match profile.route {
        AdapterRoute::ConfigSync | AdapterRoute::NativeEndpoint => TicketBindingRoute::Reshape,
        AdapterRoute::LocalBridge => TicketBindingRoute::Bridge,
        AdapterRoute::Unsupported => return None,
    };
    let ticket = ticket_id(profile.source_kind, &profile.source_id);
    // (f) source row gone → skip (ticket was never built).
    if !ticket_ids.contains(&ticket) {
        return None;
    }
    let bridge = if route == TicketBindingRoute::Bridge {
        Some(TicketBridgeRuntime {
            port: profile.local_port,
            // Core cannot observe the desktop listener; Tauri may overwrite.
            running: false,
        })
    } else {
        None
    };
    Some(TicketBinding {
        ticket_id: ticket,
        agent_id: profile.target_agent_id,
        route,
        active,
        profile_id: Some(profile.id.clone()),
        bridge,
    })
}

#[cfg(test)]
mod tests;
