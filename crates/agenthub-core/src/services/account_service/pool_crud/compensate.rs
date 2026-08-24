use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};

use crate::error::{AppError, Result};
use crate::models::{Account, AgentId, Provider};

use super::super::AccountService;
use super::types::{
    get_binding_row, AccountCommittedMutation, AccountMutationFootprint, BindingRowSnapshot,
    TrashRowSnapshot,
};

impl AccountService {
    /// Restore only the precise committed footprint after a live apply fails.
    /// Live database rows must still match the post-commit expected state;
    /// any concurrent change fails closed.
    pub(super) fn restore_committed_account_mutation(
        &self,
        agent: AgentId,
        committed: &AccountCommittedMutation,
    ) -> Result<()> {
        self.restore_account_rows_with_footprint(
            agent,
            &committed.footprint.before_accounts,
            &committed.footprint.after_accounts,
            &committed.stored,
            &committed.deleted,
            &committed.footprint,
            &committed.footprint.after_binding,
            &committed.footprint.after_trash,
        )
    }

    // Referenced only from account_service `tests.rs`.
    #[allow(dead_code)]
    pub(in crate::services::account_service) fn restore_account_rows(
        &self,
        agent: AgentId,
        before: &[Account],
        after: &[Account],
        stored: &Account,
        deleted_rows: &[Account],
    ) -> Result<()> {
        let mut affected_account_ids = before.iter().map(|row| row.id.clone()).collect::<Vec<_>>();
        if !affected_account_ids.iter().any(|id| id == &stored.id) {
            affected_account_ids.push(stored.id.clone());
        }
        let footprint = AccountMutationFootprint {
            affected_account_ids,
            before_accounts: before.to_vec(),
            after_accounts: after.to_vec(),
            ..AccountMutationFootprint::default()
        };
        self.restore_account_rows_with_footprint(
            agent,
            before,
            after,
            stored,
            deleted_rows,
            &footprint,
            &None,
            &[],
        )
    }

    fn restore_account_rows_with_footprint(
        &self,
        agent: AgentId,
        before: &[Account],
        _after: &[Account],
        stored: &Account,
        deleted_rows: &[Account],
        footprint: &AccountMutationFootprint,
        after_binding: &Option<BindingRowSnapshot>,
        after_trash: &[TrashRowSnapshot],
    ) -> Result<()> {
        // Restore the surviving merge target first. If the deleted source was
        // current, inserting it while the target is still current can violate
        // the active-row invariant in callers that enforce it.
        let mut affected_ids = if footprint.affected_account_ids.is_empty() {
            let mut ids = vec![stored.id.clone()];
            for row in deleted_rows {
                if !ids.iter().any(|id| id == &row.id) {
                    ids.push(row.id.clone());
                }
            }
            ids
        } else {
            footprint.affected_account_ids.clone()
        };
        // A current write may demote the previously-current row(s). Those
        // rows are part of this mutation even when the target was an upsert.
        for row in before.iter().filter(|row| {
            row.is_current
                && (footprint.affected_account_ids.is_empty()
                    || footprint
                        .affected_account_ids
                        .iter()
                        .any(|id| id == &row.id))
        }) {
            if !affected_ids.iter().any(|id| id == &row.id) {
                affected_ids.push(row.id.clone());
            }
        }

        if affected_ids.is_empty() {
            return Ok(());
        }

        self.db.with_conn(|conn| {
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
            for id in affected_ids {
                let original = before.iter().find(|row| row.id == id);
                let explicitly_deleted = deleted_rows.iter().any(|row| row.id == id);

                match original {
                    Some(original) if explicitly_deleted => {
                        if get_account_row(&tx, &original.id)?.is_some() {
                            return Err(account_compensation_conflict(&original.id));
                        }
                        let credentials = serde_json::to_string(&original.credentials)?;
                        let extra = serde_json::to_string(&original.extra)?;
                        tx.execute(
                            r#"
                            INSERT INTO accounts (
                                id, agent_id, kind, label, credentials, extra,
                                status, is_current, created_at, updated_at
                            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
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
                            ],
                        )?;
                    }
                    Some(original) => {
                        // The target row is fully described by `stored`. A
                        // pre-existing current row is only demoted, so its
                        // pre-mutation fields must otherwise remain intact.
                        let expected = footprint
                            .after_accounts
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
                        ensure_account_row_matches(&tx, &expected)?;
                        let credentials = serde_json::to_string(&original.credentials)?;
                        let extra = serde_json::to_string(&original.extra)?;
                        let updated = tx.execute(
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
                        if updated != 1 {
                            return Err(account_compensation_conflict(&original.id));
                        }
                    }
                    None => {
                        // Account update/merge never creates a row. A row
                        // appearing for an affected id is an external write.
                        return Err(account_compensation_conflict(&id));
                    }
                }
            }
            let expected_after_binding =
                if footprint.after_binding.is_some() || footprint.before_binding.is_some() {
                    &footprint.after_binding
                } else {
                    after_binding
                };
            let binding_changed =
                footprint.before_binding.as_ref() != expected_after_binding.as_ref();
            if binding_changed && !footprint.before_providers.is_empty() {
                if footprint.after_providers.is_empty() {
                    restore_demoted_provider_rows(&tx, agent, &footprint.before_providers)?;
                } else {
                    restore_provider_rows_from_footprint(
                        &tx,
                        agent,
                        &footprint.before_providers,
                        &footprint.after_providers,
                    )?;
                }
            }
            if binding_changed {
                restore_account_binding(
                    &tx,
                    agent,
                    stored,
                    footprint.before_binding.as_ref(),
                    expected_after_binding.as_ref(),
                )?;
            }
            let expected_after_trash = if footprint.after_trash.is_empty() {
                after_trash
            } else {
                footprint.after_trash.as_slice()
            };
            if footprint.before_trash != expected_after_trash {
                remove_new_trash_rows(
                    &tx,
                    agent,
                    &footprint.before_trash,
                    expected_after_trash,
                    deleted_rows,
                )?;
            }
            tx.commit()?;
            Ok(())
        })
    }
}

fn restore_provider_rows_from_footprint(
    conn: &Connection,
    agent: AgentId,
    before: &[Provider],
    after: &[Provider],
) -> Result<()> {
    for original in before {
        let expected = after.iter().find(|row| row.id == original.id).cloned();
        match expected {
            Some(expected) => {
                if original == &expected {
                    continue;
                }
                ensure_provider_row_matches_for_compensation(conn, &expected)?;
                let settings = serde_json::to_string(&original.settings_config)?;
                let meta = serde_json::to_string(&original.meta)?;
                let restored = conn.execute(
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
                if restored != 1 {
                    return Err(account_compensation_conflict(&original.id));
                }
            }
            None => return Err(account_compensation_conflict(&original.id)),
        }
    }
    Ok(())
}

type AccountRowSnapshot = (
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

fn get_account_row(conn: &Connection, id: &str) -> Result<Option<AccountRowSnapshot>> {
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

fn ensure_account_row_matches(conn: &Connection, expected: &Account) -> Result<()> {
    let actual = get_account_row(conn, &expected.id)?;
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
        Err(account_compensation_conflict(&expected.id))
    }
}

fn account_compensation_conflict(id: &str) -> AppError {
    AppError::message(
        "account.current.apply.rollback.database",
        format!("account compensation CAS failed for {id}; database changed concurrently"),
    )
}

type ProviderRowSnapshot = (String, String, String, String, i64, String, String, String);

fn get_provider_row_for_compensation(
    conn: &Connection,
    id: &str,
) -> Result<Option<ProviderRowSnapshot>> {
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

fn ensure_provider_row_matches_for_compensation(
    conn: &Connection,
    expected: &Provider,
) -> Result<()> {
    let actual = get_provider_row_for_compensation(conn, &expected.id)?;
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
        Err(account_compensation_conflict(&expected.id))
    }
}

fn restore_demoted_provider_rows(
    conn: &Connection,
    agent: AgentId,
    before: &[Provider],
) -> Result<()> {
    for original in before.iter().filter(|row| row.is_current) {
        let mut expected = original.clone();
        expected.is_current = false;
        ensure_provider_row_matches_for_compensation(conn, &expected)?;
        let settings = serde_json::to_string(&original.settings_config)?;
        let meta = serde_json::to_string(&original.meta)?;
        let restored = conn.execute(
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
        if restored != 1 {
            return Err(account_compensation_conflict(&original.id));
        }
    }
    Ok(())
}

fn restore_account_binding(
    conn: &Connection,
    agent: AgentId,
    stored: &Account,
    before: Option<&BindingRowSnapshot>,
    after: Option<&BindingRowSnapshot>,
) -> Result<()> {
    let Some(after) = after else {
        return Err(account_compensation_conflict(stored.id.as_str()));
    };
    let expected_revision = before.map(|row| row.revision + 1).unwrap_or(1);
    if after.revision != expected_revision
        || after.account_id.as_deref() != Some(stored.id.as_str())
        || after.provider_id.is_some()
    {
        return Err(account_compensation_conflict(stored.id.as_str()));
    }
    if get_binding_row(conn, agent)?.as_ref() != Some(after) {
        return Err(account_compensation_conflict(stored.id.as_str()));
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
            return Err(account_compensation_conflict(stored.id.as_str()));
        }
    } else {
        let removed = conn.execute(
            "DELETE FROM agent_active_bindings WHERE agent_key = ?1 AND revision = ?2 AND updated_at = ?3",
            params![&after.agent_key, after.revision, &after.updated_at],
        )?;
        if removed != 1 {
            return Err(account_compensation_conflict(stored.id.as_str()));
        }
    }
    Ok(())
}

fn get_trash_row(conn: &Connection, id: &str) -> Result<Option<TrashRowSnapshot>> {
    conn.query_row(
        r#"
        SELECT id, agent_id, source_kind, source_id, label, was_current,
               payload, deleted_at, expires_at
        FROM connection_trash WHERE id = ?1
        "#,
        params![id],
        |row| {
            Ok(TrashRowSnapshot {
                id: row.get(0)?,
                agent_id: row.get(1)?,
                source_kind: row.get(2)?,
                source_id: row.get(3)?,
                label: row.get(4)?,
                was_current: row.get(5)?,
                payload: row.get(6)?,
                deleted_at: row.get(7)?,
                expires_at: row.get(8)?,
            })
        },
    )
    .optional()
    .map_err(AppError::from)
}

fn remove_new_trash_rows(
    conn: &Connection,
    agent: AgentId,
    before: &[TrashRowSnapshot],
    after: &[TrashRowSnapshot],
    deleted_rows: &[Account],
) -> Result<()> {
    let expected = after
        .iter()
        .filter(|row| {
            row.agent_id == agent.as_str()
                && row.source_kind == "account"
                && !before.iter().any(|old| old.id == row.id)
                && deleted_rows
                    .iter()
                    .any(|deleted| deleted.id == row.source_id)
        })
        .collect::<Vec<_>>();
    if expected.len() != deleted_rows.len() {
        return Err(account_compensation_conflict("connection_trash"));
    }
    for row in expected {
        if get_trash_row(conn, &row.id)?.as_ref() != Some(row) {
            return Err(account_compensation_conflict(&row.id));
        }
        let removed = conn.execute(
            "DELETE FROM connection_trash WHERE id = ?1",
            params![&row.id],
        )?;
        if removed != 1 {
            return Err(account_compensation_conflict(&row.id));
        }
    }
    Ok(())
}
