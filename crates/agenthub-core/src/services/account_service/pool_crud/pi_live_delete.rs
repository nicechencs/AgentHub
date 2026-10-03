//! Deleting a Pi login also takes its own entry out of Pi's auth.json, so the
//! next import or background sync does not bring it back. The auth.json key
//! and the Pi default are snapshotted first, then mutated, then the row is
//! removed — the same compensating saga as cancelling a Pi provider. Only the
//! one provider key that still holds the same login is removed; anything else
//! in the file is left as it is.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::adapters::{pi_auth, AgentAdapter};
use crate::error::{AppError, Result};
use crate::logging::targets;
use crate::models::{authorization_is_route_pool_home, Account, AccountKind, AgentId};
use crate::utils::agent_lock::AgentWriteLock;
use crate::utils::atomic::with_restored_files;

use super::super::surface::{
    accounts_same_authorization, accounts_same_oauth_identity, same_live_slot,
};
use super::super::AccountService;

/// How long a delete waits for a running Pi sync to release the live lock.
const PI_LOCK_WAIT: Duration = Duration::from_secs(2);
const PI_LOCK_RETRY: Duration = Duration::from_millis(40);

#[cfg(test)]
thread_local! {
    /// Test hook: make the next auth.json removal fail as if the write failed.
    pub(crate) static FAIL_PI_AUTH_REMOVE: std::cell::Cell<bool> =
        const { std::cell::Cell::new(false) };
}

impl AccountService {
    /// Move a Pi login to the recycle bin. When auth.json still holds the same
    /// login under the row's provider key, that key and the Pi default are
    /// updated inside a file snapshot, then the row is removed. Same pattern
    /// as cancelling a Pi provider: a later failure restores the snapshotted
    /// files, so a half-finished delete cannot leave auth.json or the default
    /// already changed.
    ///
    /// Rows that have nothing to clear never touch the live lock or the file.
    /// A failed file write keeps the row.
    pub(super) fn delete_pi_account_with_live(&self, account: &Account) -> Result<()> {
        let remaining = remaining_pi_slots_except(self, &account.id)?;
        let delete_row = || self.connections.delete_account(&account.id, AgentId::Pi);
        let finish_without_live = || -> Result<()> {
            delete_row()?;
            warn_pi_default_after_delete(&remaining);
            Ok(())
        };
        let Some(provider) = pi_auth_slot_for_delete(account) else {
            return finish_without_live();
        };
        let Some((adapter, auth_path)) = self.pi_live_auth() else {
            return finish_without_live();
        };
        // Cheap check without the lock: most deletes have nothing to clear.
        if self
            .owned_pi_auth_entry(adapter.as_ref(), account, &provider, &auth_path)?
            .is_none()
        {
            return finish_without_live();
        }

        let _lock = self.wait_for_pi_live_lock()?;
        // Re-check under the lock; a sync may have changed the file.
        let Some(expected) =
            self.owned_pi_auth_entry(adapter.as_ref(), account, &provider, &auth_path)?
        else {
            return finish_without_live();
        };
        let paths = pi_delete_restore_paths(adapter.as_ref(), &auth_path);
        let path_refs: Vec<_> = paths.iter().map(PathBuf::as_path).collect();
        match with_restored_files(&path_refs, || {
            let removed = remove_owned_entry(&auth_path, &provider, &expected)?;
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
            crate::adapters::pi::reconcile_pi_default_for_remaining_slots(&remaining)
                .map_err(live_clear_error)?;
            delete_row()?;
            Ok(())
        }) {
            Ok(()) => Ok(()),
            Err(error) if error.code() == "account.delete.live" => Err(error),
            Err(error) if error.code() == "config.write" => Err(live_clear_error(error)),
            Err(error) => Err(error),
        }
    }

    /// Registered Pi adapter and the auth.json it reads. Tests register a
    /// fake adapter whose file lives in a temp dir.
    fn pi_live_auth(&self) -> Option<(std::sync::Arc<dyn AgentAdapter>, PathBuf)> {
        let adapter = self.registry.get(AgentId::Pi)?;
        let path = adapter
            .live_backup_paths()
            .into_iter()
            .find(|path| path.file_name().is_some_and(|name| name == "auth.json"))?;
        Some((adapter, path))
    }

    /// Wait briefly for a background Pi sync to finish, then give up with a
    /// plain message instead of the lock's internal error.
    fn wait_for_pi_live_lock(&self) -> Result<Option<AgentWriteLock>> {
        let deadline = Instant::now() + PI_LOCK_WAIT;
        loop {
            match self.acquire_live_lock(AgentId::Pi) {
                Ok(lock) => return Ok(lock),
                Err(error) if error.code() == "agent.lock" => {
                    if Instant::now() >= deadline {
                        return Err(AppError::message(
                            "account.delete.live",
                            "Pi 正在同步登录，请稍后再删除。",
                        ));
                    }
                    std::thread::sleep(PI_LOCK_RETRY);
                }
                Err(error) => return Err(live_clear_error(error)),
            }
        }
    }

    /// The auth.json entry under `provider` when it is this row's login and
    /// no other connection still uses it. `None` leaves the file alone
    /// (missing file or key, another login, unreadable file).
    ///
    /// Same login means the same grant, or for an official login the same
    /// person on the same provider: Pi rotates refresh tokens, so after a
    /// refresh the file no longer equals the row but is still this login.
    fn owned_pi_auth_entry(
        &self,
        adapter: &dyn AgentAdapter,
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
        let owns = |row: &Account| {
            same_live_slot(AgentId::Pi, &live.credentials, &row.credentials)
                && (accounts_same_authorization(adapter, live.kind, &live.credentials, row)
                    || (live.kind == AccountKind::Oauth
                        && accounts_same_oauth_identity(live.kind, &live.credentials, row)))
        };
        if !owns(account) {
            return Ok(None);
        }
        // Another connection with the same login still needs the file entry.
        let shared = self.repo.list(Some(AgentId::Pi))?.iter().any(|row| {
            row.id != account.id && !authorization_is_route_pool_home(&row.extra) && owns(row)
        });
        if shared {
            return Ok(None);
        }
        Ok(Some(entry))
    }
}

/// Remove the key while it still equals `expected`. Returns whether it was
/// removed; a write failure becomes a plain error and the row is kept.
fn remove_owned_entry(auth_path: &Path, provider: &str, expected: &Value) -> Result<bool> {
    #[cfg(test)]
    if FAIL_PI_AUTH_REMOVE.with(|flag| flag.replace(false)) {
        return Err(live_clear_error(AppError::message(
            "config.write",
            "injected auth.json write failure",
        )));
    }
    pi_auth::remove_auth_entry_if_unchanged(auth_path, provider, expected).map_err(live_clear_error)
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

fn live_clear_error(error: AppError) -> AppError {
    AppError::message(
        "account.delete.live",
        format!("没能把这个登录从 Pi 本机正在用的配置里移除，所以没有删除它：{error}"),
    )
}

/// Provider slots that will still have a connection-page row after `except_id`
/// is removed. Used to pin the Pi default before the database step, so a
/// failed row delete can restore both auth.json and the default.
fn remaining_pi_slots_except(svc: &AccountService, except_id: &str) -> Result<Vec<String>> {
    let mut slots = Vec::new();
    for row in svc.repo.list(Some(AgentId::Pi))? {
        if row.id == except_id {
            continue;
        }
        let Some(slot) = crate::adapters::pi::pi_slot_from_account(&row.to_live()) else {
            continue;
        };
        if !slots.iter().any(|existing| existing == &slot) {
            slots.push(slot);
        }
    }
    Ok(slots)
}

fn warn_pi_default_after_delete(slots: &[String]) {
    if let Err(error) = crate::adapters::pi::reconcile_pi_default_for_remaining_slots(slots) {
        tracing::warn!(
            module = targets::ACCOUNT,
            op = "pi_default_after_delete",
            error = %error,
            "failed to update Pi default model after removing a login"
        );
    }
}

/// Snapshot auth.json plus the settings/models files the default reconcile
/// writes. Adapter live paths cover production; `pi_config_dir` covers the
/// env-overridden directory used when the registered adapter is a test fake.
fn pi_delete_restore_paths(adapter: &dyn AgentAdapter, auth_path: &Path) -> Vec<PathBuf> {
    let mut paths = adapter.live_backup_paths();
    let mut push = |path: PathBuf| {
        if !paths.iter().any(|existing| existing == &path) {
            paths.push(path);
        }
    };
    if let Some(dir) = auth_path.parent() {
        push(dir.join("settings.json"));
        push(dir.join("models.json"));
        push(dir.join("auth.json"));
    }
    if let Ok(dir) = pi_auth::pi_config_dir() {
        push(dir.join("settings.json"));
        push(dir.join("models.json"));
        push(dir.join("auth.json"));
    }
    paths
}
