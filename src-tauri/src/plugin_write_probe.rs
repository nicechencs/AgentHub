//! Feature-gated executable probe for desktop plugin package commands.
//!
//! The shell wrapper supplies disposable Agent homes plus four real fixture
//! executables on `PATH`. This module deliberately calls the same command
//! inners as Tauri, including parsing, AppState initialization and live-write
//! locking.

use std::fs;
use std::path::{Path, PathBuf};

use agenthub_core::models::AgentId;
use agenthub_core::services::{PluginEntry, PluginInventory};
use serde_json::{json, Value};

use crate::commands::plugins::{
    disable_plugin_inner, enable_plugin_inner, install_plugin_inner, list_available_plugins_inner,
    list_plugin_inventory_inner, preview_plugin_install_inner, refresh_plugin_marketplace_inner,
    uninstall_plugin_inner, update_pi_plugins_inner, update_plugin_inner,
};
use crate::state::AppState;

type ProbeResult<T> = Result<T, String>;

const FAILURE_ENV: &str = "AGENTHUB_PLUGIN_FIXTURE_FAIL_ONCE";

pub fn main_entry() -> ProbeResult<()> {
    let mut args = std::env::args_os().skip(1);
    let root = args
        .next()
        .map(PathBuf::from)
        .ok_or_else(|| "usage: plugin_write_e2e_probe <scratch> <fixture-bin-dir>".to_string())?;
    let fixture_bin = args
        .next()
        .map(PathBuf::from)
        .ok_or_else(|| "fixture binary directory is required".to_string())?;
    if args.next().is_some() {
        return Err("unexpected extra arguments".into());
    }
    let (root, fixture_bin) = validate_isolation(&root, &fixture_bin)?;
    set_fixture_only_path(&fixture_bin);

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("create probe runtime: {error}"))?;
    let evidence = runtime.block_on(run(&root))?;
    let encoded = serde_json::to_vec_pretty(&evidence)
        .map_err(|error| format!("encode plugin probe evidence: {error}"))?;
    fs::write(root.join("plugin-write-evidence.json"), &encoded)
        .map_err(|error| format!("write plugin probe evidence: {error}"))?;
    println!(
        "{}",
        serde_json::to_string(&evidence)
            .map_err(|error| format!("encode plugin probe output: {error}"))?
    );
    Ok(())
}

async fn run(root: &Path) -> ProbeResult<Value> {
    let state = AppState::new();
    let hub = state.hub_arc()?;
    ensure(
        path_within(hub.data_dir(), root),
        "AppState opened outside the disposable scratch directory",
    )?;

    let initial = list_plugin_inventory_inner(&state).await?;
    ensure_four_supported(&initial)?;

    let claude = available_preview(&state, AgentId::Claude).await?;
    let grok = available_preview(&state, AgentId::Grok).await?;
    let codex = available_preview(&state, AgentId::Codex).await?;
    let pi_source = probe_pi_source(root)?;
    let pi = preview_plugin_install_inner(&state, "pi".into(), pi_source.clone()).await?;
    ensure(
        pi.agent == AgentId::Pi,
        "Pi preview returned the wrong Agent",
    )?;

    // The desktop boundary must take the same cross-process lock as provider
    // writes before it even reaches confirmation handling.
    let held = hub
        .backups()
        .acquire_live_write(AgentId::Claude)
        .map_err(|error| format!("hold Claude live-write lock: {error}"))?;
    let locked = install_plugin_inner(&state, "claude".into(), entry_source(&claude)?, false)
        .await
        .expect_err("held live-write lock unexpectedly allowed plugin write");
    drop(held);
    ensure(
        locked.contains("another live write is already running"),
        "held live-write lock did not preserve the desktop lock error",
    )?;

    let confirmation_errors = vec![
        rejected_install(&state, AgentId::Claude, entry_source(&claude)?).await?,
        rejected_install(&state, AgentId::Grok, entry_source(&grok)?).await?,
        rejected_install(&state, AgentId::Codex, entry_source(&codex)?).await?,
        rejected_install(&state, AgentId::Pi, pi_source.clone()).await?,
    ];

    install_plugin_inner(&state, "claude".into(), entry_source(&claude)?, true).await?;
    install_plugin_inner(&state, "grok".into(), entry_source(&grok)?, true).await?;
    install_plugin_inner(&state, "codex".into(), entry_source(&codex)?, true).await?;
    install_plugin_inner(&state, "pi".into(), pi_source.clone(), true).await?;

    let installed = list_plugin_inventory_inner(&state).await?;
    ensure_installed(&installed, &[&claude, &grok, &codex, &pi])?;

    toggle_round_trip(&state, &claude).await?;
    toggle_round_trip(&state, &grok).await?;
    toggle_round_trip(&state, &codex).await?;
    let pi_toggle_error = disable_plugin_inner(&state, "pi".into(), pi.name.clone(), None)
        .await
        .expect_err("Pi unexpectedly exposed plugin enable/disable");
    ensure(
        pi_toggle_error.contains("do not expose"),
        "Pi toggle returned an unexpected error",
    )?;

    rejected_updates(&state, &claude, &grok).await?;
    update_plugin_entry(&state, &claude, true).await?;
    update_plugin_entry(&state, &grok, true).await?;
    let codex_update_error = update_plugin_inner(
        &state,
        "codex".into(),
        codex.name.clone(),
        codex.marketplace.clone(),
        codex.scope.clone(),
        true,
    )
    .await
    .expect_err("Codex unexpectedly allowed a single-plugin update");
    ensure(
        codex_update_error.contains("marketplace-wide"),
        "Codex single-plugin update returned an unexpected error",
    )?;
    let pi_confirm_error = update_pi_plugins_inner(&state, false)
        .await
        .expect_err("Pi update ran without confirmation");
    ensure(
        pi_confirm_error.contains("needs confirmation"),
        "Pi update confirmation error changed",
    )?;
    update_pi_plugins_inner(&state, true).await?;
    let pi_update_result = fs::read_to_string(root.join("fixture-state/pi-update-result.json"))
        .map_err(|error| format!("read Pi update result: {error}"))?;
    let pi_update_result: Value = serde_json::from_str(&pi_update_result)
        .map_err(|error| format!("parse Pi update result: {error}"))?;
    ensure(
        pi_update_result
            .get("eligible")
            .and_then(Value::as_array)
            .is_some_and(|eligible| {
                eligible
                    .iter()
                    .any(|value| value.as_str() == Some(&pi_source))
            }),
        "Pi update did not receive the installed eligible extension",
    )?;
    for agent in [AgentId::Claude, AgentId::Grok, AgentId::Codex] {
        refresh_plugin_marketplace_inner(&state, agent.as_str().into()).await?;
    }

    let rollback = vec![
        failed_update_restores(&state, AgentId::Claude, &claude, "claude:update").await?,
        failed_update_restores(&state, AgentId::Grok, &grok, "grok:update").await?,
        failed_refresh_restores(&state, AgentId::Codex, "codex:marketplace-upgrade").await?,
        failed_pi_update_restores(&state, "pi:update").await?,
    ];

    // Re-scan after the injected failures, both to prove config restoration and
    // to exercise the desktop inventory path after command errors.
    let after_failures = list_plugin_inventory_inner(&state).await?;
    ensure_installed(&after_failures, &[&claude, &grok, &codex, &pi])?;

    uninstall_entry(&state, &claude).await?;
    uninstall_entry(&state, &grok).await?;
    uninstall_entry(&state, &codex).await?;
    uninstall_plugin_inner(
        &state,
        "pi".into(),
        pi.name.clone(),
        None,
        Some(pi_source),
        Some(true),
    )
    .await?;

    let final_inventory = list_plugin_inventory_inner(&state).await?;
    ensure_uninstalled(&final_inventory, &[&claude, &grok, &codex, &pi])?;
    ensure(
        !hub.data_dir()
            .join("locks")
            .read_dir()
            .map_or(false, |mut it| it.next().is_some()),
        "desktop command left a live-write lock behind",
    )?;

    Ok(json!({
        "schemaVersion": 1,
        "status": "ok",
        "ok": true,
        "agents": ["claude", "codex", "grok", "pi"],
        "sharedDesktopBoundary": {
            "appState": true,
            "agentParsing": true,
            "liveWriteLock": true,
            "commandErrors": true
        },
        "coverage": {
            "preview": 4,
            "confirmationRejected": confirmation_errors.len(),
            "install": 4,
            "inventoryScans": 10,
            "toggleRoundTrips": 3,
            "unsupportedPiToggle": pi_toggle_error,
            "individualUpdates": 2,
            "marketplaceRefreshes": 3,
            "piBulkUpdate": true,
            "failureRollbacks": rollback,
            "uninstall": 4
        },
        "inventoryCounts": {
            "initial": initial.plugins.len(),
            "installed": installed.plugins.len(),
            "afterFailures": after_failures.plugins.len(),
            "final": final_inventory.plugins.len()
        }
    }))
}

async fn available_preview(state: &AppState, agent: AgentId) -> ProbeResult<PluginEntry> {
    let rows = list_available_plugins_inner(state, agent.as_str().into()).await?;
    let row = rows
        .into_iter()
        .next()
        .ok_or_else(|| format!("{} fixture returned no available plugin", agent.as_str()))?;
    let source = entry_source(&row)?;
    let preview =
        preview_plugin_install_inner(state, agent.as_str().into(), source.clone()).await?;
    ensure(
        preview.agent == agent,
        &format!("{} preview returned the wrong Agent", agent.as_str()),
    )?;
    ensure(
        entry_source(&preview)? == source,
        &format!("{} preview changed the install source", agent.as_str()),
    )?;
    Ok(preview)
}

async fn rejected_install(state: &AppState, agent: AgentId, source: String) -> ProbeResult<String> {
    let error = install_plugin_inner(state, agent.as_str().into(), source, false)
        .await
        .expect_err("plugin install ran without confirmation");
    ensure(
        error.contains("needs confirmation"),
        &format!("{} install confirmation error changed", agent.as_str()),
    )?;
    Ok(error)
}

async fn toggle_round_trip(state: &AppState, entry: &PluginEntry) -> ProbeResult<()> {
    disable_plugin_inner(
        state,
        entry.agent.as_str().into(),
        entry.name.clone(),
        entry.marketplace.clone(),
    )
    .await?;
    ensure_enabled_state(state, entry, false).await?;
    enable_plugin_inner(
        state,
        entry.agent.as_str().into(),
        entry.name.clone(),
        entry.marketplace.clone(),
    )
    .await?;
    ensure_enabled_state(state, entry, true).await
}

async fn ensure_enabled_state(
    state: &AppState,
    entry: &PluginEntry,
    expected: bool,
) -> ProbeResult<()> {
    let inventory = list_plugin_inventory_inner(state).await?;
    let installed = inventory
        .plugins
        .iter()
        .find(|row| row.agent == entry.agent && row.name == entry.name)
        .ok_or_else(|| {
            format!(
                "{} plugin disappeared while verifying toggle",
                entry.agent.as_str()
            )
        })?;
    ensure(
        installed.enabled == Some(expected),
        &format!(
            "{} plugin enabled state did not become {expected}",
            entry.agent.as_str()
        ),
    )
}

async fn rejected_updates(
    state: &AppState,
    claude: &PluginEntry,
    grok: &PluginEntry,
) -> ProbeResult<()> {
    for entry in [claude, grok] {
        let error = update_plugin_entry(state, entry, false)
            .await
            .expect_err("plugin update ran without confirmation");
        ensure(
            error.contains("needs confirmation"),
            &format!("{} update confirmation error changed", entry.agent.as_str()),
        )?;
    }
    Ok(())
}

async fn update_plugin_entry(
    state: &AppState,
    entry: &PluginEntry,
    confirmed: bool,
) -> ProbeResult<()> {
    update_plugin_inner(
        state,
        entry.agent.as_str().into(),
        entry.name.clone(),
        entry.marketplace.clone(),
        Some("user".into()),
        confirmed,
    )
    .await
}

async fn failed_update_restores(
    state: &AppState,
    agent: AgentId,
    entry: &PluginEntry,
    fail_key: &str,
) -> ProbeResult<Value> {
    let live = live_config(agent)?;
    let before = fs::read(&live)
        .map_err(|error| format!("read {} config before failure: {error}", agent.as_str()))?;
    std::env::set_var(FAILURE_ENV, fail_key);
    let result = update_plugin_entry(state, entry, true).await;
    std::env::remove_var(FAILURE_ENV);
    ensure(
        result.is_err(),
        "fixture failure injection unexpectedly succeeded",
    )?;
    let after = fs::read(&live)
        .map_err(|error| format!("read {} config after failure: {error}", agent.as_str()))?;
    ensure(
        before == after,
        "failed plugin update did not restore live config",
    )?;
    Ok(json!({"agent": agent, "operation": "update", "restored": true}))
}

async fn failed_refresh_restores(
    state: &AppState,
    agent: AgentId,
    fail_key: &str,
) -> ProbeResult<Value> {
    let live = live_config(agent)?;
    let before = fs::read(&live)
        .map_err(|error| format!("read {} config before failure: {error}", agent.as_str()))?;
    std::env::set_var(FAILURE_ENV, fail_key);
    let result = refresh_plugin_marketplace_inner(state, agent.as_str().into()).await;
    std::env::remove_var(FAILURE_ENV);
    ensure(
        result.is_err(),
        "fixture failure injection unexpectedly succeeded",
    )?;
    let after = fs::read(&live)
        .map_err(|error| format!("read {} config after failure: {error}", agent.as_str()))?;
    ensure(
        before == after,
        "failed marketplace refresh did not restore live config",
    )?;
    Ok(json!({"agent": agent, "operation": "marketplace-refresh", "restored": true}))
}

async fn failed_pi_update_restores(state: &AppState, fail_key: &str) -> ProbeResult<Value> {
    let live = live_config(AgentId::Pi)?;
    let before =
        fs::read(&live).map_err(|error| format!("read Pi config before failure: {error}"))?;
    std::env::set_var(FAILURE_ENV, fail_key);
    let result = update_pi_plugins_inner(state, true).await;
    std::env::remove_var(FAILURE_ENV);
    ensure(
        result.is_err(),
        "fixture Pi failure injection unexpectedly succeeded",
    )?;
    let after =
        fs::read(&live).map_err(|error| format!("read Pi config after failure: {error}"))?;
    ensure(
        before == after,
        "failed Pi update did not restore live config",
    )?;
    Ok(json!({"agent": "pi", "operation": "bulk-update", "restored": true}))
}

async fn uninstall_entry(state: &AppState, entry: &PluginEntry) -> ProbeResult<()> {
    uninstall_plugin_inner(
        state,
        entry.agent.as_str().into(),
        entry.name.clone(),
        entry.marketplace.clone(),
        entry.install_source.clone(),
        Some(true),
    )
    .await
}

fn entry_source(entry: &PluginEntry) -> ProbeResult<String> {
    entry
        .install_source
        .clone()
        .or_else(|| {
            (entry.agent != AgentId::Grok).then(|| {
                entry
                    .marketplace
                    .as_ref()
                    .map(|market| format!("{}@{market}", entry.name))
            })?
        })
        .or_else(|| (!entry.name.trim().is_empty()).then(|| entry.name.clone()))
        .ok_or_else(|| {
            format!(
                "{} preview has no stable install source",
                entry.agent.as_str()
            )
        })
}

fn probe_pi_source(root: &Path) -> ProbeResult<String> {
    let source = std::env::var_os("AGENTHUB_PLUGIN_PROBE_PI_SOURCE")
        .map(PathBuf::from)
        .ok_or_else(|| "AGENTHUB_PLUGIN_PROBE_PI_SOURCE is required".to_string())?;
    ensure(
        source.is_absolute() && source.is_dir() && path_within(&source, root),
        "Pi probe source must be an existing directory inside scratch",
    )?;
    Ok(source.to_string_lossy().into_owned())
}

fn ensure_four_supported(inventory: &PluginInventory) -> ProbeResult<()> {
    for agent in [AgentId::Claude, AgentId::Codex, AgentId::Grok, AgentId::Pi] {
        let status = inventory
            .agents
            .iter()
            .find(|status| status.agent == agent)
            .ok_or_else(|| format!("{} inventory status is missing", agent.as_str()))?;
        ensure(
            status.support == "listed" && status.error_code.is_none(),
            &format!("{} inventory is not healthy", agent.as_str()),
        )?;
    }
    Ok(())
}

fn ensure_installed(inventory: &PluginInventory, entries: &[&PluginEntry]) -> ProbeResult<()> {
    ensure_four_supported(inventory)?;
    for expected in entries {
        ensure(
            inventory.plugins.iter().any(|row| {
                row.agent == expected.agent
                    && (row.name == expected.name || row.install_source == expected.install_source)
            }),
            &format!(
                "{} installed plugin was not found by re-inventory",
                expected.agent.as_str()
            ),
        )?;
    }
    Ok(())
}

fn ensure_uninstalled(inventory: &PluginInventory, entries: &[&PluginEntry]) -> ProbeResult<()> {
    ensure_four_supported(inventory)?;
    for removed in entries {
        ensure(
            !inventory.plugins.iter().any(|row| {
                row.agent == removed.agent
                    && (row.name == removed.name || row.install_source == removed.install_source)
            }),
            &format!(
                "{} removed plugin remains in re-inventory",
                removed.agent.as_str()
            ),
        )?;
    }
    Ok(())
}

fn live_config(agent: AgentId) -> ProbeResult<PathBuf> {
    let dir_var = match agent {
        AgentId::Claude => "CLAUDE_CONFIG_DIR",
        AgentId::Codex => "CODEX_HOME",
        AgentId::Grok => "GROK_HOME",
        AgentId::Pi => "PI_CODING_AGENT_DIR",
        _ => return Err("unsupported plugin probe Agent".into()),
    };
    let filename = match agent {
        AgentId::Claude | AgentId::Pi => "settings.json",
        AgentId::Codex | AgentId::Grok => "config.toml",
        _ => unreachable!(),
    };
    std::env::var_os(dir_var)
        .map(PathBuf::from)
        .map(|dir| dir.join(filename))
        .ok_or_else(|| format!("{dir_var} is required"))
}

fn validate_isolation(root: &Path, fixture_bin: &Path) -> ProbeResult<(PathBuf, PathBuf)> {
    let root = root
        .canonicalize()
        .map_err(|error| format!("canonicalize scratch: {error}"))?;
    ensure(root.is_absolute(), "scratch path must be absolute")?;
    let fixture_bin = fixture_bin
        .canonicalize()
        .map_err(|error| format!("canonicalize fixture bin directory: {error}"))?;
    ensure(
        fixture_bin.is_dir() && path_within(&fixture_bin, &root),
        "fixture binary directory must be inside scratch",
    )?;
    let current_dir = std::env::current_dir()
        .map_err(|error| format!("read probe working directory: {error}"))?;
    ensure(
        path_within(&current_dir, &root),
        "probe working directory must be inside scratch",
    )?;
    for name in ["claude", "codex", "grok", "pi"] {
        ensure(
            fixture_bin.join(name).is_file(),
            &format!("fixture command is missing: {name}"),
        )?;
    }
    for var in [
        "HOME",
        "AGENTHUB_HOME",
        "CLAUDE_CONFIG_DIR",
        "CODEX_HOME",
        "GROK_HOME",
        "PI_CODING_AGENT_DIR",
        "PI_CODING_AGENT_SESSION_DIR",
        "XDG_CONFIG_HOME",
        "XDG_DATA_HOME",
        "XDG_CACHE_HOME",
        "TMPDIR",
        "AGENTHUB_PLUGIN_FIXTURE_ROOT",
        "AGENTHUB_PLUGIN_FIXTURE_DIR",
        "AGENTHUB_PLUGIN_FIXTURE_LOG",
    ] {
        let path = std::env::var_os(var)
            .map(PathBuf::from)
            .ok_or_else(|| format!("{var} is required"))?;
        canonical_contained_path(&path, &root, var)?;
    }
    Ok((root, fixture_bin))
}

fn set_fixture_only_path(fixture_bin: &Path) {
    std::env::set_var("PATH", fixture_bin.as_os_str());
}

fn path_within(path: &Path, root: &Path) -> bool {
    let Ok(path) = path.canonicalize() else {
        return false;
    };
    let Ok(root) = root.canonicalize() else {
        return false;
    };
    path == root || path.starts_with(root)
}

fn canonical_contained_path(path: &Path, root: &Path, label: &str) -> ProbeResult<PathBuf> {
    use std::path::Component;

    ensure(path.is_absolute(), &format!("{label} must be absolute"))?;
    ensure(
        !path.components().any(|part| part == Component::ParentDir),
        &format!("{label} cannot contain parent traversal"),
    )?;
    let canonical = if path.exists() {
        path.canonicalize()
            .map_err(|error| format!("canonicalize {label}: {error}"))?
    } else {
        let parent = path
            .parent()
            .ok_or_else(|| format!("{label} has no parent directory"))?
            .canonicalize()
            .map_err(|error| format!("canonicalize {label} parent: {error}"))?;
        let name = path
            .file_name()
            .ok_or_else(|| format!("{label} has no final path component"))?;
        parent.join(name)
    };
    ensure(
        canonical == root || canonical.starts_with(root),
        &format!("{label} must stay inside scratch"),
    )?;
    Ok(canonical)
}

fn ensure(condition: bool, message: &str) -> ProbeResult<()> {
    condition.then_some(()).ok_or_else(|| message.to_string())
}
