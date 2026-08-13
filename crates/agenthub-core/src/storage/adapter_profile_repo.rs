//! Credential-free storage for persisted adapter profiles.

use rusqlite::{params, Connection, OptionalExtension, Row, Transaction, TransactionBehavior};

use crate::error::{AppError, Result};
use crate::models::{
    AdapterProfile, AdapterProfileFilter, AdapterProfileMode, AdapterProfileStatus, AdapterRoute,
    AdapterSourceKind, AgentId,
};
use crate::storage::Database;

/// SQLite access for the `adapter_profiles` table.
#[derive(Clone)]
pub struct AdapterProfileRepo {
    db: Database,
}

impl AdapterProfileRepo {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// Create a profile, or return the row already stored for its stable id.
    ///
    /// The supplied id is a caller-owned idempotency key. A duplicate id never
    /// overwrites the original row; use [`Self::update`] for deliberate changes.
    pub fn create(&self, profile: &AdapterProfile) -> Result<AdapterProfile> {
        self.create_or_get(profile)
    }

    pub fn create_or_get(&self, profile: &AdapterProfile) -> Result<AdapterProfile> {
        validate_profile(profile)?;
        self.mutate(|conn| {
            if let Some(existing) = get_conn(conn, &profile.id)? {
                return Ok(existing);
            }
            insert_conn(conn, profile)?;
            get_conn(conn, &profile.id)?.ok_or_else(|| {
                AppError::message("db.adapter_profile", "profile missing after create")
            })
        })
    }

    pub fn get(&self, id: &str) -> Result<Option<AdapterProfile>> {
        self.db.with_conn(|conn| get_conn(conn, id))
    }

    /// Lists profiles in a deterministic source/target/name/id order.
    pub fn list(
        &self,
        source_kind: Option<AdapterSourceKind>,
        source_id: Option<&str>,
        target_agent_id: Option<AgentId>,
    ) -> Result<Vec<AdapterProfile>> {
        self.list_filtered(&AdapterProfileFilter {
            source_kind,
            source_id: source_id.map(str::to_owned),
            target_agent_id,
            ..AdapterProfileFilter::default()
        })
    }

    /// Lists profiles using all persisted, typed filters.
    ///
    /// Kept alongside the legacy three-field `list` signature so existing
    /// callers do not need a flag-day migration while bridge restoration can
    /// request only active, auto-start local profiles.
    pub fn list_filtered(&self, filter: &AdapterProfileFilter) -> Result<Vec<AdapterProfile>> {
        self.db.with_conn(|conn| {
            let mut stmt = conn.prepare(
                r#"
                SELECT id, name, source_kind, source_id, target_agent_id, route, mode,
                       status, rule_id, rule_version, generated_provider_id,
                       local_port, auto_start, last_error_code, created_at, updated_at
                FROM adapter_profiles
                WHERE (?1 IS NULL OR source_kind = ?1)
                  AND (?2 IS NULL OR source_id = ?2)
                  AND (?3 IS NULL OR target_agent_id = ?3)
                  AND (?4 IS NULL OR route = ?4)
                  AND (?5 IS NULL OR status = ?5)
                  AND (?6 IS NULL OR auto_start = ?6)
                  AND (?7 IS NULL OR mode = ?7)
                ORDER BY source_kind ASC, source_id ASC, target_agent_id ASC, name ASC, id ASC
                "#,
            )?;
            let rows = stmt.query_map(
                params![
                    filter.source_kind.map(AdapterSourceKind::as_str),
                    filter.source_id.as_deref(),
                    filter.target_agent_id.map(AgentId::as_str),
                    filter.route.map(AdapterRoute::as_str),
                    filter.status.map(AdapterProfileStatus::as_str),
                    filter.auto_start.map(i64::from),
                    filter.mode.map(AdapterProfileMode::as_str),
                ],
                map_raw_row,
            )?;
            rows.map(|row| row.and_then(|raw| raw.into_profile().map_err(to_sql_error)))
                .collect::<std::result::Result<Vec<_>, _>>()
                .map_err(AppError::from)
        })
    }

    /// Updates an existing profile while preserving the original creation time.
    pub fn update(&self, profile: &AdapterProfile) -> Result<AdapterProfile> {
        validate_profile(profile)?;
        self.mutate(|conn| {
            let existing = get_conn(conn, &profile.id)?.ok_or_else(|| {
                AppError::NotFound(format!("adapter profile not found: {}", profile.id))
            })?;
            let mut stored = profile.clone();
            stored.created_at = existing.created_at;
            let changed = conn.execute(
                r#"
                UPDATE adapter_profiles
                SET name = ?2, source_kind = ?3, source_id = ?4, target_agent_id = ?5,
                    route = ?6, mode = ?7, status = ?8, rule_id = ?9, rule_version = ?10,
                    generated_provider_id = ?11, local_port = ?12, auto_start = ?13,
                    last_error_code = ?14, created_at = ?15, updated_at = ?16
                WHERE id = ?1
                "#,
                params![
                    stored.id,
                    stored.name,
                    stored.source_kind.as_str(),
                    stored.source_id,
                    stored.target_agent_id.as_str(),
                    stored.route.as_str(),
                    stored.mode.as_str(),
                    stored.status.as_str(),
                    stored.rule_id,
                    stored.rule_version,
                    stored.generated_provider_id,
                    stored.local_port.map(i64::from),
                    i64::from(stored.auto_start),
                    stored.last_error_code,
                    stored.created_at,
                    stored.updated_at,
                ],
            )?;
            if changed != 1 {
                return Err(AppError::message(
                    "db.adapter_profile",
                    "profile missing during update",
                ));
            }
            get_conn(conn, &profile.id)?.ok_or_else(|| {
                AppError::message("db.adapter_profile", "profile missing after update")
            })
        })
    }

    pub fn delete(&self, id: &str) -> Result<()> {
        self.mutate(|conn| {
            if conn.execute("DELETE FROM adapter_profiles WHERE id = ?1", params![id])? == 0 {
                return Err(AppError::NotFound(format!(
                    "adapter profile not found: {id}"
                )));
            }
            Ok(())
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

fn validate_profile(profile: &AdapterProfile) -> Result<()> {
    for (field, value) in [
        ("id", profile.id.as_str()),
        ("name", profile.name.as_str()),
        ("source_id", profile.source_id.as_str()),
        ("rule_id", profile.rule_id.as_str()),
        ("rule_version", profile.rule_version.as_str()),
        ("created_at", profile.created_at.as_str()),
        ("updated_at", profile.updated_at.as_str()),
    ] {
        if value.trim().is_empty() {
            return Err(AppError::InvalidArg(format!(
                "adapter profile {field} must not be empty"
            )));
        }
    }
    if !profile.route.is_profile_supported() {
        return Err(AppError::InvalidArg(
            "unsupported adapter route cannot be persisted as a profile".into(),
        ));
    }
    if profile.route != AdapterRoute::LocalBridge
        && (profile.local_port.is_some() || profile.auto_start)
    {
        return Err(AppError::InvalidArg(
            "adapter profile local_port and auto_start are only valid for a local_bridge route"
                .into(),
        ));
    }
    Ok(())
}

fn insert_conn(conn: &Connection, profile: &AdapterProfile) -> Result<()> {
    conn.execute(
        r#"
        INSERT INTO adapter_profiles (
            id, name, source_kind, source_id, target_agent_id, route, mode, status,
            rule_id, rule_version, generated_provider_id, last_error_code,
            local_port, auto_start, created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
        "#,
        params![
            profile.id,
            profile.name,
            profile.source_kind.as_str(),
            profile.source_id,
            profile.target_agent_id.as_str(),
            profile.route.as_str(),
            profile.mode.as_str(),
            profile.status.as_str(),
            profile.rule_id,
            profile.rule_version,
            profile.generated_provider_id,
            profile.last_error_code,
            profile.local_port.map(i64::from),
            i64::from(profile.auto_start),
            profile.created_at,
            profile.updated_at,
        ],
    )?;
    Ok(())
}

fn get_conn(conn: &Connection, id: &str) -> Result<Option<AdapterProfile>> {
    conn.query_row(
        r#"
        SELECT id, name, source_kind, source_id, target_agent_id, route, mode,
               status, rule_id, rule_version, generated_provider_id,
               local_port, auto_start, last_error_code, created_at, updated_at
        FROM adapter_profiles WHERE id = ?1
        "#,
        params![id],
        map_raw_row,
    )
    .optional()?
    .map(RawAdapterProfile::into_profile)
    .transpose()
}

#[derive(Debug)]
struct RawAdapterProfile {
    id: String,
    name: String,
    source_kind: String,
    source_id: String,
    target_agent_id: String,
    route: String,
    mode: String,
    status: String,
    rule_id: String,
    rule_version: String,
    generated_provider_id: Option<String>,
    local_port: Option<i64>,
    auto_start: i64,
    last_error_code: Option<String>,
    created_at: String,
    updated_at: String,
}

impl RawAdapterProfile {
    fn into_profile(self) -> Result<AdapterProfile> {
        let source_kind = parse_stored_source_kind(&self.source_kind)
            .ok_or_else(|| invalid_enum("source_kind", &self.source_kind, &self.id))?;
        let target_agent_id = parse_stored_agent_id(&self.target_agent_id)
            .ok_or_else(|| invalid_enum("target_agent_id", &self.target_agent_id, &self.id))?;
        let route = parse_stored_route(&self.route)
            .ok_or_else(|| invalid_enum("route", &self.route, &self.id))?;
        let mode = parse_stored_mode(&self.mode)
            .ok_or_else(|| invalid_enum("mode", &self.mode, &self.id))?;
        let status = parse_stored_status(&self.status)
            .ok_or_else(|| invalid_enum("status", &self.status, &self.id))?;
        let local_port = self
            .local_port
            .map(|port| {
                u16::try_from(port)
                    .ok()
                    .filter(|port| *port != 0)
                    .ok_or_else(|| invalid_value("local_port", &port.to_string(), &self.id))
            })
            .transpose()?;
        let auto_start = parse_stored_bool(self.auto_start)
            .ok_or_else(|| invalid_value("auto_start", &self.auto_start.to_string(), &self.id))?;
        Ok(AdapterProfile {
            id: self.id,
            name: self.name,
            source_kind,
            source_id: self.source_id,
            target_agent_id,
            route,
            mode,
            status,
            rule_id: self.rule_id,
            rule_version: self.rule_version,
            generated_provider_id: self.generated_provider_id,
            local_port,
            auto_start,
            last_error_code: self.last_error_code,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

fn parse_stored_source_kind(value: &str) -> Option<AdapterSourceKind> {
    match value {
        "account" => Some(AdapterSourceKind::Account),
        "provider" => Some(AdapterSourceKind::Provider),
        _ => None,
    }
}

fn parse_stored_agent_id(value: &str) -> Option<AgentId> {
    AgentId::ALL
        .into_iter()
        .find(|agent| agent.as_str() == value)
}

fn parse_stored_route(value: &str) -> Option<AdapterRoute> {
    match value {
        "config_sync" => Some(AdapterRoute::ConfigSync),
        "native_endpoint" => Some(AdapterRoute::NativeEndpoint),
        "local_bridge" => Some(AdapterRoute::LocalBridge),
        _ => None,
    }
}

fn parse_stored_mode(value: &str) -> Option<AdapterProfileMode> {
    match value {
        "api" => Some(AdapterProfileMode::Api),
        "oauth" => Some(AdapterProfileMode::Oauth),
        _ => None,
    }
}

fn parse_stored_status(value: &str) -> Option<AdapterProfileStatus> {
    match value {
        "applying" => Some(AdapterProfileStatus::Applying),
        "active" => Some(AdapterProfileStatus::Active),
        "needs_attention" => Some(AdapterProfileStatus::NeedsAttention),
        _ => None,
    }
}

fn map_raw_row(row: &Row<'_>) -> rusqlite::Result<RawAdapterProfile> {
    Ok(RawAdapterProfile {
        id: row.get(0)?,
        name: row.get(1)?,
        source_kind: row.get(2)?,
        source_id: row.get(3)?,
        target_agent_id: row.get(4)?,
        route: row.get(5)?,
        mode: row.get(6)?,
        status: row.get(7)?,
        rule_id: row.get(8)?,
        rule_version: row.get(9)?,
        generated_provider_id: row.get(10)?,
        local_port: row.get(11)?,
        auto_start: row.get(12)?,
        last_error_code: row.get(13)?,
        created_at: row.get(14)?,
        updated_at: row.get(15)?,
    })
}

fn invalid_enum(field: &str, value: &str, id: &str) -> AppError {
    AppError::message(
        "adapter_profile.invalid_data",
        format!("invalid adapter profile {field} '{value}' (id={id})"),
    )
}

fn invalid_value(field: &str, value: &str, id: &str) -> AppError {
    AppError::message(
        "adapter_profile.invalid_data",
        format!("invalid adapter profile {field} '{value}' (id={id})"),
    )
}

fn parse_stored_bool(value: i64) -> Option<bool> {
    match value {
        0 => Some(false),
        1 => Some(true),
        _ => None,
    }
}

fn to_sql_error(error: AppError) -> rusqlite::Error {
    rusqlite::Error::ToSqlConversionFailure(Box::new(error))
}

#[cfg(test)]
mod tests;
