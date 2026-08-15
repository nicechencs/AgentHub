//! AgentAdapter trait and shared authorization / identity defaults.

use std::path::{Path, PathBuf};

use crate::error::{AppError, Result};
use crate::models::{
    AccountKind, AgentConfig, AgentId, AuthState, Capability, CapabilityState, DetectResult,
    InstallChannel, LiveAccount, RunOptions, RunSpec,
};

pub trait AgentAdapter: Send + Sync {
    fn id(&self) -> AgentId;
    fn detect(&self) -> DetectResult;
    /// Product install channels. Production adapters use the catalog contribution.
    fn install_channels(&self) -> Vec<InstallChannel> {
        crate::catalog::install::adapter_install_channels(self.id())
    }
    fn read_config(&self) -> Result<AgentConfig>;
    /// Atomically replace the agent's live provider configuration.
    ///
    /// Adapters that do not implement a safe writer must fail closed.
    fn write_config(&self, _config: &AgentConfig) -> Result<()> {
        Err(AppError::Unsupported(format!(
            "live config writes are not supported for {}",
            self.id().as_str()
        )))
    }
    fn read_auth(&self) -> Result<AuthState>;

    /// Read live file credentials into an opaque snapshot for the account pool.
    ///
    /// Must fail closed with [`AppError::Unsupported`] when credentials cannot
    /// be reliably located (never guess paths).
    fn read_account(&self) -> Result<LiveAccount> {
        Err(AppError::Unsupported(format!(
            "account read is not supported for {}",
            self.id().as_str()
        )))
    }

    /// Atomically apply stored account credentials to live files.
    fn apply_account(&self, _account: &LiveAccount) -> Result<()> {
        Err(AppError::Unsupported(format!(
            "account apply is not supported for {}",
            self.id().as_str()
        )))
    }

    /// Build an API-key live snapshot for `account add-apikey` (no live write).
    fn build_api_key_account(&self, _api_key: &str) -> Result<LiveAccount> {
        Err(AppError::Unsupported(format!(
            "API key accounts are not supported for {}",
            self.id().as_str()
        )))
    }

    /// Authorization fingerprint: same "ticket" only (for pool dedupe).
    ///
    /// Same person with two different OAuth grants must return **different** keys.
    /// Same live re-import must return the **same** key. See `docs/account-authorization-pool.md`.
    fn authorization_key(
        &self,
        kind: AccountKind,
        credentials: &serde_json::Value,
    ) -> Option<String> {
        default_authorization_key(kind, credentials)
    }

    /// Identity label for UI grouping only — never used for dedupe/delete.
    fn identity_label(
        &self,
        kind: AccountKind,
        credentials: &serde_json::Value,
        label_hint: Option<&str>,
    ) -> Option<String> {
        default_identity_label(kind, credentials, label_hint)
    }

    fn skills_dir(&self) -> Option<PathBuf>;
    fn live_backup_paths(&self) -> Vec<PathBuf>;

    /// Build a non-interactive headless run command for this agent.
    fn build_run_spec(&self, binary: &Path, prompt: &str, opts: &RunOptions) -> Result<RunSpec>;

    /// Declared capability for this agent. Exhaustive match required — no `_ =>`.
    fn capability(&self, cap: Capability) -> CapabilityState;
}


/// Shared default for [`AgentAdapter::authorization_key`].
///
/// - ApiKey: hash of `api_key`
/// - OAuth: refresh_token hash → access-like token hash → full credentials hash
///
/// Never uses email/user_id (identity ≠ authorization).
pub fn default_authorization_key(
    kind: AccountKind,
    credentials: &serde_json::Value,
) -> Option<String> {
    match kind {
        AccountKind::ApiKey => {
            let key = extract_api_key(credentials)?;
            Some(format!("apikey:sha256:{}", short_sha(&key)))
        }
        AccountKind::Oauth => {
            if let Some(refresh) =
                find_string_field(credentials, &["refresh_token", "refreshToken", "refresh"])
            {
                return Some(format!("oauth:refresh_sha:{}", short_sha(&refresh)));
            }
            if let Some(access) = find_string_field(
                credentials,
                &[
                    "access_token",
                    "accessToken",
                    "access",
                    "id_token",
                    "idToken",
                    // Grok / some OIDC bodies store the bearer under `key`
                    "key",
                ],
            ) {
                return Some(format!("oauth:access_sha:{}", short_sha(&access)));
            }
            let raw = serde_json::to_string(credentials).ok()?;
            Some(format!("oauth:cred_sha:{}", short_sha(&raw)))
        }
    }
}

/// Shared default for [`AgentAdapter::identity_label`] (display only).
pub fn default_identity_label(
    _kind: AccountKind,
    credentials: &serde_json::Value,
    label_hint: Option<&str>,
) -> Option<String> {
    if let Some(s) = find_string_field(
        credentials,
        &[
            "email",
            "email_address",
            "emailAddress",
            "user_id",
            "userId",
            "principal_id",
            "principalId",
            "sub",
            "account_id",
            "accountId",
            "account_uuid",
        ],
    ) {
        return Some(s);
    }
    label_hint
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

fn extract_api_key(credentials: &serde_json::Value) -> Option<String> {
    credentials
        .get("api_key")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

fn short_sha(input: &str) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(input.as_bytes());
    // 16 hex chars is enough to avoid collisions in a local pool
    digest.iter().take(8).map(|b| format!("{b:02x}")).collect()
}

/// Find first non-empty string for any of `keys` at top-level, under `body`,
/// under `body.tokens`, or one level of provider-keyed objects under `body`.
fn find_string_field(credentials: &serde_json::Value, keys: &[&str]) -> Option<String> {
    fn from_map(obj: &serde_json::Map<String, serde_json::Value>, keys: &[&str]) -> Option<String> {
        for key in keys {
            if let Some(s) = obj
                .get(*key)
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                return Some(s.to_string());
            }
        }
        None
    }

    if let Some(obj) = credentials.as_object() {
        if let Some(s) = from_map(obj, keys) {
            return Some(s);
        }
    }
    let body = credentials.get("body")?;
    if let Some(obj) = body.as_object() {
        if let Some(s) = from_map(obj, keys) {
            return Some(s);
        }
        if let Some(tokens) = obj.get("tokens").and_then(|v| v.as_object()) {
            if let Some(s) = from_map(tokens, keys) {
                return Some(s);
            }
        }
        for nested in obj.values() {
            if let Some(nobj) = nested.as_object() {
                if let Some(s) = from_map(nobj, keys) {
                    return Some(s);
                }
            }
        }
    }
    None
}
