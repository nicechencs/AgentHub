use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;

use crate::error::{AppError, Result};
use crate::models::{Account, AgentId, Provider};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct BindingRowSnapshot {
    pub(super) agent_key: String,
    pub(super) account_id: Option<String>,
    pub(super) provider_id: Option<String>,
    pub(super) model_id: Option<String>,
    pub(super) config_profile_id: Option<String>,
    pub(super) revision: i64,
    pub(super) created_at: String,
    pub(super) updated_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct TrashRowSnapshot {
    pub(super) id: String,
    pub(super) agent_id: String,
    pub(super) source_kind: String,
    pub(super) source_id: String,
    pub(super) label: String,
    pub(super) was_current: i64,
    pub(super) payload: String,
    pub(super) deleted_at: String,
    pub(super) expires_at: String,
}

#[derive(Clone, Default)]
pub(super) struct AccountMutationFootprint {
    /// Exact ids this transaction owns. Compensation never infers ownership
    /// by scanning the rest of the agent pool.
    pub(super) affected_account_ids: Vec<String>,
    pub(super) before_accounts: Vec<Account>,
    pub(super) after_accounts: Vec<Account>,
    pub(super) before_providers: Vec<Provider>,
    pub(super) after_providers: Vec<Provider>,
    pub(super) before_binding: Option<BindingRowSnapshot>,
    pub(super) after_binding: Option<BindingRowSnapshot>,
    pub(super) before_trash: Vec<TrashRowSnapshot>,
    pub(super) after_trash: Vec<TrashRowSnapshot>,
}

pub struct AccountCommittedMutation {
    pub(in crate::services::account_service) stored: Account,
    pub(in crate::services::account_service) deleted: Vec<Account>,
    pub(super) footprint: AccountMutationFootprint,
}

pub(super) struct ApiKeyUpdatePayload {
    pub(super) label: String,
    pub(super) credentials: Option<Value>,
    pub(super) extra: Value,
}

/// Distinguishes a rolled-back IMMEDIATE transaction from a committed one.
/// Compensation is allowed only after commit (live-apply / post-commit
/// failures). Pre-commit errors, including in-transaction CAS conflicts that
/// abort the transaction, must not restore stale extra-transaction snapshots.
#[derive(Debug)]
pub struct AccountMutationError {
    error: AppError,
    #[allow(dead_code)]
    committed: bool,
}

impl AccountMutationError {
    pub(in crate::services::account_service) fn pre(error: AppError) -> Self {
        Self {
            error,
            committed: false,
        }
    }

    #[allow(dead_code)]
    pub(in crate::services::account_service) fn post(error: AppError) -> Self {
        Self {
            error,
            committed: true,
        }
    }

    pub(in crate::services::account_service) fn code(&self) -> &str {
        self.error.code()
    }

    pub(in crate::services::account_service) fn into_error(self) -> AppError {
        self.error
    }
}

impl From<AppError> for AccountMutationError {
    fn from(error: AppError) -> Self {
        Self::pre(error)
    }
}

pub(super) fn list_trash_conn(conn: &Connection, agent: AgentId) -> Result<Vec<TrashRowSnapshot>> {
    let mut stmt = conn.prepare(
        r#"
        SELECT id, agent_id, source_kind, source_id, label, was_current,
               payload, deleted_at, expires_at
        FROM connection_trash WHERE agent_id = ?1
        "#,
    )?;
    let rows = stmt.query_map(params![agent.as_str()], |row| {
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
    })?;
    rows.collect::<std::result::Result<Vec<_>, _>>()
        .map_err(AppError::from)
}

pub(super) fn get_binding_row(conn: &Connection, agent: AgentId) -> Result<Option<BindingRowSnapshot>> {
    let key = crate::platform::AgentKey::from_agent_id(agent).into_string();
    conn.query_row(
        r#"
        SELECT agent_key, account_id, provider_id, model_id, config_profile_id,
               revision, created_at, updated_at
        FROM agent_active_bindings WHERE agent_key = ?1
        "#,
        params![key],
        |row| {
            Ok(BindingRowSnapshot {
                agent_key: row.get(0)?,
                account_id: row.get(1)?,
                provider_id: row.get(2)?,
                model_id: row.get(3)?,
                config_profile_id: row.get(4)?,
                revision: row.get(5)?,
                created_at: row.get(6)?,
                updated_at: row.get(7)?,
            })
        },
    )
    .optional()
    .map_err(AppError::from)
}
