//! Account pool models (serde). No business logic.
//!
//! Credentials are stored as opaque JSON using the existing project storage
//! scheme (no additional at-rest encryption in this version).

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::{AgentId, BackupRecord};
use crate::utils::redact::{api_key_tail, redact_json, refresh_token_preview, refresh_token_tail};

/// How an account authenticates against an agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AccountKind {
    Oauth,
    ApiKey,
}

impl AccountKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Oauth => "oauth",
            Self::ApiKey => "apikey",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "oauth" => Some(Self::Oauth),
            "apikey" | "api_key" | "api-key" => Some(Self::ApiKey),
            _ => None,
        }
    }
}

impl std::fmt::Display for AccountKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Persisted L1 account row (`accounts` table).
///
/// `credentials` stays opaque JSON — field shapes differ per agent/kind.
/// Always call [`Account::redacted`] before serializing to users.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub id: String,
    pub agent_id: AgentId,
    pub kind: AccountKind,
    pub label: String,
    pub credentials: Value,
    pub extra: Value,
    pub status: String,
    pub is_current: bool,
    pub created_at: String,
    pub updated_at: String,
}

/// Write-side input for creating an account row (service owns id/timestamps).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountInput {
    pub agent_id: AgentId,
    pub kind: AccountKind,
    pub label: String,
    pub credentials: Value,
    pub extra: Value,
    pub is_current: bool,
}

/// Opaque live credential snapshot returned/consumed by adapters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveAccount {
    pub agent: AgentId,
    pub kind: AccountKind,
    /// Full secrets for storage/apply. Never log or return unredacted.
    pub credentials: Value,
    pub label_hint: Option<String>,
    pub extra: Value,
}

/// Outcome of applying a saved account to an agent's live credential files.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountSwitchResult {
    pub account: Account,
    /// Snapshot created before the live write. `None` when no live files existed.
    pub backup: Option<BackupRecord>,
    pub backfilled_account_id: Option<String>,
}

fn insert_extra_string(extra: &mut Value, key: &str, value: String) {
    if let Value::Object(map) = extra {
        map.insert(key.into(), json!(value));
        return;
    }
    *extra = json!({ key: value });
}

impl Account {
    /// Deep-copy with likely secret keys redacted.
    pub fn redacted(&self) -> Self {
        let credentials = redact_json(&self.credentials);
        let mut extra = redact_json(&self.extra);
        if self.kind == AccountKind::Oauth {
            if let Some(preview) = refresh_token_preview(&self.credentials) {
                insert_extra_string(&mut extra, "refreshTokenPreview", preview);
            }
            if let Some(tail) = refresh_token_tail(&self.credentials) {
                insert_extra_string(&mut extra, "secretTail", tail);
            }
        }
        if self.kind == AccountKind::ApiKey {
            if let Some(tail) = api_key_tail(&self.credentials) {
                insert_extra_string(&mut extra, "secretTail", tail);
            }
            if let Some(hash) = crate::utils::redact::api_key_secret_hash(&self.credentials) {
                insert_extra_string(&mut extra, "secretHash", hash);
            }
        }
        Self {
            id: self.id.clone(),
            agent_id: self.agent_id,
            kind: self.kind,
            label: self.label.clone(),
            credentials,
            extra,
            status: self.status.clone(),
            is_current: self.is_current,
            created_at: self.created_at.clone(),
            updated_at: self.updated_at.clone(),
        }
    }

    /// Convert a stored account into the adapter live snapshot shape.
    pub fn to_live(&self) -> LiveAccount {
        LiveAccount {
            agent: self.agent_id,
            kind: self.kind,
            credentials: self.credentials.clone(),
            label_hint: Some(self.label.clone()),
            extra: self.extra.clone(),
        }
    }
}

impl AccountSwitchResult {
    pub fn redacted(&self) -> Self {
        Self {
            account: self.account.redacted(),
            backup: self.backup.clone(),
            backfilled_account_id: self.backfilled_account_id.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn account_kind_parse_and_serde() {
        assert_eq!(AccountKind::parse("oauth"), Some(AccountKind::Oauth));
        assert_eq!(AccountKind::parse("APIKEY"), Some(AccountKind::ApiKey));
        assert_eq!(AccountKind::parse("api_key"), Some(AccountKind::ApiKey));
        assert_eq!(AccountKind::parse("nope"), None);
        assert_eq!(
            serde_json::to_string(&AccountKind::ApiKey).unwrap(),
            "\"apikey\""
        );
        assert_eq!(
            serde_json::from_str::<AccountKind>("\"oauth\"").unwrap(),
            AccountKind::Oauth
        );
    }

    #[test]
    fn account_redacted_masks_credentials() {
        let a = Account {
            id: "a1".into(),
            agent_id: AgentId::Grok,
            kind: AccountKind::ApiKey,
            label: "xai-••••e41d".into(),
            credentials: json!({"format": "api_key", "api_key": "xai-secret-value"}),
            extra: json!({"token": "t", "note": "ok"}),
            status: "active".into(),
            is_current: true,
            created_at: "t0".into(),
            updated_at: "t1".into(),
        };
        let r = a.redacted();
        assert_eq!(r.credentials["api_key"], "***");
        assert_eq!(r.extra["token"], "***");
        assert_eq!(r.extra["note"], "ok");
        assert_eq!(r.extra["secretTail"], "**alue");
        assert_eq!(a.credentials["api_key"], "xai-secret-value");
    }
}
