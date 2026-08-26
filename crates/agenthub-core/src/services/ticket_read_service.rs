//! Ticket / Binding wallet aggregation (connection-binding-model §6 steps 1–2).
//!
//! Builds a wallet from accounts + providers + adapter profiles. Prefers
//! persisted `extra.surface` / `meta.surface`. A missing key is classified for
//! display only; list_wallet does not write the classification back. An
//! unrecognized value displays as `unknown`. `plan` rejects generated
//! projection and leftover 本机路由 providers.

use std::collections::{BTreeMap, HashMap, HashSet};

use crate::error::{AppError, Result};
use crate::integrations::agents::codex::leftover;
use crate::models::{
    parse_ticket_id, ticket_id, Account, AccountKind, AdapterApplyPlan,
    AdapterProfile, AdapterProfileStatus, AdapterRoute, AdapterRouteRequest, AdapterSourceKind,
    AgentId, PersistedTicketSurface, Provider, Ticket, TicketBinding, TicketBindingRoute,
    TicketBridgeRuntime, TicketCredentialClass, TicketPlanRequest, TicketSurface,
    TicketSurfaceGroup, TicketSurfaceMember, TicketWallet, PROJECTION_NOT_A_TICKET,
};
use crate::services::adapter_projection::classify_account_live;
use crate::services::AdapterRouteService;
use crate::storage::{AccountRepo, AdapterProfileRepo, Database, ProviderRepo};

/// Aggregates Ticket / Binding read models and thin `plan(ticket, agent)` wrapping.
#[derive(Clone)]
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

    /// List all true tickets and derived bindings. Generated projection and
    /// leftover 本机路由 providers are not tickets.
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
            if classify_account_live(
                account.agent_id,
                account.kind,
                &account.credentials,
                &profiles,
                &providers,
                false,
            )
            .is_projection()
            {
                continue;
            }
            tickets.push(self.ticket_from_account(account)?);
        }
        for provider in &providers {
            if generated_provider_ids.contains(&provider.id)
                || provider
                    .meta
                    .get("generatedBy")
                    .and_then(|value| value.as_str())
                    == Some("adapter")
                || leftover::provider_is_bridge_leftover(provider)
            {
                continue;
            }
            tickets.push(self.ticket_from_provider(provider)?);
        }
        tickets.sort_by(|a, b| a.id.cmp(&b.id));

        let ticket_ids: HashSet<String> = tickets.iter().map(|t| t.id.clone()).collect();
        let bindings = derive_bindings(&accounts, &providers, &profiles, &ticket_ids);
        let surface_groups = group_ticket_surface_members(&tickets);

        Ok(TicketWallet {
            tickets,
            bindings,
            surface_groups,
        })
    }

    /// Resolve `ticketId` and delegate to [`AdapterRouteService::plan`].
    ///
    /// Generated projection providers are not tickets: refuse before routing.
    pub fn plan(&self, request: &TicketPlanRequest) -> Result<AdapterApplyPlan> {
        let (source_kind, source_id) = self.parse_bindable_ticket(&request.ticket_id)?;
        self.routes.plan(&AdapterRouteRequest {
            source_kind,
            source_id,
            target_agent_id: request.target_agent_id,
        })
    }

    /// Parse `account:<id>` / `provider:<id>` and reject generated / leftover projections.
    pub fn parse_bindable_ticket(&self, ticket_id: &str) -> Result<(AdapterSourceKind, String)> {
        let (source_kind, source_id) = parse_ticket_id(ticket_id).map_err(AppError::InvalidArg)?;
        if source_kind == AdapterSourceKind::Provider && self.is_projection_provider(&source_id)? {
            return Err(AppError::InvalidArg(format!(
                "{PROJECTION_NOT_A_TICKET}: {ticket_id}"
            )));
        }
        if source_kind == AdapterSourceKind::Account && self.is_projection_account(&source_id)? {
            return Err(AppError::InvalidArg(format!(
                "{PROJECTION_NOT_A_TICKET}: {ticket_id}"
            )));
        }
        Ok((source_kind, source_id))
    }

    fn is_projection_provider(&self, provider_id: &str) -> Result<bool> {
        let profiles = self.profiles.list_filtered(&Default::default())?;
        if profiles
            .iter()
            .any(|profile| profile.generated_provider_id.as_deref() == Some(provider_id))
        {
            return Ok(true);
        }
        let Some(provider) = self.providers.get_by_id(provider_id)? else {
            return Ok(false);
        };
        if provider
            .meta
            .get("generatedBy")
            .and_then(|value| value.as_str())
            == Some("adapter")
        {
            return Ok(true);
        }
        Ok(leftover::provider_is_bridge_leftover(&provider))
    }

    fn is_projection_account(&self, account_id: &str) -> Result<bool> {
        let Some(account) = self.accounts.get_by_id(account_id)? else {
            return Ok(false);
        };
        let profiles = self.profiles.list_filtered(&Default::default())?;
        let providers = self.providers.list(Some(account.agent_id))?;
        Ok(classify_account_live(
            account.agent_id,
            account.kind,
            &account.credentials,
            &profiles,
            &providers,
            false,
        )
        .is_projection())
    }

    fn ticket_from_account(&self, account: &Account) -> Result<Ticket> {
        let surface = self.resolve_account_surface(account)?;
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
        let surface = self.resolve_provider_surface(provider)?;
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

    fn resolve_account_surface(&self, account: &Account) -> Result<TicketSurface> {
        match TicketSurface::from_persisted_json(&account.extra) {
            PersistedTicketSurface::Known(TicketSurface::Unknown)
            | PersistedTicketSurface::Missing => {}
            PersistedTicketSurface::Known(surface) => return Ok(surface),
            PersistedTicketSurface::Unrecognized => return Ok(TicketSurface::Unknown),
        }
        let product = self
            .routes
            .classify_source_product(AdapterSourceKind::Account, &account.id)?;
        Ok(TicketSurface::from_product(product))
    }

    fn resolve_provider_surface(&self, provider: &Provider) -> Result<TicketSurface> {
        let persisted = TicketSurface::from_persisted_json(&provider.meta);
        match persisted {
            PersistedTicketSurface::Known(TicketSurface::Unknown) => {
                return Ok(TicketSurface::Unknown)
            }
            PersistedTicketSurface::Missing => {}
            PersistedTicketSurface::Known(surface) => return Ok(surface),
            PersistedTicketSurface::Unrecognized => return Ok(TicketSurface::Unknown),
        }
        let product = self
            .routes
            .classify_source_product(AdapterSourceKind::Provider, &provider.id)?;
        let surface = TicketSurface::from_product(product);
        if surface == TicketSurface::OpenaiApi
            && !crate::services::adapter_route_constants::provider_has_official_openai_api_evidence(
                provider,
            )
        {
            return Ok(TicketSurface::Unknown);
        }
        Ok(surface)
    }
}

/// Group known-surface tickets by `(surface, credential_class)` for §5.5.
///
/// Projection providers never reach `tickets`, so they cannot enter a group.
/// `unknown` surface and `unknown` credential class stay on the wallet as
/// tickets but are not pooled. Account and Provider rows mix when the key
/// matches. Member order is `ticket_id` so AccountPicker can consume this
/// list as a fixed rotation without a new table.
pub(crate) fn group_ticket_surface_members(tickets: &[Ticket]) -> Vec<TicketSurfaceGroup> {
    let mut buckets: BTreeMap<(&str, &str), Vec<&Ticket>> = BTreeMap::new();
    for ticket in tickets {
        if ticket.surface == TicketSurface::Unknown {
            continue;
        }
        if ticket.credential_class == TicketCredentialClass::Unknown {
            continue;
        }
        buckets
            .entry((ticket.surface.as_str(), ticket.credential_class.as_str()))
            .or_default()
            .push(ticket);
    }
    buckets
        .into_iter()
        .filter_map(|(_, mut members)| {
            members.sort_by(|left, right| left.id.cmp(&right.id));
            let first = members.first()?;
            Some(TicketSurfaceGroup {
                surface: first.surface,
                credential_class: first.credential_class,
                members: members
                    .into_iter()
                    .map(TicketSurfaceMember::from_ticket)
                    .collect(),
            })
        })
        .collect()
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
            // Leftover / orphan projection is current: do not keep oauth as 正用于.
            if leftover::provider_is_bridge_leftover(provider)
                || provider
                    .meta
                    .get("generatedBy")
                    .and_then(|value| value.as_str())
                    == Some("adapter")
            {
                active_by_agent.remove(&provider.agent_id);
            }
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
    // Incomplete first-time apply / failed-without-port must not appear as
    // Connections「正用于」. Active current projections still pass `active`.
    if !active
        && (profile.status == AdapterProfileStatus::Applying
            || (route == TicketBindingRoute::Bridge && profile.local_port.is_none()))
    {
        return None;
    }
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
