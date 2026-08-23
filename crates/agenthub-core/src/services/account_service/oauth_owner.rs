//! Owner split for OAuth refresh: Hub never rotates a CLI-owned refresh token.
//!
//! CLI-imported grants (`auth.json` / `live`) are followed by re-reading the
//! official file. Hub PKCE grants (`oauth_pkce` / `oauth_refresh`) may hit the
//! token endpoint. Same-identity file write-back after a Hub refresh is
//! `oauth_file_sync` (row newer than file mtime only).

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use serde_json::json;

use crate::bridge::{BridgeUpstreamProtocol, UpstreamAuthReload};
use crate::error::{AppError, Result};
use crate::logging::targets;
use crate::models::{Account, AccountKind, AdapterSourceKind, AgentId};
use crate::services::account_quota::extract_access_token;
use crate::services::adapter_secret_resolver::AdapterSecretResolver;

use super::surface::live_reconcile_lock;
use super::AccountService;

fn access_jwt_expired(token: Option<&str>) -> bool {
    let Some(token) = token.map(str::trim).filter(|value| !value.is_empty()) else {
        return false;
    };
    let Some(claims) = crate::oauth::decode_jwt_payload(token) else {
        return false;
    };
    let Some(exp) = claims.get("exp").and_then(|value| value.as_i64()) else {
        return false;
    };
    exp <= chrono::Utc::now().timestamp()
}

fn oauth_source(account: &Account) -> &str {
    account
        .extra
        .get("source")
        .and_then(|value| value.as_str())
        .unwrap_or("")
}

/// Hub PKCE / Hub-refreshed grants. Anything else on Grok/Codex OAuth is CLI-owned.
pub(super) fn oauth_grant_is_hub_owned(account: &Account) -> bool {
    matches!(oauth_source(account), "oauth_pkce" | "oauth_refresh")
}

pub(super) fn oauth_grant_is_cli_owned(account: &Account) -> bool {
    account.kind == AccountKind::Oauth
        && matches!(account.agent_id, AgentId::Grok | AgentId::Codex)
        && !oauth_grant_is_hub_owned(account)
}

fn cli_owned_refresh_error(agent: AgentId) -> AppError {
    let who = match agent {
        AgentId::Grok => "Grok CLI",
        AgentId::Codex => "Codex CLI",
        other => {
            return AppError::Unsupported(format!(
                "{} 会在本机 auth.json 中自动续期，请使用“同步当前登录”",
                other.display_name()
            ))
        }
    };
    AppError::Unsupported(format!(
        "{who} 会在本机 auth.json 中自动续期，请使用“同步当前登录”"
    ))
}

fn oauth_refresh_lock(account_id: &str) -> Arc<Mutex<()>> {
    static LOCKS: OnceLock<Mutex<HashMap<String, Arc<Mutex<()>>>>> = OnceLock::new();
    let locks = LOCKS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut locks = locks
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    Arc::clone(
        locks
            .entry(account_id.to_owned())
            .or_insert_with(|| Arc::new(Mutex::new(()))),
    )
}

impl AccountService {
    pub(super) fn refuse_cli_owned_oauth_refresh(&self, account: &Account) -> Result<()> {
        if oauth_grant_is_cli_owned(account) {
            return Err(cli_owned_refresh_error(account.agent_id));
        }
        Ok(())
    }

    pub(super) fn acquire_oauth_refresh_lock(&self, account_id: &str) -> Arc<Mutex<()>> {
        oauth_refresh_lock(account_id)
    }

    /// Re-read the official CLI auth file and upsert this pool row if access rotated.
    /// Does not call the token endpoint.
    pub fn follow_cli_owned_access(
        &self,
        id_or_label: &str,
        agent: AgentId,
    ) -> Result<Option<String>> {
        let account = self.get(id_or_label, Some(agent))?;
        if !oauth_grant_is_cli_owned(&account) {
            return Ok(None);
        }
        let prior = extract_access_token(&account);
        let process_lock = live_reconcile_lock(agent);
        let _process_lock = process_lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let _file_lock = self.acquire_live_lock(agent)?;
        let adapter = self.adapter(agent)?;
        let lives = match self.read_live_accounts(adapter.as_ref(), agent) {
            Ok(lives) => lives,
            Err(error) => {
                tracing::debug!(
                    module = targets::ACCOUNT,
                    agent = agent.as_str(),
                    account_id = %account.id,
                    error_code = error.code(),
                    "cli-owned oauth follow could not read live auth"
                );
                self.mark_cli_oauth_needs_login(&account)?;
                return Ok(None);
            }
        };
        let mut matched = None;
        for live in lives {
            match self.reconcile_live_account_with_activate(adapter.as_ref(), agent, live, false) {
                Ok(Some(row)) if row.id == account.id => matched = Some(row),
                Ok(_) => {}
                Err(error) => {
                    tracing::debug!(
                        module = targets::ACCOUNT,
                        agent = agent.as_str(),
                        account_id = %account.id,
                        error_code = error.code(),
                        "cli-owned oauth follow reconcile skipped a live slot"
                    );
                }
            }
        }
        let latest = match matched {
            Some(row) => row,
            None => self.get(&account.id, Some(agent))?,
        };
        let next = extract_access_token(&latest);
        if next.as_deref() != prior.as_deref() && next.is_some() {
            return Ok(next);
        }
        // Unchanged access after a follow is a no-op. NeedsLogin only when the
        // live file is gone or the stored access JWT is already expired.
        if access_jwt_expired(prior.as_deref()) {
            self.mark_cli_oauth_needs_login(&latest)?;
        }
        Ok(None)
    }

    /// Follow a CLI-owned grant or Hub-refresh a Hub-owned grant. Returns a new
    /// access token only when the pool value actually changed.
    pub fn reload_oauth_upstream_access(&self, id_or_label: &str) -> Result<Option<String>> {
        let account = self.get(id_or_label, None)?;
        if !matches!(account.agent_id, AgentId::Grok | AgentId::Codex) {
            return Ok(None);
        }
        if oauth_grant_is_cli_owned(&account) {
            return self.follow_cli_owned_access(&account.id, account.agent_id);
        }
        if !oauth_grant_is_hub_owned(&account) {
            return Ok(None);
        }
        let prior = extract_access_token(&account);
        let refreshed = self.refresh_token(&account.id, account.agent_id)?;
        let next = extract_access_token(&refreshed);
        if next.as_deref() != prior.as_deref() {
            return Ok(next);
        }
        Ok(None)
    }

    fn mark_cli_oauth_needs_login(&self, account: &Account) -> Result<()> {
        let mut row = account.clone();
        if !row.extra.is_object() {
            row.extra = json!({});
        }
        if let Some(obj) = row.extra.as_object_mut() {
            obj.insert("health".into(), json!("needs_login"));
            obj.insert("tokenExpired".into(), json!(true));
        }
        let expected = row.updated_at.clone();
        let _ = self.persist_healed_fields(&row, &expected)?;
        Ok(())
    }
}

/// Opaque host callback: follow/refresh then re-resolve the access token.
/// Returns `None` when the protocol is not an OAuth subscription bridge.
pub fn oauth_bridge_reload_callback(
    accounts: AccountService,
    secrets: AdapterSecretResolver,
    source_kind: AdapterSourceKind,
    source_id: String,
    protocol: BridgeUpstreamProtocol,
) -> Option<UpstreamAuthReload> {
    if source_kind != AdapterSourceKind::Account {
        return None;
    }
    if !matches!(
        protocol,
        BridgeUpstreamProtocol::XaiResponsesOauth | BridgeUpstreamProtocol::CodexResponsesOauth
    ) {
        return None;
    }
    Some(Arc::new(move || {
        match accounts.reload_oauth_upstream_access(&source_id) {
            Ok(Some(_)) => {}
            _ => return None,
        }
        let auth = match protocol {
            BridgeUpstreamProtocol::XaiResponsesOauth => {
                secrets.resolve_grok_subscription_auth(source_kind, &source_id)
            }
            BridgeUpstreamProtocol::CodexResponsesOauth => {
                secrets.resolve_codex_subscription_auth(source_kind, &source_id)
            }
            _ => return None,
        };
        auth.ok()
            .map(|resolved| resolved.token())
            .filter(|token| !token.trim().is_empty())
    }))
}
