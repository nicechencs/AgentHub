use rusqlite::{Connection, Transaction, TransactionBehavior};

use crate::error::{AppError, Result};
use crate::models::Provider;
use crate::storage::{
    account_clear_current_conn, binding_set_connection_refs_conn, provider_create_conn,
    provider_get_by_id_conn, provider_update_conn, provider_update_if_revision_conn,
    provider_upsert_conn,
};

use super::{ActiveBinding, ConnectionService};

impl ConnectionService {
    /// Provider activation for a frozen update/upsert plan on an existing
    /// IMMEDIATE transaction.
    pub(crate) fn activate_provider_if_revision_conn(
        &self,
        conn: &Connection,
        provider: &Provider,
        expected_updated_at: Option<&str>,
    ) -> Result<(Provider, ActiveBinding)> {
        self.require_current_flag(provider.is_current, "provider")?;
        let stored = match expected_updated_at {
            Some(expected) => {
                let existing = provider_get_by_id_conn(conn, &provider.id)?.ok_or_else(|| {
                    AppError::NotFound(format!("provider not found: {}", provider.id))
                })?;
                if existing.updated_at != expected {
                    return Err(AppError::message(
                        "provider.merge.conflict",
                        format!("provider changed before activation: {}", provider.id),
                    ));
                }
                provider_update_if_revision_conn(conn, provider, expected)?
            }
            None => provider_upsert_conn(conn, provider)?,
        };
        account_clear_current_conn(conn, stored.agent_id)?;
        let binding = binding_set_connection_refs_conn(
            conn,
            &Self::key(stored.agent_id),
            None,
            Some(stored.id.clone()),
            &stored.updated_at,
        )?;
        Ok((stored, binding.into()))
    }

    /// Non-current provider update/upsert on an existing IMMEDIATE transaction.
    pub(crate) fn store_provider_non_current_if_revision_conn(
        &self,
        conn: &Connection,
        provider: &Provider,
        expected_updated_at: Option<&str>,
    ) -> Result<Provider> {
        if provider.is_current {
            return Err(AppError::InvalidArg(
                "store_provider_non_current_if_revision_conn requires is_current=false".into(),
            ));
        }
        let stored = match expected_updated_at {
            Some(expected) => {
                let existing = provider_get_by_id_conn(conn, &provider.id)?.ok_or_else(|| {
                    AppError::NotFound(format!("provider not found: {}", provider.id))
                })?;
                if existing.updated_at != expected {
                    return Err(AppError::message(
                        "provider.merge.conflict",
                        format!("provider changed before update: {}", provider.id),
                    ));
                }
                provider_update_if_revision_conn(conn, provider, expected)?
            }
            None => provider_upsert_conn(conn, provider)?,
        };
        self.clear_connection_refs_if_match_conn(
            conn,
            stored.agent_id,
            None,
            Some(stored.id.as_str()),
            &stored.updated_at,
        )?;
        Ok(stored)
    }

    /// Create a new provider and make it the sole active connection.
    pub fn create_and_activate_provider(
        &self,
        provider: &Provider,
    ) -> Result<(Provider, ActiveBinding)> {
        self.require_current_flag(provider.is_current, "provider")?;
        self.db.with_conn(|conn| {
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
            let created = provider_create_conn(&tx, provider)?;
            account_clear_current_conn(&tx, created.agent_id)?;
            let binding = binding_set_connection_refs_conn(
                &tx,
                &Self::key(created.agent_id),
                None,
                Some(created.id.clone()),
                &created.updated_at,
            )?;
            tx.commit()?;
            Ok((created, binding.into()))
        })
    }

    /// Update an existing provider and make it the sole active connection.
    pub fn update_and_activate_provider(
        &self,
        provider: &Provider,
    ) -> Result<(Provider, ActiveBinding)> {
        self.require_current_flag(provider.is_current, "provider")?;
        self.db.with_conn(|conn| {
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
            let updated = provider_update_conn(&tx, provider)?;
            account_clear_current_conn(&tx, updated.agent_id)?;
            let binding = binding_set_connection_refs_conn(
                &tx,
                &Self::key(updated.agent_id),
                None,
                Some(updated.id.clone()),
                &updated.updated_at,
            )?;
            tx.commit()?;
            Ok((updated, binding.into()))
        })
    }

    /// Upsert a provider and make it the sole active connection.
    pub fn upsert_and_activate_provider(
        &self,
        provider: &Provider,
    ) -> Result<(Provider, ActiveBinding)> {
        self.require_current_flag(provider.is_current, "provider")?;
        self.db.with_conn(|conn| {
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
            let stored = provider_upsert_conn(&tx, provider)?;
            account_clear_current_conn(&tx, stored.agent_id)?;
            let binding = binding_set_connection_refs_conn(
                &tx,
                &Self::key(stored.agent_id),
                None,
                Some(stored.id.clone()),
                &stored.updated_at,
            )?;
            tx.commit()?;
            Ok((stored, binding.into()))
        })
    }

    /// Update provider with `is_current=false`. Clears connection refs if binding
    /// pointed at it; model/profile are preserved.
    pub fn update_provider_non_current(&self, provider: &Provider) -> Result<Provider> {
        if provider.is_current {
            return Err(AppError::InvalidArg(
                "update_provider_non_current requires is_current=false".into(),
            ));
        }
        self.db.with_conn(|conn| {
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
            let updated = provider_update_conn(&tx, provider)?;
            self.clear_connection_refs_if_match_conn(
                &tx,
                updated.agent_id,
                None,
                Some(updated.id.as_str()),
                &updated.updated_at,
            )?;
            tx.commit()?;
            Ok(updated)
        })
    }

    /// Upsert provider with `is_current=false`. Clears connection refs only when
    /// they reference this id; model/profile are preserved.
    pub fn upsert_provider_non_current(&self, provider: &Provider) -> Result<Provider> {
        if provider.is_current {
            return Err(AppError::InvalidArg(
                "upsert_provider_non_current requires is_current=false".into(),
            ));
        }
        self.db.with_conn(|conn| {
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
            let stored = provider_upsert_conn(&tx, provider)?;
            self.clear_connection_refs_if_match_conn(
                &tx,
                stored.agent_id,
                None,
                Some(stored.id.as_str()),
                &stored.updated_at,
            )?;
            tx.commit()?;
            Ok(stored)
        })
    }
}
