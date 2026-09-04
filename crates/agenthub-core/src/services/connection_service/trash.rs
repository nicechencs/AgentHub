use rusqlite::{Transaction, TransactionBehavior};

use crate::error::{AppError, Result};
use crate::logging::targets;
use crate::models::{
    trash_home_from_authorization_blob, Account, AccountKind, AgentId, ConnectionTrashItem,
    ConnectionTrashKind, Provider, TRASH_HOME_CONNECTIONS, TRASH_HOME_ROUTE_POOL,
};
use crate::services::account_split::{
    accounts_share_authorization, is_mixed_live_bundle, new_split_account_id, split_mixed_account,
};
use crate::storage::{
    account_create_conn, account_delete_for_agent_conn, account_get_by_id_conn,
    account_list_for_agent_conn, provider_create_conn, provider_delete_for_agent_conn,
    provider_get_by_id_conn, ConnectionTrashRepo,
};
use crate::utils::redact::api_key_tail;
use serde_json::json;

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
            let mut provider = provider;
            enrich_provider_trash_identity(&mut provider);
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
    ///
    /// Rows deleted before identity extra was persisted still carry last4 in
    /// `identityLabel` / the snapshot secret. Recover that into `extra.secretTail`
    /// so the recycle list can name them without inventing a key.
    pub fn list_trash(&self, agent: Option<AgentId>) -> Result<Vec<ConnectionTrashItem>> {
        self.list_trash_filtered(agent, None)
    }

    pub fn list_trash_filtered(
        &self,
        agent: Option<AgentId>,
        home: Option<&str>,
    ) -> Result<Vec<ConnectionTrashItem>> {
        let mut items = self.trash.list(agent, home, &Self::now())?;
        for item in &mut items {
            let persist = if let Some(account) = item.account.as_mut() {
                recover_account_trash_identity(account).then_some(serde_json::to_value(&*account))
            } else if let Some(provider) = item.provider.as_mut() {
                recover_provider_trash_identity(provider)
                    .then_some(serde_json::to_value(&*provider))
            } else {
                None
            };
            let Some(encoded) = persist else {
                continue;
            };
            let encoded = match encoded {
                Ok(value) => value,
                Err(err) => {
                    tracing::warn!(
                        module = targets::PROVIDER,
                        op = "recycle_identity_heal",
                        id = item.id.as_str(),
                        error = %err,
                        "failed to encode recovered recycle identity"
                    );
                    continue;
                }
            };
            if let Err(err) = self.trash.update_payload(&item.id, &encoded) {
                tracing::warn!(
                    module = targets::PROVIDER,
                    op = "recycle_identity_heal",
                    id = item.id.as_str(),
                    error = %err,
                    "failed to persist recovered recycle identity"
                );
            }
        }
        for item in &mut items {
            let expected = expected_trash_home(item);
            if item.home == expected {
                continue;
            }
            item.home = expected.to_string();
            if let Err(err) = self.trash.update_home(&item.id, expected) {
                tracing::warn!(
                    module = targets::PROVIDER,
                    op = "recycle_home_heal",
                    id = item.id.as_str(),
                    error = %err,
                    "failed to persist recovered recycle home"
                );
            }
        }
        Ok(items)
    }

    pub(crate) fn load_trash_payload(&self, id: &str) -> Result<crate::storage::TrashPayloadRow> {
        self.db
            .with_conn(|conn| ConnectionTrashRepo::load_payload_conn(conn, id))
    }

    /// Restore a row to the AgentHub pool without applying it to the agent.
    pub fn restore_trash(&self, id: &str) -> Result<()> {
        self.db.with_conn(|conn| {
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
            let row = ConnectionTrashRepo::load_payload_conn(&tx, id)?;
            match row.kind {
                ConnectionTrashKind::Account => {
                    let account: Account = serde_json::from_value(row.payload)?;
                    if account.id != row.source_id || account.agent_id != row.agent_id {
                        return Err(AppError::InvalidArg("回收记录与账号内容不一致".into()));
                    }
                    restore_account_payload(&tx, account, id)?;
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
                ConnectionTrashKind::Membership => {
                    return Err(AppError::InvalidArg(
                        "membership trash is restored by the connection pool".into(),
                    ));
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

/// Restore a login row without applying it to the agent.
/// Mixed official-login + API Key snapshots expand into one row per family.
/// Families that still have their own recycle-bin rows stay there.
fn restore_account_payload(
    conn: &rusqlite::Connection,
    account: Account,
    restoring_id: &str,
) -> Result<()> {
    if account_get_by_id_conn(conn, &account.id)?.is_some() {
        return Err(AppError::InvalidArg(format!(
            "account already exists: {}",
            account.id
        )));
    }
    let original_id = account.id.clone();
    let agent = account.agent_id;
    if !is_mixed_live_bundle(&account.credentials) {
        let mut account = account;
        account.is_current = false;
        account_create_conn(conn, &account)?;
        return Ok(());
    }

    let split = split_mixed_account(&account);
    let existing = account_list_for_agent_conn(conn, agent)?;
    let other_trash = ConnectionTrashRepo::list_in_conn(conn, Some(agent), None)?;
    let mut original_id_used = false;
    for mut row in split {
        row.is_current = false;
        if existing
            .iter()
            .any(|other| accounts_share_authorization(&row, other))
        {
            continue;
        }
        if other_trash
            .iter()
            .any(|item| item.id != restoring_id && trash_account_shares_authorization(item, &row))
        {
            continue;
        }
        if original_id_used || account_get_by_id_conn(conn, &row.id)?.is_some() {
            row.id = new_split_account_id(row.agent_id);
        }
        if row.id == original_id {
            original_id_used = true;
        }
        account_create_conn(conn, &row)?;
    }
    Ok(())
}

fn trash_account_shares_authorization(item: &ConnectionTrashItem, account: &Account) -> bool {
    item.account
        .as_ref()
        .is_some_and(|other| accounts_share_authorization(other, account))
}

fn extra_string_missing(extra: &serde_json::Value, key: &str) -> bool {
    extra
        .get(key)
        .and_then(|v| v.as_str())
        .map(str::trim)
        .unwrap_or("")
        .is_empty()
}

fn stored_http_url(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::Object(map) => {
            for key in ["endpoint", "baseUrl", "base_url"] {
                if let Some(url) = map
                    .get(key)
                    .and_then(|v| v.as_str())
                    .map(str::trim)
                    .filter(|url| url.starts_with("http://") || url.starts_with("https://"))
                    .filter(|url| {
                        !url.contains("127.0.0.1")
                            && !url.contains("localhost")
                            && !url.contains("::1")
                    })
                {
                    return Some(url.to_string());
                }
            }
            map.values().find_map(stored_http_url)
        }
        serde_json::Value::Array(items) => items.iter().find_map(stored_http_url),
        _ => None,
    }
}

/// Fill `extra.secretTail` / `extra.endpoint` from identity already on the row.
/// Returns whether extra changed. Does not invent a key or copy another login.
fn recover_account_trash_identity(account: &mut Account) -> bool {
    if account.kind != AccountKind::ApiKey {
        return false;
    }
    let mut extra = if account.extra.is_object() {
        account.extra.clone()
    } else {
        json!({})
    };
    let mut dirty = false;
    if extra_string_missing(&extra, "secretTail") {
        let tail = api_key_tail(&account.credentials)
            .or_else(|| first_masked_tail(&extra))
            .or_else(|| first_masked_tail(&account.credentials))
            .or_else(|| crate::utils::redact::secret_tail_from_masked_preview(&account.label));
        if let Some(tail) = tail {
            if let Some(obj) = extra.as_object_mut() {
                obj.insert("secretTail".into(), json!(tail));
                dirty = true;
            }
        }
    }
    if extra_string_missing(&extra, "endpoint") {
        if let Some(url) = stored_http_url(&extra).or_else(|| stored_http_url(&account.credentials))
        {
            if let Some(obj) = extra.as_object_mut() {
                obj.insert("endpoint".into(), json!(url));
                dirty = true;
            }
        }
    }
    if dirty {
        account.extra = extra;
    }
    dirty
}

/// Same non-inventing recovery for supplier (provider) recycle rows.
fn recover_provider_trash_identity(provider: &mut Provider) -> bool {
    let mut meta = if provider.meta.is_object() {
        provider.meta.clone()
    } else {
        json!({})
    };
    let mut dirty = false;
    if extra_string_missing(&meta, "secretTail") {
        let tail = api_key_tail(&provider.settings_config)
            .or_else(|| first_masked_tail(&meta))
            .or_else(|| first_masked_tail(&provider.settings_config))
            .or_else(|| crate::utils::redact::secret_tail_from_masked_preview(&provider.name));
        if let Some(tail) = tail {
            if let Some(obj) = meta.as_object_mut() {
                obj.insert("secretTail".into(), json!(tail));
                dirty = true;
            }
        }
    }
    if extra_string_missing(&meta, "endpoint") {
        if let Some(url) =
            stored_http_url(&meta).or_else(|| stored_http_url(&provider.settings_config))
        {
            if let Some(obj) = meta.as_object_mut() {
                obj.insert("endpoint".into(), json!(url));
                dirty = true;
            }
        }
    }
    if dirty {
        provider.meta = meta;
    }
    dirty
}

/// First `**XXXX` already stored as a mask. Never treats a raw secret as a tail.
fn first_masked_tail(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(text) => {
            crate::utils::redact::secret_tail_from_masked_preview(text)
        }
        serde_json::Value::Object(map) => {
            for key in ["secretTail", "identityLabel", "label", "name", "preview"] {
                if let Some(tail) = map.get(key).and_then(first_masked_tail) {
                    return Some(tail);
                }
            }
            map.values().find_map(first_masked_tail)
        }
        serde_json::Value::Array(items) => items.iter().find_map(first_masked_tail),
        _ => None,
    }
}

fn persist_live_grok_identity_extra(extra: &mut serde_json::Value) -> bool {
    if !extra.is_object() {
        *extra = json!({});
    }
    let mut dirty = false;
    if extra_string_missing(extra, "secretTail") {
        if let Some(tail) = crate::adapters::read_grok_live_api_key_tail() {
            if let Some(obj) = extra.as_object_mut() {
                obj.insert("secretTail".into(), json!(tail));
                dirty = true;
            }
        }
    }
    if extra_string_missing(extra, "endpoint") {
        if let Some(url) = crate::adapters::read_grok_live_base_url() {
            if url.contains("127.0.0.1") || url.contains("localhost") || url.contains("::1") {
                return dirty;
            }
            if let Some(obj) = extra.as_object_mut() {
                obj.insert("endpoint".into(), json!(url));
                dirty = true;
            }
        }
    }
    dirty
}

fn enrich_account_trash_identity(account: &mut Account) {
    recover_account_trash_identity(account);
    if account.kind != AccountKind::ApiKey {
        return;
    }
    if !(account.is_current && account.agent_id == AgentId::Grok) {
        return;
    }
    let mut extra = if account.extra.is_object() {
        account.extra.clone()
    } else {
        json!({})
    };
    if persist_live_grok_identity_extra(&mut extra) {
        account.extra = extra;
    }
}

fn enrich_provider_trash_identity(provider: &mut Provider) {
    recover_provider_trash_identity(provider);
    if !(provider.is_current && provider.agent_id == AgentId::Grok) {
        return;
    }
    let mut meta = if provider.meta.is_object() {
        provider.meta.clone()
    } else {
        json!({})
    };
    if persist_live_grok_identity_extra(&mut meta) {
        provider.meta = meta;
    }
}

fn expected_trash_home(item: &ConnectionTrashItem) -> &'static str {
    if item.kind == ConnectionTrashKind::Membership {
        return TRASH_HOME_ROUTE_POOL;
    }
    if let Some(account) = item.account.as_ref() {
        return trash_home_from_authorization_blob(&account.extra);
    }
    if let Some(provider) = item.provider.as_ref() {
        return trash_home_from_authorization_blob(&provider.meta);
    }
    if item.home == TRASH_HOME_ROUTE_POOL {
        TRASH_HOME_ROUTE_POOL
    } else {
        TRASH_HOME_CONNECTIONS
    }
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
