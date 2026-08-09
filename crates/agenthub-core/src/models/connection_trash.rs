//! Recoverable AgentHub connection records.

use serde::{Deserialize, Serialize};

use super::{Account, AgentId, Provider};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConnectionTrashKind {
    Account,
    Provider,
}

impl ConnectionTrashKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Account => "account",
            Self::Provider => "provider",
        }
    }

    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "account" => Some(Self::Account),
            "provider" => Some(Self::Provider),
            _ => None,
        }
    }
}

/// A deleted AgentHub connection.  The core keeps the complete row for
/// restore; callers must use [`Self::redacted`] before returning it to UI.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionTrashItem {
    pub id: String,
    pub agent_id: AgentId,
    pub kind: ConnectionTrashKind,
    pub source_id: String,
    pub label: String,
    pub was_current: bool,
    pub deleted_at: String,
    pub expires_at: String,
    pub account: Option<Account>,
    pub provider: Option<Provider>,
}

impl ConnectionTrashItem {
    pub fn redacted(&self) -> Self {
        Self {
            id: self.id.clone(),
            agent_id: self.agent_id,
            kind: self.kind,
            source_id: self.source_id.clone(),
            label: self.label.clone(),
            was_current: self.was_current,
            deleted_at: self.deleted_at.clone(),
            expires_at: self.expires_at.clone(),
            account: self.account.as_ref().map(Account::redacted),
            provider: self.provider.as_ref().map(Provider::redacted),
        }
    }
}
