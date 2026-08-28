use std::time::Instant;

use serde_json::json;
use uuid::Uuid;

use crate::adapters::AgentAdapter;
use crate::error::{AppError, Result};
use crate::models::{Account, AccountKind, AgentId, BackupKind, Capability};
use crate::utils::redact::mask_secret_preview;

use super::super::live_reconcile::compensated_current_account_apply_error_with_db;
use super::super::surface::*;
use super::super::{AccountService, MAX_ACCOUNT_LABEL_LEN};
use super::types::{AccountMutationError, ApiKeyUpdatePayload};

impl AccountService {
    /// Add an API Key account to the pool (does not switch live).
    ///
    /// `env_key` is optional and only applied when the adapter credentials
    /// object accepts `env_key` (e.g. Claude `settings.json` field name).
    pub fn add_api_key(
        &self,
        agent: AgentId,
        label: Option<&str>,
        api_key: &str,
    ) -> Result<Account> {
        self.add_api_key_with_env(agent, label, api_key, None)
    }

    pub fn add_api_key_with_env(
        &self,
        agent: AgentId,
        label: Option<&str>,
        api_key: &str,
        env_key: Option<&str>,
    ) -> Result<Account> {
        self.add_api_key_with_env_and_marker(agent, label, api_key, env_key, None)
    }

    /// Add an API Key account with an explicit product marker. The marker is
    /// optional for backward compatibility; the GUI supplies it for official
    /// Anthropic/OpenAI/xAI and Kimi Code/API products.
    pub fn add_api_key_with_env_and_marker(
        &self,
        agent: AgentId,
        label: Option<&str>,
        api_key: &str,
        env_key: Option<&str>,
        product_marker: Option<&str>,
    ) -> Result<Account> {
        let started = Instant::now();
        let result = self.add_api_key_inner(agent, label, api_key, env_key, product_marker);
        log_account_op("add_api_key", agent, started, &result);
        result
    }

    pub(in crate::services::account_service) fn add_api_key_inner(
        &self,
        agent: AgentId,
        label: Option<&str>,
        api_key: &str,
        env_key: Option<&str>,
        product_marker: Option<&str>,
    ) -> Result<Account> {
        let adapter = self.adapter(agent)?;
        let live = adapter.build_api_key_account(api_key)?;
        let mut credentials = live.credentials;
        if let Some(ek) = env_key.map(str::trim).filter(|s| !s.is_empty()) {
            if let Some(obj) = credentials.as_object_mut() {
                // Only set when this credential shape already uses env_key
                // or is the Claude-style api_key format.
                if obj.contains_key("env_key")
                    || obj.get("format").and_then(|v| v.as_str()) == Some("api_key")
                {
                    obj.insert("env_key".into(), json!(ek));
                }
            }
        }
        let display = label
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .or(live.label_hint.clone())
            .unwrap_or_else(|| format!("{} (API Key)", mask_secret_preview(api_key)));
        validate_label(&display, "account label", MAX_ACCOUNT_LABEL_LEN)?;

        let mut extra = attach_identity_meta(
            adapter.as_ref(),
            AccountKind::ApiKey,
            &credentials,
            &display,
            live.extra,
        );
        if let Some(marker) = product_marker
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            validate_api_key_product_marker(agent, marker)?;
            if let Some(obj) = extra.as_object_mut() {
                obj.insert("provider".into(), json!(marker));
            }
        }

        let now = now_ts();
        let row = Account {
            id: format!("{}-acc-{}", agent.as_str(), Uuid::new_v4()),
            agent_id: agent,
            kind: AccountKind::ApiKey,
            label: display.clone(),
            credentials: credentials.clone(),
            extra: extra.clone(),
            status: "active".into(),
            is_current: false,
            created_at: now.clone(),
            updated_at: now,
        };
        let row = self.prepare_account_surface(row);
        self.commit_authorization_merge(
            adapter.as_ref(),
            &row,
            AccountKind::ApiKey,
            display,
            credentials,
            extra,
            false,
        )
        .map(|committed| committed.stored)
        .map_err(AccountMutationError::into_error)
    }

    /// Update an existing API Key account (label and/or key).
    ///
    /// - `label`: when `Some` and non-empty after trim, replaces the display label
    /// - `api_key`: when `Some` and non-empty after trim, rebuilds credentials via adapter
    ///
    /// A current row with a new key is written to live files. Label-only edits
    /// and non-current rows stay pool-only. This must not reuse [`Self::switch`],
    /// which treats the existing live file as authoritative for the current row.
    pub fn update_api_key(
        &self,
        agent: AgentId,
        id_or_label: &str,
        label: Option<&str>,
        api_key: Option<&str>,
    ) -> Result<Account> {
        let started = Instant::now();
        let result = (|| {
            let key_changed = api_key
                .map(str::trim)
                .is_some_and(|value| !value.is_empty());

            // Keep the same per-agent lock across the live snapshot, DB write,
            // live apply and any compensation. The old implementation only
            // acquired it after the DB mutation, allowing another process to
            // observe a half-committed account rotation.
            let live_guard = if key_changed {
                self.backup
                    .as_ref()
                    .map(|backup| backup.acquire_live_write(agent))
                    .transpose()?
            } else {
                None
            };
            let _legacy_lock = if key_changed && live_guard.is_none() {
                self.acquire_live_lock(agent)?
            } else {
                None
            };
            let before = self.get(id_or_label, Some(agent))?;
            if before.kind != AccountKind::ApiKey {
                return Err(AppError::InvalidArg(
                    "only API Key accounts can be updated via update_api_key".into(),
                ));
            }

            let new_label = label
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string());
            let new_key = api_key
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string());
            if new_label.is_none() && new_key.is_none() {
                return Err(AppError::InvalidArg(
                    "update_api_key requires a non-empty label and/or api_key".into(),
                ));
            }

            let adapter = self.adapter(agent)?;
            let payload =
                self.materialize_api_key_update(&adapter, &before, new_label, new_key.as_deref())?;

            let live_saga = if before.is_current && key_changed {
                let backup = self.backup.as_ref();
                adapter
                    .capability(Capability::AccountSwitch)
                    .is_usable()
                    .then_some(())
                    .and(backup)
                    .map(|backup| (adapter.clone(), backup))
            } else {
                None
            };
            let live_before = if let Some((adapter, backup)) = live_saga.as_ref() {
                let live_before = match adapter.read_account() {
                    Ok(live) => Some(live),
                    Err(error) if error.code() == "not_found" => None,
                    Err(error) => return Err(error),
                };
                let snap = match live_guard.as_ref() {
                    Some(guard) => backup.snapshot_with_guard(
                        guard,
                        agent,
                        BackupKind::AutoSwitch,
                        Some(&format!("before applying current account {}", before.id)),
                    ),
                    None => backup.snapshot(
                        agent,
                        BackupKind::AutoSwitch,
                        Some(&format!("before applying current account {}", before.id)),
                    ),
                };
                if let Err(error) = snap {
                    if error.code() != "not_found" {
                        return Err(error);
                    }
                }
                Some((adapter.clone(), live_before))
            } else {
                None
            };

            // Adapter/materialization failures above never compensate. The
            // IMMEDIATE transaction below either commits a precise footprint
            // or rolls back; its errors are therefore also pre-commit.
            let committed = match self.commit_api_key_update(
                adapter.as_ref(),
                agent,
                &before.id,
                &before.updated_at,
                &payload,
            ) {
                Ok(committed) => committed,
                Err(progress) => return Err(progress.into_error()),
            };
            if let Some((adapter, live_before)) = live_before {
                let apply_live = committed.stored.to_live();
                if live_before
                    .as_ref()
                    .is_some_and(|before| before.credentials == apply_live.credentials)
                {
                    return Ok(committed.stored);
                }
                if let Err(error) = adapter.apply_account(&apply_live) {
                    // Keep the established pool-only behavior for adapters
                    // that can store a key but cannot apply it to live files.
                    if error.code() == "unsupported" {
                        if let (Some(backup), Some(guard)) =
                            (self.backup.as_ref(), live_guard.as_ref())
                        {
                            let _ = backup.snapshot_with_guard(
                                guard,
                                agent,
                                BackupKind::AutoSwitch,
                                Some("after API Key account update"),
                            );
                        } else {
                            self.snapshot_after_pool_change(agent, "after API Key account update");
                        }
                        return Ok(committed.stored);
                    }
                    let live_rollback = live_before
                        .as_ref()
                        .and_then(|before| adapter.apply_account(before).err());
                    let db_rollback = self
                        .restore_committed_account_mutation(agent, &committed)
                        .err();
                    return Err(compensated_current_account_apply_error_with_db(
                        error,
                        live_rollback,
                        db_rollback,
                    ));
                }
            } else {
                self.sync_current_account_live(
                    &committed.stored,
                    api_key,
                    "after API Key account update",
                )?;
            }
            Ok(committed.stored)
        })();
        log_account_op("update_api_key", agent, started, &result);
        result
    }

    fn materialize_api_key_update(
        &self,
        adapter: &std::sync::Arc<dyn AgentAdapter>,
        account: &Account,
        new_label: Option<String>,
        new_key: Option<&str>,
    ) -> Result<ApiKeyUpdatePayload> {
        if let Some(key) = new_key {
            let live = adapter.build_api_key_account(key)?;
            let mut creds = live.credentials;
            if let Some(prev_env) = account
                .credentials
                .get("env_key")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
            {
                if creds.get("env_key").and_then(|v| v.as_str()).is_none() {
                    if let Some(obj) = creds.as_object_mut() {
                        obj.insert("env_key".into(), json!(prev_env));
                    }
                }
            }
            let display = new_label.unwrap_or_else(|| {
                live.label_hint
                    .clone()
                    .unwrap_or_else(|| format!("{} (API Key)", mask_secret_preview(key)))
            });
            validate_label(&display, "account label", MAX_ACCOUNT_LABEL_LEN)?;
            let mut extra = attach_identity_meta(
                adapter.as_ref(),
                AccountKind::ApiKey,
                &creds,
                &display,
                live.extra,
            );
            if let Some(provider) = account.extra.get("provider").cloned() {
                if let Some(obj) = extra.as_object_mut() {
                    obj.entry("provider").or_insert(provider);
                }
            }
            Ok(ApiKeyUpdatePayload {
                label: display,
                credentials: Some(creds),
                extra,
            })
        } else {
            let display = new_label.ok_or_else(|| {
                AppError::InvalidArg(
                    "update_api_key requires a non-empty label and/or api_key".into(),
                )
            })?;
            validate_label(&display, "account label", MAX_ACCOUNT_LABEL_LEN)?;
            let extra = attach_identity_meta(
                adapter.as_ref(),
                AccountKind::ApiKey,
                &account.credentials,
                &display,
                account.extra.clone(),
            );
            Ok(ApiKeyUpdatePayload {
                label: display,
                credentials: None,
                extra,
            })
        }
    }

    // Referenced only from `tests.rs` in this crate; keep for test coverage.
    #[allow(dead_code)]
    pub(in crate::services::account_service) fn update_api_key_inner(
        &self,
        agent: AgentId,
        id_or_label: &str,
        label: Option<&str>,
        api_key: Option<&str>,
        expected_source_updated_at: &str,
    ) -> std::result::Result<(Account, Vec<Account>), AccountMutationError> {
        let account = self.get(id_or_label, Some(agent))?;
        if account.kind != AccountKind::ApiKey {
            return Err(AppError::InvalidArg(
                "only API Key accounts can be updated via update_api_key".into(),
            )
            .into());
        }
        let new_label = label
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
        let new_key = api_key
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
        if new_label.is_none() && new_key.is_none() {
            return Err(AppError::InvalidArg(
                "update_api_key requires a non-empty label and/or api_key".into(),
            )
            .into());
        }
        let adapter = self.adapter(agent)?;
        let payload =
            self.materialize_api_key_update(&adapter, &account, new_label, new_key.as_deref())?;
        self.commit_api_key_update(
            adapter.as_ref(),
            agent,
            &account.id,
            expected_source_updated_at,
            &payload,
        )
        .map(|committed| (committed.stored, committed.deleted))
    }
}

fn validate_api_key_product_marker(agent: AgentId, marker: &str) -> Result<()> {
    let allowed = match agent {
        AgentId::Claude => ["anthropic"].as_slice(),
        AgentId::Codex => ["openai", "openai-api"].as_slice(),
        AgentId::Grok => ["xai", "xai-api"].as_slice(),
        AgentId::Kimi => ["kimi-code-membership", "kimi-api"].as_slice(),
        _ => [].as_slice(),
    };
    if allowed
        .iter()
        .any(|value| marker.eq_ignore_ascii_case(value))
    {
        Ok(())
    } else {
        Err(AppError::InvalidArg(format!(
            "unsupported API key product marker for {}: {}",
            agent.as_str(),
            marker
        )))
    }
}
