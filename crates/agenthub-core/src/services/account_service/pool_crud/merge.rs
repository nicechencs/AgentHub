use rusqlite::{params, Connection, Transaction, TransactionBehavior};
use serde_json::Value;

use crate::adapters::AgentAdapter;
use crate::error::{AppError, Result};
use crate::models::{Account, AccountKind, AgentId, Provider};
use crate::services::ConnectionService;
use crate::storage::{
    account_get_by_id_conn, account_list_for_agent_conn, provider_get_by_id_conn,
    provider_list_for_agent_conn,
};
use crate::utils::loopback::credentials_are_loopback;

use super::super::surface::*;
use super::super::AccountService;
use super::types::{
    get_binding_row, list_trash_conn, AccountCommittedMutation, AccountMutationError,
    AccountMutationFootprint, ApiKeyUpdatePayload, BindingRowSnapshot, TrashRowSnapshot,
};

impl AccountService {
    /// One IMMEDIATE transaction: snapshot the agent pool, decide source /
    /// target / leftover duplicates from that snapshot, mutate those exact
    /// ids with expected-revision CAS, and return the precise before/after
    /// footprint. The transaction never re-lists the pool to guess leftovers.
    pub(super) fn commit_api_key_update(
        &self,
        adapter: &dyn AgentAdapter,
        agent: AgentId,
        source_id: &str,
        expected_source_updated_at: &str,
        payload: &ApiKeyUpdatePayload,
    ) -> std::result::Result<AccountCommittedMutation, AccountMutationError> {
        self.db
            .with_conn(|conn| {
                let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
                let now = now_ts();
                let accounts = account_list_for_agent_conn(&tx, agent)?;
                let providers = provider_list_for_agent_conn(&tx, agent)?;
                let binding = get_binding_row(&tx, agent)?;
                let trash = list_trash_conn(&tx, agent)?;

                let source = accounts
                    .iter()
                    .find(|row| row.id == source_id)
                    .cloned()
                    .ok_or_else(|| AppError::NotFound(format!("account not found: {source_id}")))?;
                if source.updated_at != expected_source_updated_at {
                    return Err(AppError::message(
                        "account.merge.conflict",
                        "account changed before API key merge",
                    ));
                }
                if source.kind != AccountKind::ApiKey {
                    return Err(AppError::InvalidArg(
                        "only API Key accounts can be updated via update_api_key".into(),
                    ));
                }

                let mut next = source.clone();
                next.label = payload.label.clone();
                if let Some(credentials) = &payload.credentials {
                    next.credentials = credentials.clone();
                }
                let mut extra = payload.extra.clone();
                Self::copy_persisted_surface(&source.extra, &mut extra);
                next.extra = extra;
                next.status = "active".into();
                next.updated_at = now.clone();
                next = self.prepare_account_surface(next);

                let duplicates = if payload.credentials.is_some() {
                    authorization_duplicates(
                        adapter,
                        agent,
                        AccountKind::ApiKey,
                        &next.credentials,
                        &accounts,
                    )
                    .into_iter()
                    .filter(|row| row.id != source.id)
                    .collect::<Vec<_>>()
                } else {
                    Vec::new()
                };

                let mark_current = source.is_current;
                let committed = if let Some(target_existing) =
                    pick_primary_authorization_match(duplicates.clone())
                {
                    next.id = target_existing.id.clone();
                    next.created_at = target_existing.created_at.clone();
                    next.is_current = mark_current || target_existing.is_current;
                    Self::copy_persisted_surface(&target_existing.extra, &mut next.extra);
                    next = self.prepare_account_surface(next);
                    let mut deletes = duplicates
                        .into_iter()
                        .filter(|row| row.id != target_existing.id)
                        .collect::<Vec<_>>();
                    if !deletes.iter().any(|row| row.id == source.id) {
                        deletes.push(source.clone());
                    }
                    apply_frozen_account_plan(
                        &self.connections,
                        &tx,
                        agent,
                        &next,
                        &target_existing,
                        &deletes,
                        &source.id,
                        next.is_current,
                        &now,
                        &accounts,
                        &providers,
                        &binding,
                        &trash,
                    )?
                } else {
                    next.is_current = source.is_current;
                    apply_frozen_account_plan(
                        &self.connections,
                        &tx,
                        agent,
                        &next,
                        &source,
                        &[],
                        &source.id,
                        next.is_current,
                        &now,
                        &accounts,
                        &providers,
                        &binding,
                        &trash,
                    )?
                };
                tx.commit()?;
                Ok(committed)
            })
            .map_err(AccountMutationError::pre)
    }

    /// Snapshot the agent pool, decide insert vs merge from that snapshot, and
    /// commit inside one IMMEDIATE transaction. Duplicate membership is never
    /// taken from an outer stale `existing.id`.
    pub(in crate::services::account_service) fn commit_authorization_merge(
        &self,
        adapter: &dyn AgentAdapter,
        incoming: &Account,
        kind: AccountKind,
        label: String,
        credentials: Value,
        extra: Value,
        mark_current: bool,
    ) -> std::result::Result<AccountCommittedMutation, AccountMutationError> {
        self.db
            .with_conn(|conn| {
                let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
                let now = now_ts();
                let agent = incoming.agent_id;
                let accounts = account_list_for_agent_conn(&tx, agent)?;
                let providers = provider_list_for_agent_conn(&tx, agent)?;
                let binding = get_binding_row(&tx, agent)?;
                let trash = list_trash_conn(&tx, agent)?;

                let duplicates = authorization_duplicates(
                    adapter,
                    agent,
                    kind,
                    &credentials,
                    &accounts,
                );
                let committed = if let Some(target_existing) =
                    pick_primary_authorization_match(duplicates.clone())
                {
                    let deletes = duplicates
                        .into_iter()
                        .filter(|row| row.id != target_existing.id)
                        .collect::<Vec<_>>();

                    let mut merged_extra = extra;
                    Self::copy_persisted_surface(&target_existing.extra, &mut merged_extra);
                    let mut next = target_existing.clone();
                    next.kind = kind;
                    next.label = label;
                    next.credentials = credentials;
                    next.extra = merged_extra;
                    next.status = "active".into();
                    next.updated_at = now.clone();
                    if mark_current {
                        next.is_current = true;
                    }
                    next = self.prepare_account_surface(next);

                    apply_frozen_account_plan(
                        &self.connections,
                        &tx,
                        agent,
                        &next,
                        &target_existing,
                        &deletes,
                        &target_existing.id,
                        next.is_current,
                        &now,
                        &accounts,
                        &providers,
                        &binding,
                        &trash,
                    )?
                } else {
                    let mut next = incoming.clone();
                    next.kind = kind;
                    next.label = label;
                    next.credentials = credentials;
                    next.extra = extra;
                    next.status = "active".into();
                    next.updated_at = now.clone();
                    next.is_current = mark_current;
                    next = self.prepare_account_surface(next);
                    apply_frozen_account_insert(
                        &self.connections,
                        &tx,
                        agent,
                        &next,
                        mark_current,
                        &accounts,
                        &providers,
                        &binding,
                        &trash,
                    )?
                };
                tx.commit()?;
                Ok(committed)
            })
            .map_err(AccountMutationError::pre)
    }
}

fn authorization_duplicates(
    adapter: &dyn AgentAdapter,
    agent: AgentId,
    kind: AccountKind,
    credentials: &Value,
    snapshot: &[Account],
) -> Vec<Account> {
    let incoming_loopback = credentials_are_loopback(credentials);
    snapshot
        .iter()
        .filter(|candidate| {
            candidate.kind == kind
                && same_live_slot(agent, credentials, &candidate.credentials)
                && if incoming_loopback {
                    credentials_are_loopback(&candidate.credentials)
                } else {
                    !credentials_are_loopback(&candidate.credentials)
                        && accounts_same_authorization(adapter, kind, credentials, candidate)
                }
        })
        .cloned()
        .collect()
}

fn freeze_account_mutation_plan(
    tx: &Transaction<'_>,
    items: &[(String, String, String)],
) -> Result<()> {
    tx.execute_batch(
        r#"
        CREATE TEMP TABLE IF NOT EXISTS account_mutation_plan (
            role TEXT NOT NULL,
            id TEXT NOT NULL,
            expected_updated_at TEXT NOT NULL
        );
        DELETE FROM account_mutation_plan;
        "#,
    )?;
    for (role, id, expected) in items {
        tx.execute(
            "INSERT INTO account_mutation_plan (role, id, expected_updated_at) VALUES (?1, ?2, ?3)",
            params![role, id, expected],
        )?;
    }
    Ok(())
}

fn revalidate_frozen_account_plan(
    conn: &Connection,
    items: &[(String, String, String)],
) -> Result<()> {
    for (_role, id, expected) in items {
        let live = account_get_by_id_conn(conn, id)?.ok_or_else(|| {
            AppError::NotFound(format!("account not found: {id}"))
        })?;
        if live.updated_at != *expected {
            return Err(AppError::message(
                "account.merge.conflict",
                format!("account {id} changed after merge snapshot"),
            ));
        }
    }
    Ok(())
}

fn apply_frozen_account_plan(
    connections: &ConnectionService,
    tx: &Transaction<'_>,
    agent: AgentId,
    next: &Account,
    target_before: &Account,
    deletes: &[Account],
    source_id: &str,
    mark_current: bool,
    now: &str,
    snapshot_accounts: &[Account],
    snapshot_providers: &[Provider],
    snapshot_binding: &Option<BindingRowSnapshot>,
    snapshot_trash: &[TrashRowSnapshot],
) -> Result<AccountCommittedMutation> {
    let mut items = vec![(
        "target".to_string(),
        target_before.id.clone(),
        target_before.updated_at.clone(),
    )];
    for row in deletes {
        if row.id == target_before.id {
            continue;
        }
        let role = if row.id == source_id {
            "source"
        } else {
            "extra"
        };
        items.push((role.to_string(), row.id.clone(), row.updated_at.clone()));
    }
    freeze_account_mutation_plan(tx, &items)?;
    revalidate_frozen_account_plan(tx, &items)?;

    let stored = if mark_current {
        connections
            .activate_account_if_revision_conn(tx, next, &target_before.updated_at)?
            .0
    } else {
        connections.update_account_if_revision_conn(tx, next, &target_before.updated_at)?
    };

    let mut deleted = Vec::new();
    let mut source_deletes = Vec::new();
    let mut extra_deletes = Vec::new();
    for row in deletes {
        if row.id == stored.id {
            continue;
        }
        if row.id == source_id {
            source_deletes.push(row);
        } else {
            extra_deletes.push(row);
        }
    }
    for row in source_deletes.into_iter().chain(extra_deletes) {
        connections.trash_delete_account_if_revision_conn(
            tx,
            &row.id,
            agent,
            &row.updated_at,
            now,
        )?;
        deleted.push(row.clone());
    }

    let mut affected_account_ids = vec![target_before.id.clone()];
    for row in &deleted {
        if !affected_account_ids.iter().any(|id| id == &row.id) {
            affected_account_ids.push(row.id.clone());
        }
    }
    if mark_current {
        for row in snapshot_accounts.iter().filter(|row| row.is_current) {
            if !affected_account_ids.iter().any(|id| id == &row.id) {
                affected_account_ids.push(row.id.clone());
            }
        }
    }

    let before_accounts = affected_account_ids
        .iter()
        .filter_map(|id| snapshot_accounts.iter().find(|row| row.id == *id).cloned())
        .collect::<Vec<_>>();
    let after_accounts = affected_account_ids
        .iter()
        .filter_map(|id| account_get_by_id_conn(tx, id).ok().flatten())
        .collect::<Vec<_>>();
    let before_providers = if mark_current {
        snapshot_providers
            .iter()
            .filter(|row| row.is_current)
            .cloned()
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    let after_providers = before_providers
        .iter()
        .filter_map(|row| provider_get_by_id_conn(tx, &row.id).ok().flatten())
        .collect::<Vec<_>>();
    let after_binding = get_binding_row(tx, agent)?;
    let after_trash = list_trash_conn(tx, agent)?;

    Ok(AccountCommittedMutation {
        stored,
        deleted,
        footprint: AccountMutationFootprint {
            affected_account_ids,
            before_accounts,
            after_accounts,
            before_providers,
            after_providers,
            before_binding: snapshot_binding.clone(),
            after_binding,
            before_trash: snapshot_trash.to_vec(),
            after_trash,
        },
    })
}

fn apply_frozen_account_insert(
    connections: &ConnectionService,
    tx: &Transaction<'_>,
    agent: AgentId,
    next: &Account,
    mark_current: bool,
    snapshot_accounts: &[Account],
    snapshot_providers: &[Provider],
    snapshot_binding: &Option<BindingRowSnapshot>,
    snapshot_trash: &[TrashRowSnapshot],
) -> Result<AccountCommittedMutation> {
    let stored = if mark_current {
        connections.create_and_activate_account_conn(tx, next)?.0
    } else {
        connections.create_account_conn(tx, next)?
    };

    let mut affected_account_ids = vec![stored.id.clone()];
    if mark_current {
        for row in snapshot_accounts.iter().filter(|row| row.is_current) {
            if !affected_account_ids.iter().any(|id| id == &row.id) {
                affected_account_ids.push(row.id.clone());
            }
        }
    }
    let before_accounts = affected_account_ids
        .iter()
        .filter_map(|id| snapshot_accounts.iter().find(|row| row.id == *id).cloned())
        .collect::<Vec<_>>();
    let after_accounts = affected_account_ids
        .iter()
        .filter_map(|id| account_get_by_id_conn(tx, id).ok().flatten())
        .collect::<Vec<_>>();
    let before_providers = if mark_current {
        snapshot_providers
            .iter()
            .filter(|row| row.is_current)
            .cloned()
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    let after_providers = before_providers
        .iter()
        .filter_map(|row| provider_get_by_id_conn(tx, &row.id).ok().flatten())
        .collect::<Vec<_>>();
    let after_binding = get_binding_row(tx, agent)?;
    let after_trash = list_trash_conn(tx, agent)?;

    Ok(AccountCommittedMutation {
        stored,
        deleted: Vec::new(),
        footprint: AccountMutationFootprint {
            affected_account_ids,
            before_accounts,
            after_accounts,
            before_providers,
            after_providers,
            before_binding: snapshot_binding.clone(),
            after_binding,
            before_trash: snapshot_trash.to_vec(),
            after_trash,
        },
    })
}
