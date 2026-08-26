//! RoutePool control plane. Flag-gated and fail-closed; runtime still uses the
//! lead member until `route_index_v2` is wired. UI stays hidden (P8).

use std::collections::HashMap;

use chrono::Utc;
use uuid::Uuid;

use crate::error::{AppError, Result};
use crate::models::{
    choose_default_pool_id, feature_flag_enabled, generate_hub_token, AdapterProfile,
    AdapterProfileFilter, AdapterRoute, AdapterSourceKind, AgentId, RouteDownstreamDialect,
    RouteDownstreamSurface, RouteMember, RoutePool, RouteSchedulePolicy, FEATURE_ROUTE_POOL_V2,
};
use crate::storage::{binding_get_conn, AdapterProfileRepo, Database, RoutePoolRepo};

#[cfg(test)]
mod tests;

pub struct RoutePoolService {
    db: Database,
    pools: RoutePoolRepo,
    profiles: AdapterProfileRepo,
}

impl RoutePoolService {
    pub fn new(db: Database) -> Self {
        Self {
            pools: RoutePoolRepo::new(db.clone()),
            profiles: AdapterProfileRepo::new(db.clone()),
            db,
        }
    }

    pub fn enabled(&self) -> Result<bool> {
        Ok(feature_flag_enabled(
            self.db.get_setting(FEATURE_ROUTE_POOL_V2)?.as_deref(),
        ))
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

    /// Persist the one-time v2 enrollment after the gateway port is already live.
    /// Occupancy failure must not call this; the historical client projection stays.
    pub fn enroll_v2(&self, pool_id: &str, gateway_port: u16) -> Result<RoutePool> {
        self.require_enabled()?;
        self.pools.enroll_v2(pool_id, gateway_port, &now())
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

    fn require_member(&self, member_id: &str) -> Result<RouteMember> {
        self.pools
            .get_member(member_id)?
            .ok_or_else(|| AppError::NotFound(format!("route member not found: {member_id}")))
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
