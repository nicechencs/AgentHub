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
use crate::logging::targets;
use crate::models::{
    authorization_is_route_pool_home, choose_default_pool_id, enroll_native_plan_is_open,
    feature_flag_enabled, generate_hub_token, list_local_bridge_models, product_flag_enabled,
    set_authorization_route_pool_home, AdapterApplyPlan, AdapterProfile, AdapterProfileFilter,
    AccountKind, AdapterRoute, AdapterRouteRequest, AdapterSourceKind, AdapterSourceProduct,
    AgentId, ConnectionTrashKind, DefaultRoutePoolList, DefaultRoutePoolOverview, LocalTokenRecord,
    ModelRouteRule,
    RouteDownstreamDialect, RouteDownstreamSurface, RouteMember, RouteMemberOverview, RoutePool,
    RouteMembershipTrashMember, RouteMembershipTrashPayload, RouteSchedulePolicy,
    SyncConnectionAuthorizationsResult, SyncConnectionSource, TicketProtocol, TicketSurface,
    FEATURE_CODEX_INGRESS_GROK_UPSTREAM, FEATURE_GROK_INGRESS_CODEX_UPSTREAM,
    FEATURE_MIXED_PROVIDER_POOL, FEATURE_ROUTE_INDEX_V2, FEATURE_ROUTE_POOL_V2,
    LOCAL_ENTRY_DESIRED_RUNNING, SHARE_CHAT_COMPLETIONS,
};
use serde_json::Value;
use crate::storage::{
    binding_get_conn, AccountRepo, AdapterProfileRepo, ConnectionTrashRepo, Database, ProviderRepo,
    RoutePoolRepo,
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
                chat_completions_shared: false,
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
            chat_completions_shared: self.chat_completions_shared()?,
        })
    }

    /// Default pools including Hub tokens, for starting the shared local entry.
    pub fn list_default_pools(&self) -> Result<Vec<RoutePool>> {
        if !self.enabled()? {
            return Ok(Vec::new());
        }
        Ok(self
            .pools
            .list_pools(None, None)?
            .into_iter()
            .filter(|pool| pool.is_default)
            .collect())
    }

    /// Loopback bearers for the tokens page. Empty when the pool flag is off.
    pub fn list_local_tokens(&self) -> Result<Vec<LocalTokenRecord>> {
        Ok(self
            .list_default_pools()?
            .into_iter()
            .map(|pool| LocalTokenRecord {
                pool_id: pool.id,
                token: pool.hub_token,
            })
            .collect())
    }

    /// Replace one default-pool loopback bearer.
    pub fn set_local_token(&self, pool_id: &str, token: &str) -> Result<LocalTokenRecord> {
        self.require_enabled()?;
        let token = token.trim();
        if token.is_empty() {
            return Err(AppError::InvalidArg("entry key must not be empty".into()));
        }
        let pool = self.pools.get_pool(pool_id)?.ok_or_else(|| {
            AppError::NotFound(format!("route pool not found: {pool_id}"))
        })?;
        if !pool.is_default {
            return Err(AppError::InvalidArg(
                "only default pool entry keys can be changed".into(),
            ));
        }
        let saved = self.pools.set_hub_token(pool_id, token, &now())?;
        Ok(LocalTokenRecord {
            pool_id: saved.id,
            token: saved.hub_token,
        })
    }

    pub fn chat_completions_shared(&self) -> Result<bool> {
        Ok(feature_flag_enabled(
            self.db.get_setting(SHARE_CHAT_COMPLETIONS)?.as_deref(),
        ))
    }

    /// Last local-entry switch. Unset stays on so existing auto-restore keeps working.
    pub fn local_entry_desired_running(&self) -> Result<bool> {
        Ok(product_flag_enabled(
            self.db.get_setting(LOCAL_ENTRY_DESIRED_RUNNING)?.as_deref(),
        ))
    }

    /// Remember the shared local-entry switch for the next process start.
    pub fn set_local_entry_desired_running(&self, running: bool) -> Result<()> {
        self.db.set_setting(
            LOCAL_ENTRY_DESIRED_RUNNING,
            if running { "true" } else { "false" },
        )
    }

    /// Kimi and DSH share one chat-completions token, or keep separate keys.
    pub fn set_chat_completions_shared(&self, shared: bool) -> Result<DefaultRoutePoolList> {
        self.require_enabled()?;
        let current = self.chat_completions_shared()?;
        if current != shared {
            self.db.set_setting(
                SHARE_CHAT_COMPLETIONS,
                if shared { "true" } else { "false" },
            )?;
            if shared {
                self.merge_chat_completions_pools()?;
            } else {
                self.split_chat_completions_pools()?;
            }
        }
        self.list_default_overviews()
    }

    pub fn overview_from_pool(&self, pool: &RoutePool) -> Result<DefaultRoutePoolOverview> {
        let members = self.pools.list_members(&pool.id)?;
        let listed_models = self.listed_models_for_pool(pool, &members);
        let mut member_overviews = Vec::with_capacity(members.len());
        for member in members {
            let (display_label, refresh_token_tail) = self.member_display_fields(&member)?;
            member_overviews.push(RouteMemberOverview {
                id: member.id,
                source_kind: member.source_kind,
                source_id: member.source_id,
                display_label,
                refresh_token_tail,
                enabled: member.enabled,
                priority: member.priority,
                availability: Some(if member.enabled {
                    crate::models::MemberAvailability::Ready
                } else {
                    crate::models::MemberAvailability::Disabled
                }),
            });
        }
        Ok(DefaultRoutePoolOverview {
            id: pool.id.clone(),
            target_agent_id: pool.target_agent_id,
            surface: pool.downstream_surface,
            dialect: pool.downstream_dialect,
            v2_enrolled: pool.v2_enrolled,
            gateway_port: pool.gateway_port,
            members: member_overviews,
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
        let pool_agent = self.writer_agent_for_pool(target_agent_id, surface)?;
        let pool = self.ensure_default_pool(pool_agent, surface)?;
        let members = self.pools.list_members(&pool.id)?;
        let added_member = if !members.iter().any(|member| {
            member.source_kind == source_kind && member.source_id == source_id
        }) {
            Some(self.add_member(&pool.id, source_kind, source_id)?)
        } else {
            None
        };
        let attached = (|| -> Result<DefaultRoutePoolOverview> {
            self.stamp_route_pool_home(target_agent_id, source_kind, source_id)?;
            let pool = self.pools.get_pool(&pool.id)?.ok_or_else(|| {
                AppError::message("db.route_pool", "pool missing after attach")
            })?;
            self.overview_from_pool(&pool)
        })();
        if let Err(error) = attached {
            if let Some(member) = added_member {
                if let Err(cleanup) = self.pools.remove_member(&member.id) {
                    tracing::warn!(
                        module = targets::ADAPTER,
                        op = "attach_rollback",
                        member_id = %member.id,
                        error_code = cleanup.code(),
                        "failed to roll back route pool member after attach failure"
                    );
                }
                let _ = self.sync_lead_projection(&member.route_pool_id);
            }
            return Err(error);
        }
        attached
    }

    /// Enroll existing Connections authorizations into the matching default
    /// pools. Does not hide them from Connections and does not copy secrets.
    ///
    /// The no-argument form is retained for the existing bulk-sync behavior.
    pub fn sync_connection_authorizations(&self) -> Result<SyncConnectionAuthorizationsResult> {
        self.sync_connection_authorizations_selected(None)
    }

    /// Enroll only the selected Connections source rows. The source's Agent is
    /// always read from the stored row, so a client cannot redirect a source to
    /// another Agent or surface by changing the request.
    pub fn sync_connection_authorizations_selected(
        &self,
        selected: Option<&[SyncConnectionSource]>,
    ) -> Result<SyncConnectionAuthorizationsResult> {
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

        let mut enroll_source = |source_kind: AdapterSourceKind, source_id: &str| -> Result<()> {
            match source_kind {
                AdapterSourceKind::Account => {
                    let Some(account) = accounts.iter().find(|row| row.id == source_id) else {
                        skipped = skipped.saturating_add(1);
                        return Ok(());
                    };
                    if account.kind == AccountKind::Oauth
                        && !oauth_login_is_pool_shareable(account.agent_id)
                    {
                        skipped = skipped.saturating_add(1);
                        return Ok(());
                    }
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
                        return Ok(());
                    }
                    let product =
                        AdapterRouteService::classify_account_source_product(account);
                    let targets = pool_targets_for_source(
                        account.agent_id,
                        product,
                        &[&account.credentials, &account.extra],
                    );
                    match self.enroll_connection_source(
                        &targets,
                        AdapterSourceKind::Account,
                        &account.id,
                    )? {
                        true => added = added.saturating_add(1),
                        false => skipped = skipped.saturating_add(1),
                    }
                }
                AdapterSourceKind::Provider => {
                    let Some(provider) = providers.iter().find(|row| row.id == source_id) else {
                        skipped = skipped.saturating_add(1);
                        return Ok(());
                    };
                    if authorization_is_route_pool_home(&provider.meta)
                        || generated_ids.contains(&provider.id)
                        || generated_provider_is_adapter_owned(provider)
                        || leftover::provider_is_bridge_leftover(provider)
                    {
                        skipped = skipped.saturating_add(1);
                        return Ok(());
                    }
                    let product =
                        AdapterRouteService::classify_provider_source_product(provider);
                    let targets = pool_targets_for_source(
                        provider.agent_id,
                        product,
                        &[&provider.settings_config, &provider.meta],
                    );
                    match self.enroll_connection_source(
                        &targets,
                        AdapterSourceKind::Provider,
                        &provider.id,
                    )? {
                        true => added = added.saturating_add(1),
                        false => skipped = skipped.saturating_add(1),
                    }
                }
            }
            Ok(())
        };

        if let Some(selected) = selected {
            for source in selected {
                enroll_source(source.source_kind, &source.source_id)?;
            }
        } else {
            for account in &accounts {
                enroll_source(AdapterSourceKind::Account, &account.id)?;
            }
            for provider in &providers {
                enroll_source(AdapterSourceKind::Provider, &provider.id)?;
            }
        }
        Ok(SyncConnectionAuthorizationsResult { added, skipped })
    }

    fn enroll_connection_source(
        &self,
        targets: &[(AgentId, RouteDownstreamSurface)],
        source_kind: AdapterSourceKind,
        source_id: &str,
    ) -> Result<bool> {
        if targets.is_empty() {
            return Ok(false);
        }
        let shared = self.chat_completions_shared()?;
        let targets = collapse_chat_targets(targets, shared);
        let mut added_any = false;
        for (agent_id, surface) in &targets {
            let pool = self.ensure_default_pool(*agent_id, *surface)?;
            let members = self.pools.list_members(&pool.id)?;
            if members
                .iter()
                .any(|member| member.source_kind == source_kind && member.source_id == source_id)
            {
                continue;
            }
            self.add_member(&pool.id, source_kind, source_id)?;
            added_any = true;
        }
        Ok(added_any)
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
        if let Err(error) = self.sync_lead_projection(pool_id) {
            if let Err(cleanup) = self.pools.remove_member(&member.id) {
                tracing::warn!(
                    module = targets::ADAPTER,
                    op = "member_rollback",
                    member_id = %member.id,
                    error_code = cleanup.code(),
                    "failed to roll back route pool member after projection failure"
                );
            }
            return Err(error);
        }
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

    /// Enable or disable every default-pool membership of one login.
    pub fn set_authorization_enabled(
        &self,
        source_kind: AdapterSourceKind,
        source_id: &str,
        enabled: bool,
    ) -> Result<u32> {
        self.require_enabled()?;
        let mut changed = 0_u32;
        for pool in self.pools.list_pools(None, None)? {
            if !pool.is_default {
                continue;
            }
            for member in self.pools.list_members(&pool.id)? {
                if member.source_kind != source_kind || member.source_id != source_id {
                    continue;
                }
                if member.enabled == enabled {
                    continue;
                }
                self.set_member_enabled(&member.id, enabled)?;
                changed = changed.saturating_add(1);
            }
        }
        Ok(changed)
    }

    /// Set priority on every default-pool membership of one login.
    pub fn set_authorization_priority(
        &self,
        source_kind: AdapterSourceKind,
        source_id: &str,
        priority: i64,
    ) -> Result<u32> {
        self.require_enabled()?;
        let mut changed = 0_u32;
        for pool in self.pools.list_pools(None, None)? {
            if !pool.is_default {
                continue;
            }
            for member in self.pools.list_members(&pool.id)? {
                if member.source_kind != source_kind || member.source_id != source_id {
                    continue;
                }
                if member.priority == priority {
                    continue;
                }
                self.set_member_priority(&member.id, priority)?;
                changed = changed.saturating_add(1);
            }
        }
        Ok(changed)
    }

    /// Remove every default-pool membership of one authorization.
    ///
    /// This is intentionally separate from deleting an Account or Provider:
    /// a pool member may outlive its source row (and is then shown as
    /// unavailable by the Routes page). Removing the membership is still a
    /// durable route-pool write and must cover every default pool.
    pub fn remove_route_authorization(
        &self,
        source_kind: AdapterSourceKind,
        source_id: &str,
    ) -> Result<u32> {
        self.require_enabled()?;
        let mut removed = 0_u32;
        for pool in self.pools.list_pools(None, None)? {
            if !pool.is_default {
                continue;
            }
            for member in self.pools.list_members(&pool.id)? {
                if member.source_kind != source_kind || member.source_id != source_id {
                    continue;
                }
                self.pools.remove_member(&member.id)?;
                self.sync_lead_projection(&member.route_pool_id)?;
                removed = removed.saturating_add(1);
            }
        }
        Ok(removed)
    }

    /// Move a Connections-managed pool member into the pool recycle bin.
    /// Does not delete the Connections login.
    pub fn recycle_route_membership(
        &self,
        source_kind: AdapterSourceKind,
        source_id: &str,
    ) -> Result<u32> {
        self.require_enabled()?;
        let mut members = Vec::new();
        for pool in self.pools.list_pools(None, None)? {
            if !pool.is_default {
                continue;
            }
            for member in self.pools.list_members(&pool.id)? {
                if member.source_kind == source_kind && member.source_id == source_id {
                    members.push(member);
                }
            }
        }
        let (agent_id, label) = self.membership_trash_identity(source_kind, source_id, &members)?;
        let payload = RouteMembershipTrashPayload {
            source_kind,
            source_id: source_id.to_owned(),
            members: members
                .iter()
                .map(|member| RouteMembershipTrashMember {
                    route_pool_id: member.route_pool_id.clone(),
                    enabled: member.enabled,
                    priority: member.priority,
                    position: member.position,
                })
                .collect(),
        };
        let now = now();
        self.db.with_conn(|conn| {
            ConnectionTrashRepo::insert_conn(
                conn,
                source_id,
                agent_id,
                ConnectionTrashKind::Membership,
                &label,
                false,
                &payload,
                &now,
            )
        })?;
        self.remove_route_authorization(source_kind, source_id)
    }

    fn membership_trash_identity(
        &self,
        source_kind: AdapterSourceKind,
        source_id: &str,
        members: &[RouteMember],
    ) -> Result<(AgentId, String)> {
        match source_kind {
            AdapterSourceKind::Account => {
                if let Some(account) = self.accounts.get_by_id(source_id)? {
                    return Ok((account.agent_id, account.label));
                }
            }
            AdapterSourceKind::Provider => {
                if let Some(provider) = self.providers.get_by_id(source_id)? {
                    return Ok((provider.agent_id, provider.name));
                }
            }
        }
        if let Some(member) = members.first() {
            if let Some(pool) = self.pools.get_pool(&member.route_pool_id)? {
                return Ok((pool.target_agent_id, source_id.to_owned()));
            }
        }
        Err(AppError::NotFound(format!(
            "route authorization not found: {source_id}"
        )))
    }

    pub fn restore_membership_trash(&self, payload: &RouteMembershipTrashPayload) -> Result<()> {
        self.require_enabled()?;
        for snapshot in &payload.members {
            let Some(pool) = self.pools.get_pool(&snapshot.route_pool_id)? else {
                continue;
            };
            let existing = self.pools.list_members(&pool.id)?;
            if existing.iter().any(|member| {
                member.source_kind == payload.source_kind && member.source_id == payload.source_id
            }) {
                continue;
            }
            let now = now();
            let position = existing
                .iter()
                .map(|member| member.position)
                .max()
                .map(|value| value + 1)
                .unwrap_or(snapshot.position);
            let member = self.pools.add_member(&RouteMember {
                id: Uuid::new_v4().to_string(),
                route_pool_id: pool.id.clone(),
                source_kind: payload.source_kind,
                source_id: payload.source_id.clone(),
                enabled: snapshot.enabled,
                priority: snapshot.priority,
                position,
                created_at: now.clone(),
                updated_at: now,
            })?;
            if let Err(error) = self.sync_lead_projection(&member.route_pool_id) {
                let _ = self.pools.remove_member(&member.id);
                return Err(error);
            }
        }
        Ok(())
    }

    pub fn reattach_restored_pool_owned(
        &self,
        agent_id: AgentId,
        source_kind: AdapterSourceKind,
        source_id: &str,
    ) -> Result<()> {
        let Some(surface) = RouteDownstreamSurface::for_agent(agent_id) else {
            return Ok(());
        };
        self.attach_pool_owned_authorization(agent_id, surface, source_kind, source_id)?;
        Ok(())
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

    /// Live catalog for a pool's enabled logins. Empty live lists fall back to
    /// the mapping table. Results are not written to the DB.
    pub fn list_upstream_models_for_pool(&self, pool_id: &str) -> Result<Vec<String>> {
        let pool = self.pools.get_pool(pool_id)?.ok_or_else(|| {
            AppError::NotFound(format!("route pool not found: {pool_id}"))
        })?;
        let members = self.pools.list_members(pool_id)?;
        let mut listed = Vec::new();
        let mut seen = HashSet::new();
        for member in members.iter().filter(|member| member.enabled) {
            for model in self.live_models_for_member(member) {
                if seen.insert(model.clone()) {
                    listed.push(model);
                }
            }
        }
        if listed.is_empty() {
            return Ok(self.listed_models_for_pool(&pool, &members));
        }
        Ok(listed)
    }

    fn live_models_for_member(&self, member: &RouteMember) -> Vec<String> {
        self.ensure_source_model_catalog(member.source_kind, &member.source_id)
            .map(|catalog| catalog.models)
            .unwrap_or_default()
    }

    /// Fetch once per URL/key/login identity, then reuse the cached list.
    pub fn ensure_source_model_catalog(
        &self,
        source_kind: AdapterSourceKind,
        source_id: &str,
    ) -> Result<crate::utils::upstream_model_catalog::SourceModelCatalog> {
        self.load_source_model_catalog(source_kind, source_id, false)
    }

    fn load_source_model_catalog(
        &self,
        source_kind: AdapterSourceKind,
        source_id: &str,
        force: bool,
    ) -> Result<crate::utils::upstream_model_catalog::SourceModelCatalog> {
        use crate::utils::upstream_model_catalog::{
            cache_is_current, read_stored_catalog, write_stored_catalog, SourceModelCatalog,
            StoredModelCatalog,
        };
        match source_kind {
            AdapterSourceKind::Account => {
                let mut account = self.accounts.get_by_id(source_id)?.ok_or_else(|| {
                    AppError::NotFound(format!("account not found: {source_id}"))
                })?;
                let fingerprint = self.catalog_fingerprint_for_account(&account);
                if let Some(stored) = read_stored_catalog(&account.extra) {
                    if cache_is_current(&stored, &fingerprint)
                        && (!force || stored.source == "custom")
                    {
                        return Ok(SourceModelCatalog::from_stored(&stored));
                    }
                }
                let models = self.fetch_live_models_for_account(&account);
                let stored = StoredModelCatalog {
                    fingerprint,
                    source: if models.is_empty() {
                        "empty".into()
                    } else {
                        "live".into()
                    },
                    models,
                    extra_models: Vec::new(),
                    attempted: true,
                    updated_at: now(),
                };
                write_stored_catalog(&mut account.extra, &stored);
                self.accounts.update(&account)?;
                Ok(SourceModelCatalog::from_stored(&stored))
            }
            AdapterSourceKind::Provider => {
                let mut provider = self.providers.get_by_id(source_id)?.ok_or_else(|| {
                    AppError::NotFound(format!("provider not found: {source_id}"))
                })?;
                let fingerprint = crate::utils::upstream_model_catalog::fingerprint_apikey(
                    provider.agent_id.as_str(),
                    &provider.settings_config,
                );
                if let Some(stored) = read_stored_catalog(&provider.meta) {
                    if cache_is_current(&stored, &fingerprint)
                        && (!force || stored.source == "custom")
                    {
                        return Ok(SourceModelCatalog::from_stored(&stored));
                    }
                }
                let models = self.live_models_from_settings(&provider.settings_config);
                let stored = StoredModelCatalog {
                    fingerprint,
                    source: if models.is_empty() {
                        "empty".into()
                    } else {
                        "live".into()
                    },
                    models,
                    extra_models: Vec::new(),
                    attempted: true,
                    updated_at: now(),
                };
                write_stored_catalog(&mut provider.meta, &stored);
                self.providers.update(&provider)?;
                Ok(SourceModelCatalog::from_stored(&stored))
            }
        }
    }

    pub fn set_source_custom_models(
        &self,
        source_kind: AdapterSourceKind,
        source_id: &str,
        models: Vec<String>,
    ) -> Result<crate::utils::upstream_model_catalog::SourceModelCatalog> {
        use crate::utils::upstream_model_catalog::{
            read_stored_catalog, with_wanted_models, write_stored_catalog, SourceModelCatalog,
            StoredModelCatalog,
        };
        match source_kind {
            AdapterSourceKind::Account => {
                let mut account = self.accounts.get_by_id(source_id)?.ok_or_else(|| {
                    AppError::NotFound(format!("account not found: {source_id}"))
                })?;
                let fingerprint = self.catalog_fingerprint_for_account(&account);
                let prior = read_stored_catalog(&account.extra).unwrap_or(StoredModelCatalog {
                    fingerprint: fingerprint.clone(),
                    source: "empty".into(),
                    models: Vec::new(),
                    extra_models: Vec::new(),
                    attempted: true,
                    updated_at: now(),
                });
                let stored = with_wanted_models(
                    StoredModelCatalog {
                        fingerprint,
                        updated_at: now(),
                        attempted: true,
                        ..prior
                    },
                    models,
                );
                write_stored_catalog(&mut account.extra, &stored);
                self.accounts.update(&account)?;
                Ok(SourceModelCatalog::from_stored(&stored))
            }
            AdapterSourceKind::Provider => {
                let mut provider = self.providers.get_by_id(source_id)?.ok_or_else(|| {
                    AppError::NotFound(format!("provider not found: {source_id}"))
                })?;
                let fingerprint = crate::utils::upstream_model_catalog::fingerprint_apikey(
                    provider.agent_id.as_str(),
                    &provider.settings_config,
                );
                let prior = read_stored_catalog(&provider.meta).unwrap_or(StoredModelCatalog {
                    fingerprint: fingerprint.clone(),
                    source: "empty".into(),
                    models: Vec::new(),
                    extra_models: Vec::new(),
                    attempted: true,
                    updated_at: now(),
                });
                let stored = with_wanted_models(
                    StoredModelCatalog {
                        fingerprint,
                        updated_at: now(),
                        attempted: true,
                        ..prior
                    },
                    models,
                );
                write_stored_catalog(&mut provider.meta, &stored);
                self.providers.update(&provider)?;
                Ok(SourceModelCatalog::from_stored(&stored))
            }
        }
    }

    pub fn set_local_token_custom_models(
        &self,
        token: &str,
        models: Vec<String>,
    ) -> Result<Vec<String>> {
        let Some(pool_id) = self.pool_id_for_token(token)? else {
            return Ok(Vec::new());
        };
        let members = self.pools.list_members(&pool_id)?;
        for member in members.iter().filter(|member| member.enabled) {
            let _ = self.set_source_custom_models(
                member.source_kind,
                &member.source_id,
                models.clone(),
            );
        }
        self.list_upstream_models_for_pool(&pool_id)
    }

    /// Re-read each enabled login's supported models, then return the union
    /// for this entry key. Custom lists on a login are kept.
    pub fn refresh_local_token_models(&self, token: &str) -> Result<Vec<String>> {
        let Some(pool_id) = self.pool_id_for_token(token)? else {
            return Ok(Vec::new());
        };
        let members = self.pools.list_members(&pool_id)?;
        for member in members.iter().filter(|member| member.enabled) {
            let _ = self.load_source_model_catalog(
                member.source_kind,
                &member.source_id,
                true,
            );
        }
        self.list_upstream_models_for_pool(&pool_id)
    }

    fn pool_id_for_token(&self, token: &str) -> Result<Option<String>> {
        Ok(self
            .list_local_tokens()?
            .into_iter()
            .find(|record| record.token == token)
            .map(|record| record.pool_id))
    }

    fn catalog_fingerprint_for_account(&self, account: &crate::models::Account) -> String {
        if account.kind == AccountKind::Oauth {
            let identity = crate::services::account_quota::extract_chatgpt_account_id(account)
                .or_else(|| nonempty_json_str(&account.extra, "accountId"))
                .or_else(|| nonempty_json_str(&account.extra, "sub"))
                .or_else(|| nonempty_json_str(&account.credentials, "account_id"))
                .or_else(|| nonempty_json_str(&account.credentials, "sub"))
                .unwrap_or_else(|| account.id.clone());
            crate::utils::upstream_model_catalog::fingerprint_oauth(
                account.agent_id.as_str(),
                &identity,
            )
        } else {
            crate::utils::upstream_model_catalog::fingerprint_apikey(
                account.agent_id.as_str(),
                &account.credentials,
            )
        }
    }

    fn fetch_live_models_for_account(&self, account: &crate::models::Account) -> Vec<String> {
        if account.kind == AccountKind::Oauth {
            return self.live_models_for_official_login(account);
        }
        self.live_models_from_settings(&account.credentials)
    }

    fn live_models_for_official_login(&self, account: &crate::models::Account) -> Vec<String> {
        match account.agent_id {
            AgentId::Codex => {
                let Some(access) =
                    crate::services::account_quota::extract_access_token(account)
                else {
                    return Vec::new();
                };
                let Some(account_id) =
                    crate::services::account_quota::extract_chatgpt_account_id(account)
                else {
                    return Vec::new();
                };
                crate::utils::chatgpt_codex_models::list_chatgpt_codex_models(
                    &access, &account_id,
                )
                .unwrap_or_default()
            }
            _ => Vec::new(),
        }
    }

    fn live_models_from_settings(&self, blob: &Value) -> Vec<String> {
        let embedded =
            crate::utils::upstream_model_catalog::embedded_listed_models(blob);
        let remote = crate::utils::upstream_model_catalog::catalog_endpoint(blob)
            .and_then(|(base, key)| {
                crate::utils::remote_openai_models::list_remote_openai_models(&base, &key).ok()
            })
            .unwrap_or_default();
        crate::utils::upstream_model_catalog::merge_model_ids([remote, embedded])
    }

    fn member_display_fields(
        &self,
        member: &RouteMember,
    ) -> Result<(Option<String>, Option<String>)> {
        match member.source_kind {
            AdapterSourceKind::Account => {
                let Some(account) = self.accounts.get_by_id(&member.source_id)? else {
                    return Ok((None, None));
                };
                let display_label = if account.kind == AccountKind::Oauth {
                    account
                        .identity_label()
                        .or_else(|| account.extra_email())
                        .or_else(|| account.credential_email())
                        .or_else(|| non_empty_label(&account.label))
                } else {
                    non_empty_label(&account.label)
                }
                .map(str::to_owned);
                let refresh_token_tail = if account.kind == AccountKind::Oauth {
                    crate::utils::redact::refresh_token_tail(&account.credentials)
                } else {
                    None
                };
                Ok((display_label, refresh_token_tail))
            }
            AdapterSourceKind::Provider => {
                let display_label = self
                    .providers
                    .get_by_id(&member.source_id)?
                    .and_then(|provider| non_empty_label(&provider.name).map(str::to_owned));
                Ok((display_label, None))
            }
        }
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

    fn writer_agent_for_pool(
        &self,
        target_agent_id: AgentId,
        surface: RouteDownstreamSurface,
    ) -> Result<AgentId> {
        Ok(if self.chat_completions_shared()? {
            shared_chat_writer(target_agent_id, surface)
        } else {
            target_agent_id
        })
    }

    fn merge_chat_completions_pools(&self) -> Result<()> {
        let dsh = self
            .pools
            .list_pools(Some(AgentId::Dsh), Some(RouteDownstreamSurface::ChatCompletions))?
            .into_iter()
            .find(|pool| pool.is_default);
        let Some(dsh) = dsh else {
            return Ok(());
        };
        let kimi = self.ensure_default_pool(AgentId::Kimi, RouteDownstreamSurface::ChatCompletions)?;
        if kimi.id == dsh.id {
            return Ok(());
        }
        let kimi_members = self.pools.list_members(&kimi.id)?;
        let dsh_members = self.pools.list_members(&dsh.id)?;
        for member in dsh_members {
            let exists = kimi_members.iter().any(|row| {
                row.source_kind == member.source_kind && row.source_id == member.source_id
            });
            if !exists {
                self.add_member(&kimi.id, member.source_kind, &member.source_id)?;
            }
            self.remove_member(&member.id)?;
        }
        Ok(())
    }

    fn split_chat_completions_pools(&self) -> Result<()> {
        let kimi = self
            .pools
            .list_pools(Some(AgentId::Kimi), Some(RouteDownstreamSurface::ChatCompletions))?
            .into_iter()
            .find(|pool| pool.is_default);
        let Some(kimi) = kimi else {
            return Ok(());
        };
        let dsh = self.ensure_default_pool(AgentId::Dsh, RouteDownstreamSurface::ChatCompletions)?;
        if kimi.id == dsh.id {
            return Ok(());
        }
        let kimi_members = self.pools.list_members(&kimi.id)?;
        let dsh_members = self.pools.list_members(&dsh.id)?;
        for member in kimi_members {
            let home = self.member_source_agent(&member)?;
            let already_on_dsh = dsh_members.iter().any(|row| {
                row.source_kind == member.source_kind && row.source_id == member.source_id
            });
            if home == Some(AgentId::Dsh) {
                if !already_on_dsh {
                    self.add_member(&dsh.id, member.source_kind, &member.source_id)?;
                }
                self.remove_member(&member.id)?;
            } else if home != Some(AgentId::Kimi) && !already_on_dsh {
                self.add_member(&dsh.id, member.source_kind, &member.source_id)?;
            }
        }
        Ok(())
    }

    fn member_source_agent(&self, member: &RouteMember) -> Result<Option<AgentId>> {
        Ok(match member.source_kind {
            AdapterSourceKind::Account => self
                .accounts
                .get_by_id(&member.source_id)?
                .map(|account| account.agent_id),
            AdapterSourceKind::Provider => self
                .providers
                .get_by_id(&member.source_id)?
                .map(|provider| provider.agent_id),
        })
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

fn nonempty_json_str(blob: &Value, key: &str) -> Option<String> {
    blob.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn non_empty_label(value: &str) -> Option<&str> {
    let value = value.trim();
    (!value.is_empty()).then_some(value)
}

fn oauth_login_is_pool_shareable(agent: AgentId) -> bool {
    matches!(agent, AgentId::Claude | AgentId::Codex | AgentId::Grok)
}

fn shared_chat_writer(agent: AgentId, surface: RouteDownstreamSurface) -> AgentId {
    if surface == RouteDownstreamSurface::ChatCompletions
        && matches!(agent, AgentId::Kimi | AgentId::Dsh)
    {
        AgentId::Kimi
    } else {
        agent
    }
}

fn collapse_chat_targets(
    targets: &[(AgentId, RouteDownstreamSurface)],
    shared: bool,
) -> Vec<(AgentId, RouteDownstreamSurface)> {
    if !shared {
        return targets.to_vec();
    }
    let mut out = Vec::new();
    for (agent, surface) in targets {
        let agent = shared_chat_writer(*agent, *surface);
        let item = (agent, *surface);
        if !out.contains(&item) {
            out.push(item);
        }
    }
    out
}

fn pool_targets_for_source(
    agent_id: AgentId,
    product: AdapterSourceProduct,
    blobs: &[&Value],
) -> Vec<(AgentId, RouteDownstreamSurface)> {
    if let Some(surface) = RouteDownstreamSurface::for_agent(agent_id) {
        return vec![(agent_id, surface)];
    }
    let mut surfaces = surfaces_from_ticket_product(product);
    if surfaces.is_empty() {
        surfaces = surfaces_from_endpoint_blobs(blobs);
    }
    if surfaces.is_empty() {
        surfaces = fallback_surfaces_for_source_agent(agent_id);
    }
    writer_pool_targets_for_surfaces(&surfaces)
}

fn writer_pool_targets_for_surfaces(
    surfaces: &[RouteDownstreamSurface],
) -> Vec<(AgentId, RouteDownstreamSurface)> {
    AgentId::ALL
        .into_iter()
        .filter_map(|agent| {
            let surface = RouteDownstreamSurface::for_agent(agent)?;
            surfaces.contains(&surface).then_some((agent, surface))
        })
        .collect()
}

fn surfaces_from_ticket_product(product: AdapterSourceProduct) -> Vec<RouteDownstreamSurface> {
    surfaces_from_protocols(TicketSurface::from_product(product).speaks())
}

fn surfaces_from_protocols(speaks: &[TicketProtocol]) -> Vec<RouteDownstreamSurface> {
    let mut out = Vec::new();
    for protocol in speaks {
        let Some(surface) = protocol_surface(*protocol) else {
            continue;
        };
        push_unique_surface(&mut out, surface);
    }
    out
}

fn protocol_surface(protocol: TicketProtocol) -> Option<RouteDownstreamSurface> {
    match protocol {
        TicketProtocol::AnthropicMessages => Some(RouteDownstreamSurface::Messages),
        TicketProtocol::OpenaiResponses => Some(RouteDownstreamSurface::Responses),
        TicketProtocol::OpenaiChat => Some(RouteDownstreamSurface::ChatCompletions),
        TicketProtocol::AnthropicPkce
        | TicketProtocol::OpenaiCodexPkce
        | TicketProtocol::XaiDeviceCode => None,
    }
}

fn surfaces_from_endpoint_blobs(blobs: &[&Value]) -> Vec<RouteDownstreamSurface> {
    let mut out = Vec::new();
    for blob in blobs {
        visit_endpoint_strings(blob, 0, &mut |value| {
            let lower = value.to_ascii_lowercase();
            if lower.contains("/v1/messages") || lower.contains("/anthropic") {
                push_unique_surface(&mut out, RouteDownstreamSurface::Messages);
            }
            if lower.contains("/v1/responses") {
                push_unique_surface(&mut out, RouteDownstreamSurface::Responses);
            }
            if lower.contains("chat/completions")
                || lower.contains("compatible-mode")
                || lower.contains("/v1/chat")
            {
                push_unique_surface(&mut out, RouteDownstreamSurface::ChatCompletions);
            }
        });
    }
    out
}

fn visit_endpoint_strings(value: &Value, depth: usize, visit: &mut impl FnMut(&str)) {
    if depth > 4 {
        return;
    }
    match value {
        Value::String(text) => {
            if text.contains("://") || text.trim_start().starts_with("/v1/") {
                visit(text);
            }
        }
        Value::Array(items) => {
            for item in items {
                visit_endpoint_strings(item, depth + 1, visit);
            }
        }
        Value::Object(map) => {
            for child in map.values() {
                visit_endpoint_strings(child, depth + 1, visit);
            }
        }
        _ => {}
    }
}

fn fallback_surfaces_for_source_agent(agent: AgentId) -> Vec<RouteDownstreamSurface> {
    match agent {
        AgentId::WorkBuddy => vec![RouteDownstreamSurface::ChatCompletions],
        AgentId::Zcode | AgentId::Pi => vec![
            RouteDownstreamSurface::Messages,
            RouteDownstreamSurface::Responses,
            RouteDownstreamSurface::ChatCompletions,
        ],
        AgentId::Cursor => Vec::new(),
        other => RouteDownstreamSurface::for_agent(other).into_iter().collect(),
    }
}

fn push_unique_surface(out: &mut Vec<RouteDownstreamSurface>, surface: RouteDownstreamSurface) {
    if !out.contains(&surface) {
        out.push(surface);
    }
}
