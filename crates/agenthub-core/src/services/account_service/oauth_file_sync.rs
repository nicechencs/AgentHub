//! Bidirectional sync of one OAuth pool row with the official CLI login file.
//!
//! Compares refresh-token equality in memory and `updated_at` vs file mtime
//! (not token `expires_at`). Raw refresh tokens are never logged.

use std::cmp::Ordering;
use std::path::{Path, PathBuf};

use chrono::{DateTime, NaiveDateTime, Utc};
use serde_json::{json, Value};

use crate::adapters::AgentAdapter;
use crate::error::Result;
use crate::logging::targets;
use crate::models::{Account, AccountKind, AgentId, LiveAccount};

use super::surface::*;
use super::AccountService;

const REFRESH_KEYS: &[&str] = &["refresh_token", "refreshToken", "refresh"];
const ACCESS_KEYS: &[&str] = &["access_token", "accessToken", "access"];
const API_KEY_KEYS: &[&str] = &["api_key", "OPENAI_API_KEY"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum OauthFileSyncAction {
    /// Secrets already match, or there is nothing to do.
    Noop,
    /// Row is newer: write the official login file from the row.
    WriteFile,
    /// File is newer: copy file secrets onto the row.
    WriteRow,
    /// Equal mtime with different refresh tokens (or keys): do not auto-overwrite.
    NeedsAttention,
    /// Different identity / never the same grant: never write across.
    Skip,
}

pub(super) fn supports_oauth_file_sync(agent: AgentId) -> bool {
    matches!(agent, AgentId::Grok | AgentId::Codex | AgentId::Claude)
}

pub(super) struct OauthFileSyncInput<'a> {
    pub row: &'a Account,
    pub file_credentials: &'a Value,
    pub file_kind: AccountKind,
    pub file_mtime: DateTime<Utc>,
}

pub(super) fn decide_oauth_file_sync(input: OauthFileSyncInput<'_>) -> OauthFileSyncAction {
    let OauthFileSyncInput {
        row,
        file_credentials,
        file_kind,
        file_mtime,
    } = input;
    if row.kind != file_kind {
        return OauthFileSyncAction::Skip;
    }

    if row.kind == AccountKind::ApiKey {
        return decide_api_key_sync(row, file_credentials, file_mtime);
    }
    if row.kind != AccountKind::Oauth {
        return OauthFileSyncAction::Skip;
    }

    let row_rt = find_named_string(&row.credentials, REFRESH_KEYS);
    let file_rt = find_named_string(file_credentials, REFRESH_KEYS);
    let same_rt = match (row_rt.as_deref(), file_rt.as_deref()) {
        (Some(a), Some(b)) => a == b,
        (None, None) => true,
        _ => false,
    };
    let same_identity = accounts_same_oauth_identity(row.kind, file_credentials, row);
    // Same person, or the same refresh token copied into the row from the file.
    if !same_identity && !same_rt {
        return OauthFileSyncAction::Skip;
    }

    let row_access = find_access_token(&row.credentials);
    let file_access = find_access_token(file_credentials);
    let secrets_equal = same_rt && row_access.as_deref() == file_access.as_deref();
    if secrets_equal {
        return OauthFileSyncAction::Noop;
    }

    let Some(row_ts) = parse_account_timestamp(&row.updated_at) else {
        return OauthFileSyncAction::NeedsAttention;
    };
    match row_ts.cmp(&file_mtime) {
        Ordering::Greater => OauthFileSyncAction::WriteFile,
        Ordering::Less => OauthFileSyncAction::WriteRow,
        Ordering::Equal => OauthFileSyncAction::NeedsAttention,
    }
}

fn decide_api_key_sync(
    row: &Account,
    file_credentials: &Value,
    file_mtime: DateTime<Utc>,
) -> OauthFileSyncAction {
    let row_key = find_named_string(&row.credentials, API_KEY_KEYS);
    let file_key = find_named_string(file_credentials, API_KEY_KEYS);
    if row_key.is_none() && file_key.is_none() {
        return OauthFileSyncAction::Skip;
    }
    if row_key.as_deref() == file_key.as_deref() {
        return OauthFileSyncAction::Noop;
    }
    let Some(row_ts) = parse_account_timestamp(&row.updated_at) else {
        return OauthFileSyncAction::NeedsAttention;
    };
    match row_ts.cmp(&file_mtime) {
        Ordering::Greater => OauthFileSyncAction::WriteFile,
        Ordering::Less => OauthFileSyncAction::WriteRow,
        Ordering::Equal => OauthFileSyncAction::NeedsAttention,
    }
}

pub(super) fn parse_account_timestamp(raw: &str) -> Option<DateTime<Utc>> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    if let Ok(dt) = DateTime::parse_from_rfc3339(raw) {
        return Some(dt.with_timezone(&Utc));
    }
    const FMTS: &[&str] = &[
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%dT%H:%M:%S%.f",
        "%Y-%m-%dT%H:%M:%S",
    ];
    for fmt in FMTS {
        if let Ok(naive) = NaiveDateTime::parse_from_str(raw, fmt) {
            return Some(naive.and_utc());
        }
    }
    None
}

pub(super) fn oauth_cli_file_path(adapter: &dyn AgentAdapter) -> Option<PathBuf> {
    let paths = adapter.live_backup_paths();
    const NAMES: &[&str] = &["auth.json", ".credentials.json", "credentials.json"];
    for name in NAMES {
        if let Some(path) = paths
            .iter()
            .find(|path| path.file_name().and_then(|n| n.to_str()) == Some(*name) && path.is_file())
        {
            return Some(path.clone());
        }
    }
    None
}

pub(super) fn path_mtime_utc(path: &Path) -> Option<DateTime<Utc>> {
    std::fs::metadata(path)
        .ok()?
        .modified()
        .ok()
        .map(DateTime::<Utc>::from)
}

fn find_named_string(value: &Value, keys: &[&str]) -> Option<String> {
    match value {
        Value::Object(map) => {
            for key in keys {
                if let Some(s) = map
                    .get(*key)
                    .and_then(|v| v.as_str())
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                {
                    return Some(s.to_string());
                }
            }
            for nested in map.values() {
                if nested.is_object() || nested.is_array() {
                    if let Some(found) = find_named_string(nested, keys) {
                        return Some(found);
                    }
                }
            }
            None
        }
        Value::Array(items) => items.iter().find_map(|item| find_named_string(item, keys)),
        _ => None,
    }
}

fn find_access_token(credentials: &Value) -> Option<String> {
    if let Some(access) = find_named_string(credentials, ACCESS_KEYS) {
        return Some(access);
    }
    // Grok auth.json uses `key` for the bearer, with or without a sibling rt.
    find_oauth_profile_key(credentials)
}

/// Grok `auth.json` stores the bearer as `key` on the profile object.
fn find_oauth_profile_key(value: &Value) -> Option<String> {
    let obj = value.as_object()?;
    let looks_oauth = obj.keys().any(|k| {
        let lower = k.to_ascii_lowercase();
        matches!(
            lower.as_str(),
            "refresh_token"
                | "refreshtoken"
                | "refresh"
                | "email"
                | "user_id"
                | "userid"
                | "access_token"
                | "accesstoken"
        )
    });
    if looks_oauth {
        if let Some(key) = obj
            .get("key")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            return Some(key.to_string());
        }
    }
    for nested in obj.values() {
        if nested.is_object() || nested.is_array() {
            if let Some(found) = find_oauth_profile_key(nested) {
                return Some(found);
            }
        }
    }
    None
}

fn patch_oauth_secrets_into_value(target: &mut Value, source: &Value) {
    let access = find_access_token(source);
    let refresh = find_named_string(source, REFRESH_KEYS);
    patch_secrets(target, access.as_deref(), refresh.as_deref());
}

fn patch_secrets(value: &mut Value, access: Option<&str>, refresh: Option<&str>) {
    let Value::Object(map) = value else {
        if let Value::Array(items) = value {
            for item in items {
                patch_secrets(item, access, refresh);
            }
        }
        return;
    };
    let looks_oauth = map.keys().any(|k| {
        let lower = k.to_ascii_lowercase();
        matches!(
            lower.as_str(),
            "refresh_token"
                | "refreshtoken"
                | "refresh"
                | "email"
                | "user_id"
                | "userid"
                | "access_token"
                | "accesstoken"
        )
    });
    for (key, nested) in map.iter_mut() {
        if nested.is_string() {
            let lower = key.to_ascii_lowercase();
            if let Some(rt) = refresh {
                if lower == "refresh_token" || lower == "refreshtoken" || lower == "refresh" {
                    *nested = json!(rt);
                }
            }
            if let Some(at) = access {
                if lower == "access_token" || lower == "accesstoken" || lower == "access" {
                    *nested = json!(at);
                } else if looks_oauth && lower == "key" {
                    *nested = json!(at);
                }
            }
        } else if nested.is_object() || nested.is_array() {
            patch_secrets(nested, access, refresh);
        }
    }
}

fn live_for_cli_write(row: &Account, observed: Option<&LiveAccount>) -> LiveAccount {
    if let Some(observed) = observed {
        let mut live = observed.clone();
        patch_oauth_secrets_into_value(&mut live.credentials, &row.credentials);
        live.kind = row.kind;
        return live;
    }
    row.to_live()
}

fn log_oauth_file_sync(agent: AgentId, account_id: &str, action: OauthFileSyncAction) {
    tracing::debug!(
        module = targets::ACCOUNT,
        op = "oauth_file_sync",
        agent = agent.as_str(),
        account_id = %account_id,
        action = ?action,
        "oauth row/file sync"
    );
}

impl AccountService {
    pub(super) fn reconcile_oauth_row_with_cli_file(
        &self,
        adapter: &dyn AgentAdapter,
        agent: AgentId,
        existing: Account,
        live: LiveAccount,
        match_count: usize,
    ) -> Result<Option<Account>> {
        let action = match oauth_cli_file_path(adapter).and_then(|path| path_mtime_utc(&path)) {
            Some(file_mtime) => decide_oauth_file_sync(OauthFileSyncInput {
                row: &existing,
                file_credentials: &live.credentials,
                file_kind: live.kind,
                file_mtime,
            }),
            // File mtime unavailable: keep following the snapshot we just read.
            None => OauthFileSyncAction::WriteRow,
        };
        log_oauth_file_sync(agent, &existing.id, action);
        match action {
            OauthFileSyncAction::Skip => {
                self.finish_live_row_update(adapter, agent, existing, live, match_count)
            }
            OauthFileSyncAction::Noop => {
                let existing = self.clear_oauth_file_sync_attention(existing)?;
                self.finish_oauth_sync_row(adapter, agent, existing, match_count)
            }
            OauthFileSyncAction::WriteRow => {
                self.finish_live_row_update(adapter, agent, existing, live, match_count)
            }
            OauthFileSyncAction::WriteFile => {
                self.apply_oauth_row_to_cli_file(adapter, &existing, Some(&live))?;
                let existing = self.clear_oauth_file_sync_attention(existing)?;
                self.finish_oauth_sync_row(adapter, agent, existing, match_count)
            }
            OauthFileSyncAction::NeedsAttention => {
                tracing::warn!(
                    module = targets::ACCOUNT,
                    op = "oauth_file_sync",
                    agent = agent.as_str(),
                    account_id = %existing.id,
                    "oauth row and CLI login file have equal mtime but different refresh tokens; leaving both unchanged"
                );
                self.mark_oauth_file_sync_needs_attention(&existing)
                    .map(Some)
            }
        }
    }

    /// After a Hub-owned refresh this process performed: write the official
    /// file only when this row is the same grant and newer than the file.
    pub(super) fn sync_refreshed_oauth_row_to_cli_file(&self, row: &Account) -> Result<()> {
        if row.kind != AccountKind::Oauth || !supports_oauth_file_sync(row.agent_id) {
            return Ok(());
        }
        let process_lock = live_reconcile_lock(row.agent_id);
        let _process_lock = process_lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let _file_lock = self.acquire_live_lock(row.agent_id)?;
        let adapter = self.adapter(row.agent_id)?;
        let live = match self.read_live_accounts(adapter.as_ref(), row.agent_id) {
            Ok(mut lives) => lives.pop(),
            Err(_) => None,
        };
        let Some(live) = live else {
            return Ok(());
        };
        let Some(file_mtime) =
            oauth_cli_file_path(adapter.as_ref()).and_then(|p| path_mtime_utc(&p))
        else {
            return Ok(());
        };
        let action = decide_oauth_file_sync(OauthFileSyncInput {
            row,
            file_credentials: &live.credentials,
            file_kind: live.kind,
            file_mtime,
        });
        log_oauth_file_sync(row.agent_id, &row.id, action);
        match action {
            OauthFileSyncAction::WriteFile => {
                self.apply_oauth_row_to_cli_file(adapter.as_ref(), row, Some(&live))?;
            }
            OauthFileSyncAction::NeedsAttention => {
                tracing::warn!(
                    module = targets::ACCOUNT,
                    op = "oauth_file_sync",
                    agent = row.agent_id.as_str(),
                    account_id = %row.id,
                    "oauth row and CLI login file have equal mtime but different refresh tokens; leaving both unchanged"
                );
                let _ = self.mark_oauth_file_sync_needs_attention(row)?;
            }
            // Do not copy the file back over a refresh this process just wrote.
            OauthFileSyncAction::WriteRow
            | OauthFileSyncAction::Noop
            | OauthFileSyncAction::Skip => {}
        }
        Ok(())
    }

    fn apply_oauth_row_to_cli_file(
        &self,
        adapter: &dyn AgentAdapter,
        row: &Account,
        observed: Option<&LiveAccount>,
    ) -> Result<()> {
        let live = live_for_cli_write(row, observed);
        adapter.apply_account(&live)
    }

    fn finish_live_row_update(
        &self,
        adapter: &dyn AgentAdapter,
        agent: AgentId,
        existing: Account,
        live: LiveAccount,
        match_count: usize,
    ) -> Result<Option<Account>> {
        let (row, changed) = self.update_live_row(adapter, existing, live);
        if match_count > 1 {
            return self.collapse_oauth_sync_matches(adapter, agent, row);
        }
        Ok(Some(self.persist_reconciled_live_row(agent, row, changed)?))
    }

    fn finish_oauth_sync_row(
        &self,
        adapter: &dyn AgentAdapter,
        agent: AgentId,
        row: Account,
        match_count: usize,
    ) -> Result<Option<Account>> {
        if match_count > 1 {
            return self.collapse_oauth_sync_matches(adapter, agent, row);
        }
        Ok(Some(self.persist_reconciled_live_row(agent, row, false)?))
    }

    fn collapse_oauth_sync_matches(
        &self,
        adapter: &dyn AgentAdapter,
        agent: AgentId,
        row: Account,
    ) -> Result<Option<Account>> {
        let mark_current = agent != AgentId::Pi;
        self.commit_authorization_merge(
            adapter,
            &row,
            row.kind,
            row.label.clone(),
            row.credentials.clone(),
            row.extra.clone(),
            mark_current,
        )
        .map(|committed| Some(committed.stored))
        .map_err(|error| error.into_error())
    }

    fn mark_oauth_file_sync_needs_attention(&self, account: &Account) -> Result<Account> {
        let mut row = account.clone();
        if !row.extra.is_object() {
            row.extra = json!({});
        }
        if let Some(obj) = row.extra.as_object_mut() {
            obj.insert("health".into(), json!("needs_attention"));
        }
        let expected = row.updated_at.clone();
        self.persist_healed_fields(&row, &expected)
    }

    fn clear_oauth_file_sync_attention(&self, mut row: Account) -> Result<Account> {
        let Some(obj) = row.extra.as_object_mut() else {
            return Ok(row);
        };
        if obj.get("health").and_then(|v| v.as_str()) != Some("needs_attention") {
            return Ok(row);
        }
        obj.remove("health");
        let expected = row.updated_at.clone();
        self.persist_healed_fields(&row, &expected)
    }
}

#[cfg(test)]
mod tests;
