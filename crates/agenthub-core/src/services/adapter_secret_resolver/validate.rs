use super::helpers::*;
use super::*;
use crate::bridge::ResolvedAuth;
use crate::error::Result;
use crate::models::{AdapterSourceKind, AgentId};
use crate::services::adapter_route_constants::*;

impl AdapterSecretResolver {
    pub fn validate_kimi_membership_source(
        &self,
        source_kind: AdapterSourceKind,
        source_id: &str,
    ) -> Result<()> {
        let _ = self.resolve_kimi_membership_api_key(source_kind, source_id)?;
        Ok(())
    }

    /// Read-only preflight for an explicit Anthropic API Key Claude provider.
    pub fn validate_anthropic_api_source(&self, source_id: &str) -> Result<()> {
        self.validate_anthropic_source(AdapterSourceKind::Provider, source_id)
    }

    /// Read-only preflight for an Anthropic API Key ticket (Provider or Account).
    pub fn validate_anthropic_source(
        &self,
        source_kind: AdapterSourceKind,
        source_id: &str,
    ) -> Result<()> {
        let _ = self.resolve_anthropic_api_key(source_kind, source_id)?;
        Ok(())
    }

    /// Read-only preflight for an explicit API Key ticket (Anthropic / OpenAI / xAI).
    pub fn validate_explicit_api_source(
        &self,
        rule_id: &str,
        source_kind: AdapterSourceKind,
        source_id: &str,
    ) -> Result<()> {
        let _ = self.resolve_explicit_api_key(rule_id, source_kind, source_id)?;
        Ok(())
    }

    /// Read-only preflight for a native subscription OAuth Account → Pi bind.
    pub fn validate_subscription_oauth_source(
        &self,
        rule_id: &str,
        source_kind: AdapterSourceKind,
        source_id: &str,
    ) -> Result<()> {
        let _ = self.resolve_subscription_oauth(rule_id, source_kind, source_id)?;
        Ok(())
    }

    /// Read-only preflight for an explicit DeepSeek API Key provider.
    pub fn validate_deepseek_api_source(&self, source_id: &str) -> Result<()> {
        let _ = self.resolve_deepseek_api_key(source_id)?;
        Ok(())
    }

    /// Resolve a Kimi Code membership API key for an in-process adapter
    /// runtime. The returned value is intentionally not serializable and must
    /// be passed directly to the runtime; callers must never persist or log it.
    pub(super) fn resolve_kimi_membership_api_key(
        &self,
        source_kind: AdapterSourceKind,
        source_id: &str,
    ) -> Result<String> {
        let source_id = source_id.trim();
        if source_id.is_empty() {
            return Err(invalid_reference());
        }
        match source_kind {
            AdapterSourceKind::Provider => {
                let source = self
                    .providers
                    .get_by_id(source_id)?
                    .ok_or_else(invalid_reference)?;
                // Same rule as classify: preset or official coding endpoint.
                // Never upgrade from agent_id=kimi alone.
                if !is_kimi_code_membership_source(
                    source.agent_id,
                    &source.meta,
                    &source.settings_config,
                ) {
                    return Err(invalid_reference());
                }
                extract_kimi_api_key(&source.settings_config)
            }
            AdapterSourceKind::Account => {
                let account = self
                    .accounts
                    .get_by_id(source_id)?
                    .ok_or_else(invalid_reference)?;
                if account.kind != crate::models::AccountKind::ApiKey
                    || !is_kimi_code_membership_account(
                        account.agent_id,
                        &account.extra,
                        &account.credentials,
                    )
                {
                    return Err(invalid_reference());
                }
                extract_account_api_key(&account.credentials)
            }
        }
    }

    pub(super) fn resolve_anthropic_api_key(
        &self,
        source_kind: AdapterSourceKind,
        source_id: &str,
    ) -> Result<String> {
        self.resolve_explicit_api_key(ANTHROPIC_TO_PI_RULE, source_kind, source_id)
    }

    pub(super) fn resolve_explicit_api_key(
        &self,
        rule_id: &str,
        source_kind: AdapterSourceKind,
        source_id: &str,
    ) -> Result<String> {
        let source_id = source_id.trim();
        if source_id.is_empty() {
            return Err(invalid_reference());
        }
        match source_kind {
            AdapterSourceKind::Provider => {
                let source = self
                    .providers
                    .get_by_id(source_id)?
                    .ok_or_else(invalid_reference)?;
                if !provider_matches_explicit_api_rule(rule_id, &source) {
                    return Err(invalid_reference());
                }
                extract_explicit_provider_api_key(rule_id, &source.settings_config)
            }
            AdapterSourceKind::Account => {
                let account = self
                    .accounts
                    .get_by_id(source_id)?
                    .ok_or_else(invalid_reference)?;
                extract_account_api_key(&account.credentials)
            }
        }
    }

    pub(super) fn resolve_deepseek_api_key(&self, source_id: &str) -> Result<String> {
        let source_id = source_id.trim();
        if source_id.is_empty() {
            return Err(invalid_reference());
        }
        let source = self
            .providers
            .get_by_id(source_id)?
            .ok_or_else(invalid_reference)?;
        if !is_deepseek_api_marker(provider_explicit_tag(&source), &source.settings_config) {
            return Err(invalid_reference());
        }
        extract_deepseek_api_key(&source.settings_config)
    }

    /// Internal bridge boundary: resolve membership auth without exposing the
    /// plaintext key to GUI/Tauri DTO layers.
    pub(crate) fn resolve_kimi_membership_auth(
        &self,
        source_kind: AdapterSourceKind,
        source_id: &str,
    ) -> Result<ResolvedAuth> {
        self.resolve_kimi_membership_api_key(source_kind, source_id)
            .map(ResolvedAuth::bearer)
    }

    /// Internal bridge boundary: resolve an Anthropic API key without exposing
    /// the plaintext to GUI/Tauri DTO layers.
    pub(crate) fn resolve_anthropic_auth(
        &self,
        source_kind: AdapterSourceKind,
        source_id: &str,
    ) -> Result<ResolvedAuth> {
        self.resolve_anthropic_api_key(source_kind, source_id)
            .map(ResolvedAuth::bearer)
    }

    /// Internal bridge boundary: resolve an OpenAI API key without exposing
    /// the plaintext to GUI/Tauri DTO layers.
    pub(crate) fn resolve_openai_auth(
        &self,
        source_kind: AdapterSourceKind,
        source_id: &str,
    ) -> Result<ResolvedAuth> {
        self.resolve_explicit_api_key(OPENAI_TO_PI_RULE, source_kind, source_id)
            .map(ResolvedAuth::bearer)
    }

    /// Resolve only the current Codex OAuth access token for a bridge upstream.
    /// Refresh is intentionally owned by the next Codex login sync; this
    /// adapter does not persist or return refresh material.
    pub(crate) fn resolve_codex_subscription_auth(
        &self,
        source_kind: AdapterSourceKind,
        source_id: &str,
    ) -> Result<ResolvedAuth> {
        if source_kind != AdapterSourceKind::Account {
            return Err(invalid_reference());
        }
        let account = self
            .accounts
            .get_by_id(source_id.trim())?
            .ok_or_else(invalid_reference)?;
        if account.kind != crate::models::AccountKind::Oauth {
            return Err(invalid_reference());
        }
        crate::bridge::session::resolve_codex_subscription_auth(&account.credentials)
    }

    /// Resolve only the current Grok OAuth access token for the Claude bridge.
    /// Refresh is intentionally not returned or persisted; the source account
    /// must be synchronized again when its access token expires.
    pub(crate) fn resolve_grok_subscription_auth(
        &self,
        source_kind: AdapterSourceKind,
        source_id: &str,
    ) -> Result<ResolvedAuth> {
        if source_kind != AdapterSourceKind::Account {
            return Err(invalid_reference());
        }
        let account = self
            .accounts
            .get_by_id(source_id.trim())?
            .ok_or_else(invalid_reference)?;
        if account.agent_id != AgentId::Grok || account.kind != crate::models::AccountKind::Oauth {
            return Err(invalid_reference());
        }
        let access = first_usable_string(
            &account.credentials,
            &[
                "/access_token",
                "/tokens/access_token",
                "/body/tokens/access_token",
                "/body/key",
                "/key",
            ],
        )
        .ok_or_else(invalid_reference)?;
        Ok(ResolvedAuth::bearer(access))
    }
}
