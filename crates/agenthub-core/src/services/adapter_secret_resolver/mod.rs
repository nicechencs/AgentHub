//! In-memory secret materialization for explicitly generated adapter providers.
//!
//! Split for maintainability only — public path stays
//! [`crate::services::AdapterSecretResolver`].

mod helpers;
mod materialize;
mod validate;

pub(super) use helpers::*;

#[cfg(test)]
mod tests;

use crate::error::{AppError, Result};
use crate::models::{AdapterSourceKind, AgentId, Provider};
use crate::services::adapter_route_constants::*;
use crate::storage::{AccountRepo, Database, ProviderRepo};
use serde_json::{json, Value};
use toml_edit::DocumentMut;

// Re-export so existing `adapter_secret_resolver::CONNECTION_SECRET_MARKER` paths keep working.
pub use crate::services::adapter_route_constants::CONNECTION_SECRET_MARKER;

use crate::services::adapter_route_constants::{
    DEEPSEEK_CLAUDE_RULE_ID, DEEPSEEK_CODEX_RULE_ID, DEEPSEEK_PI_RULE_ID, GLM_CLAUDE_RULE_ID,
    GLM_CODEX_RULE_ID, GLM_PI_RULE_ID, KIMI_CLAUDE_RULE_ID,
};

pub(super) const GENERATED_BY: &str = "adapter";
pub(super) const KIMI_TO_CLAUDE_RULE: &str = KIMI_CLAUDE_RULE_ID;
pub(super) const GLM_TO_CLAUDE_RULE: &str = GLM_CLAUDE_RULE_ID;
pub(super) const DEEPSEEK_TO_CLAUDE_RULE: &str = DEEPSEEK_CLAUDE_RULE_ID;
pub(super) const GLM_TO_CODEX_RULE: &str = GLM_CODEX_RULE_ID;
pub(super) const DEEPSEEK_TO_CODEX_RULE: &str = DEEPSEEK_CODEX_RULE_ID;
pub(super) const KIMI_TO_CODEX_BRIDGE_RULE: &str = "kimi-membership-to-codex-v1";
pub(super) const ANTHROPIC_TO_CODEX_BRIDGE_RULE: &str = "anthropic-api-to-codex-v1";
pub(super) const OPENAI_TO_CODEX_BRIDGE_RULE: &str = "openai-api-to-codex-v1";
pub(super) const CODEX_TO_CLAUDE_BRIDGE_RULE: &str = "codex-subscription-to-claude-responses-v1";
pub(super) const CODEX_TO_GROK_BRIDGE_RULE: &str = CODEX_GROK_RULE_ID;
pub(super) const CODEX_TO_KIMI_BRIDGE_RULE: &str = CODEX_KIMI_RULE_ID;
pub(super) const CODEX_TO_DSH_BRIDGE_RULE: &str = CODEX_DSH_RULE_ID;
pub(super) const KIMI_TO_GROK_RULE: &str = "kimi-membership-to-grok-v1";
pub(super) const OPENAI_TO_GROK_RULE: &str = "openai-api-to-grok-v1";
pub(super) const KIMI_TO_PI_RULE: &str = "kimi-membership-to-pi-v1";
pub(super) const ANTHROPIC_TO_PI_RULE: &str = "anthropic-api-to-pi-v1";
pub(super) const OPENAI_TO_PI_RULE: &str = "openai-api-to-pi-v1";
pub(super) const XAI_TO_PI_RULE: &str = "xai-api-to-pi-v1";
pub(super) const GLM_TO_PI_RULE: &str = GLM_PI_RULE_ID;
pub(super) const DEEPSEEK_TO_PI_RULE: &str = DEEPSEEK_PI_RULE_ID;
pub(super) const CLAUDE_SUBSCRIPTION_PI_RULE: &str = "claude-subscription-to-pi-v1";
pub(super) const CODEX_SUBSCRIPTION_PI_RULE: &str = "codex-subscription-to-pi-v1";
pub(super) const GROK_SUBSCRIPTION_PI_RULE: &str = "grok-subscription-to-pi-v1";
pub(super) const DEEPSEEK_TO_DSH_RULE: &str = "deepseek-api-to-dsh-v1";
pub(super) const SOURCE_REFERENCE_MODE: &str = "source_reference";
pub(super) const LOCAL_TOKEN_MODE: &str = "local_token";
pub(super) const SOURCE_KIND_PROVIDER: &str = "provider";
pub(super) const SOURCE_KIND_ACCOUNT: &str = "account";
pub(super) const ANTHROPIC_PRESET: &str = "anthropic";
pub(super) const ACCOUNT_API_KEY_FORMAT: &str = "api_key";

#[derive(Debug, Clone)]
pub(super) struct PiOAuthTokens {
    access: String,
    refresh: Option<String>,
    expires_at: Option<String>,
}

/// Resolves generated-provider secret references at the live boundary.
/// The repository is shared with ProviderService, but resolver work
/// itself is read-only.
#[derive(Clone)]
pub struct AdapterSecretResolver {
    pub(super) providers: ProviderRepo,
    pub(super) accounts: AccountRepo,
}

impl AdapterSecretResolver {
    pub fn new(db: Database) -> Self {
        Self {
            providers: ProviderRepo::new(db.clone()),
            accounts: AccountRepo::new(db),
        }
    }
}
