//! Post-listen persist / restore-port / rollback for local-bridge apply.
//!
//! The Tauri host still binds the listener and schedules the saga. This module
//! owns the pool/live mutations after a port is known. Guarded Provider
//! methods keep their existing contract; non-current restore-port must not
//! rewrite live config.

use crate::error::AppError;
use crate::models::{AdapterApplyResult, AgentId, Provider, ProviderInput};
use crate::services::{ProviderLiveConfigSnapshot, ProviderLiveSagaGuard, ProviderService};
use crate::utils::redact::redact_text;

use super::{AdapterBridgePrepared, AdapterBridgeProviderProjection, AdapterBridgeService};

/// Pool + live snapshot captured before this saga mutates either.
#[derive(Clone)]
pub struct BridgeProviderSnapshot {
    generated: Option<Provider>,
    current_provider: Option<Provider>,
    live_config: ProviderLiveConfigSnapshot,
}

fn map_persist_err(_op: &str, err: AppError) -> String {
    format!("{} [{}]", redact_text(&err.to_string()), err.code())
}

fn composite_saga_error(
    operation: &str,
    original: String,
    rollback: std::result::Result<(), &'static str>,
) -> String {
    match rollback {
        Ok(()) => original,
        Err(code) => format!("{operation} failed and compensation was incomplete [{code}]"),
    }
}

fn provider_to_non_current_input(provider: &Provider) -> ProviderInput {
    ProviderInput {
        id: provider.id.clone(),
        agent_id: provider.agent_id,
        name: provider.name.clone(),
        settings_config: provider.settings_config.clone(),
        meta: provider.meta.clone(),
        // Restore the saved pool row before asking ProviderService to select
        // the pre-saga current provider (if there was one).
        is_current: false,
    }
}

/// Refresh live only if the generated loopback is already current.
pub fn should_make_bridge_current(generated_was_current: bool) -> bool {
    generated_was_current
}

impl AdapterBridgeService {
    pub fn persist_bridge_projection_inner(
        &self,
        providers: &ProviderService,
        core_guard: &ProviderLiveSagaGuard<'_>,
        prepared: &AdapterBridgePrepared,
        projection: AdapterBridgeProviderProjection,
        port: u16,
        snapshot: &BridgeProviderSnapshot,
    ) -> std::result::Result<AdapterApplyResult, String> {
        let provider_id = prepared
            .profile()
            .generated_provider_id
            .as_deref()
            .ok_or_else(|| "adapter bridge profile has no generated provider".to_string())?
            .to_owned();

        // Keep the row returned by create/update.  Re-reading it below is not only
        // unnecessary, it creates a failure window after the projection has already
        // been persisted: a transient read error would bypass compensation and
        // leave the provider/profile pair out of sync.
        let (created, projected_provider) = match projection {
            AdapterBridgeProviderProjection::Create(input) => {
                let provider = providers
                    .create_with_guard(core_guard, &input)
                    .map_err(|error| map_persist_err("create_adapter_bridge_provider", error))?;
                (true, Some(provider))
            }
            AdapterBridgeProviderProjection::Update(input) => {
                let provider = providers
                    .update_with_guard(core_guard, &input)
                    .map_err(|error| map_persist_err("update_adapter_bridge_provider", error))?;
                (false, Some(provider))
            }
            AdapterBridgeProviderProjection::None => (false, None),
        };

        let generated_was_current = snapshot
            .generated
            .as_ref()
            .map(|provider| provider.is_current)
            .unwrap_or(false);
        let should_switch = should_make_bridge_current(generated_was_current);

        let previous_current_id = snapshot
            .current_provider
            .as_ref()
            .map(|provider| provider.id.as_str())
            .filter(|id| *id != provider_id.as_str());
        let provider = if should_switch {
            match providers.switch_with_guard(
                core_guard,
                &provider_id,
                prepared.profile().target_agent_id,
            ) {
                Ok(result) => {
                    let backup_id = result.backup.as_ref().map(|backup| backup.id.as_str());
                    match providers.persist_first_bind_restore_meta_with_guard(
                        core_guard,
                        &result.provider,
                        previous_current_id,
                        backup_id,
                    ) {
                        Ok(provider) => provider.redacted(),
                        Err(error) => {
                            let rollback = self.rollback_bridge_projection(
                                providers,
                                core_guard,
                                &provider_id,
                                snapshot,
                                created,
                                should_switch,
                                prepared.profile().target_agent_id,
                            );
                            return Err(composite_saga_error(
                                "persist_adapter_bridge_restore_meta",
                                map_persist_err("persist_adapter_bridge_restore_meta", error),
                                rollback,
                            ));
                        }
                    }
                }
                Err(error) => {
                    let rollback = self.rollback_bridge_projection(
                        providers,
                        core_guard,
                        &provider_id,
                        snapshot,
                        created,
                        should_switch,
                        prepared.profile().target_agent_id,
                    );
                    return Err(composite_saga_error(
                        "switch_adapter_bridge_provider",
                        map_persist_err("switch_adapter_bridge_provider", error),
                        rollback,
                    ));
                }
            }
        } else if let Some(provider) = projected_provider {
            provider.redacted()
        } else {
            // `None` means no pool mutation was needed.  The provider must still
            // exist for the result, but this read happens only on the no-op path,
            // before any new projection has been written by this saga.
            providers
                .get_by_id(&provider_id)
                .map_err(|error| map_persist_err("load_adapter_bridge_provider", error))?
                .ok_or_else(|| "adapter bridge provider missing after projection".to_string())?
                .redacted()
        };
        let profile = match self.finalize(prepared, port) {
            Ok(profile) => profile,
            Err(error) => {
                let rollback = self.rollback_bridge_projection(
                    providers,
                    core_guard,
                    &provider_id,
                    snapshot,
                    created,
                    should_switch,
                    prepared.profile().target_agent_id,
                );
                return Err(composite_saga_error(
                    "finalize_adapter_bridge",
                    map_persist_err("finalize_adapter_bridge", error),
                    rollback,
                ));
            }
        };
        Ok(AdapterApplyResult { profile, provider })
    }

    pub fn capture_provider_snapshot(
        &self,
        providers: &ProviderService,
        core_guard: &ProviderLiveSagaGuard<'_>,
        generated_provider_id: Option<&str>,
        target_agent: AgentId,
    ) -> std::result::Result<BridgeProviderSnapshot, String> {
        let _ = self;
        let generated = match generated_provider_id {
            Some(id) => providers
                .get_by_id(id)
                .map_err(|error| map_persist_err("snapshot_adapter_bridge_provider", error))?,
            None => None,
        };
        let current_provider = providers
            .get_current(target_agent)
            .map_err(|error| map_persist_err("snapshot_adapter_bridge_provider", error))?;
        let live_config = providers
            .capture_live_config_snapshot_with_guard(core_guard, target_agent)
            .map_err(|error| map_persist_err("snapshot_adapter_bridge_live_config", error))?;
        Ok(BridgeProviderSnapshot {
            generated,
            current_provider,
            live_config,
        })
    }

    /// Inverse of persist: restore the generated pool row. Reverse live switch
    /// only when this saga actually refreshed current (`switched_live`).
    pub fn rollback_bridge_projection(
        &self,
        providers: &ProviderService,
        core_guard: &ProviderLiveSagaGuard<'_>,
        provider_id: &str,
        snapshot: &BridgeProviderSnapshot,
        created: bool,
        switched_live: bool,
        target_agent: AgentId,
    ) -> std::result::Result<(), &'static str> {
        let _ = self;
        let mut failed = false;

        if let Some(old) = &snapshot.generated {
            let input = provider_to_non_current_input(old);
            if providers.update_with_guard(core_guard, &input).is_err() {
                failed = true;
            }
        } else if created
            && providers
                .delete_with_guard(core_guard, provider_id, target_agent)
                .is_err()
        {
            failed = true;
        }

        if !switched_live {
            return if failed {
                Err("adapter.bridge_rollback")
            } else {
                Ok(())
            };
        }

        if let Some(old_current) = &snapshot.current_provider {
            if providers
                .switch_with_guard(core_guard, &old_current.id, target_agent)
                .is_err()
            {
                failed = true;
            }
        }

        // A provider switch back to an old current row may backfill a drifted
        // value rather than the byte-exact snapshot captured before this saga.
        // Restore the snapshot last so failed finalize/switch cannot leave live
        // config changed.
        if providers
            .restore_live_config_snapshot_with_guard(core_guard, &snapshot.live_config)
            .is_err()
        {
            failed = true;
        }

        if failed {
            Err("adapter.bridge_rollback")
        } else {
            Ok(())
        }
    }

    pub fn realign_restored_bridge_port(
        &self,
        providers: &ProviderService,
        profile_id: &str,
        port: u16,
    ) -> std::result::Result<(), String> {
        let profile = self
            .profiles
            .get(profile_id)
            .map_err(|error| map_persist_err("load_adapter_bridge_restore_profile", error))?
            .ok_or_else(|| format!("adapter profile not found: {profile_id}"))?;
        let target_agent = profile.target_agent_id;
        let core_guard = providers
            .begin_live_saga(target_agent)
            .map_err(|error| map_persist_err("begin_adapter_bridge_restore_rebind_saga", error))?;
        let (input, was_current) = self
            .projection_for_restored_port(profile_id, port)
            .map_err(|error| map_persist_err("projection_adapter_bridge_restore_port", error))?;
        // Snapshot before any pool or live mutation so a later-stage failure can
        // restore the previous generated row and target config in reverse order.
        let snapshot = self.capture_provider_snapshot(
            providers,
            &core_guard,
            Some(input.id.as_str()),
            target_agent,
        )?;
        let provider_id = input.id.clone();

        if let Err(error) = providers.update_with_guard(&core_guard, &input) {
            // SQL ABORT leaves the pool unchanged. Restore-port projections are
            // always demoted, so live config was not written and must not be
            // rewritten here (that would replace the injected update error).
            return Err(map_persist_err("update_adapter_bridge_restore_port", error));
        }
        if was_current {
            if let Err(error) = providers.switch_with_guard(&core_guard, &provider_id, target_agent)
            {
                let rollback = self.rollback_bridge_projection(
                    providers,
                    &core_guard,
                    &provider_id,
                    &snapshot,
                    false,
                    true,
                    target_agent,
                );
                return Err(composite_saga_error(
                    "switch_adapter_bridge_restore_port",
                    map_persist_err("switch_adapter_bridge_restore_port", error),
                    rollback,
                ));
            }
        }
        if let Err(error) = self.persist_restored_port(profile_id, port) {
            // persist_restored_port is last and transactional, so a failure leaves
            // profile.local_port on the old preferred port. Restore the generated
            // provider row; rewrite live config only when switch already mutated it.
            let rollback = self.rollback_restored_bridge_port(
                providers,
                &core_guard,
                &provider_id,
                &snapshot,
                was_current,
                target_agent,
            );
            return Err(composite_saga_error(
                "persist_adapter_bridge_restore_port",
                map_persist_err("persist_adapter_bridge_restore_port", error),
                rollback,
            ));
        }
        Ok(())
    }

    /// Inverse of a restore-port apply after the demoted provider row was written.
    ///
    /// Non-current restores never call `switch_with_guard`, so live config is
    /// untouched and a full [`Self::rollback_bridge_projection`] would write the
    /// real Codex file unnecessarily. Current restores already switched, so they
    /// reuse the apply-saga inverse including live snapshot restore.
    pub fn rollback_restored_bridge_port(
        &self,
        providers: &ProviderService,
        core_guard: &ProviderLiveSagaGuard<'_>,
        provider_id: &str,
        snapshot: &BridgeProviderSnapshot,
        was_current: bool,
        target_agent: AgentId,
    ) -> std::result::Result<(), &'static str> {
        if was_current {
            return self.rollback_bridge_projection(
                providers,
                core_guard,
                provider_id,
                snapshot,
                false,
                true,
                target_agent,
            );
        }
        if let Some(old) = &snapshot.generated {
            let input = provider_to_non_current_input(old);
            if providers.update_with_guard(core_guard, &input).is_err() {
                return Err("adapter.bridge_rollback");
            }
        }
        Ok(())
    }
}
