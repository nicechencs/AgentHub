//! Persistence for RoutePool / RouteMember. Members store authorization
//! references only; the Hub token is the pool's local loopback bearer.

use rusqlite::{
    params, Connection, ErrorCode, OptionalExtension, Row, Transaction, TransactionBehavior,
};

use crate::error::{AppError, Result};
use crate::models::{
    model_route_id_is_exact, AdapterSourceKind, AgentId, ModelRouteRule, RouteDownstreamDialect,
    RouteDownstreamSurface, RouteMember, RoutePool, RouteSchedulePolicy,
};
use crate::storage::Database;

#[cfg(test)]
mod tests;

const POOL_COLUMNS: &str = r#"
    id, target_agent_id, downstream_surface, downstream_dialect, hub_token,
    schedule_policy, is_default, v2_enrolled, policy_revision, auto_start,
    gateway_port, created_at, updated_at
"#;

const MEMBER_COLUMNS: &str = r#"
    id, route_pool_id, source_kind, source_id, enabled, priority, position,
    created_at, updated_at
"#;

const RULE_COLUMNS: &str = r#"
    id, route_pool_id, public_model, endpoint_family, upstream_provider,
    upstream_dialect, upstream_model, priority, equivalent_group, enabled,
    created_at, updated_at
"#;

#[derive(Clone)]
pub struct RoutePoolRepo {
    db: Database,
}

impl RoutePoolRepo {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    pub fn create_pool(&self, pool: &RoutePool) -> Result<RoutePool> {
        validate_pool(pool)?;
        self.mutate(|conn| {
            if get_pool_conn(conn, &pool.id)?.is_some() {
                return Err(AppError::InvalidArg(format!(
                    "route pool already exists: {}",
                    pool.id
                )));
            }
            insert_pool_conn(conn, pool).map_err(map_pool_constraint)?;
            get_pool_conn(conn, &pool.id)?
                .ok_or_else(|| AppError::message("db.route_pool", "pool missing after create"))
        })
    }

    pub fn get_pool(&self, id: &str) -> Result<Option<RoutePool>> {
        self.db.with_conn(|conn| get_pool_conn(conn, id))
    }

    pub fn list_pools(
        &self,
        target_agent_id: Option<AgentId>,
        surface: Option<RouteDownstreamSurface>,
    ) -> Result<Vec<RoutePool>> {
        self.db.with_conn(|conn| {
            let mut stmt = conn.prepare(&format!(
                r#"
                SELECT {POOL_COLUMNS}
                FROM route_pools
                WHERE (?1 IS NULL OR target_agent_id = ?1)
                  AND (?2 IS NULL OR downstream_surface = ?2)
                ORDER BY target_agent_id ASC, downstream_surface ASC, id ASC
                "#
            ))?;
            let rows = stmt.query_map(
                params![
                    target_agent_id.map(AgentId::as_str),
                    surface.map(RouteDownstreamSurface::as_str),
                ],
                map_pool_row,
            )?;
            rows.map(|row| row.and_then(|raw| raw.into_pool().map_err(to_sql_error)))
                .collect::<std::result::Result<Vec<_>, _>>()
                .map_err(AppError::from)
        })
    }

    pub fn update_pool(&self, pool: &RoutePool) -> Result<RoutePool> {
        validate_pool(pool)?;
        self.mutate(|conn| {
            let existing = get_pool_conn(conn, &pool.id)?
                .ok_or_else(|| AppError::NotFound(format!("route pool not found: {}", pool.id)))?;
            let mut stored = pool.clone();
            stored.created_at = existing.created_at;
            stored.hub_token = existing.hub_token;
            update_pool_conn(conn, &stored).map_err(map_pool_constraint)?;
            get_pool_conn(conn, &pool.id)?
                .ok_or_else(|| AppError::message("db.route_pool", "pool missing after update"))
        })
    }

    /// Mark this pool as the unique default for its Agent / surface.
    pub fn set_default(&self, pool_id: &str) -> Result<RoutePool> {
        self.mutate(|conn| {
            let mut pool = get_pool_conn(conn, pool_id)?.ok_or_else(|| {
                AppError::NotFound(format!("route pool not found: {pool_id}"))
            })?;
            conn.execute(
                r#"
                UPDATE route_pools
                SET is_default = 0, updated_at = ?3
                WHERE target_agent_id = ?1 AND downstream_surface = ?2 AND id != ?4 AND is_default = 1
                "#,
                params![
                    pool.target_agent_id.as_str(),
                    pool.downstream_surface.as_str(),
                    pool.updated_at,
                    pool.id,
                ],
            )?;
            pool.is_default = true;
            pool.policy_revision = pool.policy_revision.saturating_add(1);
            update_pool_conn(conn, &pool)?;
            get_pool_conn(conn, pool_id)?
                .ok_or_else(|| AppError::message("db.route_pool", "pool missing after set_default"))
        })
    }

    pub fn add_member(&self, member: &RouteMember) -> Result<RouteMember> {
        validate_member(member)?;
        self.mutate(|conn| {
            if get_pool_conn(conn, &member.route_pool_id)?.is_none() {
                return Err(AppError::NotFound(format!(
                    "route pool not found: {}",
                    member.route_pool_id
                )));
            }
            insert_member_conn(conn, member).map_err(map_member_constraint)?;
            bump_revision_conn(conn, &member.route_pool_id)?;
            get_member_conn(conn, &member.id)?
                .ok_or_else(|| AppError::message("db.route_member", "member missing after create"))
        })
    }

    pub fn get_member(&self, id: &str) -> Result<Option<RouteMember>> {
        self.db.with_conn(|conn| get_member_conn(conn, id))
    }

    pub fn list_members(&self, pool_id: &str) -> Result<Vec<RouteMember>> {
        self.db.with_conn(|conn| list_members_conn(conn, pool_id))
    }

    pub fn update_member(&self, member: &RouteMember) -> Result<RouteMember> {
        validate_member(member)?;
        self.mutate(|conn| {
            let existing = get_member_conn(conn, &member.id)?.ok_or_else(|| {
                AppError::NotFound(format!("route member not found: {}", member.id))
            })?;
            if existing.route_pool_id != member.route_pool_id
                || existing.source_kind != member.source_kind
                || existing.source_id != member.source_id
            {
                return Err(AppError::InvalidArg(
                    "route member pool and authorization reference are immutable".into(),
                ));
            }
            let mut stored = member.clone();
            stored.created_at = existing.created_at;
            update_member_conn(conn, &stored)?;
            bump_revision_conn(conn, &stored.route_pool_id)?;
            get_member_conn(conn, &member.id)?
                .ok_or_else(|| AppError::message("db.route_member", "member missing after update"))
        })
    }

    pub fn remove_member(&self, id: &str) -> Result<()> {
        self.mutate(|conn| {
            let existing = get_member_conn(conn, id)?
                .ok_or_else(|| AppError::NotFound(format!("route member not found: {id}")))?;
            conn.execute("DELETE FROM route_members WHERE id = ?1", params![id])?;
            bump_revision_conn(conn, &existing.route_pool_id)?;
            Ok(())
        })
    }

    pub fn reorder_members(
        &self,
        pool_id: &str,
        member_ids: &[String],
    ) -> Result<Vec<RouteMember>> {
        self.mutate(|conn| {
            let existing = list_members_conn(conn, pool_id)?;
            if existing.len() != member_ids.len()
                || member_ids
                    .iter()
                    .any(|id| existing.iter().all(|row| row.id != *id))
            {
                return Err(AppError::InvalidArg(
                    "reorder must include each member of the pool exactly once".into(),
                ));
            }
            let stamp = existing
                .first()
                .map(|row| row.updated_at.clone())
                .unwrap_or_default();
            for (position, member_id) in member_ids.iter().enumerate() {
                conn.execute(
                    "UPDATE route_members SET position = ?2, updated_at = ?3 WHERE id = ?1",
                    params![member_id, position as i64, stamp],
                )?;
            }
            bump_revision_conn(conn, pool_id)?;
            list_members_conn(conn, pool_id)
        })
    }

    pub fn add_rule(&self, rule: &ModelRouteRule) -> Result<ModelRouteRule> {
        validate_rule(rule)?;
        self.mutate(|conn| {
            if get_pool_conn(conn, &rule.route_pool_id)?.is_none() {
                return Err(AppError::NotFound(format!(
                    "route pool not found: {}",
                    rule.route_pool_id
                )));
            }
            insert_rule_conn(conn, rule).map_err(map_rule_constraint)?;
            bump_revision_conn(conn, &rule.route_pool_id)?;
            get_rule_conn(conn, &rule.id)?.ok_or_else(|| {
                AppError::message("db.model_route_rule", "rule missing after create")
            })
        })
    }

    pub fn get_rule(&self, id: &str) -> Result<Option<ModelRouteRule>> {
        self.db.with_conn(|conn| get_rule_conn(conn, id))
    }

    pub fn list_rules(&self, pool_id: &str) -> Result<Vec<ModelRouteRule>> {
        self.db.with_conn(|conn| list_rules_conn(conn, pool_id))
    }

    pub fn update_rule(&self, rule: &ModelRouteRule) -> Result<ModelRouteRule> {
        validate_rule(rule)?;
        self.mutate(|conn| {
            let existing = get_rule_conn(conn, &rule.id)?.ok_or_else(|| {
                AppError::NotFound(format!("model route rule not found: {}", rule.id))
            })?;
            if existing.route_pool_id != rule.route_pool_id {
                return Err(AppError::InvalidArg(
                    "model route rule pool is immutable".into(),
                ));
            }
            let mut stored = rule.clone();
            stored.created_at = existing.created_at;
            stored.equivalent_group = stored
                .equivalent_group
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned);
            update_rule_conn(conn, &stored).map_err(map_rule_constraint)?;
            bump_revision_conn(conn, &stored.route_pool_id)?;
            get_rule_conn(conn, &rule.id)?.ok_or_else(|| {
                AppError::message("db.model_route_rule", "rule missing after update")
            })
        })
    }

    pub fn remove_rule(&self, id: &str) -> Result<()> {
        self.mutate(|conn| {
            let existing = get_rule_conn(conn, id)?
                .ok_or_else(|| AppError::NotFound(format!("model route rule not found: {id}")))?;
            conn.execute("DELETE FROM model_route_rules WHERE id = ?1", params![id])?;
            bump_revision_conn(conn, &existing.route_pool_id)?;
            Ok(())
        })
    }

    pub fn enroll_v2(
        &self,
        pool_id: &str,
        gateway_port: u16,
        updated_at: &str,
    ) -> Result<RoutePool> {
        if gateway_port == 0 {
            return Err(AppError::InvalidArg(
                "v2 gateway port must be between 1 and 65535".into(),
            ));
        }
        self.mutate(|conn| {
            let mut pool = get_pool_conn(conn, pool_id)?
                .ok_or_else(|| AppError::NotFound(format!("route pool not found: {pool_id}")))?;
            if pool.v2_enrolled {
                if pool.gateway_port == Some(gateway_port) {
                    return Ok(pool);
                }
                return Err(AppError::InvalidArg(
                    "v2 gateway port is frozen after enroll".into(),
                ));
            }
            pool.v2_enrolled = true;
            pool.gateway_port = Some(gateway_port);
            pool.updated_at = updated_at.to_owned();
            pool.policy_revision = pool.policy_revision.saturating_add(1);
            update_pool_conn(conn, &pool)?;
            get_pool_conn(conn, pool_id)?
                .ok_or_else(|| AppError::message("db.route_pool", "pool missing after enroll"))
        })
    }

    fn mutate<T, F>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&Connection) -> Result<T>,
    {
        self.db.with_conn(|conn| {
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
            let out = f(&tx)?;
            tx.commit()?;
            Ok(out)
        })
    }
}

fn validate_pool(pool: &RoutePool) -> Result<()> {
    if pool.id.trim().is_empty() {
        return Err(AppError::InvalidArg(
            "route pool id must not be empty".into(),
        ));
    }
    if pool.hub_token.trim().is_empty() {
        return Err(AppError::InvalidArg(
            "route pool hub token must not be empty".into(),
        ));
    }
    if pool.policy_revision < 1 {
        return Err(AppError::InvalidArg(
            "route pool revision must be >= 1".into(),
        ));
    }
    if pool.created_at.trim().is_empty() || pool.updated_at.trim().is_empty() {
        return Err(AppError::InvalidArg(
            "route pool timestamps must not be empty".into(),
        ));
    }
    Ok(())
}

fn validate_rule(rule: &ModelRouteRule) -> Result<()> {
    for (field, value) in [
        ("id", rule.id.as_str()),
        ("route_pool_id", rule.route_pool_id.as_str()),
        ("public_model", rule.public_model.as_str()),
        ("endpoint_family", rule.endpoint_family.as_str()),
        ("upstream_provider", rule.upstream_provider.as_str()),
        ("upstream_dialect", rule.upstream_dialect.as_str()),
        ("upstream_model", rule.upstream_model.as_str()),
        ("created_at", rule.created_at.as_str()),
        ("updated_at", rule.updated_at.as_str()),
    ] {
        if value.trim().is_empty() {
            return Err(AppError::InvalidArg(format!(
                "model route rule {field} must not be empty"
            )));
        }
    }
    if !model_route_id_is_exact(&rule.public_model)
        || !model_route_id_is_exact(&rule.upstream_model)
    {
        return Err(AppError::InvalidArg(
            "model route rules match exact model ids only".into(),
        ));
    }
    Ok(())
}

fn validate_member(member: &RouteMember) -> Result<()> {
    for (field, value) in [
        ("id", member.id.as_str()),
        ("route_pool_id", member.route_pool_id.as_str()),
        ("source_id", member.source_id.as_str()),
        ("created_at", member.created_at.as_str()),
        ("updated_at", member.updated_at.as_str()),
    ] {
        if value.trim().is_empty() {
            return Err(AppError::InvalidArg(format!(
                "route member {field} must not be empty"
            )));
        }
    }
    Ok(())
}

fn insert_pool_conn(conn: &Connection, pool: &RoutePool) -> rusqlite::Result<usize> {
    conn.execute(
        r#"
        INSERT INTO route_pools (
            id, target_agent_id, downstream_surface, downstream_dialect, hub_token,
            schedule_policy, is_default, v2_enrolled, policy_revision, auto_start,
            gateway_port, created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
        "#,
        params![
            pool.id,
            pool.target_agent_id.as_str(),
            pool.downstream_surface.as_str(),
            pool.downstream_dialect.as_str(),
            pool.hub_token,
            pool.schedule_policy.as_str(),
            i64::from(pool.is_default),
            i64::from(pool.v2_enrolled),
            pool.policy_revision,
            i64::from(pool.auto_start),
            pool.gateway_port.map(i64::from),
            pool.created_at,
            pool.updated_at,
        ],
    )
}

fn update_pool_conn(conn: &Connection, pool: &RoutePool) -> rusqlite::Result<usize> {
    conn.execute(
        r#"
        UPDATE route_pools
        SET target_agent_id = ?2, downstream_surface = ?3, downstream_dialect = ?4,
            schedule_policy = ?5, is_default = ?6, v2_enrolled = ?7,
            policy_revision = ?8, auto_start = ?9, gateway_port = ?10,
            created_at = ?11, updated_at = ?12
        WHERE id = ?1
        "#,
        params![
            pool.id,
            pool.target_agent_id.as_str(),
            pool.downstream_surface.as_str(),
            pool.downstream_dialect.as_str(),
            pool.schedule_policy.as_str(),
            i64::from(pool.is_default),
            i64::from(pool.v2_enrolled),
            pool.policy_revision,
            i64::from(pool.auto_start),
            pool.gateway_port.map(i64::from),
            pool.created_at,
            pool.updated_at,
        ],
    )
}

fn insert_member_conn(conn: &Connection, member: &RouteMember) -> rusqlite::Result<usize> {
    conn.execute(
        r#"
        INSERT INTO route_members (
            id, route_pool_id, source_kind, source_id, enabled, priority, position,
            created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
        "#,
        params![
            member.id,
            member.route_pool_id,
            member.source_kind.as_str(),
            member.source_id,
            i64::from(member.enabled),
            member.priority,
            member.position,
            member.created_at,
            member.updated_at,
        ],
    )
}

fn update_member_conn(conn: &Connection, member: &RouteMember) -> rusqlite::Result<usize> {
    conn.execute(
        r#"
        UPDATE route_members
        SET enabled = ?2, priority = ?3, position = ?4, updated_at = ?5
        WHERE id = ?1
        "#,
        params![
            member.id,
            i64::from(member.enabled),
            member.priority,
            member.position,
            member.updated_at,
        ],
    )
}

fn insert_rule_conn(conn: &Connection, rule: &ModelRouteRule) -> rusqlite::Result<usize> {
    conn.execute(
        r#"
        INSERT INTO model_route_rules (
            id, route_pool_id, public_model, endpoint_family, upstream_provider,
            upstream_dialect, upstream_model, priority, equivalent_group, enabled,
            created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
        "#,
        params![
            rule.id,
            rule.route_pool_id,
            rule.public_model.trim(),
            rule.endpoint_family.trim(),
            rule.upstream_provider.trim(),
            rule.upstream_dialect.trim(),
            rule.upstream_model.trim(),
            rule.priority,
            rule.normalized_equivalent_group(),
            i64::from(rule.enabled),
            rule.created_at,
            rule.updated_at,
        ],
    )
}

fn update_rule_conn(conn: &Connection, rule: &ModelRouteRule) -> rusqlite::Result<usize> {
    conn.execute(
        r#"
        UPDATE model_route_rules
        SET public_model = ?2, endpoint_family = ?3, upstream_provider = ?4,
            upstream_dialect = ?5, upstream_model = ?6, priority = ?7,
            equivalent_group = ?8, enabled = ?9, updated_at = ?10
        WHERE id = ?1
        "#,
        params![
            rule.id,
            rule.public_model.trim(),
            rule.endpoint_family.trim(),
            rule.upstream_provider.trim(),
            rule.upstream_dialect.trim(),
            rule.upstream_model.trim(),
            rule.priority,
            rule.normalized_equivalent_group(),
            i64::from(rule.enabled),
            rule.updated_at,
        ],
    )
}

fn get_rule_conn(conn: &Connection, id: &str) -> Result<Option<ModelRouteRule>> {
    conn.query_row(
        &format!("SELECT {RULE_COLUMNS} FROM model_route_rules WHERE id = ?1"),
        params![id],
        map_rule_row,
    )
    .optional()?
    .map(RawModelRouteRule::into_rule)
    .transpose()
}

fn list_rules_conn(conn: &Connection, pool_id: &str) -> Result<Vec<ModelRouteRule>> {
    let mut stmt = conn.prepare(&format!(
        r#"
        SELECT {RULE_COLUMNS}
        FROM model_route_rules
        WHERE route_pool_id = ?1
        ORDER BY priority ASC, id ASC
        "#
    ))?;
    let rows = stmt.query_map(params![pool_id], map_rule_row)?;
    rows.map(|row| row.and_then(|raw| raw.into_rule().map_err(to_sql_error)))
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(AppError::from)
}

fn bump_revision_conn(conn: &Connection, pool_id: &str) -> Result<()> {
    let changed = conn.execute(
        "UPDATE route_pools SET policy_revision = policy_revision + 1 WHERE id = ?1",
        params![pool_id],
    )?;
    if changed != 1 {
        return Err(AppError::message(
            "db.route_pool",
            "pool missing during revision bump",
        ));
    }
    Ok(())
}

fn get_pool_conn(conn: &Connection, id: &str) -> Result<Option<RoutePool>> {
    conn.query_row(
        &format!("SELECT {POOL_COLUMNS} FROM route_pools WHERE id = ?1"),
        params![id],
        map_pool_row,
    )
    .optional()?
    .map(RawRoutePool::into_pool)
    .transpose()
}

fn get_member_conn(conn: &Connection, id: &str) -> Result<Option<RouteMember>> {
    conn.query_row(
        &format!("SELECT {MEMBER_COLUMNS} FROM route_members WHERE id = ?1"),
        params![id],
        map_member_row,
    )
    .optional()?
    .map(RawRouteMember::into_member)
    .transpose()
}

fn list_members_conn(conn: &Connection, pool_id: &str) -> Result<Vec<RouteMember>> {
    let mut stmt = conn.prepare(&format!(
        r#"
        SELECT {MEMBER_COLUMNS}
        FROM route_members
        WHERE route_pool_id = ?1
        ORDER BY priority ASC, position ASC, id ASC
        "#
    ))?;
    let rows = stmt.query_map(params![pool_id], map_member_row)?;
    rows.map(|row| row.and_then(|raw| raw.into_member().map_err(to_sql_error)))
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(AppError::from)
}

struct RawRoutePool {
    id: String,
    target_agent_id: String,
    downstream_surface: String,
    downstream_dialect: String,
    hub_token: String,
    schedule_policy: String,
    is_default: i64,
    v2_enrolled: i64,
    policy_revision: i64,
    auto_start: i64,
    gateway_port: Option<i64>,
    created_at: String,
    updated_at: String,
}

impl RawRoutePool {
    fn into_pool(self) -> Result<RoutePool> {
        let target_agent_id = AgentId::parse(&self.target_agent_id)
            .ok_or_else(|| invalid_enum("target_agent_id", &self.target_agent_id, &self.id))?;
        let downstream_surface = RouteDownstreamSurface::parse(&self.downstream_surface)
            .ok_or_else(|| {
                invalid_enum("downstream_surface", &self.downstream_surface, &self.id)
            })?;
        let downstream_dialect = RouteDownstreamDialect::parse(&self.downstream_dialect)
            .ok_or_else(|| {
                invalid_enum("downstream_dialect", &self.downstream_dialect, &self.id)
            })?;
        let schedule_policy = RouteSchedulePolicy::parse(&self.schedule_policy)
            .ok_or_else(|| invalid_enum("schedule_policy", &self.schedule_policy, &self.id))?;
        let gateway_port = self
            .gateway_port
            .map(|port| {
                u16::try_from(port)
                    .ok()
                    .filter(|port| *port != 0)
                    .ok_or_else(|| invalid_value("gateway_port", &port.to_string(), &self.id))
            })
            .transpose()?;
        Ok(RoutePool {
            id: self.id,
            target_agent_id,
            downstream_surface,
            downstream_dialect,
            hub_token: self.hub_token,
            schedule_policy,
            is_default: parse_bool(self.is_default, "is_default")?,
            v2_enrolled: parse_bool(self.v2_enrolled, "v2_enrolled")?,
            policy_revision: self.policy_revision,
            auto_start: parse_bool(self.auto_start, "auto_start")?,
            gateway_port,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

struct RawRouteMember {
    id: String,
    route_pool_id: String,
    source_kind: String,
    source_id: String,
    enabled: i64,
    priority: i64,
    position: i64,
    created_at: String,
    updated_at: String,
}

impl RawRouteMember {
    fn into_member(self) -> Result<RouteMember> {
        let source_kind = AdapterSourceKind::parse(&self.source_kind)
            .ok_or_else(|| invalid_enum("source_kind", &self.source_kind, &self.id))?;
        Ok(RouteMember {
            id: self.id,
            route_pool_id: self.route_pool_id,
            source_kind,
            source_id: self.source_id,
            enabled: parse_bool(self.enabled, "enabled")?,
            priority: self.priority,
            position: self.position,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

fn map_pool_row(row: &Row<'_>) -> rusqlite::Result<RawRoutePool> {
    Ok(RawRoutePool {
        id: row.get(0)?,
        target_agent_id: row.get(1)?,
        downstream_surface: row.get(2)?,
        downstream_dialect: row.get(3)?,
        hub_token: row.get(4)?,
        schedule_policy: row.get(5)?,
        is_default: row.get(6)?,
        v2_enrolled: row.get(7)?,
        policy_revision: row.get(8)?,
        auto_start: row.get(9)?,
        gateway_port: row.get(10)?,
        created_at: row.get(11)?,
        updated_at: row.get(12)?,
    })
}

struct RawModelRouteRule {
    id: String,
    route_pool_id: String,
    public_model: String,
    endpoint_family: String,
    upstream_provider: String,
    upstream_dialect: String,
    upstream_model: String,
    priority: i64,
    equivalent_group: Option<String>,
    enabled: i64,
    created_at: String,
    updated_at: String,
}

impl RawModelRouteRule {
    fn into_rule(self) -> Result<ModelRouteRule> {
        Ok(ModelRouteRule {
            id: self.id,
            route_pool_id: self.route_pool_id,
            public_model: self.public_model,
            endpoint_family: self.endpoint_family,
            upstream_provider: self.upstream_provider,
            upstream_dialect: self.upstream_dialect,
            upstream_model: self.upstream_model,
            priority: self.priority,
            equivalent_group: self
                .equivalent_group
                .map(|value| value.trim().to_owned())
                .filter(|value| !value.is_empty()),
            enabled: parse_bool(self.enabled, "enabled")?,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

fn map_rule_row(row: &Row<'_>) -> rusqlite::Result<RawModelRouteRule> {
    Ok(RawModelRouteRule {
        id: row.get(0)?,
        route_pool_id: row.get(1)?,
        public_model: row.get(2)?,
        endpoint_family: row.get(3)?,
        upstream_provider: row.get(4)?,
        upstream_dialect: row.get(5)?,
        upstream_model: row.get(6)?,
        priority: row.get(7)?,
        equivalent_group: row.get(8)?,
        enabled: row.get(9)?,
        created_at: row.get(10)?,
        updated_at: row.get(11)?,
    })
}

fn map_member_row(row: &Row<'_>) -> rusqlite::Result<RawRouteMember> {
    Ok(RawRouteMember {
        id: row.get(0)?,
        route_pool_id: row.get(1)?,
        source_kind: row.get(2)?,
        source_id: row.get(3)?,
        enabled: row.get(4)?,
        priority: row.get(5)?,
        position: row.get(6)?,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
    })
}

fn parse_bool(value: i64, field: &str) -> Result<bool> {
    match value {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(AppError::message(
            "route_pool.invalid_data",
            format!("invalid {field} '{value}'"),
        )),
    }
}

fn invalid_enum(field: &str, value: &str, id: &str) -> AppError {
    AppError::message(
        "route_pool.invalid_data",
        format!("invalid {field} '{value}' (id={id})"),
    )
}

fn invalid_value(field: &str, value: &str, id: &str) -> AppError {
    AppError::message(
        "route_pool.invalid_data",
        format!("invalid {field} '{value}' (id={id})"),
    )
}

fn to_sql_error(error: AppError) -> rusqlite::Error {
    rusqlite::Error::ToSqlConversionFailure(Box::new(error))
}

fn is_constraint(error: &rusqlite::Error) -> bool {
    matches!(
        error,
        rusqlite::Error::SqliteFailure(code, _) if code.code == ErrorCode::ConstraintViolation
    )
}

fn map_pool_constraint(error: rusqlite::Error) -> AppError {
    if is_constraint(&error) {
        AppError::InvalidArg(
            "route pool default or hub token uniqueness constraint violated".into(),
        )
    } else {
        AppError::from(error)
    }
}

fn map_member_constraint(error: rusqlite::Error) -> AppError {
    if is_constraint(&error) {
        AppError::InvalidArg("duplicate route member authorization fingerprint".into())
    } else {
        AppError::from(error)
    }
}

fn map_rule_constraint(error: rusqlite::Error) -> AppError {
    if is_constraint(&error) {
        AppError::InvalidArg(
            "duplicate model route rule for the same pool, public model, endpoint, provider, and upstream model".into(),
        )
    } else {
        AppError::from(error)
    }
}
