//! Vendor plugin / extension pack inventory and supported package operations.

use agenthub_core::models::AgentId;
use agenthub_core::services::{
    disable_plugin as disable_plugin_impl, enable_plugin as enable_plugin_impl,
    install_plugin as install_plugin_impl, list_available_plugins as list_available_plugins_impl,
    list_plugin_inventory as list_plugin_inventory_impl, plugin_inventory_agent_ids,
    plugin_inventory_agent_verified, plugin_inventory_target, plugin_inventory_target_candidates,
    plugin_inventory_target_is_ambiguous, plugin_inventory_versions_or_entries_changed,
    preview_plugin_install as preview_plugin_install_impl,
    refresh_plugin_marketplace as refresh_plugin_marketplace_impl,
    uninstall_plugin as uninstall_plugin_impl, update_pi_plugins as update_pi_plugins_impl,
    update_plugin as update_plugin_impl, PluginEntry, PluginInstallOptions, PluginInventory,
    PluginMutationAction, PluginMutationOutcome, PluginMutationReinventory,
    PluginMutationReinventoryScope, PluginMutationTarget, PluginMutationUnconfirmedReason,
    PluginUninstallOptions, PluginUpdateOptions,
};
use tauri::State;

use super::parse_agent;
use crate::state::AppState;

/// Shared desktop boundary for inventory reads. Keep reads available even when
/// the AgentHub database failed to open; only writes need its live-write lock.
pub(crate) async fn list_plugin_inventory_inner(
    _state: &AppState,
) -> Result<PluginInventory, String> {
    tauri::async_runtime::spawn_blocking(list_plugin_inventory_impl)
        .await
        .map_err(|e| format!("list_plugin_inventory join error: {e}"))
}

/// Invoke: `list_plugin_inventory` — Claude/Codex/Grok/Pi plugin packs (not MCP).
#[tauri::command]
pub async fn list_plugin_inventory(state: State<'_, AppState>) -> Result<PluginInventory, String> {
    list_plugin_inventory_inner(&state).await
}

pub(crate) async fn enable_plugin_inner(
    state: &AppState,
    agent: String,
    name: String,
    marketplace: Option<String>,
) -> Result<PluginMutationOutcome, String> {
    let hub = state.hub_arc()?;
    tauri::async_runtime::spawn_blocking(move || {
        let agent = parse_agent(&agent)?;
        let _live_write = acquire_plugin_write(&hub, agent)?;
        enable_plugin_impl(agent, &name, marketplace.as_deref())?;
        let inventory = list_plugin_inventory_impl();
        Ok(target_mutation_outcome(
            PluginMutationAction::Enable,
            PluginMutationTarget::new(agent, name, marketplace.as_deref(), None),
            Some(true),
            true,
            inventory,
        ))
    })
    .await
    .map_err(|e| format!("enable_plugin join error: {e}"))?
}

/// Invoke: `enable_plugin` — supported CLI/config enablement for the selected Agent.
#[tauri::command]
pub async fn enable_plugin(
    state: State<'_, AppState>,
    agent: String,
    name: String,
    marketplace: Option<String>,
) -> Result<PluginMutationOutcome, String> {
    enable_plugin_inner(&state, agent, name, marketplace).await
}

pub(crate) async fn disable_plugin_inner(
    state: &AppState,
    agent: String,
    name: String,
    marketplace: Option<String>,
) -> Result<PluginMutationOutcome, String> {
    let hub = state.hub_arc()?;
    tauri::async_runtime::spawn_blocking(move || {
        let agent = parse_agent(&agent)?;
        let _live_write = acquire_plugin_write(&hub, agent)?;
        disable_plugin_impl(agent, &name, marketplace.as_deref())?;
        let inventory = list_plugin_inventory_impl();
        Ok(target_mutation_outcome(
            PluginMutationAction::Disable,
            PluginMutationTarget::new(agent, name, marketplace.as_deref(), None),
            Some(false),
            true,
            inventory,
        ))
    })
    .await
    .map_err(|e| format!("disable_plugin join error: {e}"))?
}

/// Invoke: `disable_plugin` — supported CLI/config disablement for the selected Agent.
#[tauri::command]
pub async fn disable_plugin(
    state: State<'_, AppState>,
    agent: String,
    name: String,
    marketplace: Option<String>,
) -> Result<PluginMutationOutcome, String> {
    disable_plugin_inner(&state, agent, name, marketplace).await
}

pub(crate) async fn list_available_plugins_inner(
    _state: &AppState,
    agent: String,
) -> Result<Vec<PluginEntry>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let agent = parse_agent(&agent)?;
        list_available_plugins_impl(agent)
    })
    .await
    .map_err(|e| format!("list_available_plugins join error: {e}"))?
}

/// Invoke: `list_available_plugins` — marketplace rows, not installed inventory.
#[tauri::command]
pub async fn list_available_plugins(
    state: State<'_, AppState>,
    agent: String,
) -> Result<Vec<PluginEntry>, String> {
    list_available_plugins_inner(&state, agent).await
}

pub(crate) async fn preview_plugin_install_inner(
    _state: &AppState,
    agent: String,
    source: String,
) -> Result<PluginEntry, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let agent = parse_agent(&agent)?;
        preview_plugin_install_impl(agent, &source)
    })
    .await
    .map_err(|e| format!("preview_plugin_install join error: {e}"))?
}

/// Invoke: `preview_plugin_install` — component list before confirm.
#[tauri::command]
pub async fn preview_plugin_install(
    state: State<'_, AppState>,
    agent: String,
    source: String,
) -> Result<PluginEntry, String> {
    preview_plugin_install_inner(&state, agent, source).await
}

pub(crate) async fn install_plugin_inner(
    state: &AppState,
    agent: String,
    source: String,
    confirmed: bool,
) -> Result<PluginMutationOutcome, String> {
    let hub = state.hub_arc()?;
    tauri::async_runtime::spawn_blocking(move || {
        let agent = parse_agent(&agent)?;
        let _live_write = acquire_plugin_write(&hub, agent)?;
        // The target must be captured before invoking the vendor command; the
        // following scan is then matched to this exact vendor identity while
        // the same live-write lock remains held.
        let target =
            PluginMutationTarget::from_entry(&preview_plugin_install_impl(agent, &source)?);
        install_plugin_impl(agent, &source, PluginInstallOptions { confirmed })?;
        let inventory = list_plugin_inventory_impl();
        Ok(target_mutation_outcome(
            PluginMutationAction::Install,
            target,
            None,
            true,
            inventory,
        ))
    })
    .await
    .map_err(|e| format!("install_plugin join error: {e}"))?
}

/// Invoke: `install_plugin` — official CLI after UI confirm (`--trust` / `-y`).
#[tauri::command]
pub async fn install_plugin(
    state: State<'_, AppState>,
    agent: String,
    source: String,
    confirmed: bool,
) -> Result<PluginMutationOutcome, String> {
    install_plugin_inner(&state, agent, source, confirmed).await
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn uninstall_plugin_inner(
    state: &AppState,
    agent: String,
    name: String,
    marketplace: Option<String>,
    install_source: Option<String>,
    keep_data: Option<bool>,
) -> Result<PluginMutationOutcome, String> {
    let hub = state.hub_arc()?;
    tauri::async_runtime::spawn_blocking(move || {
        let agent = parse_agent(&agent)?;
        let _live_write = acquire_plugin_write(&hub, agent)?;
        let target = PluginMutationTarget::new(
            agent,
            name.as_str(),
            marketplace.as_deref(),
            install_source.as_deref(),
        );
        uninstall_plugin_impl(
            agent,
            &name,
            marketplace.as_deref(),
            install_source.as_deref(),
            PluginUninstallOptions {
                keep_data: keep_data.unwrap_or(true),
            },
        )?;
        let inventory = list_plugin_inventory_impl();
        Ok(target_mutation_outcome(
            PluginMutationAction::Uninstall,
            target,
            None,
            false,
            inventory,
        ))
    })
    .await
    .map_err(|e| format!("uninstall_plugin join error: {e}"))?
}

/// Invoke: `uninstall_plugin` — official CLI. Default keeps plugin data.
#[tauri::command]
pub async fn uninstall_plugin(
    state: State<'_, AppState>,
    agent: String,
    name: String,
    marketplace: Option<String>,
    install_source: Option<String>,
    keep_data: Option<bool>,
) -> Result<PluginMutationOutcome, String> {
    uninstall_plugin_inner(&state, agent, name, marketplace, install_source, keep_data).await
}

pub(crate) async fn refresh_plugin_marketplace_inner(
    state: &AppState,
    agent: String,
) -> Result<PluginMutationOutcome, String> {
    let hub = state.hub_arc()?;
    tauri::async_runtime::spawn_blocking(move || {
        let agent = parse_agent(&agent)?;
        let _live_write = acquire_plugin_write(&hub, agent)?;
        let before_catalog = list_available_plugins_impl(agent);
        refresh_plugin_marketplace_impl(agent)?;
        let after_catalog = list_available_plugins_impl(agent);
        let inventory = list_plugin_inventory_impl();
        Ok(marketplace_mutation_outcome(
            agent,
            before_catalog,
            after_catalog,
            inventory,
        ))
    })
    .await
    .map_err(|e| format!("refresh_plugin_marketplace join error: {e}"))?
}

/// Invoke: `refresh_plugin_marketplace` — refresh Claude/Codex/Grok catalogs.
#[tauri::command]
pub async fn refresh_plugin_marketplace(
    state: State<'_, AppState>,
    agent: String,
) -> Result<PluginMutationOutcome, String> {
    refresh_plugin_marketplace_inner(&state, agent).await
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn update_plugin_inner(
    state: &AppState,
    agent: String,
    name: String,
    marketplace: Option<String>,
    scope: Option<String>,
    confirmed: bool,
) -> Result<PluginMutationOutcome, String> {
    let hub = state.hub_arc()?;
    tauri::async_runtime::spawn_blocking(move || {
        let agent = parse_agent(&agent)?;
        let _live_write = acquire_plugin_write(&hub, agent)?;
        let target = PluginMutationTarget::new(agent, name.as_str(), marketplace.as_deref(), None);
        let before = list_plugin_inventory_impl();
        update_plugin_impl(
            agent,
            &name,
            marketplace.as_deref(),
            scope.as_deref(),
            PluginUpdateOptions { confirmed },
        )?;
        let inventory = list_plugin_inventory_impl();
        Ok(update_mutation_outcome(target, before, inventory))
    })
    .await
    .map_err(|e| format!("update_plugin join error: {e}"))?
}

/// Invoke: `update_plugin` — update one installed Claude/Grok pack after confirm.
#[tauri::command]
pub async fn update_plugin(
    state: State<'_, AppState>,
    agent: String,
    name: String,
    marketplace: Option<String>,
    scope: Option<String>,
    confirmed: bool,
) -> Result<PluginMutationOutcome, String> {
    update_plugin_inner(&state, agent, name, marketplace, scope, confirmed).await
}

pub(crate) async fn update_pi_plugins_inner(
    state: &AppState,
    confirmed: bool,
) -> Result<PluginMutationOutcome, String> {
    let hub = state.hub_arc()?;
    tauri::async_runtime::spawn_blocking(move || {
        let _live_write = acquire_plugin_write(&hub, AgentId::Pi)?;
        let before = list_plugin_inventory_impl();
        update_pi_plugins_impl(PluginUpdateOptions { confirmed })?;
        let inventory = list_plugin_inventory_impl();
        Ok(pi_update_mutation_outcome(before, inventory))
    })
    .await
    .map_err(|e| format!("update_pi_plugins join error: {e}"))?
}

/// Invoke: `update_pi_plugins` — update eligible Pi extensions after confirm.
#[tauri::command]
pub async fn update_pi_plugins(
    state: State<'_, AppState>,
    confirmed: bool,
) -> Result<PluginMutationOutcome, String> {
    update_pi_plugins_inner(&state, confirmed).await
}

fn target_reinventory(
    inventory: &PluginInventory,
    target: &PluginMutationTarget,
) -> PluginMutationReinventory {
    // A name-only Grok operation can match more than one local row. Return
    // every already-safe inventory id so an unconfirmed result never implies
    // that the first row was the one the vendor command affected.
    let scanned_plugin_ids = plugin_inventory_target_candidates(inventory, target)
        .into_iter()
        .map(|row| row.id.clone())
        .collect();
    PluginMutationReinventory {
        scope: PluginMutationReinventoryScope::Target,
        scanned_plugin_ids,
        marketplace_entries: None,
    }
}

fn target_mutation_outcome(
    action: PluginMutationAction,
    target: PluginMutationTarget,
    expected_enabled: Option<bool>,
    expected_present: bool,
    inventory: PluginInventory,
) -> PluginMutationOutcome {
    let agent = target.agent;
    let reinventory = target_reinventory(&inventory, &target);
    if !plugin_inventory_agent_verified(&inventory, agent) {
        return PluginMutationOutcome::unconfirmed(
            action,
            agent,
            Some(target),
            PluginMutationUnconfirmedReason::InventoryUnavailable,
            inventory,
            reinventory,
        );
    }
    if plugin_inventory_target_is_ambiguous(&inventory, &target) {
        return PluginMutationOutcome::unconfirmed(
            action,
            agent,
            Some(target),
            PluginMutationUnconfirmedReason::AmbiguousTarget,
            inventory,
            reinventory,
        );
    }
    let found = plugin_inventory_target(&inventory, &target);
    if !expected_present {
        return if found.is_none() {
            PluginMutationOutcome::confirmed(action, agent, Some(target), inventory, reinventory)
        } else {
            PluginMutationOutcome::unconfirmed(
                action,
                agent,
                Some(target),
                PluginMutationUnconfirmedReason::TargetStillListed,
                inventory,
                reinventory,
            )
        };
    }
    let Some(found) = found else {
        return PluginMutationOutcome::unconfirmed(
            action,
            agent,
            Some(target),
            PluginMutationUnconfirmedReason::TargetNotListed,
            inventory,
            reinventory,
        );
    };
    if expected_enabled.is_some_and(|expected| found.enabled != Some(expected)) {
        return PluginMutationOutcome::unconfirmed(
            action,
            agent,
            Some(target),
            PluginMutationUnconfirmedReason::EnabledStateMismatch,
            inventory,
            reinventory,
        );
    }
    PluginMutationOutcome::confirmed(action, agent, Some(target), inventory, reinventory)
}

fn update_mutation_outcome(
    target: PluginMutationTarget,
    before: PluginInventory,
    inventory: PluginInventory,
) -> PluginMutationOutcome {
    let agent = target.agent;
    let mut reinventory = target_reinventory(&inventory, &target);
    let mut before_candidate_ids = plugin_inventory_target_candidates(&before, &target)
        .into_iter()
        .map(|row| row.id.clone())
        .collect::<Vec<_>>();
    reinventory
        .scanned_plugin_ids
        .append(&mut before_candidate_ids);
    reinventory.scanned_plugin_ids.sort();
    reinventory.scanned_plugin_ids.dedup();
    if !plugin_inventory_agent_verified(&before, agent)
        || !plugin_inventory_agent_verified(&inventory, agent)
    {
        return PluginMutationOutcome::unconfirmed(
            PluginMutationAction::Update,
            agent,
            Some(target),
            PluginMutationUnconfirmedReason::InventoryUnavailable,
            inventory,
            reinventory,
        );
    }
    if plugin_inventory_target_is_ambiguous(&before, &target)
        || plugin_inventory_target_is_ambiguous(&inventory, &target)
    {
        return PluginMutationOutcome::unconfirmed(
            PluginMutationAction::Update,
            agent,
            Some(target),
            PluginMutationUnconfirmedReason::AmbiguousTarget,
            inventory,
            reinventory,
        );
    }
    let Some(before_entry) = plugin_inventory_target(&before, &target) else {
        return PluginMutationOutcome::unconfirmed(
            PluginMutationAction::Update,
            agent,
            Some(target),
            PluginMutationUnconfirmedReason::TargetNotListed,
            inventory,
            reinventory,
        );
    };
    let Some(after_entry) = plugin_inventory_target(&inventory, &target) else {
        return PluginMutationOutcome::unconfirmed(
            PluginMutationAction::Update,
            agent,
            Some(target),
            PluginMutationUnconfirmedReason::TargetNotListed,
            inventory,
            reinventory,
        );
    };
    if before_entry.version.is_some()
        && after_entry.version.is_some()
        && before_entry.version != after_entry.version
    {
        PluginMutationOutcome::confirmed(
            PluginMutationAction::Update,
            agent,
            Some(target),
            inventory,
            reinventory,
        )
    } else {
        PluginMutationOutcome::unconfirmed(
            PluginMutationAction::Update,
            agent,
            Some(target),
            PluginMutationUnconfirmedReason::VersionUnchangedOrUnknown,
            inventory,
            reinventory,
        )
    }
}

fn marketplace_mutation_outcome(
    agent: AgentId,
    before_catalog: Result<Vec<PluginEntry>, String>,
    after_catalog: Result<Vec<PluginEntry>, String>,
    inventory: PluginInventory,
) -> PluginMutationOutcome {
    let catalog = after_catalog.ok();
    let reinventory = PluginMutationReinventory {
        scope: PluginMutationReinventoryScope::Marketplace,
        scanned_plugin_ids: catalog
            .as_ref()
            .map(|rows| rows.iter().map(|row| row.id.clone()).collect())
            .unwrap_or_default(),
        marketplace_entries: catalog.clone(),
    };
    if !plugin_inventory_agent_verified(&inventory, agent) {
        return PluginMutationOutcome::unconfirmed(
            PluginMutationAction::MarketplaceRefresh,
            agent,
            None,
            PluginMutationUnconfirmedReason::InventoryUnavailable,
            inventory,
            reinventory,
        );
    }
    let (Ok(mut before), Some(mut after)) = (before_catalog, catalog) else {
        return PluginMutationOutcome::unconfirmed(
            PluginMutationAction::MarketplaceRefresh,
            agent,
            None,
            PluginMutationUnconfirmedReason::MarketplaceSnapshotUnavailable,
            inventory,
            reinventory,
        );
    };
    before.sort_by(|left, right| left.id.cmp(&right.id));
    after.sort_by(|left, right| left.id.cmp(&right.id));
    if before == after {
        PluginMutationOutcome::unconfirmed(
            PluginMutationAction::MarketplaceRefresh,
            agent,
            None,
            PluginMutationUnconfirmedReason::MarketplaceEntriesUnchanged,
            inventory,
            reinventory,
        )
    } else {
        PluginMutationOutcome::confirmed(
            PluginMutationAction::MarketplaceRefresh,
            agent,
            None,
            inventory,
            reinventory,
        )
    }
}

fn pi_update_mutation_outcome(
    before: PluginInventory,
    inventory: PluginInventory,
) -> PluginMutationOutcome {
    let agent = AgentId::Pi;
    let reinventory = PluginMutationReinventory {
        scope: PluginMutationReinventoryScope::Agent,
        scanned_plugin_ids: plugin_inventory_agent_ids(&inventory, agent),
        marketplace_entries: None,
    };
    if !plugin_inventory_agent_verified(&before, agent)
        || !plugin_inventory_agent_verified(&inventory, agent)
    {
        return PluginMutationOutcome::unconfirmed(
            PluginMutationAction::PiUpdate,
            agent,
            None,
            PluginMutationUnconfirmedReason::InventoryUnavailable,
            inventory,
            reinventory,
        );
    }
    if plugin_inventory_versions_or_entries_changed(&before, &inventory, agent) {
        PluginMutationOutcome::confirmed(
            PluginMutationAction::PiUpdate,
            agent,
            None,
            inventory,
            reinventory,
        )
    } else {
        PluginMutationOutcome::unconfirmed(
            PluginMutationAction::PiUpdate,
            agent,
            None,
            PluginMutationUnconfirmedReason::PiScopeUnchanged,
            inventory,
            reinventory,
        )
    }
}

fn acquire_plugin_write(
    hub: &agenthub_core::AgentHub,
    agent: AgentId,
) -> Result<agenthub_core::services::LiveWriteGuard, String> {
    hub.backups()
        .acquire_live_write(agent)
        .map_err(|error| error.to_string())
}
