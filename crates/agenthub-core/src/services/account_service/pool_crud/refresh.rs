use std::time::Instant;

use serde_json::json;

use crate::adapters::AgentAdapter;
use crate::error::{AppError, Result};
use crate::models::{Account, AccountKind, AgentId};

use super::super::surface::*;
use super::super::AccountService;

impl AccountService {
    /// Refresh OAuth tokens for a saved account (uses `refresh_token` grant).
    /// Updates the pool row. If this row is the same identity as the official
    /// CLI login file and newer than the file, write the file too.
    pub fn refresh_token(&self, id_or_label: &str, agent: AgentId) -> Result<Account> {
        let started = Instant::now();
        let result = self.refresh_token_inner(id_or_label, agent);
        log_account_op("refresh_token", agent, started, &result);
        result
    }

    pub(in crate::services::account_service) fn refresh_token_inner(
        &self,
        id_or_label: &str,
        agent: AgentId,
    ) -> Result<Account> {
        let mut account = self.get(id_or_label, Some(agent))?;
        if account.kind != AccountKind::Oauth {
            return Err(AppError::Unsupported(
                "token refresh is only supported for OAuth accounts".into(),
            ));
        }
        // CLI-owned grants rotate in the official auth.json. Hitting the token
        // endpoint here would invalidate the CLI's refresh token.
        self.refuse_cli_owned_oauth_refresh(&account)?;
        let refresh_lock = self.acquire_oauth_refresh_lock(&account.id);
        let _refresh_lock = refresh_lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        account = self.get(id_or_label, Some(agent))?;
        self.refuse_cli_owned_oauth_refresh(&account)?;
        let expected_updated_at = account.updated_at.clone();

        // Heal first so Pi body.refresh is promoted to refresh_token.
        let _ = crate::services::account_identity_heal::heal_account_identity(&mut account);

        let (mut creds, extra_base, new_identity) = if agent == AgentId::Pi {
            let creds = crate::oauth::refresh_pi_provider(&account.credentials)?;
            let identity = crate::oauth::identity_from_credentials(&creds);
            let extra = json!({
                "source": "oauth_refresh",
                "provider": creds.get("provider").cloned().unwrap_or(json!(null)),
            });
            (creds, extra, identity)
        } else {
            let provider_id = account
                .credentials
                .get("provider")
                .and_then(|v| v.as_str())
                .unwrap_or(agent.as_str());
            let refresh = account
                .credentials
                .get("refresh_token")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .ok_or_else(|| {
                    AppError::message(
                        "oauth.refresh",
                        "account has no refresh_token; re-run OAuth login",
                    )
                })?;

            let provider = crate::oauth::oauth_provider_for(agent).ok_or_else(|| {
                AppError::Unsupported(format!(
                    "OAuth refresh is not configured for {} (provider={provider_id})",
                    agent.as_str()
                ))
            })?;

            let bundle = provider.refresh(refresh)?;
            let mut creds = bundle.credentials;
            if creds
                .get("refresh_token")
                .and_then(|v| v.as_str())
                .is_none()
                || creds
                    .get("refresh_token")
                    .and_then(|v| v.as_str())
                    .map(|s| s.is_empty())
                    .unwrap_or(true)
            {
                if let Some(obj) = creds.as_object_mut() {
                    obj.insert("refresh_token".into(), serde_json::json!(refresh));
                }
            }
            // Keep Codex accounts in live-writable auth_json shape after refresh.
            // Generic OAuth refresh returns a flat token bundle; without this step
            // a successful refresh would re-break account switch.
            if agent == AgentId::Codex {
                // Preserve prior body tokens (account_id / id_token) when refresh omits them.
                if let Some(prior_body) = account.credentials.get("body").cloned() {
                    if let Some(obj) = creds.as_object_mut() {
                        obj.entry("body".to_string()).or_insert(prior_body);
                    }
                }
                for key in ["account_id", "id_token", "email", "sub", "plan_type"] {
                    if creds.get(key).and_then(|v| v.as_str()).is_none() {
                        if let Some(v) = account.credentials.get(key).cloned() {
                            if let Some(obj) = creds.as_object_mut() {
                                obj.insert(key.into(), v);
                            }
                        }
                    }
                }
                creds = crate::adapters::normalize_codex_oauth_credentials(&creds)?;
            }
            let prior_identity = crate::oauth::identity_from_credentials(&account.credentials);
            let mut new_identity = crate::oauth::identity_from_credentials(&creds);
            new_identity.merge_missing(&prior_identity);
            if let Some(obj) = creds.as_object_mut() {
                crate::oauth::apply_identity_to_credentials(obj, &new_identity);
            }
            (creds, bundle.extra, new_identity)
        };

        // Keep prior identity fields when the refresh response omits them.
        let prior_identity = crate::oauth::identity_from_credentials(&account.credentials);
        let mut new_identity = new_identity;
        new_identity.merge_missing(&prior_identity);
        if let Some(obj) = creds.as_object_mut() {
            crate::oauth::apply_identity_to_credentials(obj, &new_identity);
        }

        account.credentials = creds;

        let mut extra = extra_base;
        if let Some(obj) = extra.as_object_mut() {
            if let Some(exp) = account.credentials.get("expires_at").cloned() {
                obj.insert("expiresAt".into(), exp);
            }
            obj.insert("source".into(), serde_json::json!("oauth_refresh"));
            if let Some(ref email) = new_identity.email {
                obj.insert("email".into(), json!(email));
            }
            if let Some(ref plan) = new_identity.subscription {
                obj.insert("subscription".into(), json!(plan));
            }
            if let Some(label) = new_identity.display_label() {
                obj.insert("identityLabel".into(), json!(label));
            }
            if let Some(p) = account.credentials.get("provider").and_then(|v| v.as_str()) {
                obj.insert("provider".into(), json!(p));
            }
        }
        // Prefer adapter identity_label for final extra shape.
        if let Ok(adapter) = self.adapter(agent) {
            extra = attach_identity_meta(
                adapter.as_ref(),
                account.kind,
                &account.credentials,
                &account.label,
                extra,
            );
        }
        account.extra = extra;

        // Upgrade generic OAuth labels once we learn a real identity.
        if let Some(lab) = new_identity.display_label() {
            if is_generic_oauth_label(&account.label, agent)
                || crate::services::account_identity_heal::needs_identity_heal(&account)
            {
                if agent == AgentId::Pi {
                    if let Some(p) = account.credentials.get("provider").and_then(|v| v.as_str()) {
                        account.label = format!("pi:{p} · {lab}");
                    } else {
                        account.label = lab;
                    }
                } else {
                    account.label = lab;
                }
            }
        }

        let _ = crate::services::account_quota::heal_token_expiry(&mut account);
        // Fresh access token → re-probe 5h/7d windows when supported.
        let _ = crate::services::account_quota::try_refresh_account_quota(&mut account, true);

        account.updated_at = now_ts();
        account.status = "active".into();
        account = self.prepare_account_surface(account);
        let adapter = self.adapter(agent).ok();
        let persisted = self.persist_refreshed_account(
            adapter.as_deref(),
            account,
            &expected_updated_at,
            agent == AgentId::Pi,
        )?;
        if let Err(error) = self.sync_refreshed_oauth_row_to_cli_file(&persisted) {
            tracing::warn!(
                module = crate::logging::targets::ACCOUNT,
                op = "oauth_file_sync",
                agent = agent.as_str(),
                account_id = %persisted.id,
                error_code = error.code(),
                "hub oauth refresh could not sync the CLI login file"
            );
            return self.finish_refresh_after_cli_file_miss(persisted);
        }
        Ok(persisted)
    }

    fn persist_refreshed_account(
        &self,
        adapter: Option<&dyn AgentAdapter>,
        account: Account,
        expected_updated_at: &str,
        pi: bool,
    ) -> Result<Account> {
        let intended = account.clone();
        let persisted = if pi {
            self.persist_pi_oauth_account_update(&account, expected_updated_at)?
        } else {
            self.persist_healed_fields(&account, expected_updated_at)?
        };
        if persisted.credentials == intended.credentials {
            return Ok(persisted);
        }
        let same_grant = adapter.is_some_and(|adapter| {
            accounts_same_authorization(adapter, intended.kind, &intended.credentials, &persisted)
        });
        if !same_grant {
            return Ok(persisted);
        }
        let expected = persisted.updated_at.clone();
        let mut extra = intended.extra;
        Self::copy_persisted_surface(&persisted.extra, &mut extra);
        let mut retry = persisted;
        retry.credentials = intended.credentials;
        retry.label = intended.label;
        retry.extra = extra;
        retry.status = "active".into();
        retry = self.prepare_account_surface(retry);
        if pi {
            self.persist_pi_oauth_account_update(&retry, &expected)
        } else {
            self.persist_healed_fields(&retry, &expected)
        }
    }
}
