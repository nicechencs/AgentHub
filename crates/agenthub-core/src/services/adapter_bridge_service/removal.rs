use super::*;
use super::rules::*;

impl AdapterBridgeService {
    pub fn preflight_remove(&self, profile_id: &str) -> Result<AdapterBridgeRemoval> {
        let profile = self.bridge_profile(profile_id)?;
        let rule = rule_for_id(&profile.rule_id).ok_or_else(|| {
            AppError::InvalidArg("adapter profile is not a supported local bridge".into())
        })?;
        let expected_provider_id = stable_id(rule.provider_prefix, &profile.source_id);
        if profile.generated_provider_id.as_deref() != Some(expected_provider_id.as_str()) {
            return Err(AppError::message(
                "adapter.provider_conflict",
                "bridge profile generated provider id is invalid",
            ));
        }

        let generated_provider = match self.providers.get_by_id(&expected_provider_id)? {
            Some(provider) => {
                validate_generated_provider(&provider, &profile, profile.local_port)?;
                if provider.is_current {
                    return Err(AppError::Unsupported(
                        "先在 Connections 切换后再移除此适配器".into(),
                    ));
                }
                Some(provider)
            }
            // A listener or projection failure can leave an applying or
            // retryable/attention-needed profile before any generated provider exists.
            // Such a profile is still safe to clean up, but an active profile
            // without its projection is corrupted and must fail closed.
            None if profile.status != AdapterProfileStatus::Active => None,
            None => {
                return Err(AppError::message(
                    "adapter.provider_missing",
                    "active bridge profile has no generated provider",
                ));
            }
        };

        Ok(AdapterBridgeRemoval {
            profile,
            generated_provider,
        })
    }

    /// Deletes the already-validated profile after its non-current generated
    /// provider has been removed through `ProviderService`.
    ///
    /// The check is repeated after listener shutdown to detect any profile or
    /// provider mutation that occurred after `preflight_remove`.
    pub fn complete_remove(&self, removal: &AdapterBridgeRemoval) -> Result<()> {
        let profile = self.bridge_profile(&removal.profile.id)?;
        if profile.generated_provider_id != removal.profile.generated_provider_id
            || profile.source_id != removal.profile.source_id
            || profile.target_agent_id != removal.profile.target_agent_id
            || profile.rule_id != removal.profile.rule_id
            || profile.rule_version != removal.profile.rule_version
        {
            return Err(AppError::message(
                "adapter.profile_conflict",
                "adapter bridge profile changed while removing it",
            ));
        }
        if let Some(provider_id) = removal.generated_provider_id() {
            if self.providers.get_by_id(provider_id)?.is_some() {
                return Err(AppError::message(
                    "adapter.provider_conflict",
                    "generated bridge provider still exists during profile removal",
                ));
            }
        }
        self.profiles.delete(&profile.id)
    }
}
