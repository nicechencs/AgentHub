//! Provider table repository — storage boundary only (no business rules).

use rusqlite::{params, Connection, OptionalExtension, Row, Transaction, TransactionBehavior};

use crate::error::{AppError, Result};
use crate::models::{AgentId, Provider};
use crate::storage::Database;

/// SQLite access for the `providers` table.
#[derive(Clone)]
pub struct ProviderRepo {
    db: Database,
}

impl ProviderRepo {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// Insert a new provider row.
    ///
    /// - Duplicate primary key → [`AppError::InvalidArg`]
    /// - When `is_current`, clears other currents for the same agent in one
    ///   `BEGIN IMMEDIATE` transaction.
    pub fn create(&self, provider: &Provider) -> Result<Provider> {
        self.mutate(|conn| {
            if get_by_id_conn(conn, &provider.id)?.is_some() {
                return Err(AppError::InvalidArg(format!(
                    "provider already exists: {}",
                    provider.id
                )));
            }
            insert_row(conn, provider)?;
            clear_other_currents_if_needed(conn, provider)?;
            get_by_id_conn(conn, &provider.id)?
                .ok_or_else(|| AppError::message("db.provider", "provider missing after create"))
        })
    }

    /// Update an existing provider by id.
    ///
    /// - Missing → [`AppError::NotFound`]
    /// - `agent_id` change → [`AppError::InvalidArg`]
    /// - Preserves stored `created_at`
    /// - When `is_current`, clears other currents for the same agent
    pub fn update(&self, provider: &Provider) -> Result<Provider> {
        self.mutate(|conn| {
            let existing = get_by_id_conn(conn, &provider.id)?.ok_or_else(|| {
                AppError::NotFound(format!("provider not found: {}", provider.id))
            })?;
            if existing.agent_id != provider.agent_id {
                return Err(AppError::InvalidArg(format!(
                    "provider agent_id is immutable (id={}, existing={}, requested={})",
                    provider.id,
                    existing.agent_id.as_str(),
                    provider.agent_id.as_str()
                )));
            }
            let mut row = provider.clone();
            row.created_at = existing.created_at;
            update_row(conn, &row)?;
            clear_other_currents_if_needed(conn, &row)?;
            get_by_id_conn(conn, &row.id)?
                .ok_or_else(|| AppError::message("db.provider", "provider missing after update"))
        })
    }

    /// Persist healed `meta` (ticket surface) without allowing a stale caller
    /// to overwrite `is_current`, `settings_config`, or `name`.
    ///
    /// Narrower than [`AccountRepo::update_healed_fields`]: only `meta` and
    /// `updated_at` are written. The expected `updated_at` is an optimistic
    /// concurrency token.
    pub fn update_healed_fields(
        &self,
        provider: &Provider,
        expected_updated_at: &str,
        updated_at: &str,
    ) -> Result<Provider> {
        self.mutate(|conn| {
            let meta = serde_json::to_string(&provider.meta)?;
            let changed = conn.execute(
                r#"
                UPDATE providers SET
                    meta = ?2,
                    updated_at = ?3
                WHERE id = ?1 AND agent_id = ?4 AND updated_at = ?5
                "#,
                params![
                    provider.id,
                    meta,
                    updated_at,
                    provider.agent_id.as_str(),
                    expected_updated_at,
                ],
            )?;
            if changed != 1 {
                return Err(AppError::message(
                    "provider.conflict",
                    format!("provider changed before field update: {}", provider.id),
                ));
            }
            get_by_id_conn(conn, &provider.id)?.ok_or_else(|| {
                AppError::message("db.provider", "provider missing after field update")
            })
        })
    }

    /// Insert or update a full provider row.
    ///
    /// Existing-row path preserves `created_at` and rejects `agent_id` changes.
    /// When `is_current`, clears other currents for the same agent in one
    /// `BEGIN IMMEDIATE` transaction.
    ///
    /// Returns the stored [`Provider`]. Usable by existing tests that seed rows.
    pub fn upsert(&self, provider: &Provider) -> Result<Provider> {
        self.mutate(|conn| {
            if let Some(existing) = get_by_id_conn(conn, &provider.id)? {
                if existing.agent_id != provider.agent_id {
                    return Err(AppError::InvalidArg(format!(
                        "provider agent_id is immutable (id={}, existing={}, requested={})",
                        provider.id,
                        existing.agent_id.as_str(),
                        provider.agent_id.as_str()
                    )));
                }
                let mut row = provider.clone();
                row.created_at = existing.created_at;
                update_row(conn, &row)?;
                clear_other_currents_if_needed(conn, &row)?;
            } else {
                insert_row(conn, provider)?;
                clear_other_currents_if_needed(conn, provider)?;
            }
            get_by_id_conn(conn, &provider.id)?
                .ok_or_else(|| AppError::message("db.provider", "provider missing after upsert"))
        })
    }

    /// Delete by primary key. Missing → [`AppError::NotFound`].
    pub fn delete(&self, id: &str) -> Result<()> {
        self.mutate(|conn| {
            let n = conn.execute("DELETE FROM providers WHERE id = ?1", params![id])?;
            if n == 0 {
                return Err(AppError::NotFound(format!("provider not found: {id}")));
            }
            Ok(())
        })
    }

    /// Delete by primary key only when it belongs to `agent`.
    ///
    /// A missing id and an id owned by another agent are both reported as
    /// not-found so callers cannot accidentally delete across agent scopes.
    pub fn delete_for_agent(&self, id: &str, agent: AgentId) -> Result<()> {
        self.mutate(|conn| {
            let n = conn.execute(
                "DELETE FROM providers WHERE id = ?1 AND agent_id = ?2",
                params![id, agent.as_str()],
            )?;
            if n == 0 {
                return Err(AppError::NotFound(format!(
                    "provider not found: {id} (agent filter: {})",
                    agent.as_str()
                )));
            }
            Ok(())
        })
    }

    /// List all providers, optionally filtered by agent. Order is stable by
    /// `agent_id`, `name`, `id` (service may re-sort to product agent order).
    pub fn list(&self, agent: Option<AgentId>) -> Result<Vec<Provider>> {
        self.db.with_conn(|conn| {
            let mut out = Vec::new();
            if let Some(agent) = agent {
                let mut stmt = conn.prepare(
                    r#"
                    SELECT id, agent_id, name, settings_config, meta,
                           is_current, created_at, updated_at
                    FROM providers
                    WHERE agent_id = ?1
                    ORDER BY agent_id ASC, name ASC, id ASC
                    "#,
                )?;
                let rows = stmt.query_map(params![agent.as_str()], map_provider_row)?;
                for row in rows {
                    out.push(row?);
                }
            } else {
                let mut stmt = conn.prepare(
                    r#"
                    SELECT id, agent_id, name, settings_config, meta,
                           is_current, created_at, updated_at
                    FROM providers
                    ORDER BY agent_id ASC, name ASC, id ASC
                    "#,
                )?;
                let rows = stmt.query_map([], map_provider_row)?;
                for row in rows {
                    out.push(row?);
                }
            }
            Ok(out)
        })
    }

    pub fn get_by_id(&self, id: &str) -> Result<Option<Provider>> {
        self.db.with_conn(|conn| get_by_id_conn(conn, id))
    }

    /// Return the sole current provider for `agent`.
    ///
    /// Legacy/corrupt databases with multiple current rows fail closed rather
    /// than choosing an arbitrary row for backfill.
    pub fn get_current(&self, agent: AgentId) -> Result<Option<Provider>> {
        self.db.with_conn(|conn| get_current_conn(conn, agent))
    }

    /// Clear `is_current` for every provider of `agent` (cross-pool demotion).
    ///
    /// Used when an account becomes the active live auth path so official
    /// account and API config stay mutually exclusive at the DB layer.
    pub fn clear_current(&self, agent: AgentId) -> Result<()> {
        self.mutate(|conn| {
            conn.execute(
                "UPDATE providers SET is_current = 0 WHERE agent_id = ?1 AND is_current != 0",
                params![agent.as_str()],
            )?;
            Ok(())
        })
    }

    /// Persist the complete live config into the observed current provider.
    pub fn backfill_current(
        &self,
        observed: &Provider,
        raw: &serde_json::Value,
        updated_at: &str,
    ) -> Result<Provider> {
        self.mutate(|conn| {
            let current = get_current_conn(conn, observed.agent_id)?.ok_or_else(|| {
                AppError::message("provider.state", "current provider changed before backfill")
            })?;
            if current.id != observed.id || current.updated_at != observed.updated_at {
                return Err(AppError::message(
                    "provider.state",
                    "current provider changed before backfill",
                ));
            }
            let raw = serde_json::to_string(raw)?;
            let changed = conn.execute(
                r#"
                UPDATE providers
                SET settings_config = ?2, updated_at = ?3
                WHERE id = ?1 AND agent_id = ?4 AND is_current != 0
                  AND updated_at = ?5
                "#,
                params![
                    observed.id,
                    raw,
                    updated_at,
                    observed.agent_id.as_str(),
                    observed.updated_at,
                ],
            )?;
            if changed != 1 {
                return Err(AppError::message(
                    "provider.state",
                    "current provider changed during backfill",
                ));
            }
            get_by_id_conn(conn, &observed.id)?.ok_or_else(|| {
                AppError::message("db.provider", "provider missing after live backfill")
            })
        })
    }

    /// Restore the exact row changed by a successful live backfill.
    pub fn restore_backfill(
        &self,
        original: &Provider,
        expected_backfilled_updated_at: &str,
    ) -> Result<()> {
        self.mutate(|conn| {
            let current = get_current_conn(conn, original.agent_id)?.ok_or_else(|| {
                AppError::message(
                    "provider.state",
                    "current provider changed before backfill rollback",
                )
            })?;
            if current.id != original.id || current.updated_at != expected_backfilled_updated_at {
                return Err(AppError::message(
                    "provider.state",
                    "current provider changed before backfill rollback",
                ));
            }
            let raw = serde_json::to_string(&original.settings_config)?;
            let changed = conn.execute(
                r#"
                UPDATE providers
                SET settings_config = ?2, updated_at = ?3
                WHERE id = ?1 AND agent_id = ?4 AND is_current != 0
                  AND updated_at = ?5
                "#,
                params![
                    original.id,
                    raw,
                    original.updated_at,
                    original.agent_id.as_str(),
                    expected_backfilled_updated_at,
                ],
            )?;
            if changed != 1 {
                return Err(AppError::message(
                    "provider.state",
                    "provider changed during backfill rollback",
                ));
            }
            Ok(())
        })
    }

    /// Atomically select a provider after backup and live apply succeed.
    pub fn select_current(
        &self,
        target_id: &str,
        agent: AgentId,
        expected_target_updated_at: &str,
        updated_at: &str,
    ) -> Result<Provider> {
        self.mutate(|conn| {
            select_current_conn(
                conn,
                target_id,
                agent,
                expected_target_updated_at,
                updated_at,
            )
        })
    }

    /// Atomically backfill the old current provider and select `target_id`.
    ///
    /// The target must belong to `agent`. `backfill`, when present, must name
    /// the current provider observed by the service immediately before the
    /// live write. All DB changes happen in one `BEGIN IMMEDIATE` transaction.
    pub fn switch_current(
        &self,
        target_id: &str,
        agent: AgentId,
        expected_target_updated_at: &str,
        backfill: Option<(&str, &str, &serde_json::Value)>,
        updated_at: &str,
    ) -> Result<Provider> {
        self.mutate(|conn| {
            let target = get_by_id_conn(conn, target_id)?
                .ok_or_else(|| AppError::NotFound(format!("provider not found: {target_id}")))?;
            if target.agent_id != agent {
                return Err(AppError::NotFound(format!(
                    "provider not found: {target_id} (agent filter: {})",
                    agent.as_str()
                )));
            }
            if target.updated_at != expected_target_updated_at {
                return Err(AppError::message(
                    "provider.state",
                    format!("provider changed before switch: {target_id}"),
                ));
            }

            let current = get_current_conn(conn, agent)?;
            if let Some((backfill_id, expected_current_updated_at, raw)) = backfill {
                let current = current.as_ref().ok_or_else(|| {
                    AppError::message(
                        "provider.state",
                        format!(
                            "current provider changed before switch for agent {}",
                            agent.as_str()
                        ),
                    )
                })?;
                if current.id != backfill_id || current.updated_at != expected_current_updated_at {
                    return Err(AppError::message(
                        "provider.state",
                        format!(
                            "current provider changed before switch for agent {}",
                            agent.as_str()
                        ),
                    ));
                }
                let raw = serde_json::to_string(raw)?;
                conn.execute(
                    r#"
                    UPDATE providers
                    SET settings_config = ?2, updated_at = ?3
                    WHERE id = ?1 AND agent_id = ?4 AND is_current != 0
                    "#,
                    params![backfill_id, raw, updated_at, agent.as_str()],
                )?;
            }

            conn.execute(
                r#"
                UPDATE providers
                SET is_current = 0, updated_at = ?2
                WHERE agent_id = ?1 AND is_current != 0
                "#,
                params![agent.as_str(), updated_at],
            )?;
            let changed = conn.execute(
                r#"
                UPDATE providers
                SET is_current = 1, updated_at = ?3
                WHERE id = ?1 AND agent_id = ?2
                "#,
                params![target_id, agent.as_str(), updated_at],
            )?;
            if changed != 1 {
                return Err(AppError::message(
                    "db.provider",
                    format!("provider missing during switch: {target_id}"),
                ));
            }

            get_by_id_conn(conn, target_id)?
                .ok_or_else(|| AppError::message("db.provider", "provider missing after switch"))
        })
    }

    /// Exact name match, optionally scoped to an agent.
    pub fn list_by_name(&self, name: &str, agent: Option<AgentId>) -> Result<Vec<Provider>> {
        self.db.with_conn(|conn| {
            let mut out = Vec::new();
            if let Some(agent) = agent {
                let mut stmt = conn.prepare(
                    r#"
                    SELECT id, agent_id, name, settings_config, meta,
                           is_current, created_at, updated_at
                    FROM providers
                    WHERE name = ?1 AND agent_id = ?2
                    ORDER BY agent_id ASC, name ASC, id ASC
                    "#,
                )?;
                let rows = stmt.query_map(params![name, agent.as_str()], map_provider_row)?;
                for row in rows {
                    out.push(row?);
                }
            } else {
                let mut stmt = conn.prepare(
                    r#"
                    SELECT id, agent_id, name, settings_config, meta,
                           is_current, created_at, updated_at
                    FROM providers
                    WHERE name = ?1
                    ORDER BY agent_id ASC, name ASC, id ASC
                    "#,
                )?;
                let rows = stmt.query_map(params![name], map_provider_row)?;
                for row in rows {
                    out.push(row?);
                }
            }
            Ok(out)
        })
    }

    /// Run `f` inside a single `BEGIN IMMEDIATE` transaction; roll back on error.
    ///
    /// Uses [`Transaction::new_unchecked`] because [`Database::with_conn`] only
    /// yields `&Connection` (not `&mut Connection`).
    fn mutate<T, F>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&Connection) -> Result<T>,
    {
        self.db.with_conn(|conn| {
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
            // On `f` error, `tx` drops and rolls back (default DropBehavior).
            let out = f(&tx)?;
            tx.commit()?;
            Ok(out)
        })
    }
}

fn get_by_id_conn(conn: &Connection, id: &str) -> Result<Option<Provider>> {
    conn.query_row(
        r#"
        SELECT id, agent_id, name, settings_config, meta,
               is_current, created_at, updated_at
        FROM providers
        WHERE id = ?1
        "#,
        params![id],
        map_provider_row,
    )
    .optional()
    .map_err(AppError::from)
}

fn get_current_conn(conn: &Connection, agent: AgentId) -> Result<Option<Provider>> {
    let mut stmt = conn.prepare(
        r#"
        SELECT id, agent_id, name, settings_config, meta,
               is_current, created_at, updated_at
        FROM providers
        WHERE agent_id = ?1 AND is_current != 0
        ORDER BY id ASC
        "#,
    )?;
    let mut rows = stmt.query(params![agent.as_str()])?;
    let first = rows.next()?.map(map_provider_row).transpose()?;
    if rows.next()?.is_some() {
        return Err(AppError::message(
            "provider.state",
            format!(
                "multiple current providers found for agent {}",
                agent.as_str()
            ),
        ));
    }
    Ok(first)
}

/// Clear currents (connection-scoped) for multi-table transactions.
pub(crate) fn clear_current_conn(conn: &Connection, agent: AgentId) -> Result<()> {
    conn.execute(
        "UPDATE providers SET is_current = 0 WHERE agent_id = ?1 AND is_current != 0",
        params![agent.as_str()],
    )?;
    Ok(())
}

/// Connection-scoped get by id.
pub(crate) fn get_by_id_conn_pub(conn: &Connection, id: &str) -> Result<Option<Provider>> {
    get_by_id_conn(conn, id)
}

/// Force one provider to be the sole current for `agent` (no expected-revision check).
///
/// Used by active-binding reconcile so legacy `is_current` mirrors a valid binding.
pub(crate) fn force_sole_current_conn(
    conn: &Connection,
    target_id: &str,
    agent: AgentId,
    updated_at: &str,
) -> Result<Provider> {
    let target = get_by_id_conn(conn, target_id)?
        .ok_or_else(|| AppError::NotFound(format!("provider not found: {target_id}")))?;
    if target.agent_id != agent {
        return Err(AppError::NotFound(format!(
            "provider not found: {target_id} (agent filter: {})",
            agent.as_str()
        )));
    }
    conn.execute(
        "UPDATE providers SET is_current = 0, updated_at = ?2 WHERE agent_id = ?1 AND is_current != 0",
        params![agent.as_str(), updated_at],
    )?;
    let changed = conn.execute(
        "UPDATE providers SET is_current = 1, updated_at = ?3 WHERE id = ?1 AND agent_id = ?2",
        params![target_id, agent.as_str(), updated_at],
    )?;
    if changed != 1 {
        return Err(AppError::message(
            "db.provider",
            format!("provider missing while enforcing current: {target_id}"),
        ));
    }
    get_by_id_conn(conn, target_id)?
        .ok_or_else(|| AppError::message("db.provider", "provider missing after enforce current"))
}

/// Connection-scoped list of rows with `is_current != 0` for an agent.
pub(crate) fn list_current_conn(conn: &Connection, agent: AgentId) -> Result<Vec<Provider>> {
    let mut stmt = conn.prepare(
        r#"
        SELECT id, agent_id, name, settings_config, meta,
               is_current, created_at, updated_at
        FROM providers
        WHERE agent_id = ?1 AND is_current != 0
        "#,
    )?;
    let rows = stmt.query_map(params![agent.as_str()], map_provider_row)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

/// Connection-scoped create (insert + same-table current demotion).
pub(crate) fn create_conn(conn: &Connection, provider: &Provider) -> Result<Provider> {
    if get_by_id_conn(conn, &provider.id)?.is_some() {
        return Err(AppError::InvalidArg(format!(
            "provider already exists: {}",
            provider.id
        )));
    }
    insert_row(conn, provider)?;
    clear_other_currents_if_needed(conn, provider)?;
    get_by_id_conn(conn, &provider.id)?
        .ok_or_else(|| AppError::message("db.provider", "provider missing after create"))
}

/// Connection-scoped update (preserve created_at + same-table current demotion).
pub(crate) fn update_conn(conn: &Connection, provider: &Provider) -> Result<Provider> {
    let existing = get_by_id_conn(conn, &provider.id)?
        .ok_or_else(|| AppError::NotFound(format!("provider not found: {}", provider.id)))?;
    if existing.agent_id != provider.agent_id {
        return Err(AppError::InvalidArg(format!(
            "provider agent_id is immutable (id={}, existing={}, requested={})",
            provider.id,
            existing.agent_id.as_str(),
            provider.agent_id.as_str()
        )));
    }
    let mut row = provider.clone();
    row.created_at = existing.created_at;
    update_row(conn, &row)?;
    clear_other_currents_if_needed(conn, &row)?;
    get_by_id_conn(conn, &row.id)?
        .ok_or_else(|| AppError::message("db.provider", "provider missing after update"))
}

/// Connection-scoped upsert (insert or update + same-table current demotion).
pub(crate) fn upsert_conn(conn: &Connection, provider: &Provider) -> Result<Provider> {
    if let Some(existing) = get_by_id_conn(conn, &provider.id)? {
        if existing.agent_id != provider.agent_id {
            return Err(AppError::InvalidArg(format!(
                "provider agent_id is immutable (id={}, existing={}, requested={})",
                provider.id,
                existing.agent_id.as_str(),
                provider.agent_id.as_str()
            )));
        }
        let mut row = provider.clone();
        row.created_at = existing.created_at;
        update_row(conn, &row)?;
        clear_other_currents_if_needed(conn, &row)?;
    } else {
        insert_row(conn, provider)?;
        clear_other_currents_if_needed(conn, provider)?;
    }
    get_by_id_conn(conn, &provider.id)?
        .ok_or_else(|| AppError::message("db.provider", "provider missing after upsert"))
}

/// Connection-scoped delete scoped to agent.
pub(crate) fn delete_for_agent_conn(conn: &Connection, id: &str, agent: AgentId) -> Result<()> {
    let n = conn.execute(
        "DELETE FROM providers WHERE id = ?1 AND agent_id = ?2",
        params![id, agent.as_str()],
    )?;
    if n == 0 {
        return Err(AppError::NotFound(format!(
            "provider not found: {id} (agent filter: {})",
            agent.as_str()
        )));
    }
    Ok(())
}

/// Connection-scoped select_current for multi-table transactions.
pub(crate) fn select_current_conn(
    conn: &Connection,
    target_id: &str,
    agent: AgentId,
    expected_target_updated_at: &str,
    updated_at: &str,
) -> Result<Provider> {
    let target = get_by_id_conn(conn, target_id)?
        .ok_or_else(|| AppError::NotFound(format!("provider not found: {target_id}")))?;
    if target.agent_id != agent {
        return Err(AppError::NotFound(format!(
            "provider not found: {target_id} (agent filter: {})",
            agent.as_str()
        )));
    }
    if target.updated_at != expected_target_updated_at {
        return Err(AppError::message(
            "provider.state",
            format!("provider changed before switch: {target_id}"),
        ));
    }
    let _ = get_current_conn(conn, agent)?;
    conn.execute(
        "UPDATE providers SET is_current = 0, updated_at = ?2 WHERE agent_id = ?1 AND is_current != 0",
        params![agent.as_str(), updated_at],
    )?;
    let changed = conn.execute(
        "UPDATE providers SET is_current = 1, updated_at = ?3 WHERE id = ?1 AND agent_id = ?2",
        params![target_id, agent.as_str(), updated_at],
    )?;
    if changed != 1 {
        return Err(AppError::message(
            "db.provider",
            format!("provider missing during switch: {target_id}"),
        ));
    }
    get_by_id_conn(conn, target_id)?
        .ok_or_else(|| AppError::message("db.provider", "provider missing after switch"))
}

fn insert_row(conn: &Connection, provider: &Provider) -> Result<()> {
    let settings = serde_json::to_string(&provider.settings_config)?;
    let meta = serde_json::to_string(&provider.meta)?;
    let is_current: i64 = if provider.is_current { 1 } else { 0 };
    conn.execute(
        r#"
        INSERT INTO providers (
            id, agent_id, name, settings_config, meta,
            is_current, created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
        "#,
        params![
            provider.id,
            provider.agent_id.as_str(),
            provider.name,
            settings,
            meta,
            is_current,
            provider.created_at,
            provider.updated_at,
        ],
    )?;
    Ok(())
}

fn update_row(conn: &Connection, provider: &Provider) -> Result<()> {
    let settings = serde_json::to_string(&provider.settings_config)?;
    let meta = serde_json::to_string(&provider.meta)?;
    let is_current: i64 = if provider.is_current { 1 } else { 0 };
    let n = conn.execute(
        r#"
        UPDATE providers SET
            agent_id = ?2,
            name = ?3,
            settings_config = ?4,
            meta = ?5,
            is_current = ?6,
            created_at = ?7,
            updated_at = ?8
        WHERE id = ?1
        "#,
        params![
            provider.id,
            provider.agent_id.as_str(),
            provider.name,
            settings,
            meta,
            is_current,
            provider.created_at,
            provider.updated_at,
        ],
    )?;
    if n == 0 {
        return Err(AppError::NotFound(format!(
            "provider not found: {}",
            provider.id
        )));
    }
    Ok(())
}

/// Ensure at most one `is_current` provider per agent when this row is current.
fn clear_other_currents_if_needed(conn: &Connection, provider: &Provider) -> Result<()> {
    if !provider.is_current {
        return Ok(());
    }
    conn.execute(
        r#"
        UPDATE providers
        SET is_current = 0
        WHERE agent_id = ?1 AND id != ?2 AND is_current != 0
        "#,
        params![provider.agent_id.as_str(), provider.id],
    )?;
    Ok(())
}

fn map_provider_row(row: &Row<'_>) -> rusqlite::Result<Provider> {
    let id: String = row.get(0)?;
    let agent_raw: String = row.get(1)?;
    let agent_id = AgentId::parse(&agent_raw).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(
            1,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("invalid agent_id in providers row: {agent_raw}"),
            )),
        )
    })?;
    let name: String = row.get(2)?;
    let settings_raw: String = row.get(3)?;
    let meta_raw: String = row.get(4)?;
    let is_current_i: i64 = row.get(5)?;
    let created_at: String = row.get(6)?;
    let updated_at: String = row.get(7)?;

    let settings_config = serde_json::from_str(&settings_raw).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(3, rusqlite::types::Type::Text, Box::new(e))
    })?;
    let meta = serde_json::from_str(&meta_raw).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(4, rusqlite::types::Type::Text, Box::new(e))
    })?;

    Ok(Provider {
        id,
        agent_id,
        name,
        settings_config,
        meta,
        is_current: is_current_i != 0,
        created_at,
        updated_at,
    })
}

#[cfg(test)]
mod tests;
