//! Bidirectional sync of one OAuth pool row with the official CLI login file.
//!
//! Compares refresh-token equality in memory and `updated_at` vs file mtime
//! (not token `expires_at`). Raw refresh tokens are never logged.

use std::cmp::Ordering;
use std::path::{Path, PathBuf};

use chrono::{DateTime, NaiveDateTime, Utc};
use serde_json::{json, Value};

use crate::adapters::AgentAdapter;
use crate::error::{AppError, Result};
use crate::logging::targets;
use crate::models::{Account, AccountKind, AgentId, LiveAccount};

use super::oauth_owner::oauth_grant_is_hub_owned;
use super::surface::*;
use super::AccountService;

const OAUTH_FILE_SYNC_EXTRA: &str = "oauthFileSync";
const OAUTH_FILE_SYNC_NEEDS_ATTENTION: &str = "needs_attention";

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
    /// Equal mtime with different or unknown refresh tokens: do not auto-overwrite.
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

    let file_slice = matching_oauth_slice(file_credentials, row).unwrap_or(file_credentials);
    let row_rt = find_named_string(&row.credentials, REFRESH_KEYS);
    let file_rt = find_named_string(file_slice, REFRESH_KEYS);
    // Missing rts are unknown lineage, not equal. Same person still matches via identity.
    let same_rt = matches!((row_rt.as_deref(), file_rt.as_deref()), (Some(a), Some(b)) if a == b);
    let rts_differ =
        matches!((row_rt.as_deref(), file_rt.as_deref()), (Some(a), Some(b)) if a != b);
    let same_identity = accounts_same_oauth_identity(row.kind, file_slice, row)
        || accounts_same_oauth_identity(row.kind, file_credentials, row);
    if !same_identity && !same_rt {
        return OauthFileSyncAction::Skip;
    }

    let row_access = find_access_token(&row.credentials);
    let file_access = find_access_token(file_slice);
    let secrets_equal = (same_rt && row_access.as_deref() == file_access.as_deref())
        || (row_rt.is_none()
            && file_rt.is_none()
            && row_access.is_some()
            && row_access.as_deref() == file_access.as_deref());
    if secrets_equal {
        return OauthFileSyncAction::Noop;
    }

    let Some(row_ts) = parse_account_timestamp(&row.updated_at) else {
        return OauthFileSyncAction::NeedsAttention;
    };
    match row_ts.cmp(&file_mtime) {
        Ordering::Greater => OauthFileSyncAction::WriteFile,
        Ordering::Less => OauthFileSyncAction::WriteRow,
        Ordering::Equal if same_rt => OauthFileSyncAction::Noop,
        Ordering::Equal if rts_differ || !same_rt => OauthFileSyncAction::NeedsAttention,
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

fn matching_oauth_slice<'a>(file_credentials: &'a Value, row: &Account) -> Option<&'a Value> {
    fn walk<'a>(value: &'a Value, row: &Account, found: &mut Option<&'a Value>) {
        if found.is_some() {
            return;
        }
        match value {
            Value::Object(map) => {
                if looks_like_oauth_profile(map)
                    && accounts_same_oauth_identity(AccountKind::Oauth, value, row)
                {
                    *found = Some(value);
                    return;
                }
                for nested in map.values() {
                    walk(nested, row, found);
                }
            }
            Value::Array(items) => {
                for item in items {
                    walk(item, row, found);
                }
            }
            _ => {}
        }
    }
    let mut found = None;
    walk(file_credentials, row, &mut found);
    found
}

fn looks_like_oauth_profile(map: &serde_json::Map<String, Value>) -> bool {
    map.keys().any(|key| {
        matches!(
            key.to_ascii_lowercase().as_str(),
            "refresh_token"
                | "refreshtoken"
                | "refresh"
                | "access_token"
                | "accesstoken"
                | "access"
        )
    }) || (map.contains_key("key")
        && map.keys().any(|key| {
            matches!(
                key.to_ascii_lowercase().as_str(),
                "email" | "user_id" | "userid" | "sub" | "refresh_token" | "refreshtoken"
            )
        }))
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

fn patch_oauth_secrets_into_value(target: &mut Value, source: &Value) -> bool {
    let access = find_access_token(source);
    let refresh = find_named_string(source, REFRESH_KEYS);
    patch_matching_profiles(target, source, access.as_deref(), refresh.as_deref())
}

fn patch_matching_profiles(
    value: &mut Value,
    identity: &Value,
    access: Option<&str>,
    refresh: Option<&str>,
) -> bool {
    match value {
        Value::Object(map) => {
            if looks_like_oauth_profile(map) {
                let snapshot = Value::Object(map.clone());
                // Identity-matching Grok slots, or Codex `tokens` maps with no email/sub.
                if oauth_credentials_same_identity(&snapshot, identity)
                    || oauth_credentials_identity_unknown(&snapshot)
                {
                    apply_secret_fields(map, access, refresh);
                    return true;
                }
                return false;
            }
            let mut patched = false;
            for nested in map.values_mut() {
                patched |= patch_matching_profiles(nested, identity, access, refresh);
            }
            patched
        }
        Value::Array(items) => {
            let mut patched = false;
            for item in items {
                patched |= patch_matching_profiles(item, identity, access, refresh);
            }
            patched
        }
        _ => false,
    }
}

fn apply_secret_fields(
    map: &mut serde_json::Map<String, Value>,
    access: Option<&str>,
    refresh: Option<&str>,
) {
    let looks_oauth = looks_like_oauth_profile(map);
    for (key, nested) in map.iter_mut() {
        if !nested.is_string() {
            continue;
        }
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
    }
}

/// Hub write-back applies **this row's** grant, not the full live snapshot.
/// Grok/Claude: PKCE / single-profile row so on-disk merge pins email/`user_id`/`sub`.
/// Codex: patch `body.tokens` on the observed file (token-only, no identity) when
/// present so extra auth.json keys survive; otherwise write the row.
fn live_for_cli_write(row: &Account, observed: Option<&LiveAccount>) -> LiveAccount {
    if matches!(row.agent_id, AgentId::Grok | AgentId::Claude) {
        return row.to_live();
    }
    if let Some(observed) = observed {
        let mut live = observed.clone();
        if patch_oauth_secrets_into_value(&mut live.credentials, &row.credentials) {
            live.kind = row.kind;
            return live;
        }
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
        activate: bool,
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
            OauthFileSyncAction::Skip | OauthFileSyncAction::WriteFile => {
                // List never writes the CLI file. Hub refresh write-back is
                // `sync_refreshed_oauth_row_to_cli_file`. Skip does not copy live secrets.
                self.keep_existing_oauth_row(adapter, agent, existing, match_count, activate)
            }
            OauthFileSyncAction::Noop => {
                let existing = self.clear_oauth_file_sync_attention(existing)?;
                self.finish_oauth_sync_row(adapter, agent, existing, match_count, activate)
            }
            OauthFileSyncAction::WriteRow => {
                self.finish_live_row_update(adapter, agent, existing, live, match_count, activate)
            }
            OauthFileSyncAction::NeedsAttention => {
                tracing::warn!(
                    module = targets::ACCOUNT,
                    op = "oauth_file_sync",
                    agent = agent.as_str(),
                    account_id = %existing.id,
                    "oauth row and CLI login file conflict (equal mtime with different or unknown refresh tokens); leaving both unchanged"
                );
                self.mark_oauth_file_sync_needs_attention(&existing)
                    .map(Some)
            }
        }
    }

    /// After a Hub-owned refresh this process performed: write the official
    /// file only when this row is the same grant and newer than the file.
    pub(super) fn sync_refreshed_oauth_row_to_cli_file(&self, row: &Account) -> Result<()> {
        if row.kind != AccountKind::Oauth
            || !supports_oauth_file_sync(row.agent_id)
            || !oauth_grant_is_hub_owned(row)
        {
            return Ok(());
        }
        let process_lock = live_reconcile_lock(row.agent_id);
        let _process_lock = process_lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let _file_lock = self.acquire_live_lock(row.agent_id)?;
        let adapter = self.adapter(row.agent_id)?;
        let live = match self.read_live_accounts(adapter.as_ref(), row.agent_id) {
            Ok(lives) => lives.into_iter().find(|live| {
                live.kind == row.kind
                    && (accounts_same_authorization(
                        adapter.as_ref(),
                        row.kind,
                        &live.credentials,
                        row,
                    ) || accounts_same_oauth_identity(row.kind, &live.credentials, row))
            }),
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
                    "oauth row and CLI login file conflict (equal mtime with different or unknown refresh tokens); leaving both unchanged"
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
        if !oauth_grant_is_hub_owned(row) {
            return Ok(());
        }
        let live = live_for_cli_write(row, observed);
        adapter.apply_account(&live)?;
        let _ = self.clear_oauth_file_sync_attention(row.clone());
        Ok(())
    }

    fn keep_existing_oauth_row(
        &self,
        adapter: &dyn AgentAdapter,
        agent: AgentId,
        row: Account,
        match_count: usize,
        activate: bool,
    ) -> Result<Option<Account>> {
        if match_count > 1 {
            return self.collapse_oauth_sync_matches(adapter, agent, row, activate);
        }
        Ok(Some(row))
    }

    fn finish_live_row_update(
        &self,
        adapter: &dyn AgentAdapter,
        agent: AgentId,
        existing: Account,
        live: LiveAccount,
        match_count: usize,
        activate: bool,
    ) -> Result<Option<Account>> {
        let (row, changed) = self.update_live_row(adapter, existing, live);
        if match_count > 1 {
            return self.collapse_oauth_sync_matches(adapter, agent, row, activate);
        }
        Ok(Some(self.persist_reconciled_live_row(
            agent, row, changed, activate,
        )?))
    }

    fn finish_oauth_sync_row(
        &self,
        adapter: &dyn AgentAdapter,
        agent: AgentId,
        row: Account,
        match_count: usize,
        activate: bool,
    ) -> Result<Option<Account>> {
        if match_count > 1 {
            return self.collapse_oauth_sync_matches(adapter, agent, row, activate);
        }
        Ok(Some(self.persist_reconciled_live_row(
            agent, row, false, activate,
        )?))
    }

    fn collapse_oauth_sync_matches(
        &self,
        adapter: &dyn AgentAdapter,
        agent: AgentId,
        row: Account,
        activate: bool,
    ) -> Result<Option<Account>> {
        let mark_current = activate && agent != AgentId::Pi;
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

    pub(super) fn mark_oauth_file_sync_needs_attention(&self, account: &Account) -> Result<Account> {
        let mut extra = account.extra.clone();
        if !extra.is_object() {
            extra = json!({});
        }
        if extra.get(OAUTH_FILE_SYNC_EXTRA).and_then(|v| v.as_str())
            == Some(OAUTH_FILE_SYNC_NEEDS_ATTENTION)
        {
            return Ok(account.clone());
        }
        if let Some(obj) = extra.as_object_mut() {
            obj.insert(
                OAUTH_FILE_SYNC_EXTRA.into(),
                json!(OAUTH_FILE_SYNC_NEEDS_ATTENTION),
            );
        }
        self.persist_extra_keep_timestamp(account, extra)
    }

    fn clear_oauth_file_sync_attention(&self, row: Account) -> Result<Account> {
        let Some(obj) = row.extra.as_object() else {
            return Ok(row);
        };
        if obj.get(OAUTH_FILE_SYNC_EXTRA).and_then(|v| v.as_str())
            != Some(OAUTH_FILE_SYNC_NEEDS_ATTENTION)
        {
            return Ok(row);
        }
        let mut extra = row.extra.clone();
        if let Some(obj) = extra.as_object_mut() {
            obj.remove(OAUTH_FILE_SYNC_EXTRA);
        }
        self.persist_extra_keep_timestamp(&row, extra)
    }

    /// Persist extra only. Must not bump `updated_at` or heal/quota clocks win the file.
    fn persist_extra_keep_timestamp(&self, account: &Account, extra: Value) -> Result<Account> {
        let extra_json = serde_json::to_string(&extra)?;
        let changed = self.db.with_conn(|conn| {
            conn.execute(
                "UPDATE accounts SET extra = ?2 WHERE id = ?1 AND agent_id = ?3 AND updated_at = ?4",
                rusqlite::params![
                    &account.id,
                    extra_json,
                    account.agent_id.as_str(),
                    &account.updated_at,
                ],
            )
            .map_err(AppError::from)
        })?;
        if changed != 1 {
            return self
                .repo
                .get_by_id(&account.id)?
                .ok_or_else(|| AppError::NotFound(format!("account not found: {}", account.id)));
        }
        self.repo
            .get_by_id(&account.id)?
            .ok_or_else(|| AppError::NotFound(format!("account not found: {}", account.id)))
    }
}

#[cfg(test)]
mod tests;
