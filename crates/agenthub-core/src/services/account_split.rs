//! Split mixed official-login + API Key snapshots.
//!
//! `grok_bundle` / `kimi_bundle` / `claude_bundle` are on-disk mixed snapshots.
//! The account pool stores one row per family.

use serde_json::Value;
use uuid::Uuid;

use crate::adapters::{
    default_authorization_key, expand_claude_live_accounts, expand_grok_auth_to_live_accounts,
    expand_kimi_live_accounts,
};
use crate::models::{Account, LiveAccount};

#[cfg(test)]
mod tests;

pub(crate) fn is_mixed_live_bundle(credentials: &Value) -> bool {
    matches!(
        credentials.get("format").and_then(Value::as_str),
        Some("grok_bundle" | "kimi_bundle" | "claude_bundle")
    )
}

/// Expand a mixed pool/recovery row into one account per login family.
///
/// The first row keeps `original.id` and `is_current`. Extra rows get new ids
/// and are never current. Unsplittable payloads stay a single clone.
pub(crate) fn split_mixed_account(account: &Account) -> Vec<Account> {
    if !is_mixed_live_bundle(&account.credentials) {
        return vec![account.clone()];
    }
    let lives = expand_stored_mixed_account(account);
    if lives.is_empty() {
        return vec![account.clone()];
    }
    // One extracted family still needs a rewrite so the pool does not keep
    // grok_bundle / kimi_bundle / claude_bundle after a partial snapshot.
    if lives.len() == 1 && is_mixed_live_bundle(&lives[0].credentials) {
        return vec![account.clone()];
    }
    lives
        .into_iter()
        .enumerate()
        .map(|(index, live)| account_from_split_live(account, live, index == 0))
        .collect()
}

pub(crate) fn accounts_share_authorization(left: &Account, right: &Account) -> bool {
    if left.kind != right.kind || left.agent_id != right.agent_id {
        return false;
    }
    if left.credentials == right.credentials {
        return true;
    }
    match (
        default_authorization_key(left.kind, &left.credentials),
        default_authorization_key(right.kind, &right.credentials),
    ) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}

pub(crate) fn new_split_account_id(agent: crate::models::AgentId) -> String {
    format!("{}-live-{}", agent.as_str(), Uuid::new_v4())
}

fn expand_stored_mixed_account(account: &Account) -> Vec<LiveAccount> {
    let live = account.to_live();
    match account.agent_id {
        crate::models::AgentId::Grok => expand_grok_auth_to_live_accounts(&live),
        crate::models::AgentId::Kimi => expand_kimi_live_accounts(&live),
        crate::models::AgentId::Claude => expand_claude_live_accounts(&live),
        _ => vec![live],
    }
}

fn account_from_split_live(original: &Account, live: LiveAccount, keep_id: bool) -> Account {
    let id = if keep_id {
        original.id.clone()
    } else {
        new_split_account_id(original.agent_id)
    };
    let label = live
        .label_hint
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(original.label.as_str())
        .to_string();
    Account {
        id,
        agent_id: original.agent_id,
        kind: live.kind,
        label,
        credentials: live.credentials,
        extra: live.extra,
        status: original.status.clone(),
        is_current: keep_id && original.is_current,
        created_at: original.created_at.clone(),
        updated_at: original.updated_at.clone(),
    }
}
