use rusqlite::{Transaction, TransactionBehavior};

use crate::error::{AppError, Result};
use crate::logging::targets;
use crate::models::{Account, AccountKind, AgentId, ConnectionTrashItem, ConnectionTrashKind, Provider};
use crate::utils::redact::api_key_tail;
use serde_json::json;
use crate::storage::{
    account_create_conn, account_delete_for_agent_conn, account_get_by_id_conn,
    provider_create_conn, provider_delete_for_agent_conn, provider_get_by_id_conn,
    ConnectionTrashRepo,
};

use super::ConnectionService;

impl ConnectionService {
    /// Move an account to the local recovery bin, then clear its active binding.
    /// The agent's own files are intentionally untouched.
    pub fn delete_account(&self, id: &str, agent: AgentId) -> Result<()> {
        let now = Self::now();
        self.db.with_conn(|conn| {
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
            let mut account = account_get_by_id_conn(&tx, id)?
                .ok_or_else(|| AppError::NotFound(format!("account not found: {id}")))?;
            if account.agent_id != agent {
                return Err(AppError::NotFound(format!(
                    "account not found: {id} (agent filter: {})",
                    agent.as_str()
                )));
            }
            enrich_account_trash_identity(&mut account);
            ConnectionTrashRepo::insert_conn(
                &tx,
                &account.id,
                agent,
                ConnectionTrashKind::Account,
                &account.label,
                account.is_current,
                &account,
                &now,
            )?;
            account_delete_for_agent_conn(&tx, id, agent)?;
            self.clear_connection_refs_if_match_conn(&tx, agent, Some(id), None, &now)?;
            tx.commit()?;
            log_recycle(agent, &account.id, &account.label);
            Ok(())
        })
    }

    /// Delete an account only when the caller's post-activation revision is
    /// still present. Duplicate merge uses this CAS boundary so a concurrent
    /// label/heal/credential writer cannot be moved to trash after target
    /// activation.
    pub fn delete_account_if_revision(
        &self,
        id: &str,
        agent: AgentId,
        expected_updated_at: &str,
    ) -> Result<()> {
        let now = Self::now();
        self.db.with_conn(|conn| {
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
            let mut account = account_get_by_id_conn(&tx, id)?
                .ok_or_else(|| AppError::NotFound(format!("account not found: {id}")))?;
            if account.agent_id != agent {
                return Err(AppError::NotFound(format!(
                    "account not found: {id} (agent filter: {})",
                    agent.as_str()
                )));
            }
            if account.updated_at != expected_updated_at {
                return Err(AppError::message(
                    "account.merge.delete.conflict",
                    format!("account changed before duplicate deletion: {id}"),
                ));
            }
            enrich_account_trash_identity(&mut account);
            ConnectionTrashRepo::insert_conn(
                &tx,
                &account.id,
                agent,
                ConnectionTrashKind::Account,
                &account.label,
                account.is_current,
                &account,
                &now,
            )?;
            account_delete_for_agent_conn(&tx, id, agent)?;
            self.clear_connection_refs_if_match_conn(&tx, agent, Some(id), None, &now)?;
            tx.commit()?;
            log_recycle(agent, &account.id, &account.label);
            Ok(())
        })
    }

    /// Move a provider to the local recovery bin, then clear its active binding.
    /// The agent's own files are intentionally untouched.
    pub fn delete_provider(&self, id: &str, agent: AgentId) -> Result<()> {
        let now = Self::now();
        self.db.with_conn(|conn| {
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
            let provider = provider_get_by_id_conn(&tx, id)?
                .ok_or_else(|| AppError::NotFound(format!("provider not found: {id}")))?;
            if provider.agent_id != agent {
                return Err(AppError::NotFound(format!(
                    "provider not found: {id} (agent filter: {})",
                    agent.as_str()
                )));
            }
            ConnectionTrashRepo::insert_conn(
                &tx,
                &provider.id,
                agent,
                ConnectionTrashKind::Provider,
                &provider.name,
                provider.is_current,
                &provider,
                &now,
            )?;
            provider_delete_for_agent_conn(&tx, id, agent)?;
            self.clear_connection_refs_if_match_conn(&tx, agent, None, Some(id), &now)?;
            tx.commit()?;
            log_recycle(agent, &provider.id, &provider.name);
            Ok(())
        })
    }

    /// List recoverable connection rows.  Secret material is retained in the
    /// database for restore and redacted by the Tauri boundary before return.
    pub fn list_trash(&self, agent: Option<AgentId>) -> Result<Vec<ConnectionTrashItem>> {
        self.trash.list(agent, &Self::now())
    }

    /// Restore a row to the AgentHub pool without applying it to the agent.
    pub fn restore_trash(&self, id: &str) -> Result<()> {
        self.db.with_conn(|conn| {
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
            let row = ConnectionTrashRepo::load_payload_conn(&tx, id)?;
            match row.kind {
                ConnectionTrashKind::Account => {
                    let mut account: Account = serde_json::from_value(row.payload)?;
                    if account.id != row.source_id || account.agent_id != row.agent_id {
                        return Err(AppError::InvalidArg("回收记录与账号内容不一致".into()));
                    }
                    if account_get_by_id_conn(&tx, &account.id)?.is_some() {
                        return Err(AppError::InvalidArg(format!(
                            "account already exists: {}",
                            account.id
                        )));
                    }
                    // Restoring never silently applies a live credential.
                    account.is_current = false;
                    account_create_conn(&tx, &account)?;
                }
                ConnectionTrashKind::Provider => {
                    let mut provider: Provider = serde_json::from_value(row.payload)?;
                    if provider.id != row.source_id || provider.agent_id != row.agent_id {
                        return Err(AppError::InvalidArg("回收记录与 API Key 配置不一致".into()));
                    }
                    if provider_get_by_id_conn(&tx, &provider.id)?.is_some() {
                        return Err(AppError::InvalidArg(format!(
                            "provider already exists: {}",
                            provider.id
                        )));
                    }
                    provider.is_current = false;
                    provider_create_conn(&tx, &provider)?;
                }
            }
            ConnectionTrashRepo::delete_conn(&tx, id)?;
            tx.commit()?;
            Ok(())
        })
    }

    /// Permanently remove one recovery-bin row.
    pub fn delete_trash(&self, id: &str) -> Result<()> {
        self.trash.delete(id)
    }
}

fn enrich_account_trash_identity(account: &mut Account) {
    if account.kind != AccountKind::ApiKey {
        return;
    }
    let extra = if account.extra.is_object() {
        account.extra.clone()
    } else {
        json!({})
    };
    let mut extra = extra;
    if extra.get("secretTail").and_then(|v| v.as_str()).map(str::trim).unwrap_or("").is_empty() {
        if let Some(tail) = api_key_tail(&account.credentials) {
            if let Some(obj) = extra.as_object_mut() {
                obj.insert("secretTail".into(), json!(tail));
            }
        }
    }
    let endpoint_missing = extra
        .get("endpoint")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .unwrap_or("")
        .is_empty();
    if endpoint_missing && account.is_current && account.agent_id == AgentId::Grok {
        if let Some(url) = crate::adapters::read_grok_live_base_url() {
            if let Some(obj) = extra.as_object_mut() {
                obj.insert("endpoint".into(), json!(url));
            }
        }
    }
    account.extra = extra;
}

pub(crate) fn log_recycle(agent: AgentId, id: &str, name: &str) {
    tracing::info!(
        module = targets::PROVIDER,
        op = "recycle",
        agent = agent.as_str(),
        id,
        name,
        "moved login to recovery bin"
    );
}
