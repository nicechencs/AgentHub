//! In-memory secret materialization for explicitly generated adapter providers.
//!
//! Supported generated-provider modes:
//! - Claude Kimi membership and Pi config_sync: `source_reference`
//! - Codex Kimi bridge: `local_token` (pass through unchanged)
//!
//! This module never mutates a provider row: callers receive a clone suitable
//! for a live write, and must scrub live state before any database backfill.

use serde_json::Value;
use toml_edit::DocumentMut;

use crate::bridge::ResolvedAuth;
use crate::error::{AppError, Result};
use crate::models::{AgentId, Provider};
use crate::services::adapter_route_constants::{
    is_kimi_code_membership_source, settings_contain_anthropic_api_endpoint, ANTHROPIC_API_KEY_ENV,
    ANTHROPIC_AUTH_TOKEN_ENV, ANTHROPIC_BASE_URL_ENV, ANTHROPIC_PI_PROVIDER_SLOT,
    KIMI_CLAUDE_BASE_URL, KIMI_PI_BASE_URL, KIMI_PI_PROVIDER_SLOT,
};
use crate::storage::{Database, ProviderRepo};

// Re-export so existing `adapter_secret_resolver::CONNECTION_SECRET_MARKER` paths keep working.
pub use crate::services::adapter_route_constants::CONNECTION_SECRET_MARKER;

const GENERATED_BY: &str = "adapter";
const KIMI_TO_CLAUDE_RULE: &str = "kimi-membership-to-claude-v1";
const KIMI_TO_CODEX_BRIDGE_RULE: &str = "kimi-membership-to-codex-v1";
const KIMI_TO_PI_RULE: &str = "kimi-membership-to-pi-v1";
const ANTHROPIC_TO_PI_RULE: &str = "anthropic-api-to-pi-v1";
const SOURCE_REFERENCE_MODE: &str = "source_reference";
const LOCAL_TOKEN_MODE: &str = "local_token";
const SOURCE_KIND_PROVIDER: &str = "provider";
const ANTHROPIC_PRESET: &str = "anthropic";

/// Resolves generated-provider secret references at the live boundary.
/// The repository is shared with ProviderService, but resolver work
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

    /// Read-only preflight for an explicit Anthropic API Key Claude provider.
    pub fn validate_anthropic_api_source(&self, source_id: &str) -> Result<()> {
        let _ = self.resolve_anthropic_api_key(source_id)?;
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
        // Same rule as classify: preset or official coding endpoint. Never
        // upgrade from agent_id=kimi alone.
        if !is_kimi_code_membership_source(source.agent_id, &source.meta, &source.settings_config) {
            return Err(invalid_reference());
        }
        extract_kimi_api_key(&source.settings_config)
    }

    fn resolve_anthropic_api_key(&self, source_id: &str) -> Result<String> {
        let source_id = source_id.trim();
        if source_id.is_empty() {
            return Err(invalid_reference());
        }
        let source = self
            .providers
            .get_by_id(source_id)?
            .ok_or_else(invalid_reference)?;
        if !is_anthropic_api_source(&source) {
            return Err(invalid_reference());
        }
        extract_anthropic_api_key(&source.settings_config)
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
    /// - Claude's Kimi membership projection and Pi config_sync slots are
    ///   `source_reference`: the target carries a marker and is materialized
    ///   in memory.
    /// - Codex's Kimi bridge is a `local_token`: its Provider owns a distinct
    ///   loopback bearer and must pass through unchanged.
    ///
    /// Any incomplete or unknown `generatedBy=adapter` declaration is rejected
    /// rather than accidentally treated as an ordinary user provider.
    pub fn is_reference_provider(&self, provider: &Provider) -> Result<bool> {
        match provider.meta.get("generatedBy").and_then(Value::as_str) {
            Some(GENERATED_BY) if is_claude_source_reference(provider) => {
                self.validate_claude_reference_target(provider)?;
                Ok(true)
            }
            Some(GENERATED_BY) if is_pi_source_reference(provider) => {
                self.validate_pi_reference_target(provider)?;
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

        if is_claude_source_reference(target) {
            self.validate_claude_reference_target(target)?;
            let source = self.reference_source(target)?;
            let api_key = extract_kimi_api_key(&source.settings_config)?;
            let mut materialized = target.clone();
            let env = materialized
                .settings_config
                .get_mut("env")
                .and_then(Value::as_object_mut)
                .ok_or_else(invalid_reference)?;
            env.insert(ANTHROPIC_AUTH_TOKEN_ENV.into(), Value::String(api_key));
            return Ok(materialized);
        }

        self.validate_pi_reference_target(target)?;
        let source = self.reference_source(target)?;
        let api_key = extract_source_api_key(target, &source)?;
        let slot = pi_slot_name(target)?;
        let mut materialized = target.clone();
        set_pi_slot_api_key(&mut materialized.settings_config, slot, &api_key)?;
        Ok(materialized)
    }

    /// Prepare a live configuration for backfill into a generated reference
    /// row. This preserves the required Kimi endpoint while removing the live
    /// secret so a materialized value cannot reach the database.
    pub fn scrub_for_backfill(&self, provider: &Provider, live_raw: &Value) -> Result<Value> {
        if !self.is_reference_provider(provider)? {
            return Ok(live_raw.clone());
        }

        if is_claude_source_reference(provider) {
            self.validate_claude_reference_target(provider)?;
            // A deleted or invalid source must not cause us to persist a live secret
            // into a row which we can no longer safely re-materialize.
            let source = self.reference_source(provider)?;
            let _ = extract_kimi_api_key(&source.settings_config)?;

            let mut scrubbed = live_raw.clone();
            let env = scrubbed
                .get_mut("env")
                .and_then(Value::as_object_mut)
                .ok_or_else(invalid_reference)?;
            if env.get(ANTHROPIC_BASE_URL_ENV).and_then(Value::as_str) != Some(KIMI_CLAUDE_BASE_URL)
            {
                return Err(invalid_reference());
            }
            if !env.contains_key(ANTHROPIC_AUTH_TOKEN_ENV) {
                return Err(invalid_reference());
            }
            env.insert(
                ANTHROPIC_AUTH_TOKEN_ENV.into(),
                Value::String(CONNECTION_SECRET_MARKER.into()),
            );
            return Ok(scrubbed);
        }

        self.validate_pi_reference_target(provider)?;
        let source = self.reference_source(provider)?;
        let _ = extract_source_api_key(provider, &source)?;
        let slot = pi_slot_name(provider)?;
        let mut scrubbed = live_raw.clone();
        let live_slot = pi_slot_object(&scrubbed, slot).ok_or_else(invalid_reference)?;
        if slot == KIMI_PI_PROVIDER_SLOT
            && live_slot.get("baseUrl").and_then(Value::as_str) != Some(KIMI_PI_BASE_URL)
        {
            return Err(invalid_reference());
        }
        if !live_slot
            .get("apiKey")
            .and_then(Value::as_str)
            .is_some_and(|value| !value.trim().is_empty())
        {
            return Err(invalid_reference());
        }
        set_pi_slot_api_key(&mut scrubbed, slot, CONNECTION_SECRET_MARKER)?;
        Ok(scrubbed)
    }

    fn reference_source(&self, target: &Provider) -> Result<Provider> {
        let source_id = self.reference_source_id(target)?;
        let rule = adapter_rule_id(target).ok_or_else(invalid_reference)?;
        match rule {
            KIMI_TO_CLAUDE_RULE | KIMI_TO_PI_RULE => {
                self.validate_kimi_membership_source(source_id)?;
            }
            ANTHROPIC_TO_PI_RULE => {
                self.validate_anthropic_api_source(source_id)?;
            }
            _ => return Err(invalid_reference()),
        }
        self.providers
            .get_by_id(source_id)?
            .ok_or_else(invalid_reference)
    }

    fn reference_source_id<'a>(&self, target: &'a Provider) -> Result<&'a str> {
        if !is_claude_source_reference(target) && !is_pi_source_reference(target) {
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

    fn validate_claude_reference_target(&self, target: &Provider) -> Result<()> {
        self.reference_source_id(target)?;
        let env = target
            .settings_config
            .get("env")
            .and_then(Value::as_object)
            .ok_or_else(invalid_reference)?;
        if env.get(ANTHROPIC_AUTH_TOKEN_ENV).and_then(Value::as_str)
            != Some(CONNECTION_SECRET_MARKER)
            || env.get(ANTHROPIC_BASE_URL_ENV).and_then(Value::as_str) != Some(KIMI_CLAUDE_BASE_URL)
        {
            return Err(invalid_reference());
        }
        Ok(())
    }

    fn validate_pi_reference_target(&self, target: &Provider) -> Result<()> {
        self.reference_source_id(target)?;
        let slot = pi_slot_name(target)?;
        let slot_obj =
            pi_slot_object(&target.settings_config, slot).ok_or_else(invalid_reference)?;
        if slot_obj.get("apiKey").and_then(Value::as_str) != Some(CONNECTION_SECRET_MARKER) {
            return Err(invalid_reference());
        }
        if slot == KIMI_PI_PROVIDER_SLOT
            && slot_obj.get("baseUrl").and_then(Value::as_str) != Some(KIMI_PI_BASE_URL)
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
        && adapter_rule_id(provider) == Some(KIMI_TO_CLAUDE_RULE)
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

fn is_pi_source_reference(provider: &Provider) -> bool {
    provider.agent_id == AgentId::Pi
        && matches!(
            adapter_rule_id(provider),
            Some(KIMI_TO_PI_RULE) | Some(ANTHROPIC_TO_PI_RULE)
        )
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

fn adapter_rule_id(provider: &Provider) -> Option<&str> {
    provider.meta.get("adapterRuleId").and_then(Value::as_str)
}

fn pi_slot_name(provider: &Provider) -> Result<&'static str> {
    match adapter_rule_id(provider) {
        Some(KIMI_TO_PI_RULE) => Ok(KIMI_PI_PROVIDER_SLOT),
        Some(ANTHROPIC_TO_PI_RULE) => Ok(ANTHROPIC_PI_PROVIDER_SLOT),
        _ => Err(invalid_reference()),
    }
}

fn pi_slot_object<'a>(settings: &'a Value, slot: &str) -> Option<&'a Value> {
    settings
        .get("models")
        .and_then(|models| models.get("providers"))
        .and_then(|providers| providers.get(slot))
        .filter(|value| value.is_object())
}

fn set_pi_slot_api_key(settings: &mut Value, slot: &str, api_key: &str) -> Result<()> {
    let provider = settings
        .get_mut("models")
        .and_then(Value::as_object_mut)
        .and_then(|models| models.get_mut("providers"))
        .and_then(Value::as_object_mut)
        .and_then(|providers| providers.get_mut(slot))
        .and_then(Value::as_object_mut)
        .ok_or_else(invalid_reference)?;
    provider.insert("apiKey".into(), Value::String(api_key.into()));
    Ok(())
}

fn is_anthropic_api_source(source: &Provider) -> bool {
    source.agent_id == AgentId::Claude
        && (source.meta.get("preset").and_then(Value::as_str) == Some(ANTHROPIC_PRESET)
            || settings_contain_anthropic_api_endpoint(&source.settings_config))
}

fn extract_source_api_key(target: &Provider, source: &Provider) -> Result<String> {
    match adapter_rule_id(target) {
        Some(KIMI_TO_CLAUDE_RULE) | Some(KIMI_TO_PI_RULE) => {
            extract_kimi_api_key(&source.settings_config)
        }
        Some(ANTHROPIC_TO_PI_RULE) => extract_anthropic_api_key(&source.settings_config),
        _ => Err(invalid_reference()),
    }
}

fn extract_anthropic_api_key(settings: &Value) -> Result<String> {
    let env = settings.get("env");
    for candidate in [
        env.and_then(|value| value.get(ANTHROPIC_AUTH_TOKEN_ENV))
            .and_then(Value::as_str),
        env.and_then(|value| value.get(ANTHROPIC_API_KEY_ENV))
            .and_then(Value::as_str),
        settings.get("apiKey").and_then(Value::as_str),
    ]
    .into_iter()
    .flatten()
    {
        if let Some(key) = usable_secret(candidate) {
            return Ok(key.to_owned());
        }
    }
    Err(invalid_reference())
}

fn usable_secret(value: &str) -> Option<&str> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed == "***" || trimmed == CONNECTION_SECRET_MARKER {
        None
    } else {
        Some(trimmed)
    }
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
