use crate::error::{AppError, Result};
use crate::models::{
    decide_adapter_capability, Account, AccountKind, AdapterCapabilityDecision,
    AdapterCredentialClass, AdapterRouteRequest, AdapterSourceKind, AdapterSourceProduct, AgentId,
    Provider,
};
use crate::services::adapter_route_constants::{
    is_deepseek_api_marker, is_glm_coding_plan_marker, is_kimi_code_membership_account,
    is_kimi_code_membership_source, is_non_openai_provider_tag, is_openai_api_marker,
    is_xai_api_marker, settings_contain_anthropic_api_endpoint,
};

use super::actions::*;
use super::{AdapterRouteService, ClassifiedRoute};

impl AdapterRouteService {
    /// Classify a prospective account without reading it back from SQLite.
    ///
    /// Create/import/update sagas use this before their first transaction so
    /// the persisted ticket surface is part of the atomic row write.
    pub fn classify_account_source_product(account: &Account) -> AdapterSourceProduct {
        let explicit_provider = json_string(&account.extra, "provider")
            .or_else(|| json_string(&account.credentials, "provider"));
        let credential_format = json_string(&account.credentials, "format")
            .or_else(|| json_string(&account.extra, "format"));

        if account.kind == AccountKind::ApiKey
            && (explicit_provider.is_some_and(|value| value.eq_ignore_ascii_case("anthropic"))
                || settings_contain_anthropic_api_endpoint(&account.credentials)
                || settings_contain_anthropic_api_endpoint(&account.extra))
        {
            AdapterSourceProduct::AnthropicApi
        } else if account.kind == AccountKind::ApiKey
            && (is_openai_api_marker(explicit_provider, &account.credentials)
                || is_openai_api_marker(explicit_provider, &account.extra))
        {
            AdapterSourceProduct::OpenaiApi
        } else if account.kind == AccountKind::ApiKey
            && (is_xai_api_marker(explicit_provider, &account.credentials)
                || is_xai_api_marker(explicit_provider, &account.extra))
        {
            AdapterSourceProduct::XaiApi
        } else if account.kind == AccountKind::ApiKey
            && (is_glm_coding_plan_marker(explicit_provider, &account.credentials)
                || is_glm_coding_plan_marker(explicit_provider, &account.extra))
        {
            AdapterSourceProduct::GlmCodingPlan
        } else if account.kind == AccountKind::ApiKey
            && (is_deepseek_api_marker(explicit_provider, &account.credentials)
                || is_deepseek_api_marker(explicit_provider, &account.extra))
        {
            AdapterSourceProduct::DeepseekApi
        } else if account.kind == AccountKind::ApiKey
            && is_kimi_code_membership_account(
                account.agent_id,
                &account.extra,
                &account.credentials,
            )
        {
            AdapterSourceProduct::KimiCodeMembership
        } else if account.agent_id == AgentId::Codex
            && account.kind == AccountKind::Oauth
            && is_codex_auth_json(credential_format, &account.credentials)
        {
            AdapterSourceProduct::CodexChatGptSubscription
        } else if account.agent_id == AgentId::Codex && account.kind == AccountKind::Oauth {
            AdapterSourceProduct::CodexChatGptSubscription
        } else if account.agent_id == AgentId::Claude && account.kind == AccountKind::Oauth {
            AdapterSourceProduct::ClaudeSubscription
        } else if account.agent_id == AgentId::Grok && account.kind == AccountKind::Oauth {
            AdapterSourceProduct::XaiGrokSubscription
        } else {
            AdapterSourceProduct::Other
        }
    }

    /// Classify a prospective provider without reading it back from SQLite.
    pub fn classify_provider_source_product(provider: &Provider) -> AdapterSourceProduct {
        let explicit_tag = provider_classification_tag(&provider.meta);
        if is_kimi_code_membership_source(
            provider.agent_id,
            &provider.meta,
            &provider.settings_config,
        ) {
            AdapterSourceProduct::KimiCodeMembership
        } else if provider.agent_id == AgentId::Claude
            && (explicit_tag == Some("anthropic")
                || settings_contain_anthropic_api_endpoint(&provider.settings_config))
        {
            AdapterSourceProduct::AnthropicApi
        } else if is_openai_api_marker(explicit_tag, &provider.settings_config) {
            AdapterSourceProduct::OpenaiApi
        } else if is_xai_api_marker(explicit_tag, &provider.settings_config) {
            AdapterSourceProduct::XaiApi
        } else if is_glm_coding_plan_marker(explicit_tag, &provider.settings_config) {
            AdapterSourceProduct::GlmCodingPlan
        } else if is_deepseek_api_marker(explicit_tag, &provider.settings_config) {
            AdapterSourceProduct::DeepseekApi
        } else {
            AdapterSourceProduct::Other
        }
    }

    /// analyze/plan. Does not inspect or return credentials.
    pub fn classify_source_product(
        &self,
        source_kind: AdapterSourceKind,
        source_id: &str,
    ) -> Result<AdapterSourceProduct> {
        Ok(self.identify_source(source_kind, source_id)?.product)
    }

    pub(super) fn classify(&self, request: &AdapterRouteRequest) -> Result<ClassifiedRoute> {
        let identity = self.identify_source(request.source_kind, &request.source_id)?;

        let mut decision = decide_adapter_capability(
            identity.product,
            identity.credential,
            request.target_agent_id,
        )
        .public_surface();
        // Replace the generic Other reason with an actionable product hint when we have one.
        if matches!(identity.product, AdapterSourceProduct::Other) {
            if let Some(hint) = identity.reason_hint {
                decision = AdapterCapabilityDecision::unsupported(hint).public_surface();
            }
        }

        Ok(ClassifiedRoute {
            source: identity.label,
            credential: identity.credential,
            decision,
        })
    }

    pub(super) fn identify_source(
        &self,
        source_kind: AdapterSourceKind,
        source_id: &str,
    ) -> Result<SourceIdentity> {
        let source_id = source_id.trim();
        if source_id.is_empty() {
            return Err(AppError::InvalidArg(
                "adapter source id must not be empty".into(),
            ));
        }

        match source_kind {
            AdapterSourceKind::Provider => {
                let provider = self.providers.get_by_id(source_id)?.ok_or_else(|| {
                    AppError::NotFound(format!("provider not found: {source_id}"))
                })?;
                let explicit_tag = provider_classification_tag(&provider.meta);
                // Membership is explicit preset *or* official Kimi coding endpoint in config.
                // Do not invent membership from agent_id alone (moonshot / custom stay closed).
                if is_kimi_code_membership_source(
                    provider.agent_id,
                    &provider.meta,
                    &provider.settings_config,
                ) {
                    Ok(SourceIdentity {
                        product: AdapterSourceProduct::KimiCodeMembership,
                        credential: AdapterCredentialClass::ApiKey,
                        label: RouteSourceLabel::KimiMembership,
                        reason_hint: None,
                    })
                } else if provider.agent_id == AgentId::Claude
                    && (explicit_tag == Some("anthropic")
                        || settings_contain_anthropic_api_endpoint(&provider.settings_config))
                {
                    Ok(SourceIdentity {
                        product: AdapterSourceProduct::AnthropicApi,
                        credential: AdapterCredentialClass::ApiKey,
                        label: RouteSourceLabel::AnthropicApiKey,
                        reason_hint: None,
                    })
                } else if is_openai_api_marker(explicit_tag, &provider.settings_config) {
                    Ok(SourceIdentity {
                        product: AdapterSourceProduct::OpenaiApi,
                        credential: AdapterCredentialClass::ApiKey,
                        label: RouteSourceLabel::OpenaiApiKey,
                        reason_hint: None,
                    })
                } else if is_xai_api_marker(explicit_tag, &provider.settings_config) {
                    Ok(SourceIdentity {
                        product: AdapterSourceProduct::XaiApi,
                        credential: AdapterCredentialClass::ApiKey,
                        label: RouteSourceLabel::XaiApiKey,
                        reason_hint: None,
                    })
                } else if is_glm_coding_plan_marker(explicit_tag, &provider.settings_config) {
                    Ok(SourceIdentity {
                        product: AdapterSourceProduct::GlmCodingPlan,
                        credential: AdapterCredentialClass::ApiKey,
                        label: RouteSourceLabel::GlmCodingPlan,
                        reason_hint: None,
                    })
                } else if is_deepseek_api_marker(explicit_tag, &provider.settings_config) {
                    Ok(SourceIdentity {
                        product: AdapterSourceProduct::DeepseekApi,
                        credential: AdapterCredentialClass::ApiKey,
                        label: RouteSourceLabel::DeepseekApi,
                        reason_hint: None,
                    })
                } else if provider.agent_id == AgentId::Kimi {
                    Ok(SourceIdentity {
                        product: AdapterSourceProduct::Other,
                        credential: AdapterCredentialClass::ApiKey,
                        label: RouteSourceLabel::Other,
                        reason_hint: Some(KIMI_NON_MEMBERSHIP_REASON),
                    })
                } else {
                    Ok(SourceIdentity {
                        product: AdapterSourceProduct::Other,
                        credential: AdapterCredentialClass::Unknown,
                        label: RouteSourceLabel::Other,
                        reason_hint: None,
                    })
                }
            }
            AdapterSourceKind::Account => {
                let account = self
                    .accounts
                    .get_by_id(source_id)?
                    .ok_or_else(|| AppError::NotFound(format!("account not found: {source_id}")))?;
                let explicit_provider = json_string(&account.extra, "provider")
                    .or_else(|| json_string(&account.credentials, "provider"));
                let credential_format = json_string(&account.credentials, "format")
                    .or_else(|| json_string(&account.extra, "format"));

                if account.kind == AccountKind::ApiKey
                    && (explicit_provider
                        .is_some_and(|value| value.eq_ignore_ascii_case("anthropic"))
                        || settings_contain_anthropic_api_endpoint(&account.credentials)
                        || settings_contain_anthropic_api_endpoint(&account.extra))
                {
                    Ok(SourceIdentity {
                        product: AdapterSourceProduct::AnthropicApi,
                        credential: AdapterCredentialClass::ApiKey,
                        label: RouteSourceLabel::AnthropicApiKey,
                        reason_hint: None,
                    })
                } else if account.kind == AccountKind::ApiKey
                    && (is_openai_api_marker(explicit_provider, &account.credentials)
                        || is_openai_api_marker(explicit_provider, &account.extra))
                {
                    Ok(SourceIdentity {
                        product: AdapterSourceProduct::OpenaiApi,
                        credential: AdapterCredentialClass::ApiKey,
                        label: RouteSourceLabel::OpenaiApiKey,
                        reason_hint: None,
                    })
                } else if account.kind == AccountKind::ApiKey
                    && (is_xai_api_marker(explicit_provider, &account.credentials)
                        || is_xai_api_marker(explicit_provider, &account.extra))
                {
                    Ok(SourceIdentity {
                        product: AdapterSourceProduct::XaiApi,
                        credential: AdapterCredentialClass::ApiKey,
                        label: RouteSourceLabel::XaiApiKey,
                        reason_hint: None,
                    })
                } else if account.kind == AccountKind::ApiKey
                    && (is_glm_coding_plan_marker(explicit_provider, &account.credentials)
                        || is_glm_coding_plan_marker(explicit_provider, &account.extra))
                {
                    Ok(SourceIdentity {
                        product: AdapterSourceProduct::GlmCodingPlan,
                        credential: AdapterCredentialClass::ApiKey,
                        label: RouteSourceLabel::GlmCodingPlan,
                        reason_hint: None,
                    })
                } else if account.kind == AccountKind::ApiKey
                    && (is_deepseek_api_marker(explicit_provider, &account.credentials)
                        || is_deepseek_api_marker(explicit_provider, &account.extra))
                {
                    Ok(SourceIdentity {
                        product: AdapterSourceProduct::DeepseekApi,
                        credential: AdapterCredentialClass::ApiKey,
                        label: RouteSourceLabel::DeepseekApi,
                        reason_hint: None,
                    })
                } else if account.kind == AccountKind::ApiKey
                    && is_kimi_code_membership_account(
                        account.agent_id,
                        &account.extra,
                        &account.credentials,
                    )
                {
                    Ok(SourceIdentity {
                        product: AdapterSourceProduct::KimiCodeMembership,
                        credential: AdapterCredentialClass::ApiKey,
                        label: RouteSourceLabel::KimiMembership,
                        reason_hint: None,
                    })
                } else if account.kind == AccountKind::ApiKey && account.agent_id == AgentId::Kimi {
                    Ok(SourceIdentity {
                        product: AdapterSourceProduct::Other,
                        credential: AdapterCredentialClass::ApiKey,
                        label: RouteSourceLabel::Other,
                        reason_hint: Some(KIMI_NON_MEMBERSHIP_REASON),
                    })
                } else if account.agent_id == AgentId::Codex
                    && account.kind == AccountKind::Oauth
                    && is_codex_auth_json(credential_format, &account.credentials)
                {
                    // Explicit Codex / ChatGPT subscription (`format=auth_json` or tokens blob).
                    // The Responses → Claude cell is open, subject to the Account secret gate.
                    Ok(SourceIdentity {
                        product: AdapterSourceProduct::CodexChatGptSubscription,
                        credential: AdapterCredentialClass::OauthAuthJson,
                        label: RouteSourceLabel::CodexSubscription,
                        reason_hint: None,
                    })
                } else if account.agent_id == AgentId::Codex && account.kind == AccountKind::Oauth {
                    // Codex OAuth without auth_json shape: same product messaging, but do not
                    // pretend the closed auth_json matrix cell matched. Fail closed via empty
                    // candidate → subscription_candidate unsupported surface for Claude.
                    Ok(SourceIdentity {
                        product: AdapterSourceProduct::CodexChatGptSubscription,
                        credential: AdapterCredentialClass::OauthOther,
                        label: RouteSourceLabel::CodexSubscription,
                        reason_hint: None,
                    })
                } else if account.agent_id == AgentId::Claude && account.kind == AccountKind::Oauth
                {
                    Ok(SourceIdentity {
                        product: AdapterSourceProduct::ClaudeSubscription,
                        credential: AdapterCredentialClass::OauthOther,
                        label: RouteSourceLabel::ClaudeSubscription,
                        reason_hint: None,
                    })
                } else if account.agent_id == AgentId::Grok && account.kind == AccountKind::Oauth {
                    Ok(SourceIdentity {
                        product: AdapterSourceProduct::XaiGrokSubscription,
                        credential: AdapterCredentialClass::OauthOther,
                        label: RouteSourceLabel::XaiGrokSubscription,
                        reason_hint: None,
                    })
                } else {
                    Ok(SourceIdentity {
                        product: AdapterSourceProduct::Other,
                        credential: match account.kind {
                            AccountKind::ApiKey => AdapterCredentialClass::ApiKey,
                            AccountKind::Oauth => AdapterCredentialClass::OauthOther,
                        },
                        label: RouteSourceLabel::Other,
                        reason_hint: None,
                    })
                }
            }
        }
    }
}

/// Prefer a known non-OpenAI product tag when legacy rows carry both
/// `preset` and `provider`. A generic compatibility preset must not mask the
/// explicit xAI/GLM/DeepSeek/Kimi/Anthropic identity.
fn provider_classification_tag<'a>(meta: &'a serde_json::Value) -> Option<&'a str> {
    let preset = json_string(meta, "preset");
    let provider = json_string(meta, "provider");
    if preset.is_some_and(|tag| is_non_openai_provider_tag(Some(tag))) {
        return preset;
    }
    if provider.is_some_and(|tag| is_non_openai_provider_tag(Some(tag))) {
        return provider;
    }
    preset.or(provider)
}

pub(super) struct SourceIdentity {
    product: AdapterSourceProduct,
    credential: AdapterCredentialClass,
    label: RouteSourceLabel,
    /// Optional replace for the generic Other unsupported reason (never secrets).
    reason_hint: Option<&'static str>,
}
