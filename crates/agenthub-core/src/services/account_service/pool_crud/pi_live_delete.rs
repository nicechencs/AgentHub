//! Deleting a Pi login also takes its own entry out of Pi's auth.json, so the
//! next import or background sync does not bring it back. Only the one
//! provider key that still holds the very same grant is removed; anything
//! else in the file is left as it is.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::Value;

use crate::adapters::{pi_auth, AgentAdapter};
use crate::error::{AppError, Result};
use crate::logging::targets;
use crate::models::{authorization_is_route_pool_home, Account, AgentId};
use crate::utils::atomic::with_restored_files;

use super::super::surface::{accounts_same_authorization, same_live_slot};
use super::super::AccountService;

impl AccountService {
    /// Move a Pi login to the recycle bin. When auth.json still holds the same
    /// grant under the row's provider key, that key is removed first and the
    /// file is put back if the database step fails. A failed file write keeps
    /// the row.
    pub(super) fn delete_pi_account_with_live(&self, account: &Account) -> Result<()> {
        let Some(provider) = pi_auth_slot_for_delete(account) else {
            return self.connections.delete_account(&account.id, AgentId::Pi);
        };
        let Some(dir) = pi_live_auth_dir()? else {
            return self.connections.delete_account(&account.id, AgentId::Pi);
        };
        let _lock = self.acquire_live_lock(AgentId::Pi)?;
        let auth_path = dir.join("auth.json");
        let Some(expected) = self.owned_pi_auth_entry(account, &provider, &auth_path)? else {
            return self.connections.delete_account(&account.id, AgentId::Pi);
        };

        with_restored_files(&[auth_path.as_path()], || {
            let removed = pi_auth::remove_auth_entry_if_unchanged(&auth_path, &provider, &expected)
                .map_err(live_clear_error)?;
            if removed {
                tracing::info!(
                    module = targets::ACCOUNT,
                    op = "pi_delete_live",
                    agent = "pi",
                    provider = provider.as_str(),
                    account_id = account.id.as_str(),
                    "removed the deleted login from Pi auth.json"
                );
            }
            self.connections.delete_account(&account.id, AgentId::Pi)
        })
    }

    /// The auth.json entry under `provider`, when it is the same grant as
    /// `account` and no other pool row still uses it. `None` leaves the file
    /// alone (missing key, different login, unreadable file).
    fn owned_pi_auth_entry(
        &self,
        account: &Account,
        provider: &str,
        auth_path: &Path,
    ) -> Result<Option<Value>> {
        if !auth_path.exists() {
            return Ok(None);
        }
        let body = match pi_auth::read_auth_json_file(auth_path) {
            Ok(body) => body,
            Err(error) => {
                tracing::warn!(
                    module = targets::ACCOUNT,
                    op = "pi_delete_live",
                    agent = "pi",
                    error_code = error.code(),
                    "Pi auth.json unreadable; deleting the login without touching it"
                );
                return Ok(None);
            }
        };
        let Some(entry) = body.get(provider).cloned() else {
            return Ok(None);
        };
        let Ok(live) = pi_auth::live_account_for_provider(provider, &entry) else {
            return Ok(None);
        };
        let adapter: Arc<dyn AgentAdapter> = self
            .registry
            .get(AgentId::Pi)
            .unwrap_or_else(|| Arc::new(crate::adapters::pi::PiAdapter));
        let owns = |row: &Account| {
            same_live_slot(AgentId::Pi, &live.credentials, &row.credentials)
                && accounts_same_authorization(adapter.as_ref(), live.kind, &live.credentials, row)
        };
        if !owns(account) {
            return Ok(None);
        }
        // Another connection with the same grant still needs the file entry.
        let shared = self.repo.list(Some(AgentId::Pi))?.iter().any(|row| {
            row.id != account.id && !authorization_is_route_pool_home(&row.extra) && owns(row)
        });
        if shared {
            return Ok(None);
        }
        Ok(Some(entry))
    }
}

/// Provider key in auth.json that a deleted Pi row may own. Logins kept only
/// in the connection pool are not Pi's own configuration.
fn pi_auth_slot_for_delete(account: &Account) -> Option<String> {
    if account.agent_id != AgentId::Pi || authorization_is_route_pool_home(&account.extra) {
        return None;
    }
    account
        .credentials
        .get("provider")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|provider| !provider.is_empty())
        .map(str::to_string)
}

/// Pi config dir for the delete cleanup. Unit tests only touch a dir they set
/// through `PI_CODING_AGENT_DIR`, never the real `~/.pi`.
fn pi_live_auth_dir() -> Result<Option<PathBuf>> {
    #[cfg(test)]
    {
        Ok(std::env::var_os("PI_CODING_AGENT_DIR").map(PathBuf::from))
    }
    #[cfg(not(test))]
    {
        pi_auth::pi_config_dir().map(Some)
    }
}

fn live_clear_error(error: AppError) -> AppError {
    AppError::message(
        "account.delete.live",
        format!("没能把这个登录从 Pi 本机正在用的配置里移除，所以没有删除它：{error}"),
    )
}
