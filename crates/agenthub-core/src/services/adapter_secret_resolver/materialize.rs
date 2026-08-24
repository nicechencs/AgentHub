use serde_json::{json, Value};
use toml_edit::DocumentMut;

use super::helpers::*;
use super::*;
use crate::error::Result;
use crate::models::{AdapterSourceKind, Provider};
use crate::services::adapter_route_constants::*;

impl AdapterSecretResolver {
    pub fn is_reference_provider(&self, provider: &Provider) -> Result<bool> {
        match provider.meta.get("generatedBy").and_then(Value::as_str) {
            Some(GENERATED_BY) if is_claude_source_reference(provider) => {
                self.validate_claude_reference_target(provider)?;
                Ok(true)
            }
            Some(GENERATED_BY) if is_codex_source_reference(provider) => {
                self.validate_codex_reference_target(provider)?;
                Ok(true)
            }
            Some(GENERATED_BY) if is_pi_source_reference(provider) => {
                self.validate_pi_reference_target(provider)?;
                Ok(true)
            }
            Some(GENERATED_BY) if is_dsh_source_reference(provider) => {
                self.validate_dsh_reference_target(provider)?;
                Ok(true)
            }
            Some(GENERATED_BY) if is_grok_source_reference(provider) => {
                self.validate_grok_reference_target(provider)?;
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
            let api_key = self.resolve_referenced_api_key(target)?;
            let mut materialized = target.clone();
            let env = materialized
                .settings_config
                .get_mut("env")
                .and_then(Value::as_object_mut)
                .ok_or_else(invalid_reference)?;
            env.insert(ANTHROPIC_AUTH_TOKEN_ENV.into(), Value::String(api_key));
            return Ok(materialized);
        }

        if is_codex_source_reference(target) {
            self.validate_codex_reference_target(target)?;
            let api_key = self.resolve_referenced_api_key(target)?;
            let mut materialized = target.clone();
            let content = materialized
                .settings_config
                .get("content")
                .and_then(Value::as_str)
                .ok_or_else(invalid_reference)?
                .parse::<DocumentMut>()
                .map_err(|_| invalid_reference())?;
            let mut document = content;
            let slug = codex_provider_slug(adapter_rule_id(target).unwrap_or(""))?;
            document["model_providers"][slug]["experimental_bearer_token"] =
                toml_edit::value(api_key.as_str());
            materialized.settings_config["content"] = Value::String(document.to_string());
            materialized.settings_config["auth"]["OPENAI_API_KEY"] = Value::String(api_key);
            return Ok(materialized);
        }

        if is_dsh_source_reference(target) {
            self.validate_dsh_reference_target(target)?;
            let api_key = self.resolve_referenced_api_key(target)?;
            let mut materialized = target.clone();
            let obj = materialized
                .settings_config
                .as_object_mut()
                .ok_or_else(invalid_reference)?;
            obj.insert("api_key".into(), Value::String(api_key));
            return Ok(materialized);
        }

        if is_grok_source_reference(target) {
            self.validate_grok_reference_target(target)?;
            let api_key = self.resolve_referenced_api_key(target)?;
            let mut materialized = target.clone();
            let content = materialized
                .settings_config
                .get("content")
                .and_then(Value::as_str)
                .ok_or_else(invalid_reference)?
                .parse::<DocumentMut>()
                .map_err(|_| invalid_reference())?;
            let (_, _, alias) = grok_contract(adapter_rule_id(target).unwrap_or(""))?;
            let mut document = content;
            document["model"][alias]["api_key"] = toml_edit::value(api_key);
            materialized.settings_config["content"] = Value::String(document.to_string());
            return Ok(materialized);
        }

        if is_pi_subscription_reference(target) {
            self.validate_pi_reference_target(target)?;
            let (kind, source_id) = self.reference_source_ref(target)?;
            let rule = adapter_rule_id(target).ok_or_else(invalid_reference)?;
            let tokens = self.resolve_subscription_oauth(rule, kind, source_id)?;
            let slot = pi_slot_name(target)?;
            let mut materialized = target.clone();
            set_pi_slot_oauth(
                &mut materialized.settings_config,
                slot,
                &tokens.access,
                tokens.refresh.as_deref(),
                tokens.expires_at.as_deref(),
            )?;
            return Ok(materialized);
        }

        self.validate_pi_reference_target(target)?;
        let api_key = self.resolve_referenced_api_key(target)?;
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
            return Ok(strip_pi_auth_for_persist(provider, live_raw));
        }

        if is_claude_source_reference(provider) {
            self.validate_claude_reference_target(provider)?;
            // A deleted or invalid source must not cause us to persist a live secret
            // into a row which we can no longer safely re-materialize.
            let _ = self.resolve_referenced_api_key(provider)?;

            let mut scrubbed = live_raw.clone();
            let env = scrubbed
                .get_mut("env")
                .and_then(Value::as_object_mut)
                .ok_or_else(invalid_reference)?;
            let expected_base = claude_native_base_url(adapter_rule_id(provider).unwrap_or(""))
                .ok_or_else(invalid_reference)?;
            if env.get(ANTHROPIC_BASE_URL_ENV).and_then(Value::as_str) != Some(expected_base) {
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

        if is_codex_source_reference(provider) {
            self.validate_codex_reference_target(provider)?;
            let _ = self.resolve_referenced_api_key(provider)?;
            let mut scrubbed = live_raw.clone();
            if scrubbed.get("format").and_then(Value::as_str) != Some("toml") {
                return Err(invalid_reference());
            }
            let content = scrubbed
                .get("content")
                .and_then(Value::as_str)
                .ok_or_else(invalid_reference)?
                .to_owned();
            let mut document = content
                .parse::<DocumentMut>()
                .map_err(|_| invalid_reference())?;
            let (expected_base, slug) = codex_contract(adapter_rule_id(provider).unwrap_or(""))?;
            let table = document["model_providers"]
                .get(slug)
                .and_then(|item| item.as_table())
                .ok_or_else(invalid_reference)?;
            if document
                .get("model_provider")
                .and_then(|item| item.as_str())
                != Some(slug)
                || table.get("base_url").and_then(|item| item.as_str()) != Some(expected_base)
                || table.get("wire_api").and_then(|item| item.as_str()) != Some("responses")
                || table
                    .get("experimental_bearer_token")
                    .and_then(|item| item.as_str())
                    .is_none_or(|value| value.trim().is_empty())
            {
                return Err(invalid_reference());
            }
            document["model_providers"][slug]["experimental_bearer_token"] =
                toml_edit::value(CONNECTION_SECRET_MARKER);
            scrubbed["content"] = Value::String(document.to_string());
            let auth = scrubbed
                .get_mut("auth")
                .and_then(Value::as_object_mut)
                .ok_or_else(invalid_reference)?;
            if auth
                .get("OPENAI_API_KEY")
                .and_then(Value::as_str)
                .is_none_or(|value| value.trim().is_empty())
            {
                return Err(invalid_reference());
            }
            auth.insert(
                "OPENAI_API_KEY".into(),
                Value::String(CONNECTION_SECRET_MARKER.into()),
            );
            return Ok(scrubbed);
        }

        if is_dsh_source_reference(provider) {
            self.validate_dsh_reference_target(provider)?;
            let _ = self.resolve_referenced_api_key(provider)?;
            let mut scrubbed = live_raw.clone();
            let obj = scrubbed.as_object_mut().ok_or_else(invalid_reference)?;
            if obj.get("baseURL").and_then(Value::as_str) != Some(DEEPSEEK_API_BASE_URL)
                && obj.get("baseUrl").and_then(Value::as_str) != Some(DEEPSEEK_API_BASE_URL)
            {
                return Err(invalid_reference());
            }
            if obj.get("api_key").or_else(|| obj.get("apiKey")).is_none() {
                return Err(invalid_reference());
            }
            obj.insert(
                "api_key".into(),
                Value::String(CONNECTION_SECRET_MARKER.into()),
            );
            obj.remove("apiKey");
            return Ok(scrubbed);
        }

        if is_grok_source_reference(provider) {
            self.validate_grok_reference_target(provider)?;
            let _ = self.resolve_referenced_api_key(provider)?;
            let mut scrubbed = live_raw.clone();
            if scrubbed.get("format").and_then(Value::as_str) != Some("toml") {
                return Err(invalid_reference());
            }
            let content = scrubbed
                .get("content")
                .and_then(Value::as_str)
                .ok_or_else(invalid_reference)?
                .parse::<DocumentMut>()
                .map_err(|_| invalid_reference())?;
            let (expected_base, expected_model, alias) =
                grok_contract(adapter_rule_id(provider).unwrap_or(""))?;
            let table = content["model"]
                .get(alias)
                .and_then(|item| item.as_table())
                .ok_or_else(invalid_reference)?;
            if content["models"]
                .get("default")
                .and_then(|item| item.as_str())
                != Some(alias)
                || table.get("base_url").and_then(|item| item.as_str()) != Some(expected_base)
                || table.get("model").and_then(|item| item.as_str()) != Some(expected_model)
                || table.get("api_backend").and_then(|item| item.as_str())
                    != Some("chat_completions")
                || table
                    .get("api_key")
                    .and_then(|item| item.as_str())
                    .is_none_or(|value| value.trim().is_empty())
            {
                return Err(invalid_reference());
            }
            let mut document = content;
            document["model"][alias]["api_key"] = toml_edit::value(CONNECTION_SECRET_MARKER);
            scrubbed["content"] = Value::String(document.to_string());
            return Ok(scrubbed);
        }

        if is_pi_subscription_reference(provider) {
            self.validate_pi_reference_target(provider)?;
            let _ = self.resolve_referenced_subscription_oauth(provider)?;
            let slot = pi_slot_name(provider)?;
            let live_slot = pi_auth_slot_object(live_raw, slot).ok_or_else(invalid_reference)?;
            if !live_slot
                .get("access")
                .and_then(Value::as_str)
                .is_some_and(|value| !value.trim().is_empty())
            {
                return Err(invalid_reference());
            }
            // Persist only the owned slot as markers. Drop `paths` so a later
            // switch merges instead of replacing the whole auth.json file.
            let mut scrubbed = json!({
                "auth": {
                    slot: {
                        "type": "oauth",
                        "access": CONNECTION_SECRET_MARKER,
                        "refresh": CONNECTION_SECRET_MARKER
                    }
                }
            });
            if let Some(settings) = live_raw.get("settings") {
                scrubbed
                    .as_object_mut()
                    .expect("scrubbed auth object")
                    .insert("settings".into(), settings.clone());
            }
            if let Some(models) = live_raw.get("models") {
                scrubbed
                    .as_object_mut()
                    .expect("scrubbed auth object")
                    .insert("models".into(), models.clone());
            }
            return Ok(scrubbed);
        }

        self.validate_pi_reference_target(provider)?;
        let _ = self.resolve_referenced_api_key(provider)?;
        let slot = pi_slot_name(provider)?;
        let mut scrubbed = strip_pi_auth_for_persist(provider, live_raw);
        let live_slot = pi_slot_object(&scrubbed, slot).ok_or_else(invalid_reference)?;
        if let Some(expected_base) = pi_base_url_for_rule(adapter_rule_id(provider).unwrap_or("")) {
            if live_slot.get("baseUrl").and_then(Value::as_str) != Some(expected_base) {
                return Err(invalid_reference());
            }
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

    pub(super) fn resolve_referenced_api_key(&self, target: &Provider) -> Result<String> {
        let (kind, source_id) = self.reference_source_ref(target)?;
        let rule = adapter_rule_id(target).ok_or_else(invalid_reference)?;
        match (rule, kind) {
            (
                KIMI_TO_CLAUDE_RULE | KIMI_TO_PI_RULE | KIMI_TO_GROK_RULE,
                AdapterSourceKind::Provider | AdapterSourceKind::Account,
            ) => self.resolve_kimi_membership_api_key(kind, source_id),
            (
                ANTHROPIC_TO_PI_RULE
                | OPENAI_TO_PI_RULE
                | XAI_TO_PI_RULE
                | GLM_TO_PI_RULE
                | DEEPSEEK_TO_PI_RULE
                | GLM_TO_CLAUDE_RULE
                | DEEPSEEK_TO_CLAUDE_RULE,
                AdapterSourceKind::Provider | AdapterSourceKind::Account,
            ) => self.resolve_explicit_api_key(rule, kind, source_id),
            (OPENAI_TO_GROK_RULE, AdapterSourceKind::Provider | AdapterSourceKind::Account) => {
                self.resolve_explicit_api_key(rule, kind, source_id)
            }
            (
                GLM_TO_CODEX_RULE | DEEPSEEK_TO_CODEX_RULE,
                AdapterSourceKind::Provider | AdapterSourceKind::Account,
            ) => self.resolve_explicit_api_key(rule, kind, source_id),
            (DEEPSEEK_TO_DSH_RULE, AdapterSourceKind::Provider) => {
                self.resolve_deepseek_api_key(source_id)
            }
            (
                CLAUDE_SUBSCRIPTION_PI_RULE
                | CODEX_SUBSCRIPTION_PI_RULE
                | GROK_SUBSCRIPTION_PI_RULE,
                AdapterSourceKind::Account,
            ) => self
                .resolve_subscription_oauth(rule, kind, source_id)
                .map(|tokens| tokens.access),
            _ => Err(invalid_reference()),
        }
    }

    pub(super) fn resolve_referenced_subscription_oauth(
        &self,
        target: &Provider,
    ) -> Result<PiOAuthTokens> {
        let (kind, source_id) = self.reference_source_ref(target)?;
        let rule = adapter_rule_id(target).ok_or_else(invalid_reference)?;
        self.resolve_subscription_oauth(rule, kind, source_id)
    }

    pub(super) fn resolve_subscription_oauth(
        &self,
        rule_id: &str,
        source_kind: AdapterSourceKind,
        source_id: &str,
    ) -> Result<PiOAuthTokens> {
        if !is_subscription_pi_rule(rule_id) || source_kind != AdapterSourceKind::Account {
            return Err(invalid_reference());
        }
        let account = self
            .accounts
            .get_by_id(source_id.trim())?
            .ok_or_else(invalid_reference)?;
        if account.kind != crate::models::AccountKind::Oauth {
            return Err(invalid_reference());
        }
        let credentials = &account.credentials;
        let access = first_usable_string(
            credentials,
            &[
                "/access_token",
                "/tokens/access_token",
                "/body/tokens/access_token",
            ],
        )
        .ok_or_else(invalid_reference)?;
        let refresh = first_usable_string(
            credentials,
            &[
                "/refresh_token",
                "/tokens/refresh_token",
                "/body/tokens/refresh_token",
            ],
        );
        let expires_at = first_usable_string(
            credentials,
            &[
                "/expires_at",
                "/tokens/expires_at",
                "/body/tokens/expires_at",
            ],
        );
        Ok(PiOAuthTokens {
            access,
            refresh,
            expires_at,
        })
    }

    pub(super) fn reference_source_id<'a>(&self, target: &'a Provider) -> Result<&'a str> {
        self.reference_source_ref(target).map(|(_, id)| id)
    }

    pub(super) fn reference_source_ref<'a>(
        &self,
        target: &'a Provider,
    ) -> Result<(AdapterSourceKind, &'a str)> {
        if !is_claude_source_reference(target)
            && !is_codex_source_reference(target)
            && !is_pi_source_reference(target)
            && !is_dsh_source_reference(target)
            && !is_grok_source_reference(target)
        {
            return Err(invalid_reference());
        }
        let source = target
            .meta
            .get("adapterSourceRef")
            .and_then(Value::as_object)
            .ok_or_else(invalid_reference)?;
        let kind = match source.get("kind").and_then(Value::as_str) {
            Some(SOURCE_KIND_PROVIDER) => AdapterSourceKind::Provider,
            Some(SOURCE_KIND_ACCOUNT) if !is_dsh_source_reference(target) => {
                AdapterSourceKind::Account
            }
            _ => return Err(invalid_reference()),
        };
        let id = source
            .get("id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .ok_or_else(invalid_reference)?;
        Ok((kind, id))
    }

    pub(super) fn validate_claude_reference_target(&self, target: &Provider) -> Result<()> {
        self.reference_source_id(target)?;
        let env = target
            .settings_config
            .get("env")
            .and_then(Value::as_object)
            .ok_or_else(invalid_reference)?;
        let expected_base = claude_native_base_url(adapter_rule_id(target).unwrap_or(""))
            .ok_or_else(invalid_reference)?;
        if env.get(ANTHROPIC_AUTH_TOKEN_ENV).and_then(Value::as_str)
            != Some(CONNECTION_SECRET_MARKER)
            || env.get(ANTHROPIC_BASE_URL_ENV).and_then(Value::as_str) != Some(expected_base)
        {
            return Err(invalid_reference());
        }
        Ok(())
    }

    pub(super) fn validate_pi_reference_target(&self, target: &Provider) -> Result<()> {
        self.reference_source_id(target)?;
        let slot = pi_slot_name(target)?;
        if is_pi_subscription_reference(target) {
            let slot_obj =
                pi_auth_slot_object(&target.settings_config, slot).ok_or_else(invalid_reference)?;
            if slot_obj.get("type").and_then(Value::as_str) != Some("oauth")
                || slot_obj.get("access").and_then(Value::as_str) != Some(CONNECTION_SECRET_MARKER)
                || slot_obj.get("refresh").and_then(Value::as_str) != Some(CONNECTION_SECRET_MARKER)
            {
                return Err(invalid_reference());
            }
            return Ok(());
        }
        let slot_obj =
            pi_slot_object(&target.settings_config, slot).ok_or_else(invalid_reference)?;
        if slot_obj.get("apiKey").and_then(Value::as_str) != Some(CONNECTION_SECRET_MARKER) {
            return Err(invalid_reference());
        }
        if let Some(expected_base) = pi_base_url_for_rule(adapter_rule_id(target).unwrap_or("")) {
            if slot_obj.get("baseUrl").and_then(Value::as_str) != Some(expected_base) {
                return Err(invalid_reference());
            }
        }
        if matches!(
            adapter_rule_id(target),
            Some(GLM_TO_PI_RULE) | Some(DEEPSEEK_TO_PI_RULE)
        ) && (slot_obj.get("api").and_then(Value::as_str) != Some("openai-completions")
            || slot_obj.get("models").and_then(Value::as_array).is_none())
        {
            return Err(invalid_reference());
        }
        Ok(())
    }

    pub(super) fn validate_dsh_reference_target(&self, target: &Provider) -> Result<()> {
        self.reference_source_id(target)?;
        let obj = target
            .settings_config
            .as_object()
            .ok_or_else(invalid_reference)?;
        if obj.get("api_key").and_then(Value::as_str) != Some(CONNECTION_SECRET_MARKER) {
            return Err(invalid_reference());
        }
        if obj.get("apiKeyEnv").and_then(Value::as_str) != Some(DSH_API_KEY_ENV) {
            return Err(invalid_reference());
        }
        if obj.get("provider").and_then(Value::as_str) != Some(DSH_DEEPSEEK_PROVIDER_SLOT) {
            return Err(invalid_reference());
        }
        if obj.get("baseURL").and_then(Value::as_str) != Some(DEEPSEEK_API_BASE_URL)
            && obj.get("baseUrl").and_then(Value::as_str) != Some(DEEPSEEK_API_BASE_URL)
        {
            return Err(invalid_reference());
        }
        Ok(())
    }

    pub(super) fn validate_grok_reference_target(&self, target: &Provider) -> Result<()> {
        self.reference_source_id(target)?;
        if target.settings_config.get("format").and_then(Value::as_str) != Some("toml") {
            return Err(invalid_reference());
        }
        let content = target
            .settings_config
            .get("content")
            .and_then(Value::as_str)
            .ok_or_else(invalid_reference)?;
        let document = content
            .parse::<DocumentMut>()
            .map_err(|_| invalid_reference())?;
        let (expected_base, expected_model, alias) =
            grok_contract(adapter_rule_id(target).unwrap_or(""))?;
        let table = document["model"]
            .get(alias)
            .and_then(|item| item.as_table())
            .ok_or_else(invalid_reference)?;
        if document["models"]
            .get("default")
            .and_then(|item| item.as_str())
            != Some(alias)
            || table.get("base_url").and_then(|item| item.as_str()) != Some(expected_base)
            || table.get("model").and_then(|item| item.as_str()) != Some(expected_model)
            || table.get("api_backend").and_then(|item| item.as_str()) != Some("chat_completions")
            || table.get("api_key").and_then(|item| item.as_str()) != Some(CONNECTION_SECRET_MARKER)
        {
            return Err(invalid_reference());
        }
        Ok(())
    }

    pub(super) fn validate_codex_reference_target(&self, target: &Provider) -> Result<()> {
        self.reference_source_id(target)?;
        if target.settings_config.get("format").and_then(Value::as_str) != Some("toml") {
            return Err(invalid_reference());
        }
        let content = target
            .settings_config
            .get("content")
            .and_then(Value::as_str)
            .ok_or_else(invalid_reference)?;
        let document = content
            .parse::<DocumentMut>()
            .map_err(|_| invalid_reference())?;
        let rule = adapter_rule_id(target).ok_or_else(invalid_reference)?;
        let (expected_base, slug) = codex_contract(rule)?;
        let table = document["model_providers"]
            .get(slug)
            .and_then(|item| item.as_table())
            .ok_or_else(invalid_reference)?;
        if document
            .get("model_provider")
            .and_then(|item| item.as_str())
            != Some(slug)
            || table.get("base_url").and_then(|item| item.as_str()) != Some(expected_base)
            || table.get("wire_api").and_then(|item| item.as_str()) != Some("responses")
            || table
                .get("experimental_bearer_token")
                .and_then(|item| item.as_str())
                != Some(CONNECTION_SECRET_MARKER)
            || target
                .settings_config
                .get("auth")
                .and_then(Value::as_object)
                .and_then(|auth| auth.get("OPENAI_API_KEY"))
                .and_then(Value::as_str)
                != Some(CONNECTION_SECRET_MARKER)
        {
            return Err(invalid_reference());
        }
        Ok(())
    }

    pub(super) fn validate_local_token_target(&self, target: &Provider) -> Result<()> {
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
                    !matches!(
                        source.get("kind").and_then(Value::as_str),
                        Some(SOURCE_KIND_PROVIDER) | Some(SOURCE_KIND_ACCOUNT)
                    ) || source
                        .get("id")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|id| !id.is_empty())
                        .is_none()
                })
            || !valid_local_token_projection(target)
        {
            return Err(invalid_reference());
        }
        Ok(())
    }
}
