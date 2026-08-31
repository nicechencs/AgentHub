//! RoutePool control plane. `feature.route_pool_v2` ships on; explicit off
//! disables. `feature.route_index_v2` attaches the shared resolver on enrolled
//! start (also default on). Mixed-provider and pair-adapter flags stay
//! fail-closed.

use std::collections::{HashMap, HashSet};

use chrono::Utc;
use uuid::Uuid;

use super::adapter_projection::{classify_account_live, generated_provider_is_adapter_owned};
use super::AdapterRouteService;
use crate::bridge::BridgeRuntimeHost;
use crate::error::{AppError, Result};
use crate::integrations::agents::codex::leftover;
use crate::models::{
    authorization_is_route_pool_home, choose_default_pool_id, enroll_native_plan_is_open,
    feature_flag_enabled, generate_hub_token, list_local_bridge_models, product_flag_enabled,
    set_authorization_route_pool_home, AdapterApplyPlan, AdapterProfile, AdapterProfileFilter,
    AdapterRoute, AdapterRouteRequest, AdapterSourceKind, AgentId, DefaultRoutePoolList,
    DefaultRoutePoolOverview, ModelRouteRule, RouteDownstreamDialect, RouteDownstreamSurface,
    RouteMember, RouteMemberOverview, RoutePool, RouteSchedulePolicy,
    SyncConnectionAuthorizationsResult, FEATURE_CODEX_INGRESS_GROK_UPSTREAM,
    FEATURE_GROK_INGRESS_CODEX_UPSTREAM, FEATURE_MIXED_PROVIDER_POOL, FEATURE_ROUTE_INDEX_V2,
    FEATURE_ROUTE_POOL_V2,
};
use crate::storage::{
    binding_get_conn, AccountRepo, AdapterProfileRepo, Database, ProviderRepo, RoutePoolRepo,
};

#[cfg(test)]
mod tests;

pub struct RoutePoolService {
    db: Database,
    pools: RoutePoolRepo,
    profiles: AdapterProfileRepo,
    accounts: AccountRepo,
    providers: ProviderRepo,
}

impl RoutePoolService {
    pub fn new(db: Database) -> Self {
        Self {
            pools: RoutePoolRepo::new(db.clone()),
            profiles: AdapterProfileRepo::new(db.clone()),
            accounts: AccountRepo::new(db.clone()),
            providers: ProviderRepo::new(db.clone()),
            db,
        }
    }

    pub fn enabled(&self) -> Result<bool> {
        Ok(product_flag_enabled(
            self.db.get_setting(FEATURE_ROUTE_POOL_V2)?.as_deref(),
        ))
    }

    /// `feature.route_index_v2` (and persistence) must both be on. One flag
    /// still controls dispatch and `/models` together.
    pub fn index_enabled(&self) -> bool {
        self.enabled().unwrap_or(false)
            && product_flag_enabled(
                self.db
                    .get_setting(FEATURE_ROUTE_INDEX_V2)
                    .ok()
                    .flatten()
                    .as_deref(),
            )
    }

    /// Independent Codex↔Grok pair-adapter flags. Fail-closed; does not
    /// require `route_pool_v2`.
    pub fn pair_adapter_flags(&self) -> (bool, bool) {
        (
            feature_flag_enabled(
                self.db
                    .get_setting(FEATURE_CODEX_INGRESS_GROK_UPSTREAM)
                    .ok()
                    .flatten()
                    .as_deref(),
            ),
            feature_flag_enabled(
                self.db
                    .get_setting(FEATURE_GROK_INGRESS_CODEX_UPSTREAM)
                    .ok()
                    .flatten()
                    .as_deref(),
            ),
        )
    }

    /// Mixed-provider resolve. Persistence writes stay on `route_pool_v2`;
    /// this flag additionally gates the index path and does not activate v1
    /// `switch_edge_for_model`.
    pub fn mixed_provider_enabled(&self) -> bool {
        self.index_enabled()
            && feature_flag_enabled(
                self.db
                    .get_setting(FEATURE_MIXED_PROVIDER_POOL)
                    .ok()
                    .flatten()
                    .as_deref(),
            )
    }

    pub fn get(&self, pool_id: &str) -> Result<Option<RoutePool>> {
        self.require_enabled()?;
        self.pools.get_pool(pool_id)
    }

    pub fn list(
        &self,
        target_agent_id: Option<AgentId>,
        surface: Option<RouteDownstreamSurface>,
    ) -> Result<Vec<RoutePool>> {
        self.require_enabled()?;
        self.pools.list_pools(target_agent_id, surface)
    }

    pub fn get_adapter_profile(&self, id: &str) -> Result<Option<AdapterProfile>> {
        self.profiles.get(id)
    }

    /// Default pools only. Explicit flag off returns `{ enabled: false, pools: [] }`
    /// so the Routes page can hide pool chrome without treating this as an error.
    pub fn list_default_overviews(&self) -> Result<DefaultRoutePoolList> {
        if !self.enabled()? {
            return Ok(DefaultRoutePoolList {
                enabled: false,
                pools: Vec::new(),
            });
        }
        let mut overviews = Vec::new();
        for pool in self.pools.list_pools(None, None)? {
            if !pool.is_default {
                continue;
            }
            overviews.push(self.overview_from_pool(&pool)?);
        }
        Ok(DefaultRoutePoolList {
            enabled: true,
            pools: overviews,
        })
    }

    pub fn overview_from_pool(&self, pool: &RoutePool) -> Result<DefaultRoutePoolOverview> {
        let members = self.pools.list_members(&pool.id)?;
        let listed_models = self.listed_models_for_pool(pool, &members);
        Ok(DefaultRoutePoolOverview {
            id: pool.id.clone(),
            target_agent_id: pool.target_agent_id,
            surface: pool.downstream_surface,
            dialect: pool.downstream_dialect,
            v2_enrolled: pool.v2_enrolled,
            gateway_port: pool.gateway_port,
            members: members
                .into_iter()
                .map(|member| RouteMemberOverview {
                    source_kind: member.source_kind,
                    source_id: member.source_id,
                    enabled: member.enabled,
                    availability: Some(if member.enabled {
                        crate::models::MemberAvailability::Ready
                    } else {
                        crate::models::MemberAvailability::Disabled
                    }),
                })
                .collect(),
            listed_models,
        })
    }

    /// `plan()` then refuse unless the matrix allows a local-bridge write now.
    pub fn evaluate_enroll_native(&self, profile: &AdapterProfile) -> Result<AdapterApplyPlan> {
        self.require_enabled()?;
        let routes = AdapterRouteService::new(self.db.clone());
        let plan = routes.plan(&AdapterRouteRequest {
            source_kind: profile.source_kind,
            source_id: profile.source_id.clone(),
            target_agent_id: profile.target_agent_id,
        })?;
        enroll_native_plan_is_open(profile, &plan)?;
        Ok(plan)
    }

    /// Persist v2 enrollment only after a healthy local-bridge bind.
    /// Occupancy / bind failure must not call this.
    pub fn persist_enroll_after_native_bind(
        &self,
        bound_profile: &AdapterProfile,
        port: u16,
    ) -> Result<DefaultRoutePoolOverview> {
        self.require_enabled()?;
        if bound_profile.route != AdapterRoute::LocalBridge {
            return Err(AppError::Unsupported(
                "native enroll persist requires a local_bridge profile".into(),
            ));
        }
        let previous_default_id = RouteDownstreamSurface::for_agent(bound_profile.target_agent_id)
            .and_then(|surface| {
                self.pools
                    .list_pools(Some(bound_profile.target_agent_id), Some(surface))
                    .ok()
            })
            .and_then(|pools| {
                pools
                    .into_iter()
                    .find(|pool| pool.is_default && pool.id != bound_profile.id)
                    .map(|pool| pool.id)
            });
        self.ensure_legacy_pool(bound_profile)?;
        let enrolled = self.enroll_v2(&bound_profile.id, port)?;
        // Bind just made this login the Agent's active route; it is the default
        // pool for that surface. A leftover sibling default would hide this
        // overview from list_default_overviews.
        let pool = if enrolled.is_default {
            enrolled
        } else {
            self.pools.set_default(&enrolled.id)?
        };
        if let Some(previous_id) = previous_default_id {
            if previous_id != pool.id {
                self.copy_members(&previous_id, &pool.id)?;
            }
        }
        self.overview_from_pool(&pool)
    }

    /// Create or reuse the default pool for this Agent/surface, mark the
    /// authorization as pool-owned, and enroll it as a member.
    ///
    /// Pool-owned rows stay off the Connections ticket list until the user
    /// later associates them with a tool.
    pub fn attach_pool_owned_authorization(
        &self,
        target_agent_id: AgentId,
        surface: RouteDownstreamSurface,
        source_kind: AdapterSourceKind,
        source_id: &str,
    ) -> Result<DefaultRoutePoolOverview> {
        self.require_enabled()?;
        let pool = self.ensure_default_pool(target_agent_id, surface)?;
        let members = self.pools.list_members(&pool.id)?;
        if !members.iter().any(|member| {
            member.source_kind == source_kind && member.source_id == source_id
        }) {
            self.add_member(&pool.id, source_kind, source_id)?;
        }
        self.stamp_route_pool_home(target_agent_id, source_kind, source_id)?;
        let pool = self.pools.get_pool(&pool.id)?.ok_or_else(|| {
            AppError::message("db.route_pool", "pool missing after attach")
        })?;
        self.overview_from_pool(&pool)
    }

    /// Enroll existing Connections authorizations into the matching default
    /// pools. Does not hide them from Connections and does not copy secrets.
    pub fn sync_connection_authorizations(&self) -> Result<SyncConnectionAuthorizationsResult> {
        self.require_enabled()?;
        let profiles = self.profiles.list_filtered(&Default::default())?;
        let providers = self.providers.list(None)?;
        let accounts = self.accounts.list(None)?;
        let generated_ids: HashSet<String> = profiles
            .iter()
            .filter_map(|profile| profile.generated_provider_id.clone())
            .collect();
        let mut added = 0_u32;
        let mut skipped = 0_u32;

        for account in &accounts {
            if authorization_is_route_pool_home(&account.extra)
                || classify_account_live(
                    account.agent_id,
                    account.kind,
                    &account.credentials,
                    &profiles,
                    &providers,
                    false,
                )
                .is_projection()
            {
                skipped = skipped.saturating_add(1);
                continue;
            }
            match self.enroll_connection_source(
                account.agent_id,
                AdapterSourceKind::Account,
                &account.id,
            )? {
                true => added = added.saturating_add(1),
                false => skipped = skipped.saturating_add(1),
            }
        }
        for provider in &providers {
            if authorization_is_route_pool_home(&provider.meta)
                || generated_ids.contains(&provider.id)
                || generated_provider_is_adapter_owned(provider)
                || leftover::provider_is_bridge_leftover(provider)
            {
                skipped = skipped.saturating_add(1);
                continue;
            }
            match self.enroll_connection_source(
                provider.agent_id,
                AdapterSourceKind::Provider,
                &provider.id,
            )? {
                true => added = added.saturating_add(1),
                false => skipped = skipped.saturating_add(1),
            }
        }
        Ok(SyncConnectionAuthorizationsResult { added, skipped })
    }

    fn enroll_connection_source(
        &self,
        agent_id: AgentId,
        source_kind: AdapterSourceKind,
        source_id: &str,
    ) -> Result<bool> {
        let Some(surface) = RouteDownstreamSurface::for_agent(agent_id) else {
            return Ok(false);
        };
        let pool = self.ensure_default_pool(agent_id, surface)?;
        let members = self.pools.list_members(&pool.id)?;
        if members
            .iter()
            .any(|member| member.source_kind == source_kind && member.source_id == source_id)
        {
            return Ok(false);
        }
        self.add_member(&pool.id, source_kind, source_id)?;
        Ok(true)
    }

    pub fn ensure_default_pool(
        &self,
        target_agent_id: AgentId,
        surface: RouteDownstreamSurface,
    ) -> Result<RoutePool> {
        self.require_enabled()?;
        let existing = self.pools.list_pools(Some(target_agent_id), Some(surface))?;
        if let Some(pool) = existing.into_iter().find(|pool| pool.is_default) {
            return Ok(pool);
        }
        let now = now();
        self.pools.create_pool(&RoutePool {
            id: Uuid::new_v4().to_string(),
            target_agent_id,
            downstream_surface: surface,
            downstream_dialect: RouteDownstreamDialect::for_agent(target_agent_id),
            hub_token: generate_hub_token()?,
            schedule_policy: RouteSchedulePolicy::PriorityFailover,
            is_default: true,
            v2_enrolled: false,
            policy_revision: 1,
            auto_start: true,
            gateway_port: None,
            created_at: now.clone(),
            updated_at: now,
        })
    }

    pub fn list_members(&self, pool_id: &str) -> Result<Vec<RouteMember>> {
        self.require_enabled()?;
        self.pools.list_members(pool_id)
    }

    pub fn add_member(
        &self,
        pool_id: &str,
        source_kind: AdapterSourceKind,
        source_id: &str,
    ) -> Result<RouteMember> {
        self.require_enabled()?;
        let now = now();
        let existing = self.pools.list_members(pool_id)?;
        let position = existing
            .iter()
            .map(|member| member.position)
            .max()
            .map(|value| value + 1)
            .unwrap_or(0);
        let member = self.pools.add_member(&RouteMember {
            id: Uuid::new_v4().to_string(),
            route_pool_id: pool_id.to_owned(),
            source_kind,
            source_id: source_id.to_owned(),
            enabled: true,
            priority: 0,
            position,
            created_at: now.clone(),
            updated_at: now,
        })?;
        self.sync_lead_projection(pool_id)?;
        Ok(member)
    }

    pub fn set_member_enabled(&self, member_id: &str, enabled: bool) -> Result<RouteMember> {
        self.require_enabled()?;
        let mut member = self.require_member(member_id)?;
        member.enabled = enabled;
        member.updated_at = now();
        let saved = self.pools.update_member(&member)?;
        self.sync_lead_projection(&saved.route_pool_id)?;
        Ok(saved)
    }

    pub fn set_member_priority(&self, member_id: &str, priority: i64) -> Result<RouteMember> {
        self.require_enabled()?;
        let mut member = self.require_member(member_id)?;
        member.priority = priority;
        member.updated_at = now();
        let saved = self.pools.update_member(&member)?;
        self.sync_lead_projection(&saved.route_pool_id)?;
        Ok(saved)
    }

    pub fn reorder_members(
        &self,
        pool_id: &str,
        member_ids: &[String],
    ) -> Result<Vec<RouteMember>> {
        self.require_enabled()?;
        let members = self.pools.reorder_members(pool_id, member_ids)?;
        self.sync_lead_projection(pool_id)?;
        Ok(members)
    }

    pub fn remove_member(&self, member_id: &str) -> Result<()> {
        self.require_enabled()?;
        let member = self.require_member(member_id)?;
        self.pools.remove_member(member_id)?;
        self.sync_lead_projection(&member.route_pool_id)?;
        Ok(())
    }

    pub fn list_rules(&self, pool_id: &str) -> Result<Vec<ModelRouteRule>> {
        self.require_enabled()?;
        self.pools.list_rules(pool_id)
    }

    pub fn get_rule(&self, rule_id: &str) -> Result<Option<ModelRouteRule>> {
        self.require_enabled()?;
        self.pools.get_rule(rule_id)
    }

    /// Operators insert rules explicitly. Listed member models are not copied.
    pub fn add_rule(
        &self,
        pool_id: &str,
        public_model: &str,
        endpoint_family: &str,
        upstream_provider: &str,
        upstream_dialect: &str,
        upstream_model: &str,
        priority: i64,
        equivalent_group: Option<&str>,
    ) -> Result<ModelRouteRule> {
        self.require_enabled()?;
        let now = now();
        self.pools.add_rule(&ModelRouteRule {
            id: Uuid::new_v4().to_string(),
            route_pool_id: pool_id.to_owned(),
            public_model: public_model.to_owned(),
            endpoint_family: endpoint_family.to_owned(),
            upstream_provider: upstream_provider.to_owned(),
            upstream_dialect: upstream_dialect.to_owned(),
            upstream_model: upstream_model.to_owned(),
            priority,
            equivalent_group: equivalent_group
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned),
            enabled: true,
            created_at: now.clone(),
            updated_at: now,
        })
    }

    pub fn set_rule_enabled(&self, rule_id: &str, enabled: bool) -> Result<ModelRouteRule> {
        self.require_enabled()?;
        let mut rule = self.require_rule(rule_id)?;
        rule.enabled = enabled;
        rule.updated_at = now();
        self.pools.update_rule(&rule)
    }

    pub fn set_rule_priority(&self, rule_id: &str, priority: i64) -> Result<ModelRouteRule> {
        self.require_enabled()?;
        let mut rule = self.require_rule(rule_id)?;
        rule.priority = priority;
        rule.updated_at = now();
        self.pools.update_rule(&rule)
    }

    pub fn set_rule_equivalent_group(
        &self,
        rule_id: &str,
        equivalent_group: Option<&str>,
    ) -> Result<ModelRouteRule> {
        self.require_enabled()?;
        let mut rule = self.require_rule(rule_id)?;
        rule.equivalent_group = equivalent_group
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned);
        rule.updated_at = now();
        self.pools.update_rule(&rule)
    }

    pub fn remove_rule(&self, rule_id: &str) -> Result<()> {
        self.require_enabled()?;
        let _rule = self.require_rule(rule_id)?;
        self.pools.remove_rule(rule_id)
    }

    /// Persist the one-time v2 enrollment after the gateway port is already live.
    /// Occupancy failure must not call this; the historical client projection stays.
    pub fn enroll_v2(&self, pool_id: &str, gateway_port: u16) -> Result<RoutePool> {
        self.require_enabled()?;
        if let Some(profile) = self.profiles.get(pool_id)? {
            if profile.route != AdapterRoute::LocalBridge {
                return Err(AppError::Unsupported(
                    "native_endpoint and config_sync do not enroll into a loopback pool".into(),
                ));
            }
        }
        self.pools.enroll_v2(pool_id, gateway_port, &now())
    }

    /// Bind the unified gateway first; persist enrollment only after bind.
    /// Occupancy / bind failure leaves the pool unenrolled and does not rewrite
    /// the client-facing port or Hub token.
    pub async fn bind_then_enroll(
        &self,
        host: &BridgeRuntimeHost,
        pool_id: &str,
        port: u16,
    ) -> Result<RoutePool> {
        self.require_enabled()?;
        let bound = host
            .set_gateway_port(port)
            .await
            .map_err(|error| match error {
                crate::bridge::BridgeHostError::Bind(io) => AppError::Io(io),
                crate::bridge::BridgeHostError::InvalidGatewayPort => {
                    AppError::InvalidArg("v2 gateway port must be between 1 and 65535".into())
                }
                other => AppError::message("adapter.bridge_start", other.to_string()),
            })?;
        self.enroll_v2(pool_id, bound)
    }

    /// Project this local-bridge profile to a one-member unenrolled pool.
    /// Does not enroll v2 and never hijacks native_endpoint / config_sync.
    pub fn ensure_legacy_pool(&self, profile: &AdapterProfile) -> Result<Option<RoutePool>> {
        if !self.enabled()? {
            return Ok(None);
        }
        if profile.route != AdapterRoute::LocalBridge {
            return Ok(None);
        }
        Ok(Some(self.project_one(profile)?))
    }

    /// Project each old local-bridge profile to one RoutePool + lead member.
    /// Does not merge profiles. Hub token / id / auto-start stay on the pool.
    pub fn project_legacy_local_bridges(&self) -> Result<Vec<RoutePool>> {
        self.require_enabled()?;
        let profiles = self.profiles.list_filtered(&AdapterProfileFilter {
            route: Some(AdapterRoute::LocalBridge),
            ..AdapterProfileFilter::default()
        })?;
        let active = self.active_profile_ids(&profiles)?;
        let mut grouped: HashMap<(AgentId, RouteDownstreamSurface), Vec<String>> = HashMap::new();
        for profile in &profiles {
            let Some(surface) = RouteDownstreamSurface::for_agent(profile.target_agent_id) else {
                continue;
            };
            let pool = self.project_one(profile)?;
            grouped
                .entry((profile.target_agent_id, surface))
                .or_default()
                .push(pool.id.clone());
        }
        for ((agent, surface), ids) in grouped {
            let chosen = choose_default_pool_id(
                ids.iter().map(String::as_str),
                active.get(&agent).map(String::as_str),
            );
            if let Some(default_id) = chosen {
                let current = self.pools.get_pool(&default_id)?;
                if current.as_ref().is_some_and(|pool| {
                    pool.target_agent_id == agent
                        && pool.downstream_surface == surface
                        && !pool.is_default
                }) {
                    self.pools.set_default(&default_id)?;
                }
            }
        }
        self.pools.list_pools(None, None)
    }

    fn project_one(&self, profile: &AdapterProfile) -> Result<RoutePool> {
        let Some(surface) = RouteDownstreamSurface::for_agent(profile.target_agent_id) else {
            return Err(AppError::Unsupported(format!(
                "agent {} has no local-bridge surface",
                profile.target_agent_id.as_str()
            )));
        };
        if let Some(existing) = self.pools.get_pool(&profile.id)? {
            self.ensure_lead_member(&existing.id, profile)?;
            return Ok(existing);
        }
        let now = now();
        let pool = self.pools.create_pool(&RoutePool {
            id: profile.id.clone(),
            target_agent_id: profile.target_agent_id,
            downstream_surface: surface,
            downstream_dialect: RouteDownstreamDialect::for_agent(profile.target_agent_id),
            hub_token: generate_hub_token()?,
            schedule_policy: RouteSchedulePolicy::PriorityFailover,
            is_default: false,
            v2_enrolled: false,
            policy_revision: 1,
            auto_start: profile.auto_start,
            gateway_port: None,
            created_at: profile.created_at.clone(),
            updated_at: now,
        })?;
        self.ensure_lead_member(&pool.id, profile)?;
        Ok(pool)
    }

    /// Test/control-plane helper: create a pool with an explicit Hub token so
    /// legacy projection can prove the token is not rotated.
    pub fn create_legacy_pool(
        &self,
        profile: &AdapterProfile,
        hub_token: &str,
        is_default: bool,
    ) -> Result<RoutePool> {
        self.require_enabled()?;
        let Some(surface) = RouteDownstreamSurface::for_agent(profile.target_agent_id) else {
            return Err(AppError::Unsupported(format!(
                "agent {} has no local-bridge surface",
                profile.target_agent_id.as_str()
            )));
        };
        let now = now();
        let pool = self.pools.create_pool(&RoutePool {
            id: profile.id.clone(),
            target_agent_id: profile.target_agent_id,
            downstream_surface: surface,
            downstream_dialect: RouteDownstreamDialect::for_agent(profile.target_agent_id),
            hub_token: hub_token.to_owned(),
            schedule_policy: RouteSchedulePolicy::PriorityFailover,
            is_default,
            v2_enrolled: false,
            policy_revision: 1,
            auto_start: profile.auto_start,
            gateway_port: None,
            created_at: profile.created_at.clone(),
            updated_at: now,
        })?;
        self.ensure_lead_member(&pool.id, profile)?;
        Ok(pool)
    }

    fn ensure_lead_member(&self, pool_id: &str, profile: &AdapterProfile) -> Result<()> {
        let members = self.pools.list_members(pool_id)?;
        if members.iter().any(|member| {
            member.source_kind == profile.source_kind && member.source_id == profile.source_id
        }) {
            return Ok(());
        }
        let now = now();
        let position = members
            .iter()
            .map(|member| member.position)
            .max()
            .map(|value| value + 1)
            .unwrap_or(0);
        self.pools.add_member(&RouteMember {
            id: Uuid::new_v4().to_string(),
            route_pool_id: pool_id.to_owned(),
            source_kind: profile.source_kind,
            source_id: profile.source_id.clone(),
            enabled: true,
            priority: 0,
            position,
            created_at: now.clone(),
            updated_at: now,
        })?;
        Ok(())
    }

    fn stamp_route_pool_home(
        &self,
        target_agent_id: AgentId,
        source_kind: AdapterSourceKind,
        source_id: &str,
    ) -> Result<()> {
        match source_kind {
            AdapterSourceKind::Provider => {
                let mut provider = self.providers.get_by_id(source_id)?.ok_or_else(|| {
                    AppError::NotFound(format!("provider not found: {source_id}"))
                })?;
                if provider.agent_id != target_agent_id {
                    return Err(AppError::InvalidArg(
                        "authorization does not belong to this Agent".into(),
                    ));
                }
                if provider.is_current {
                    return Err(AppError::InvalidArg(
                        "the live login cannot be pool-only".into(),
                    ));
                }
                if authorization_is_route_pool_home(&provider.meta) {
                    return Ok(());
                }
                set_authorization_route_pool_home(&mut provider.meta);
                provider.updated_at = now();
                self.providers.update(&provider)?;
            }
            AdapterSourceKind::Account => {
                let mut account = self.accounts.get_by_id(source_id)?.ok_or_else(|| {
                    AppError::NotFound(format!("account not found: {source_id}"))
                })?;
                if account.agent_id != target_agent_id {
                    return Err(AppError::InvalidArg(
                        "authorization does not belong to this Agent".into(),
                    ));
                }
                if account.is_current {
                    return Err(AppError::InvalidArg(
                        "the live login cannot be pool-only".into(),
                    ));
                }
                if authorization_is_route_pool_home(&account.extra) {
                    return Ok(());
                }
                set_authorization_route_pool_home(&mut account.extra);
                account.updated_at = now();
                self.accounts.update(&account)?;
            }
        }
        Ok(())
    }

    fn copy_members(&self, from_pool_id: &str, to_pool_id: &str) -> Result<()> {
        let existing = self.pools.list_members(to_pool_id)?;
        for member in self.pools.list_members(from_pool_id)? {
            if existing.iter().any(|row| {
                row.source_kind == member.source_kind && row.source_id == member.source_id
            }) {
                continue;
            }
            self.add_member(to_pool_id, member.source_kind, &member.source_id)?;
        }
        Ok(())
    }

    fn sync_lead_projection(&self, pool_id: &str) -> Result<()> {
        let Some(mut profile) = self.profiles.get(pool_id)? else {
            return Ok(());
        };
        let Some(lead) = self
            .pools
            .list_members(pool_id)?
            .into_iter()
            .find(|member| member.enabled)
        else {
            return Ok(());
        };
        if profile.source_kind == lead.source_kind && profile.source_id == lead.source_id {
            return Ok(());
        }
        profile.source_kind = lead.source_kind;
        profile.source_id = lead.source_id;
        profile.updated_at = now();
        self.profiles.update(&profile)?;
        Ok(())
    }

    fn active_profile_ids(&self, profiles: &[AdapterProfile]) -> Result<HashMap<AgentId, String>> {
        let mut active = HashMap::new();
        self.db.with_conn(|conn| {
            for profile in profiles {
                if active.contains_key(&profile.target_agent_id) {
                    continue;
                }
                let Some(binding) = binding_get_conn(conn, profile.target_agent_id.as_str())?
                else {
                    continue;
                };
                let bound_id = binding
                    .account_id
                    .as_deref()
                    .or(binding.provider_id.as_deref());
                let Some(bound_id) = bound_id else {
                    continue;
                };
                if let Some(matched) = profiles.iter().find(|candidate| {
                    candidate.target_agent_id == profile.target_agent_id
                        && candidate.source_id == bound_id
                }) {
                    active.insert(profile.target_agent_id, matched.id.clone());
                }
            }
            Ok(())
        })?;
        Ok(active)
    }

    fn listed_models_for_pool(&self, pool: &RoutePool, members: &[RouteMember]) -> Vec<String> {
        let Some(lead) = members.iter().find(|member| member.enabled) else {
            return Vec::new();
        };
        let routes = AdapterRouteService::new(self.db.clone());
        let Ok(product) = routes.classify_source_product(lead.source_kind, &lead.source_id) else {
            return Vec::new();
        };
        list_local_bridge_models(product, pool.target_agent_id, None)
    }

    fn require_member(&self, member_id: &str) -> Result<RouteMember> {
        self.pools
            .get_member(member_id)?
            .ok_or_else(|| AppError::NotFound(format!("route member not found: {member_id}")))
    }

    fn require_rule(&self, rule_id: &str) -> Result<ModelRouteRule> {
        self.pools
            .get_rule(rule_id)?
            .ok_or_else(|| AppError::NotFound(format!("model route rule not found: {rule_id}")))
    }

    fn require_enabled(&self) -> Result<()> {
        if self.enabled()? {
            Ok(())
        } else {
            Err(AppError::Unsupported("route_pool_v2 is disabled".into()))
        }
    }
}

fn now() -> String {
    Utc::now().to_rfc3339()
}
