//! In-memory secret materialization for explicitly generated adapter providers.
//!
//! This module intentionally owns the narrow Kimi membership -> Claude rule only.
//! It never mutates a provider row: callers receive a clone suitable for a live
//! write, and must scrub live state before any database backfill.

use serde_json::Value;
use toml_edit::DocumentMut;

use crate::bridge::ResolvedAuth;
use crate::error::{AppError, Result};
use crate::models::{AgentId, Provider};
use crate::storage::{Database, ProviderRepo};

/// Stored in generated reference providers instead of the source API key.
pub const CONNECTION_SECRET_MARKER: &str = "$AGENTHUB_CONNECTION_SECRET$";

const GENERATED_BY: &str = "adapter";
const KIMI_TO_CLAUDE_RULE: &str = "kimi-membership-to-claude-v1";
const KIMI_TO_CODEX_BRIDGE_RULE: &str = "kimi-membership-to-codex-v1";
const SOURCE_REFERENCE_MODE: &str = "source_reference";
const LOCAL_TOKEN_MODE: &str = "local_token";
const SOURCE_KIND_PROVIDER: &str = "provider";
const KIMI_MEMBERSHIP_PRESET: &str = "kimi-code-membership";
const KIMI_CLAUDE_BASE_URL: &str = "https://api.kimi.com/coding/";

/// Resolves the one supported generated-provider secret reference at the live
/// boundary. The repository is shared with ProviderService, but resolver work
/// itself is read-only.
pub struct AdapterSecretResolver {
    providers: ProviderRepo,
}

impl AdapterSecretResolver {
    pub fn new(db: Database) -> Self {
        Self {
            providers: ProviderRepo::new(db),
        }
    }

    /// Read-only preflight for the Kimi membership source used by adapter apply.
    /// This deliberately exposes only the normal validation error, never source
    /// configuration or a secret value.
    pub fn validate_kimi_membership_source(&self, source_id: &str) -> Result<()> {
        let _ = self.resolve_kimi_membership_api_key(source_id)?;
        Ok(())
    }

    /// Resolve a Kimi Code membership API key for an in-process adapter
    /// runtime. The returned value is intentionally not serializable and must
    /// be passed directly to the runtime; callers must never persist or log it.
    fn resolve_kimi_membership_api_key(&self, source_id: &str) -> Result<String> {
        let source_id = source_id.trim();
        if source_id.is_empty() {
            return Err(invalid_reference());
        }
        let source = self
            .providers
            .get_by_id(source_id)?
            .ok_or_else(invalid_reference)?;
        if source.agent_id != AgentId::Kimi
            || source.meta.get("preset").and_then(Value::as_str) != Some(KIMI_MEMBERSHIP_PRESET)
        {
            return Err(invalid_reference());
        }
        extract_kimi_api_key(&source.settings_config)
    }

    /// Internal bridge boundary: resolve membership auth without exposing the
    /// plaintext key to GUI/Tauri DTO layers.
    pub(crate) fn resolve_kimi_membership_auth(&self, source_id: &str) -> Result<ResolvedAuth> {
        self.resolve_kimi_membership_api_key(source_id)
            .map(ResolvedAuth::bearer)
    }

    /// Whether this row requires source-secret materialization before a live
    /// write. There are two explicit generated-provider credential modes:
    ///
    /// - Claude's Kimi membership projection is a `source_reference`: the
    ///   target carries a marker and is materialized in memory.
    /// - Codex's Kimi bridge is a `local_token`: its Provider owns a distinct
    ///   loopback bearer and must pass through unchanged.
    ///
    /// Any incomplete or unknown `generatedBy=adapter` declaration is rejected
    /// rather than accidentally treated as an ordinary user provider.
    pub fn is_reference_provider(&self, provider: &Provider) -> Result<bool> {
        match provider.meta.get("generatedBy").and_then(Value::as_str) {
            Some(GENERATED_BY) if is_claude_source_reference(provider) => {
                self.validate_reference_target(provider)?;
                Ok(true)
            }
            Some(GENERATED_BY) if is_codex_local_token(provider) => {
                self.validate_local_token_target(provider)?;
                Ok(false)
            }
            Some(GENERATED_BY) => Err(invalid_reference()),
            _ => Ok(false),
        }
    }

    /// Return a live-write clone of a provider. Ordinary providers pass through
    /// unchanged. A generated reference is materialized only in this returned
    /// clone, never in the source or target provider row.
    pub fn materialize_for_live(&self, target: &Provider) -> Result<Provider> {
        if !self.is_reference_provider(target)? {
            return Ok(target.clone());
        }

        self.validate_reference_target(target)?;
        let source = self.reference_source(target)?;
        let api_key = extract_kimi_api_key(&source.settings_config)?;

        let mut materialized = target.clone();
        let env = materialized
            .settings_config
            .get_mut("env")
            .and_then(Value::as_object_mut)
            .ok_or_else(invalid_reference)?;
        env.insert("ANTHROPIC_AUTH_TOKEN".into(), Value::String(api_key));
        Ok(materialized)
    }

    /// Prepare a live configuration for backfill into a generated reference
    /// row. This preserves the required Kimi endpoint while removing the live
    /// secret so a materialized value cannot reach the database.
    pub fn scrub_for_backfill(&self, provider: &Provider, live_raw: &Value) -> Result<Value> {
        if !self.is_reference_provider(provider)? {
            return Ok(live_raw.clone());
        }

        self.validate_reference_target(provider)?;
        // A deleted or invalid source must not cause us to persist a live secret
        // into a row which we can no longer safely re-materialize.
        let source = self.reference_source(provider)?;
        let _ = extract_kimi_api_key(&source.settings_config)?;

        let mut scrubbed = live_raw.clone();
        let env = scrubbed
            .get_mut("env")
            .and_then(Value::as_object_mut)
            .ok_or_else(invalid_reference)?;
        if env.get("ANTHROPIC_BASE_URL").and_then(Value::as_str) != Some(KIMI_CLAUDE_BASE_URL) {
            return Err(invalid_reference());
        }
        if !env.contains_key("ANTHROPIC_AUTH_TOKEN") {
            return Err(invalid_reference());
        }
        env.insert(
            "ANTHROPIC_AUTH_TOKEN".into(),
            Value::String(CONNECTION_SECRET_MARKER.into()),
        );
        Ok(scrubbed)
    }

    fn reference_source(&self, target: &Provider) -> Result<Provider> {
        let source_id = self.reference_source_id(target)?;
        self.validate_kimi_membership_source(source_id)?;
        let source = self
            .providers
            .get_by_id(source_id)?
            .ok_or_else(invalid_reference)?;
        Ok(source)
    }

    fn reference_source_id<'a>(&self, target: &'a Provider) -> Result<&'a str> {
        if !is_claude_source_reference(target) {
            return Err(invalid_reference());
        }
        let source = target
            .meta
            .get("adapterSourceRef")
            .and_then(Value::as_object)
            .ok_or_else(invalid_reference)?;
        if source.get("kind").and_then(Value::as_str) != Some(SOURCE_KIND_PROVIDER) {
            return Err(invalid_reference());
        }
        source
            .get("id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .ok_or_else(invalid_reference)
    }

    fn validate_reference_target(&self, target: &Provider) -> Result<()> {
        self.reference_source_id(target)?;
        let env = target
            .settings_config
            .get("env")
            .and_then(Value::as_object)
            .ok_or_else(invalid_reference)?;
        if env.get("ANTHROPIC_AUTH_TOKEN").and_then(Value::as_str) != Some(CONNECTION_SECRET_MARKER)
            || env.get("ANTHROPIC_BASE_URL").and_then(Value::as_str) != Some(KIMI_CLAUDE_BASE_URL)
        {
            return Err(invalid_reference());
        }
        Ok(())
    }

    fn validate_local_token_target(&self, target: &Provider) -> Result<()> {
        if !is_codex_local_token(target)
            || target
                .meta
                .get("adapterProfileId")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|id| !id.is_empty())
                .is_none()
            || target
                .meta
                .get("adapterSourceRef")
                .and_then(Value::as_object)
                .is_none_or(|source| {
                    source.get("kind").and_then(Value::as_str) != Some(SOURCE_KIND_PROVIDER)
                        || source
                            .get("id")
                            .and_then(Value::as_str)
                            .map(str::trim)
                            .filter(|id| !id.is_empty())
                            .is_none()
                })
            || target.settings_config.get("format").and_then(Value::as_str) != Some("toml")
            || target
                .settings_config
                .get("auth")
                .and_then(Value::as_object)
                .and_then(|auth| auth.get("OPENAI_API_KEY"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|token| {
                    !token.is_empty() && *token != CONNECTION_SECRET_MARKER && *token != "***"
                })
                .is_none()
        {
            return Err(invalid_reference());
        }
        Ok(())
    }
}

fn is_claude_source_reference(provider: &Provider) -> bool {
    provider.agent_id == AgentId::Claude
        && provider.meta.get("adapterRuleId").and_then(Value::as_str) == Some(KIMI_TO_CLAUDE_RULE)
        && provider
            .meta
            .get("adapterRuleVersion")
            .and_then(Value::as_u64)
            == Some(1)
        && provider
            .meta
            .get("adapterSecretMode")
            .and_then(Value::as_str)
            == Some(SOURCE_REFERENCE_MODE)
}

fn is_codex_local_token(provider: &Provider) -> bool {
    provider.agent_id == AgentId::Codex
        && provider.meta.get("adapterRuleId").and_then(Value::as_str)
            == Some(KIMI_TO_CODEX_BRIDGE_RULE)
        && provider
            .meta
            .get("adapterRuleVersion")
            .and_then(Value::as_u64)
            == Some(1)
        && provider
            .meta
            .get("adapterSecretMode")
            .and_then(Value::as_str)
            == Some(LOCAL_TOKEN_MODE)
}

fn extract_kimi_api_key(settings: &Value) -> Result<String> {
    let value = if let Some(api_key) = settings.get("apiKey").and_then(Value::as_str) {
        api_key.to_owned()
    } else if settings.get("format").and_then(Value::as_str) == Some("toml") {
        let content = settings
            .get("content")
            .and_then(Value::as_str)
            .ok_or_else(invalid_reference)?;
        let document = content
            .parse::<DocumentMut>()
            .map_err(|_| invalid_reference())?;
        // Prefer Kimi's selected provider, then the first configured provider
        // with a non-empty key, and finally the legacy top-level key. These
        // are the only accepted TOML paths; do not recursively search
        // arbitrary user content for something key-shaped.
        document
            .get("default_provider")
            .and_then(|item| item.as_str())
            .map(str::trim)
            .filter(|provider| !provider.is_empty())
            .and_then(|provider| {
                document
                    .get("providers")
                    .and_then(|item| item.as_table())
                    .and_then(|providers| providers.get(provider))
                    .and_then(toml_provider_api_key)
            })
            .or_else(|| {
                document
                    .get("providers")
                    .and_then(|item| item.as_table())
                    .and_then(|providers| {
                        providers
                            .iter()
                            .find_map(|(_, provider)| toml_provider_api_key(provider))
                    })
            })
            .or_else(|| toml_non_empty(document.get("api_key").and_then(|item| item.as_str())))
            .unwrap_or_default()
            .to_owned()
    } else {
        return Err(invalid_reference());
    };

    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed == "***" || trimmed == CONNECTION_SECRET_MARKER {
        return Err(invalid_reference());
    }
    Ok(trimmed.to_owned())
}

fn toml_provider_api_key(provider: &toml_edit::Item) -> Option<&str> {
    toml_non_empty(provider.get("api_key").and_then(|item| item.as_str()))
}

fn toml_non_empty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn invalid_reference() -> AppError {
    AppError::InvalidArg("invalid adapter secret reference".into())
}

#[cfg(test)]
mod tests;
