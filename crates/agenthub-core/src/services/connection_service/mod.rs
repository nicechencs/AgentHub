//! ConnectionService — unique **ActiveBinding** write entry (P10 cleanup B1 / R01).
//!
//! `ActiveBinding` is the Agent's current account/provider pointer
//! (`accounts`/`providers`.`is_current` + `agent_active_bindings`).
//! It is **not** the product Binding (`TicketBinding`: ticket → Agent route).
//! Do not shorten either type to "Binding" in new code.
//!
//! Single DB transaction dual-writes:
//! - legacy `accounts` / `providers`.`is_current`
//! - `agent_active_bindings` connection refs (`account_id` / `provider_id`)
//!
//! Independent extension fields (`model_id`, `config_profile_id`) are preserved
//! across Account/Provider lifecycle ops. Only [`Self::clear`] deletes the whole row.
//!
//! Live apply (adapter FS writes) stays in AccountService / ProviderService
//! and runs **before** this service is called for switch paths.

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::error::{AppError, Result};
use crate::models::AgentId;
use crate::platform::AgentKey;
use crate::storage::{
    binding_clear_connection_refs_conn, binding_get_conn, ActiveBindingRow, ConnectionTrashRepo,
    Database,
};

#[cfg(test)]
use crate::storage::AccountRepo;

/// Agent → current account/provider pointer. Not [`crate::models::TicketBinding`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActiveBinding {
    pub agent_key: String,
    pub account_id: Option<String>,
    pub provider_id: Option<String>,
    pub model_id: Option<String>,
    pub config_profile_id: Option<String>,
    pub revision: i64,
    pub created_at: String,
    pub updated_at: String,
}

impl From<ActiveBindingRow> for ActiveBinding {
    fn from(r: ActiveBindingRow) -> Self {
        Self {
            agent_key: r.agent_key,
            account_id: r.account_id,
            provider_id: r.provider_id,
            model_id: r.model_id,
            config_profile_id: r.config_profile_id,
            revision: r.revision,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

#[derive(Clone)]
pub struct ConnectionService {
    pub(super) db: Database,
    pub(super) trash: ConnectionTrashRepo,
    /// Used only by cfg(test) activate helpers that resolve rows before activate.
    #[cfg(test)]
    pub(super) accounts: AccountRepo,
}

impl ConnectionService {
    pub fn new(db: Database) -> Self {
        Self {
            trash: ConnectionTrashRepo::new(db.clone()),
            #[cfg(test)]
            accounts: AccountRepo::new(db.clone()),
            db,
        }
    }

    pub(super) fn key(agent: AgentId) -> String {
        AgentKey::from_agent_id(agent).into_string()
    }

    /// Project-local timestamp format (matches Account/Provider service stamps).
    pub(super) fn now() -> String {
        chrono::Utc::now()
            .format("%Y-%m-%d %H:%M:%S%.6f")
            .to_string()
    }

    /// Repair `is_current` so it mirrors `agent_active_bindings` for one agent
    /// or every catalog agent. List/wallet callers use this; it is a no-op write
    /// when flags already match the pointer.
    pub fn reconcile_known_agents(&self, agent: Option<AgentId>) {
        match agent {
            Some(id) => {
                if let Err(error) = self.get_active(id) {
                    tracing::warn!(
                        module = crate::logging::targets::ACCOUNT,
                        agent = id.as_str(),
                        error = %error,
                        "active binding reconcile failed"
                    );
                }
            }
            None => {
                for id in AgentId::ALL {
                    if let Err(error) = self.get_active(id) {
                        tracing::warn!(
                            module = crate::logging::targets::ACCOUNT,
                            agent = id.as_str(),
                            error = %error,
                            "active binding reconcile failed"
                        );
                    }
                }
            }
        }
    }

    pub(super) fn require_current_flag(&self, is_current: bool, kind: &str) -> Result<()> {
        if !is_current {
            return Err(AppError::InvalidArg(format!(
                "{kind} activate path requires is_current=true"
            )));
        }
        Ok(())
    }

    /// Clear connection refs only when they match the deleted/demoted object.
    pub(super) fn clear_connection_refs_if_match_conn(
        &self,
        conn: &Connection,
        agent: AgentId,
        account_id: Option<&str>,
        provider_id: Option<&str>,
        now: &str,
    ) -> Result<()> {
        let key = Self::key(agent);
        let Some(row) = binding_get_conn(conn, &key)? else {
            return Ok(());
        };
        let matches_account = account_id.is_some_and(|id| row.account_id.as_deref() == Some(id));
        let matches_provider = provider_id.is_some_and(|id| row.provider_id.as_deref() == Some(id));
        if matches_account || matches_provider {
            binding_clear_connection_refs_conn(conn, &key, now)?;
        }
        Ok(())
    }
}

mod account;
mod active;
mod provider;
pub(crate) mod trash;

#[cfg(test)]
mod tests;
