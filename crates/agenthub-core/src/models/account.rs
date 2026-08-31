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

fn json_str<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

fn json_bool(value: &Value, key: &str) -> Option<bool> {
    value.get(key).and_then(Value::as_bool)
}

fn json_i64(value: &Value, key: &str) -> Option<i64> {
    value.get(key).and_then(Value::as_i64)
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
            } else if extra
                .get("secretTail")
                .and_then(Value::as_str)
                .map(str::trim)
                .unwrap_or("")
                .is_empty()
            {
                let from_preview = self
                    .identity_label()
                    .and_then(crate::utils::redact::secret_tail_from_masked_preview)
                    .or_else(|| crate::utils::redact::secret_tail_from_masked_preview(&self.label));
                if let Some(tail) = from_preview {
                    insert_extra_string(&mut extra, "secretTail", tail);
                }
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

    /// `extra.source` — Hub PKCE vs CLI-owned grant provenance. Unknown keys stay on `extra`.
    pub fn source(&self) -> Option<&str> {
        json_str(&self.extra, "source")
    }

    /// `extra.home` — `route_pool` stays off the Connections ticket list.
    pub fn home(&self) -> Option<&str> {
        json_str(&self.extra, "home")
    }

    pub fn extra_provider(&self) -> Option<&str> {
        json_str(&self.extra, "provider")
    }

    pub fn extra_email(&self) -> Option<&str> {
        json_str(&self.extra, "email")
    }

    pub fn identity_label(&self) -> Option<&str> {
        json_str(&self.extra, "identityLabel")
    }

    pub fn subscription(&self) -> Option<&str> {
        json_str(&self.extra, "subscription")
    }

    pub fn extra_health(&self) -> Option<&str> {
        json_str(&self.extra, "health")
    }

    pub fn extra_auth_health(&self) -> Option<&str> {
        json_str(&self.extra, "authHealth")
    }

    pub fn extra_auth_source(&self) -> Option<&str> {
        json_str(&self.extra, "authSource")
    }

    pub fn extra_live_revision(&self) -> Option<&str> {
        json_str(&self.extra, "liveRevision")
    }

    pub fn token_expired(&self) -> Option<bool> {
        json_bool(&self.extra, "tokenExpired")
    }

    pub fn extra_expires_at(&self) -> Option<&str> {
        json_str(&self.extra, "expiresAt")
    }

    pub fn quota_5h_pct(&self) -> Option<i64> {
        json_i64(&self.extra, "quota5hPct")
    }

    pub fn quota_7d_pct(&self) -> Option<i64> {
        json_i64(&self.extra, "quota7dPct")
    }

    pub fn quota_reset_in(&self) -> Option<&str> {
        json_str(&self.extra, "quotaResetIn")
    }

    pub fn credential_format(&self) -> Option<&str> {
        json_str(&self.credentials, "format")
    }

    pub fn credential_provider(&self) -> Option<&str> {
        json_str(&self.credentials, "provider")
    }

    pub fn credential_email(&self) -> Option<&str> {
        json_str(&self.credentials, "email")
    }

    pub fn credential_env_key(&self) -> Option<&str> {
        json_str(&self.credentials, "env_key")
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
mod tests;
