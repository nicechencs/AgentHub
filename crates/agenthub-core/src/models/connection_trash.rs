//! Recoverable AgentHub connection records.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{authorization_is_route_pool_home, Account, AdapterSourceKind, AgentId, Provider};

/// Connections-page recycle bin. Default for rows without `home=route_pool`.
pub const TRASH_HOME_CONNECTIONS: &str = "connections";
/// Connection-pool recycle bin. Independent from [`TRASH_HOME_CONNECTIONS`].
pub const TRASH_HOME_ROUTE_POOL: &str = "route_pool";

pub fn trash_home_from_authorization_blob(blob: &Value) -> &'static str {
    if authorization_is_route_pool_home(blob) {
        TRASH_HOME_ROUTE_POOL
    } else {
        TRASH_HOME_CONNECTIONS
    }
}

fn default_trash_home() -> String {
    TRASH_HOME_CONNECTIONS.to_string()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConnectionTrashKind {
    Account,
    Provider,
    Membership,
}

impl ConnectionTrashKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Account => "account",
            Self::Provider => "provider",
            Self::Membership => "membership",
        }
    }

    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "account" => Some(Self::Account),
            "provider" => Some(Self::Provider),
            "membership" => Some(Self::Membership),
            _ => None,
        }
    }
}

/// Pool membership snapshot. The Connections login is left in place.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteMembershipTrashPayload {
    pub source_kind: AdapterSourceKind,
    pub source_id: String,
    pub members: Vec<RouteMembershipTrashMember>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteMembershipTrashMember {
    pub route_pool_id: String,
    pub enabled: bool,
    pub priority: i64,
    pub position: i64,
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
    #[serde(default = "default_trash_home")]
    pub home: String,
    pub account: Option<Account>,
    pub provider: Option<Provider>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub membership: Option<RouteMembershipTrashPayload>,
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
            home: self.home.clone(),
            account: self.account.as_ref().map(Account::redacted),
            provider: self.provider.as_ref().map(Provider::redacted),
            membership: self.membership.clone(),
        }
    }
}
