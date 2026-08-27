//! Pool footprint restore after a current-row live apply fails.
//!
//! This is not ConnectionService ownership and is not switch compensation.

use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};

use crate::error::{AppError, Result};
use crate::models::{Account, AgentId, Provider};

use super::pool::{
    get_provider_binding_row, ProviderBindingSnapshot, ProviderCommittedMutation,
    ProviderMutationFootprint,
};
use super::ProviderService;

impl ProviderService {
    pub(super) fn restore_committed_provider_mutation(
        &self,
        agent: AgentId,
        committed: &ProviderCommittedMutation,
    ) -> Result<()> {
        self.restore_provider_rows_with_footprint(
            agent,
            &committed.footprint.before_providers,
            &committed.footprint.after_providers,
            &committed.stored,
            committed.footprint.target_was_new,
            &committed.footprint.affected_provider_ids,
            &committed.footprint,
            &committed.footprint.after_accounts,
            &committed.footprint.after_binding,
        )
    }

    /// Restore only the rows touched by one current update/upsert after the
    /// live write fails. Every restore first compares the complete stored row
    /// (including the surface stamp revision), so unrelated concurrent CRUD
    /// is never overwritten or deleted.
    // Referenced only from provider_service `tests.rs` in this crate; keep for test coverage.
    #[allow(dead_code)]
    pub(super) fn restore_provider_rows(
        &self,
        agent: AgentId,
        before: &[Provider],
        after: &[Provider],
        stored: &Provider,
        target_was_new: bool,
        affected_ids: &[String],
    ) -> Result<()> {
        let footprint = ProviderMutationFootprint {
            affected_provider_ids: affected_ids.to_vec(),
            before_providers: before.to_vec(),
            after_providers: after.to_vec(),
            target_was_new,
            ..ProviderMutationFootprint::default()
        };
        self.restore_provider_rows_with_footprint(
            agent,
            before,
            after,
            stored,
            target_was_new,
            affected_ids,
            &footprint,
            &[],
            &None,
        )
    }

    pub(super) fn restore_provider_rows_with_footprint(
        &self,
        agent: AgentId,
        before: &[Provider],
        _after: &[Provider],
        stored: &Provider,
        target_was_new: bool,
        affected_ids: &[String],
        footprint: &ProviderMutationFootprint,
        _after_accounts: &[Account],
        after_binding: &Option<ProviderBindingSnapshot>,
    ) -> Result<()> {
        if affected_ids.is_empty() {
            return Ok(());
        }

        self.db.with_conn(|conn| {
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
            for id in affected_ids {
                let original = before.iter().find(|row| row.id == *id);

                match original {
                    Some(original) => {
                        let expected = footprint
                            .after_providers
                            .iter()
                            .find(|row| row.id == original.id)
                            .cloned()
                            .unwrap_or_else(|| {
                                if original.id == stored.id {
                                    stored.clone()
                                } else {
                                    let mut demoted = original.clone();
                                    demoted.is_current = false;
                                    demoted
                                }
                            });
                        if original == &expected {
                            continue;
                        }
                        ensure_provider_row_matches(&tx, &expected)?;
                        let settings = serde_json::to_string(&original.settings_config)?;
                        let meta = serde_json::to_string(&original.meta)?;
                        let updated = tx.execute(
                            r#"
                            UPDATE providers SET
                                agent_id = ?2, name = ?3, settings_config = ?4,
                                meta = ?5, is_current = ?6, created_at = ?7,
                                updated_at = ?8
                            WHERE id = ?1 AND agent_id = ?9 AND updated_at = ?10
                            "#,
                            params![
                                &original.id,
                                original.agent_id.as_str(),
                                &original.name,
                                settings,
                                meta,
                                if original.is_current { 1 } else { 0 },
                                &original.created_at,
                                &original.updated_at,
                                agent.as_str(),
                                &expected.updated_at,
                            ],
                        )?;
                        if updated != 1 {
                            return Err(provider_compensation_conflict(&original.id));
                        }
                    }
                    None if target_was_new && id == &stored.id => {
                        // A new upsert may be removed only while its full
                        // post-mutation state still matches this operation.
                        ensure_provider_row_matches(&tx, stored)?;
                        let removed = tx.execute(
                            "DELETE FROM providers WHERE id = ?1 AND agent_id = ?2 AND updated_at = ?3",
                            params![&stored.id, agent.as_str(), &stored.updated_at],
                        )?;
                        if removed != 1 {
                            return Err(provider_compensation_conflict(&stored.id));
                        }
                    }
                    None => {
                        return Err(provider_compensation_conflict(id));
                    }
                }
            }
            let expected_after_binding = if footprint.after_binding.is_some()
                || footprint.before_binding.is_some()
            {
                &footprint.after_binding
            } else {
                after_binding
            };
            let binding_changed =
                footprint.before_binding.as_ref() != expected_after_binding.as_ref();
            if binding_changed && !footprint.before_accounts.is_empty() {
                if footprint.after_accounts.is_empty() {
                    restore_demoted_account_rows(&tx, agent, &footprint.before_accounts)?;
                } else {
                    restore_account_rows_from_provider_footprint(
                        &tx,
                        agent,
                        &footprint.before_accounts,
                        &footprint.after_accounts,
                    )?;
                }
            }
            if binding_changed {
                restore_provider_binding(
                    &tx,
                    agent,
                    stored,
                    footprint.before_binding.as_ref(),
                    expected_after_binding.as_ref(),
                )?;
            }
            tx.commit()?;
            Ok(())
        })
    }
}

pub(super) fn get_provider_row(
    conn: &Connection,
    id: &str,
) -> Result<Option<(String, String, String, String, i64, String, String, String)>> {
    conn.query_row(
        r#"
        SELECT agent_id, name, settings_config, meta,
               is_current, created_at, updated_at, id
        FROM providers WHERE id = ?1
        "#,
        params![id],
        |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
                row.get(6)?,
                row.get(7)?,
            ))
        },
    )
    .optional()
    .map_err(AppError::from)
}

pub(super) fn ensure_provider_row_matches(conn: &Connection, expected: &Provider) -> Result<()> {
    let actual = get_provider_row(conn, &expected.id)?;
    let settings = serde_json::to_string(&expected.settings_config)?;
    let meta = serde_json::to_string(&expected.meta)?;
    let matches = actual
        == Some((
            expected.agent_id.as_str().to_string(),
            expected.name.clone(),
            settings,
            meta,
            if expected.is_current { 1 } else { 0 },
            expected.created_at.clone(),
            expected.updated_at.clone(),
            expected.id.clone(),
        ));
    if matches {
        Ok(())
    } else {
        Err(provider_compensation_conflict(&expected.id))
    }
}

pub(super) fn provider_compensation_conflict(id: &str) -> AppError {
    AppError::message(
        "provider.current.apply.rollback.database",
        format!("provider compensation CAS failed for {id}; database changed concurrently"),
    )
}

pub(super) type AccountRowSnapshot = (
    String,
    String,
    String,
    String,
    String,
    String,
    i64,
    String,
    String,
);

pub(super) fn get_account_row_for_provider_compensation(
    conn: &Connection,
    id: &str,
) -> Result<Option<AccountRowSnapshot>> {
    conn.query_row(
        r#"
        SELECT agent_id, kind, label, credentials, extra, status,
               is_current, created_at, updated_at
        FROM accounts WHERE id = ?1
        "#,
        params![id],
        |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get::<_, Option<String>>(2)?.unwrap_or_default(),
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
                row.get(6)?,
                row.get(7)?,
                row.get(8)?,
            ))
        },
    )
    .optional()
    .map_err(AppError::from)
}

pub(super) fn ensure_account_row_matches_for_provider_compensation(
    conn: &Connection,
    expected: &Account,
) -> Result<()> {
    let actual = get_account_row_for_provider_compensation(conn, &expected.id)?;
    let credentials = serde_json::to_string(&expected.credentials)?;
    let extra = serde_json::to_string(&expected.extra)?;
    let matches = actual
        == Some((
            expected.agent_id.as_str().to_string(),
            expected.kind.as_str().to_string(),
            expected.label.clone(),
            credentials,
            extra,
            expected.status.clone(),
            if expected.is_current { 1 } else { 0 },
            expected.created_at.clone(),
            expected.updated_at.clone(),
        ));
    if matches {
        Ok(())
    } else {
        Err(provider_compensation_conflict(&expected.id))
    }
}

pub(super) fn restore_account_rows_from_provider_footprint(
    conn: &Connection,
    agent: AgentId,
    before: &[Account],
    after: &[Account],
) -> Result<()> {
    for original in before {
        let expected = after.iter().find(|row| row.id == original.id).cloned();
        match expected {
            Some(expected) => {
                if original == &expected {
                    continue;
                }
                ensure_account_row_matches_for_provider_compensation(conn, &expected)?;
                let credentials = serde_json::to_string(&original.credentials)?;
                let extra = serde_json::to_string(&original.extra)?;
                let restored = conn.execute(
                    r#"
                    UPDATE accounts SET
                        agent_id = ?2, kind = ?3, label = ?4, credentials = ?5,
                        extra = ?6, status = ?7, is_current = ?8,
                        created_at = ?9, updated_at = ?10
                    WHERE id = ?1 AND agent_id = ?11 AND updated_at = ?12
                    "#,
                    params![
                        &original.id,
                        original.agent_id.as_str(),
                        original.kind.as_str(),
                        &original.label,
                        credentials,
                        extra,
                        &original.status,
                        if original.is_current { 1 } else { 0 },
                        &original.created_at,
                        &original.updated_at,
                        agent.as_str(),
                        &expected.updated_at,
                    ],
                )?;
                if restored != 1 {
                    return Err(provider_compensation_conflict(&original.id));
                }
            }
            None => return Err(provider_compensation_conflict(&original.id)),
        }
    }
    Ok(())
}

pub(super) fn restore_demoted_account_rows(
    conn: &Connection,
    agent: AgentId,
    before: &[Account],
) -> Result<()> {
    for original in before.iter().filter(|row| row.is_current) {
        let mut expected = original.clone();
        expected.is_current = false;
        ensure_account_row_matches_for_provider_compensation(conn, &expected)?;
        let credentials = serde_json::to_string(&original.credentials)?;
        let extra = serde_json::to_string(&original.extra)?;
        let restored = conn.execute(
            r#"
            UPDATE accounts SET
                agent_id = ?2, kind = ?3, label = ?4, credentials = ?5,
                extra = ?6, status = ?7, is_current = ?8,
                created_at = ?9, updated_at = ?10
            WHERE id = ?1 AND agent_id = ?11 AND updated_at = ?12
            "#,
            params![
                &original.id,
                original.agent_id.as_str(),
                original.kind.as_str(),
                &original.label,
                credentials,
                extra,
                &original.status,
                if original.is_current { 1 } else { 0 },
                &original.created_at,
                &original.updated_at,
                agent.as_str(),
                &expected.updated_at,
            ],
        )?;
        if restored != 1 {
            return Err(provider_compensation_conflict(&original.id));
        }
    }
    Ok(())
}

pub(super) fn restore_provider_binding(
    conn: &Connection,
    agent: AgentId,
    stored: &Provider,
    before: Option<&ProviderBindingSnapshot>,
    after: Option<&ProviderBindingSnapshot>,
) -> Result<()> {
    let Some(after) = after else {
        return Err(provider_compensation_conflict(stored.id.as_str()));
    };
    let expected_revision = before.map(|row| row.revision + 1).unwrap_or(1);
    if after.revision != expected_revision
        || after.account_id.is_some()
        || after.provider_id.as_deref() != Some(stored.id.as_str())
    {
        return Err(provider_compensation_conflict(stored.id.as_str()));
    }
    if get_provider_binding_row(conn, agent)?.as_ref() != Some(after) {
        return Err(provider_compensation_conflict(stored.id.as_str()));
    }

    if let Some(original) = before {
        let changed = conn.execute(
            r#"
            UPDATE agent_active_bindings SET
                account_id = ?2, provider_id = ?3, model_id = ?4,
                config_profile_id = ?5, revision = ?6, created_at = ?7,
                updated_at = ?8
            WHERE agent_key = ?1 AND revision = ?9 AND updated_at = ?10
            "#,
            params![
                &original.agent_key,
                &original.account_id,
                &original.provider_id,
                &original.model_id,
                &original.config_profile_id,
                original.revision,
                &original.created_at,
                &original.updated_at,
                after.revision,
                &after.updated_at,
            ],
        )?;
        if changed != 1 {
            return Err(provider_compensation_conflict(stored.id.as_str()));
        }
    } else {
        let removed = conn.execute(
            "DELETE FROM agent_active_bindings WHERE agent_key = ?1 AND revision = ?2 AND updated_at = ?3",
            params![&after.agent_key, after.revision, &after.updated_at],
        )?;
        if removed != 1 {
            return Err(provider_compensation_conflict(stored.id.as_str()));
        }
    }
    Ok(())
}

pub(super) fn compensated_current_apply_error(
    primary: AppError,
    live_rollback: Option<AppError>,
) -> AppError {
    let Some(rollback) = live_rollback else {
        return primary;
    };
    AppError::message(
        "provider.current.apply.rollback",
        format!(
            "applying the current provider failed [{}]; compensation status: live={}",
            primary.code(),
            rollback.code()
        ),
    )
}

pub(super) fn compensated_current_apply_error_with_db(
    primary: AppError,
    live_rollback: Option<AppError>,
    db_rollback: Option<AppError>,
) -> AppError {
    if live_rollback.is_none() && db_rollback.is_none() {
        return primary;
    }
    let live = live_rollback.as_ref().map_or("ok", AppError::code);
    let database = db_rollback.as_ref().map_or("ok", AppError::code);
    AppError::message(
        "provider.current.apply.rollback",
        format!(
            "applying the current provider failed [{}]; compensation status: live={live}, database={database}",
            primary.code()
        ),
    )
}
