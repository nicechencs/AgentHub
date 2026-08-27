use super::rules::*;
use super::*;

impl AdapterBridgeService {
    pub fn profile_id_for_request(&self, request: &AdapterBridgePrepareRequest) -> Result<String> {
        let rule = self.ensure_supported(request)?;
        Ok(stable_id(rule.profile_prefix, request.source_id.trim()))
    }

    /// Begin an idempotent local-bridge saga without starting a listener or
    /// writing any live agent configuration.
    pub fn prepare(&self, request: &AdapterBridgePrepareRequest) -> Result<AdapterBridgePrepared> {
        let rule = self.ensure_supported(request)?;
        let source_id = request.source_id.trim();
        // Validate/retrieve the source before any profile row is written. The
        // token stays only in the returned in-memory material.
        let upstream_auth = self.resolve_upstream_auth(&rule, request.source_kind, source_id)?;
        let profile_id = stable_id(rule.profile_prefix, source_id);
        let provider_id = stable_id(rule.provider_prefix, source_id);
        let stamp = now();
        let proposed = AdapterProfile {
            id: profile_id,
            name: format!("{} ({})", rule.profile_name, safe_label(source_id)),
            source_kind: request.source_kind,
            source_id: source_id.into(),
            target_agent_id: rule.target_agent,
            route: AdapterRoute::LocalBridge,
            mode: rule.mode,
            status: AdapterProfileStatus::Applying,
            rule_id: rule.rule_id.into(),
            rule_version: RULE_VERSION.into(),
            generated_provider_id: Some(provider_id.clone()),
            local_port: None,
            auto_start: request.auto_start,
            last_error_code: None,
            created_at: stamp.clone(),
            updated_at: stamp,
        };

        let prior_profile = self.profiles.get(&proposed.id)?;
        let existing_provider = self.providers.get_by_id(&provider_id)?;
        // A provider can only be reused when the profile already established
        // ownership. This prevents a user-created/provider-id collision from
        // being silently adopted by a new bridge profile.
        if prior_profile.is_none() && existing_provider.is_some() {
            return Err(AppError::message(
                "adapter.provider_conflict",
                "generated provider id is owned by another provider",
            ));
        }

        let mut profile = self.profiles.create_or_get(&proposed)?;
        if !same_profile_contract(&profile, &proposed) {
            return Err(AppError::message(
                "adapter.profile_conflict",
                "adapter profile conflicts with requested bridge rule",
            ));
        }

        let (
            upstream_base_url,
            upstream_model,
            configured_listed_models,
            protocol,
            context_window_tokens,
        ) = openai_source_upstream(self, &rule, request.source_kind, source_id);
        let (generated_provider_exists, generated_provider_is_current) =
            if let Some(provider) = existing_provider.as_ref() {
                validate_generated_provider(provider, &profile, profile.local_port)?;
                (
                    true,
                    provider_matches_current_projection(
                        provider,
                        &profile,
                        profile.local_port,
                        &upstream_model,
                        context_window_tokens,
                    ),
                )
            } else {
                (false, false)
            };

        if profile.status == AdapterProfileStatus::NeedsAttention {
            profile.status = AdapterProfileStatus::Applying;
            profile.last_error_code = None;
            profile.updated_at = now();
            profile = self.profiles.update(&profile)?;
        }

        let local_bearer = match existing_provider.as_ref() {
            Some(provider) => local_bearer_from_provider(provider)?,
            None => generate_local_bearer()?,
        };
        let material = self.attach_route_index(
            AdapterBridgeRuntimeMaterial {
                profile_id: profile.id.clone(),
                source_connection_id: profile.source_id.clone(),
                preferred_port: profile.local_port,
                upstream_base_url,
                upstream_model,
                configured_listed_models,
                context_window_tokens,
                protocol,
                local_surface: rule.local_surface,
                source: rule.source,
                target_agent: rule.target_agent,
                upstream_auth,
                local_bearer,
                route_index: None,
                index_enabled: false,
                codex_ingress_grok_upstream: false,
                grok_ingress_codex_upstream: false,
                schedule_policy: Default::default(),
            },
            &profile,
        );
        Ok(AdapterBridgePrepared {
            material,
            profile,
            generated_provider_exists,
            generated_provider_is_current,
        })
    }

    /// Re-read the generated provider immediately before the desktop host
    /// persists its projection. `prepare` can run before listener bind/health,
    /// so its cached existence result is not safe to use after that gap.  This
    /// revalidation rejects a user-owned id collision and derives Create,
    /// Update, or None from the current row while the caller holds its provider
    /// live-saga guard.
    pub fn revalidate_provider_projection(
        &self,
        prepared: &AdapterBridgePrepared,
        port: u16,
    ) -> Result<AdapterBridgeProviderProjection> {
        validate_bound_port(port)?;
        let profile = self.profiles.get(&prepared.profile.id)?.ok_or_else(|| {
            AppError::NotFound(format!(
                "adapter profile not found: {}",
                prepared.profile.id
            ))
        })?;
        if !same_profile_contract(&profile, &prepared.profile) {
            return Err(AppError::message(
                "adapter.profile_conflict",
                "adapter profile changed while bridge was starting",
            ));
        }
        let provider_id = profile.generated_provider_id.as_deref().ok_or_else(|| {
            AppError::message(
                "adapter.provider_conflict",
                "bridge profile has no generated provider id",
            )
        })?;
        let input = projected_provider_input(
            &profile,
            &prepared.material.local_bearer,
            port,
            &prepared.material.upstream_model,
            prepared.material.context_window_tokens,
        )?;
        match self.providers.get_by_id(provider_id)? {
            Some(provider) => {
                validate_generated_provider(&provider, &profile, profile.local_port)?;
                if local_bearer_from_provider(&provider)? != prepared.material.local_bearer {
                    return Err(AppError::message(
                        "adapter.provider_conflict",
                        "generated bridge provider bearer changed while bridge was starting",
                    ));
                }
                if profile.status == AdapterProfileStatus::Active
                    && profile.local_port == Some(port)
                    && provider_matches_current_projection(
                        &provider,
                        &profile,
                        Some(port),
                        &prepared.material.upstream_model,
                        prepared.material.context_window_tokens,
                    )
                {
                    Ok(AdapterBridgeProviderProjection::None)
                } else {
                    Ok(AdapterBridgeProviderProjection::Update(input))
                }
            }
            None => Ok(AdapterBridgeProviderProjection::Create(input)),
        }
    }

    /// Mark a saga complete after its generated provider was persisted and
    /// switched by `ProviderService`. This method owns profile state only.
    pub(super) fn ensure_supported(
        &self,
        request: &AdapterBridgePrepareRequest,
    ) -> Result<CodexBridgeRule> {
        let analysis = self.routes.analyze(&AdapterRouteRequest {
            source_kind: request.source_kind,
            source_id: request.source_id.clone(),
            target_agent_id: request.target_agent_id,
        })?;
        let rule = analysis
            .rule_id
            .as_deref()
            .and_then(rule_for_id)
            .ok_or_else(|| {
                AppError::Unsupported(
                    "adapter bridge currently supports Kimi / Anthropic / OpenAI → Codex / Claude / Grok, Codex subscription → Claude / Grok / Kimi / DSH, or Grok subscription → Claude / Codex"
                        .into(),
                )
            })?;
        let source_ok = match rule.protocol {
            BridgeUpstreamProtocol::OpenAiChatCompletions => matches!(
                request.source_kind,
                AdapterSourceKind::Provider | AdapterSourceKind::Account
            ),
            BridgeUpstreamProtocol::AnthropicMessages => matches!(
                request.source_kind,
                AdapterSourceKind::Provider | AdapterSourceKind::Account
            ),
            BridgeUpstreamProtocol::CodexResponsesOauth
            | BridgeUpstreamProtocol::XaiResponsesOauth => {
                request.source_kind == AdapterSourceKind::Account
            }
        };
        if source_ok
            && request.target_agent_id == rule.target_agent
            && analysis.route == AdapterRoute::LocalBridge
            && analysis.support == AdapterSupport::Experimental
            && analysis.rule_id.as_deref() == Some(rule.rule_id)
        {
            Ok(rule)
        } else {
            Err(AppError::Unsupported(
                "adapter bridge currently supports Kimi / Anthropic / OpenAI → Codex or Codex subscription → Claude".into(),
            ))
        }
    }

    pub(super) fn resolve_upstream_auth(
        &self,
        rule: &CodexBridgeRule,
        source_kind: AdapterSourceKind,
        source_id: &str,
    ) -> Result<crate::bridge::ResolvedAuth> {
        match (rule.protocol, rule.rule_id) {
            (BridgeUpstreamProtocol::OpenAiChatCompletions, rule_id)
                if rule_id == OPENAI_RULE_ID
                    || rule_id == OPENAI_CLAUDE_EDGE.rule_id
                    || rule_id == OPENAI_GROK_BRIDGE_EDGE.rule_id =>
            {
                self.secrets.resolve_openai_auth(source_kind, source_id)
            }
            (BridgeUpstreamProtocol::OpenAiChatCompletions, _) => self
                .secrets
                .resolve_kimi_membership_auth(source_kind, source_id),
            (BridgeUpstreamProtocol::AnthropicMessages, _) => {
                self.secrets.resolve_anthropic_auth(source_kind, source_id)
            }
            (BridgeUpstreamProtocol::CodexResponsesOauth, _) => self
                .secrets
                .resolve_codex_subscription_auth(source_kind, source_id),
            (BridgeUpstreamProtocol::XaiResponsesOauth, _) => self
                .secrets
                .resolve_grok_subscription_auth(source_kind, source_id),
        }
    }

    /// Resolve one pool member's upstream secret. A sibling failure is isolated
    /// by the caller rather than failing the whole start.
    pub fn resolve_member_auth(
        &self,
        rule_id: &str,
        source_kind: AdapterSourceKind,
        source_id: &str,
    ) -> Result<crate::bridge::ResolvedAuth> {
        let rule = rule_for_id(rule_id).ok_or_else(|| {
            AppError::InvalidArg("adapter profile is not a supported local bridge".into())
        })?;
        self.resolve_upstream_auth(&rule, source_kind, source_id)
    }

    pub(super) fn bridge_profile(&self, profile_id: &str) -> Result<AdapterProfile> {
        let profile_id = profile_id.trim();
        if profile_id.is_empty() {
            return Err(AppError::InvalidArg(
                "adapter bridge profile id must not be empty".into(),
            ));
        }
        let profile = self.profiles.get(profile_id)?.ok_or_else(|| {
            AppError::NotFound(format!("adapter profile not found: {profile_id}"))
        })?;
        let supported_source = match profile.rule_id.as_str() {
            RULE_ID
            | ANTHROPIC_RULE_ID
            | OPENAI_RULE_ID
            | OPENAI_CLAUDE_BRIDGE_RULE_ID
            | OPENAI_GROK_LOCAL_RULE_ID => matches!(
                profile.source_kind,
                AdapterSourceKind::Provider | AdapterSourceKind::Account
            ),
            CODEX_CLAUDE_RULE_ID | CODEX_GROK_RULE_ID | CODEX_KIMI_RULE_ID | CODEX_DSH_RULE_ID => {
                profile.source_kind == AdapterSourceKind::Account
            }
            GROK_CLAUDE_RULE_ID | GROK_CODEX_RULE_ID => {
                profile.source_kind == AdapterSourceKind::Account
            }
            _ => false,
        };
        if !supported_source
            || profile.route != AdapterRoute::LocalBridge
            || rule_for_id(&profile.rule_id).is_none()
            || profile.rule_version != RULE_VERSION
            || rule_for_id(&profile.rule_id)
                .is_none_or(|rule| profile.target_agent_id != rule.target_agent)
        {
            return Err(AppError::InvalidArg(
                "adapter profile is not a supported local bridge".into(),
            ));
        }
        Ok(profile)
    }
}

pub(super) fn openai_source_upstream(
    service: &AdapterBridgeService,
    rule: &CodexBridgeRule,
    source_kind: AdapterSourceKind,
    source_id: &str,
) -> (
    String,
    String,
    Vec<String>,
    crate::bridge::BridgeUpstreamProtocol,
    Option<u32>,
) {
    let mut url = rule.upstream_base_url.to_string();
    let mut model = rule.default_model.to_string();
    let mut listed = Vec::new();
    let mut protocol = rule.protocol;
    let mut context_window_tokens = None;
    if rule.source != crate::models::AdapterSourceProduct::OpenaiApi {
        return (url, model, listed, protocol, context_window_tokens);
    }
    if source_kind != AdapterSourceKind::Provider {
        return (url, model, listed, protocol, context_window_tokens);
    }
    let Ok(Some(provider)) = service.providers.get_by_id(source_id) else {
        return (url, model, listed, protocol, context_window_tokens);
    };
    let target = match rule.target_agent {
        crate::models::AgentId::Claude => "claude",
        crate::models::AgentId::Codex => "codex",
        crate::models::AgentId::Grok => "grok",
        _ => "",
    };
    // For Codex TOML, `openai_compat_base_url` resolves the runtime
    // `model_provider` slug before reading a provider table. Do not let a
    // document-order provider or a textual host match change the upstream.
    if let Some(endpoint) = crate::services::adapter_route_constants::openai_compat_endpoint_url(
        &provider.settings_config,
        target,
    ) {
        url = endpoint;
    } else if let Some(active_base_url) =
        crate::services::adapter_route_constants::openai_compat_base_url(&provider.settings_config)
    {
        url = active_base_url;
    }
    listed = crate::services::adapter_route_constants::openai_compat_listed_models(
        &provider.settings_config,
    );
    if let Some(pinned) = crate::services::adapter_route_constants::openai_compat_pinned_model(
        &provider.settings_config,
    ) {
        model = pinned;
    }
    context_window_tokens =
        crate::services::adapter_route_constants::openai_compat_context_window_tokens(
            &provider.settings_config,
        );
    if crate::services::adapter_route_constants::looks_like_anthropic_messages_url(&url) {
        protocol = crate::bridge::BridgeUpstreamProtocol::AnthropicMessages;
    }
    (url, model, listed, protocol, context_window_tokens)
}
