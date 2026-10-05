//! Manage Claude, Codex, Grok, and Pi plugin packs through supported vendor
//! CLI/config interfaces.
//!
//! Snapshots the agent's settings/config file before the CLI runs. If the CLI
//! fails, the snapshot is restored so a half-written file is not left behind.
//! Vendor package/cache changes already made by the CLI are not rolled back.
//! AgentHub does not edit vendor plugin cache itself. Install without confirm
//! does not call the official command.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use serde::Serialize;
use serde_json::Value as JsonValue;
use toml_edit::{value, DocumentMut};

use crate::models::AgentId;
use crate::models::{RunSpec, RunStatus};
use crate::services::plugin_inventory::{
    parse_cli_available_plugin_list, preview_local_plugin, CliRun, PluginCliRunner, PluginEntry,
    PluginInventory, SystemPluginCliRunner,
};
use crate::utils::atomic::atomic_write;
use crate::utils::paths::{agent_config_dir, agent_home, home_dir};
use crate::utils::process::{ProcessRunner, SystemProcessRunner};

const AVAILABLE_TIMEOUT: Duration = Duration::from_secs(20);
const INSTALL_TIMEOUT: Duration = Duration::from_secs(120);
const UNINSTALL_TIMEOUT: Duration = Duration::from_secs(30);
const UPDATE_TIMEOUT: Duration = Duration::from_secs(180);
const UPDATE_OUTPUT_LIMIT: usize = 64 * 1024;
static CLAUDE_WRITE_LOCK: Mutex<()> = Mutex::new(());
static GROK_WRITE_LOCK: Mutex<()> = Mutex::new(());
static PI_WRITE_LOCK: Mutex<()> = Mutex::new(());
static CODEX_WRITE_LOCK: Mutex<()> = Mutex::new(());

/// Homes + binaries + CLI runner used by Claude/Grok operations (tests inject fakes).
pub struct PluginApplyContext<'a> {
    pub user_home: PathBuf,
    pub claude_home: PathBuf,
    pub grok_home: PathBuf,
    pub claude_bin: Option<PathBuf>,
    pub grok_bin: Option<PathBuf>,
    pub runner: &'a dyn PluginCliRunner,
}

/// Codex/Pi wiring layered over the legacy Claude/Grok injectable context.
/// Keeping this separate avoids breaking existing context literals.
pub struct PluginApplyContextV2<'a> {
    pub base: PluginApplyContext<'a>,
    pub codex_home: PathBuf,
    pub pi_config: PathBuf,
    pub codex_bin: Option<PathBuf>,
    pub pi_bin: Option<PathBuf>,
}

/// Confirm-before-CLI install. `confirmed` maps to Grok `--trust` / Claude `-y`.
#[derive(Debug, Clone, Copy)]
pub struct PluginInstallOptions {
    pub confirmed: bool,
}

/// Uninstall options. Default `keep_data` is true (do not delete `plugins/data`).
#[derive(Debug, Clone, Copy)]
pub struct PluginUninstallOptions {
    pub keep_data: bool,
}

/// Confirm-before-CLI update. Updating a pack may execute vendor/package code.
#[derive(Debug, Clone, Copy)]
pub struct PluginUpdateOptions {
    pub confirmed: bool,
}

/// The fixed vendor operation whose result was re-inventoried while the
/// desktop live-write lock was still held. This is deliberately not an
/// extension SDK or a dynamic plugin operation vocabulary.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum PluginMutationAction {
    Install,
    Uninstall,
    Enable,
    Disable,
    MarketplaceRefresh,
    Update,
    PiUpdate,
}

/// The smallest stable identity AgentHub can prove from a post-command scan.
/// Grok has no marketplace selector in its official operation, while Codex
/// and Pi require their exact install source.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PluginMutationTarget {
    pub agent: AgentId,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub marketplace: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub install_source: Option<String>,
}

impl PluginMutationTarget {
    pub fn new(
        agent: AgentId,
        name: impl Into<String>,
        marketplace: Option<&str>,
        install_source: Option<&str>,
    ) -> Self {
        let name = name.into();
        Self {
            agent,
            install_source: install_source.map(str::to_string).or_else(|| {
                if agent != AgentId::Codex {
                    return None;
                }
                if name.contains('@') {
                    Some(name.clone())
                } else {
                    marketplace.map(|market| format!("{name}@{market}"))
                }
            }),
            name,
            marketplace: marketplace.map(str::to_string),
        }
    }

    pub fn from_entry(entry: &PluginEntry) -> Self {
        Self {
            agent: entry.agent,
            name: entry.name.clone(),
            marketplace: entry.marketplace.clone(),
            install_source: entry.install_source.clone(),
        }
    }
}

/// The portion of the post-command scan that the outcome actually checked.
/// Pi updates are intentionally reported as an Agent-wide scan, never as a
/// pretend confirmation of one package.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PluginMutationReinventory {
    pub scope: PluginMutationReinventoryScope,
    pub scanned_plugin_ids: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub marketplace_entries: Option<Vec<PluginEntry>>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum PluginMutationReinventoryScope {
    Target,
    Marketplace,
    Agent,
}

/// Why a successful official command was not shown as a confirmed product
/// result. Command failures remain `Err`; this enum only covers successful
/// commands whose required re-inventory evidence was absent or inconclusive.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum PluginMutationUnconfirmedReason {
    InventoryUnavailable,
    /// Grok accepts only the package name. More than one installed row with
    /// that name leaves no stable identity to confirm after the command.
    AmbiguousTarget,
    TargetNotListed,
    TargetStillListed,
    EnabledStateMismatch,
    VersionUnchangedOrUnknown,
    MarketplaceSnapshotUnavailable,
    MarketplaceEntriesUnchanged,
    PiScopeUnchanged,
}

/// A tagged, non-ambiguous desktop mutation result. A successful process exit
/// is insufficient: callers only receive `Confirmed` when the immediate scan
/// proves the requested fixed-vendor state transition.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum PluginMutationOutcome {
    Confirmed {
        action: PluginMutationAction,
        agent: AgentId,
        #[serde(skip_serializing_if = "Option::is_none")]
        target: Option<PluginMutationTarget>,
        inventory: PluginInventory,
        reinventory: PluginMutationReinventory,
    },
    Unconfirmed {
        action: PluginMutationAction,
        agent: AgentId,
        #[serde(skip_serializing_if = "Option::is_none")]
        target: Option<PluginMutationTarget>,
        reason: PluginMutationUnconfirmedReason,
        inventory: PluginInventory,
        reinventory: PluginMutationReinventory,
    },
}

impl PluginMutationOutcome {
    pub fn confirmed(
        action: PluginMutationAction,
        agent: AgentId,
        target: Option<PluginMutationTarget>,
        inventory: PluginInventory,
        reinventory: PluginMutationReinventory,
    ) -> Self {
        Self::Confirmed {
            action,
            agent,
            target,
            inventory,
            reinventory,
        }
    }

    pub fn unconfirmed(
        action: PluginMutationAction,
        agent: AgentId,
        target: Option<PluginMutationTarget>,
        reason: PluginMutationUnconfirmedReason,
        inventory: PluginInventory,
        reinventory: PluginMutationReinventory,
    ) -> Self {
        Self::Unconfirmed {
            action,
            agent,
            target,
            reason,
            inventory,
            reinventory,
        }
    }

    pub fn inventory(&self) -> &PluginInventory {
        match self {
            Self::Confirmed { inventory, .. } | Self::Unconfirmed { inventory, .. } => inventory,
        }
    }

    pub fn is_confirmed(&self) -> bool {
        matches!(self, Self::Confirmed { .. })
    }
}

/// A scan is usable as mutation evidence only when that Agent's own list
/// completed. An empty row set from a failed list must never prove removal.
pub fn plugin_inventory_agent_verified(inventory: &PluginInventory, agent: AgentId) -> bool {
    inventory.agents.iter().any(|status| {
        status.agent == agent && status.support == "listed" && status.error_code.is_none()
    })
}

/// Match the vendor identity that its fixed command actually accepts. This is
/// intentionally not a cross-vendor plugin identity abstraction.
pub fn plugin_inventory_target_matches(row: &PluginEntry, target: &PluginMutationTarget) -> bool {
    if row.agent != target.agent {
        return false;
    }
    // Pi's command accepts a source while its re-inventory names the package
    // from its manifest. The normalized source is therefore the only exact
    // stable identity shared by both sides of that operation.
    if target.agent != AgentId::Pi && row.name != target.name {
        return false;
    }
    match target.agent {
        AgentId::Codex | AgentId::Pi => {
            target.install_source.is_some() && row.install_source == target.install_source
        }
        AgentId::Claude => {
            row.marketplace == target.marketplace
                && target
                    .install_source
                    .as_ref()
                    .is_none_or(|source| row.install_source.as_ref() == Some(source))
        }
        // Grok's official operation accepts its name. Its listing may omit a
        // marketplace and may represent git/local source differently, so
        // this deliberately identifies its name candidates. Callers must
        // require exactly one candidate before presenting a confirmed result.
        AgentId::Grok => true,
        _ => false,
    }
}

pub fn plugin_inventory_target<'a>(
    inventory: &'a PluginInventory,
    target: &PluginMutationTarget,
) -> Option<&'a PluginEntry> {
    plugin_inventory_target_candidates(inventory, target)
        .into_iter()
        .next()
}

/// All inventory rows the fixed vendor operation could mean. In particular,
/// Grok accepts only a name, so callers must not treat the first same-name row
/// as proof when more than one row is present.
pub fn plugin_inventory_target_candidates<'a>(
    inventory: &'a PluginInventory,
    target: &PluginMutationTarget,
) -> Vec<&'a PluginEntry> {
    inventory
        .plugins
        .iter()
        .filter(|row| plugin_inventory_target_matches(row, target))
        .collect()
}

pub fn plugin_inventory_target_is_ambiguous(
    inventory: &PluginInventory,
    target: &PluginMutationTarget,
) -> bool {
    target.agent == AgentId::Grok && plugin_inventory_target_candidates(inventory, target).len() > 1
}

pub fn plugin_inventory_agent_ids(inventory: &PluginInventory, agent: AgentId) -> Vec<String> {
    inventory
        .plugins
        .iter()
        .filter(|row| row.agent == agent)
        .map(|row| row.id.clone())
        .collect()
}

/// Reports only observable entry or version changes. It deliberately does not
/// infer that an unchanged Pi package was updated by a successful bulk CLI.
pub fn plugin_inventory_versions_or_entries_changed(
    before: &PluginInventory,
    after: &PluginInventory,
    agent: AgentId,
) -> bool {
    let signatures = |inventory: &PluginInventory| {
        let mut rows = inventory
            .plugins
            .iter()
            .filter(|row| row.agent == agent)
            .map(|row| {
                (
                    row.id.clone(),
                    row.name.clone(),
                    row.marketplace.clone(),
                    row.install_source.clone(),
                    row.version.clone(),
                )
            })
            .collect::<Vec<_>>();
        rows.sort_unstable();
        rows
    };
    signatures(before) != signatures(after)
}

impl Default for PluginUninstallOptions {
    fn default() -> Self {
        Self { keep_data: true }
    }
}

struct FileSnapshot {
    path: PathBuf,
    contents: Option<Vec<u8>>,
}

/// Enable a listed Claude, Codex, or Grok pack through its supported interface.
pub fn enable_plugin(agent: AgentId, name: &str, marketplace: Option<&str>) -> Result<(), String> {
    enable_plugin_with_v2(&system_ctx_v2(), agent, name, marketplace)
}

/// Disable a listed Claude, Codex, or Grok pack through its supported interface.
pub fn disable_plugin(agent: AgentId, name: &str, marketplace: Option<&str>) -> Result<(), String> {
    disable_plugin_with_v2(&system_ctx_v2(), agent, name, marketplace)
}

pub fn enable_plugin_with_v2(
    ctx: &PluginApplyContextV2<'_>,
    agent: AgentId,
    name: &str,
    marketplace: Option<&str>,
) -> Result<(), String> {
    match agent {
        AgentId::Codex => set_codex_plugin_enabled(ctx, name, marketplace, true),
        AgentId::Pi => Err("Pi extensions do not expose an enable/disable operation".into()),
        _ => enable_plugin_with(&ctx.base, agent, name, marketplace),
    }
}

pub fn disable_plugin_with_v2(
    ctx: &PluginApplyContextV2<'_>,
    agent: AgentId,
    name: &str,
    marketplace: Option<&str>,
) -> Result<(), String> {
    match agent {
        AgentId::Codex => set_codex_plugin_enabled(ctx, name, marketplace, false),
        AgentId::Pi => Err("Pi extensions do not expose an enable/disable operation".into()),
        _ => disable_plugin_with(&ctx.base, agent, name, marketplace),
    }
}

pub fn enable_plugin_with(
    ctx: &PluginApplyContext<'_>,
    agent: AgentId,
    name: &str,
    marketplace: Option<&str>,
) -> Result<(), String> {
    set_plugin_enabled_with(ctx, agent, name, marketplace, true)
}

pub fn disable_plugin_with(
    ctx: &PluginApplyContext<'_>,
    agent: AgentId,
    name: &str,
    marketplace: Option<&str>,
) -> Result<(), String> {
    set_plugin_enabled_with(ctx, agent, name, marketplace, false)
}

/// Refresh the configured Claude, Codex, or Grok marketplace catalogs.
pub fn refresh_plugin_marketplace(agent: AgentId) -> Result<(), String> {
    refresh_plugin_marketplace_with_v2(&system_ctx_v2(), agent)
}

pub fn refresh_plugin_marketplace_with_v2(
    ctx: &PluginApplyContextV2<'_>,
    agent: AgentId,
) -> Result<(), String> {
    if agent != AgentId::Codex {
        return refresh_plugin_marketplace_with(&ctx.base, agent);
    }
    let bin = ctx.codex_bin.as_deref().ok_or_else(|| {
        "official Codex command not found; cannot upgrade plugin marketplaces".to_string()
    })?;
    run_with_snapshot(
        agent,
        ctx.base.runner,
        bin,
        &["plugin", "marketplace", "upgrade"],
        UPDATE_TIMEOUT,
        &ctx.codex_home.join("config.toml"),
        "marketplace upgrade",
    )
}

pub fn refresh_plugin_marketplace_with(
    ctx: &PluginApplyContext<'_>,
    agent: AgentId,
) -> Result<(), String> {
    let (bin, live) = update_agent_paths(ctx, agent)?;
    run_with_snapshot(
        agent,
        ctx.runner,
        bin,
        &["plugin", "marketplace", "update"],
        UPDATE_TIMEOUT,
        &live,
        "marketplace refresh",
    )
}

/// Update one installed Claude or Grok pack through its official CLI.
pub fn update_plugin(
    agent: AgentId,
    name: &str,
    marketplace: Option<&str>,
    scope: Option<&str>,
    options: PluginUpdateOptions,
) -> Result<(), String> {
    update_plugin_with_v2(&system_ctx_v2(), agent, name, marketplace, scope, options)
}

pub fn update_plugin_with_v2(
    ctx: &PluginApplyContextV2<'_>,
    agent: AgentId,
    name: &str,
    marketplace: Option<&str>,
    scope: Option<&str>,
    options: PluginUpdateOptions,
) -> Result<(), String> {
    match agent {
        AgentId::Codex => Err("Codex only supports marketplace-wide plugin upgrades".into()),
        AgentId::Pi => Err("Pi extensions are updated together, not one package at a time".into()),
        _ => update_plugin_with(&ctx.base, agent, name, marketplace, scope, options),
    }
}

pub fn update_plugin_with(
    ctx: &PluginApplyContext<'_>,
    agent: AgentId,
    name: &str,
    marketplace: Option<&str>,
    scope: Option<&str>,
    options: PluginUpdateOptions,
) -> Result<(), String> {
    if !options.confirmed {
        return Err("update needs confirmation".into());
    }
    if scope != Some("user") {
        return Err("only user-scope plugin packs can be updated here".into());
    }
    let spec = vendor_spec(agent, name, marketplace)?;
    let (bin, live) = update_agent_paths(ctx, agent)?;
    let args = update_args(agent, &spec)?;
    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    run_with_snapshot(
        agent,
        ctx.runner,
        bin,
        &arg_refs,
        UPDATE_TIMEOUT,
        &live,
        "update",
    )
}

/// Update eligible Pi extensions. Pi skips exact npm semver pins itself.
pub fn update_pi_plugins(options: PluginUpdateOptions) -> Result<(), String> {
    if !options.confirmed {
        return Err("update needs confirmation".into());
    }
    let user_home = home_dir().unwrap_or_else(|_| PathBuf::from("/"));
    let pi_config =
        agent_config_dir(AgentId::Pi).unwrap_or_else(|_| user_home.join(".pi").join("agent"));
    let bin = which::which("pi")
        .map_err(|_| "official Pi command not found; cannot update plugins".to_string())?;
    run_pi_update_with_snapshot(&bin, &pi_config.join("settings.json"))
}

fn system_ctx() -> PluginApplyContext<'static> {
    static RUNNER: SystemPluginCliRunner = SystemPluginCliRunner;
    let user_home = home_dir().unwrap_or_else(|_| PathBuf::from("/"));
    let claude_home = agent_home(AgentId::Claude).unwrap_or_else(|_| user_home.join(".claude"));
    let grok_home = agent_home(AgentId::Grok).unwrap_or_else(|_| user_home.join(".grok"));
    PluginApplyContext {
        user_home,
        claude_home,
        grok_home,
        claude_bin: which::which("claude").ok(),
        grok_bin: which::which("grok").ok(),
        runner: &RUNNER,
    }
}

fn system_ctx_v2() -> PluginApplyContextV2<'static> {
    let base = system_ctx();
    let codex_home = agent_home(AgentId::Codex).unwrap_or_else(|_| base.user_home.join(".codex"));
    let pi_config =
        agent_config_dir(AgentId::Pi).unwrap_or_else(|_| base.user_home.join(".pi").join("agent"));
    PluginApplyContextV2 {
        base,
        codex_home,
        pi_config,
        codex_bin: which::which("codex").ok(),
        pi_bin: which::which("pi").ok(),
    }
}

/// Marketplace rows from `plugin list --json --available` (not installed inventory).
pub fn list_available_plugins(agent: AgentId) -> Result<Vec<PluginEntry>, String> {
    list_available_plugins_with_v2(&system_ctx_v2(), agent)
}

pub fn list_available_plugins_with_v2(
    ctx: &PluginApplyContextV2<'_>,
    agent: AgentId,
) -> Result<Vec<PluginEntry>, String> {
    if agent == AgentId::Pi {
        return Ok(Vec::new());
    }
    if agent != AgentId::Codex {
        return list_available_plugins_with(&ctx.base, agent);
    }
    let bin = ctx
        .codex_bin
        .as_deref()
        .ok_or_else(|| "official Codex command not found; cannot list plugins".to_string())?;
    let run = ctx.base.runner.run_plugin_with_timeout(
        bin,
        &["plugin", "list", "--available", "--json"],
        AVAILABLE_TIMEOUT,
    );
    if !run.success() {
        return Err(cli_error_detail(&run, "list"));
    }
    parse_cli_available_plugin_list(agent, &run.stdout, &ctx.base.user_home)
}

pub fn list_available_plugins_with(
    ctx: &PluginApplyContext<'_>,
    agent: AgentId,
) -> Result<Vec<PluginEntry>, String> {
    let bin = agent_bin(ctx, agent)?;
    let run = ctx.runner.run_plugin_with_timeout(
        bin,
        &["plugin", "list", "--json", "--available"],
        AVAILABLE_TIMEOUT,
    );
    if !run.success() {
        return Err(cli_error_detail(&run, "list"));
    }
    parse_cli_available_plugin_list(agent, &run.stdout, &ctx.user_home)
}

/// Preview components for a marketplace name, Grok git/local source, or typed spec.
pub fn preview_plugin_install(agent: AgentId, source: &str) -> Result<PluginEntry, String> {
    preview_plugin_install_with_v2(&system_ctx_v2(), agent, source)
}

pub fn preview_plugin_install_with_v2(
    ctx: &PluginApplyContextV2<'_>,
    agent: AgentId,
    source: &str,
) -> Result<PluginEntry, String> {
    match agent {
        AgentId::Codex => {
            let spec = codex_plugin_spec(source, None)?;
            let rows = list_available_plugins_with_v2(ctx, agent)?;
            rows.into_iter()
                .find(|row| row.install_source.as_deref() == Some(spec.as_str()))
                .ok_or_else(|| "plugin is not available from a configured Codex marketplace".into())
        }
        AgentId::Pi => {
            let source = normalize_pi_install_source(source, &ctx.base.user_home)?;
            Ok(stub_preview_with_source(
                agent,
                &pi_display_name(&source),
                None,
                &source,
            ))
        }
        _ => preview_plugin_install_with(&ctx.base, agent, source),
    }
}

pub fn preview_plugin_install_with(
    ctx: &PluginApplyContext<'_>,
    agent: AgentId,
    source: &str,
) -> Result<PluginEntry, String> {
    let kind = classify_install_source(agent, source)?;
    match kind {
        InstallSource::Local(path) => preview_local_plugin(agent, &path, &ctx.user_home),
        InstallSource::Git(raw) => Ok(stub_preview(agent, &raw, None)),
        InstallSource::Marketplace { name, marketplace } => {
            let rows = list_available_plugins_with(ctx, agent)?;
            if let Some(row) = rows.into_iter().find(|row| {
                row.name == name
                    && marketplace_matches(row.marketplace.as_deref(), marketplace.as_deref())
            }) {
                return Ok(row);
            }
            Ok(stub_preview(agent, &name, marketplace.as_deref()))
        }
    }
}

/// Install after UI confirm. Without `confirmed`, the official command is not called.
pub fn install_plugin(
    agent: AgentId,
    source: &str,
    options: PluginInstallOptions,
) -> Result<(), String> {
    install_plugin_with_v2(&system_ctx_v2(), agent, source, options)
}

pub fn install_plugin_with_v2(
    ctx: &PluginApplyContextV2<'_>,
    agent: AgentId,
    source: &str,
    options: PluginInstallOptions,
) -> Result<(), String> {
    if !options.confirmed {
        return Err("installation needs confirmation".into());
    }
    match agent {
        AgentId::Codex => {
            let source = codex_plugin_spec(source, None)?;
            let bin = ctx.codex_bin.as_deref().ok_or_else(|| {
                "official Codex command not found; cannot install plugins".to_string()
            })?;
            run_with_snapshot(
                agent,
                ctx.base.runner,
                bin,
                &["plugin", "add", source.as_str(), "--json"],
                INSTALL_TIMEOUT,
                &ctx.codex_home.join("config.toml"),
                "install",
            )
        }
        AgentId::Pi => {
            let source = normalize_pi_install_source(source, &ctx.base.user_home)?;
            run_pi_package_command(ctx, "install", &source, INSTALL_TIMEOUT, "install", false)
        }
        _ => install_plugin_with(&ctx.base, agent, source, options),
    }
}

pub fn install_plugin_with(
    ctx: &PluginApplyContext<'_>,
    agent: AgentId,
    source: &str,
    options: PluginInstallOptions,
) -> Result<(), String> {
    let kind = classify_install_source(agent, source)?;
    if !options.confirmed {
        return Err("installation needs confirmation".into());
    }
    let spec = kind.cli_source();
    let (bin, live) = agent_paths(ctx, agent)?;
    let _guard = plugin_write_guard(agent)?;
    let snapshot = snapshot_file(&live)?;
    let args = install_args(agent, &spec);
    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let run = ctx
        .runner
        .run_plugin_with_timeout(bin, &arg_refs, INSTALL_TIMEOUT);
    if run.success() {
        return Ok(());
    }
    let restore_err = restore_file(&snapshot).err();
    let detail = cli_error_detail(&run, "install");
    Err(cli_failure_with_restore(detail, restore_err))
}

/// Remove a listed Claude, Codex, Grok, or Pi package through its supported interface.
pub fn uninstall_plugin(
    agent: AgentId,
    name: &str,
    marketplace: Option<&str>,
    install_source: Option<&str>,
    options: PluginUninstallOptions,
) -> Result<(), String> {
    uninstall_plugin_with_v2(
        &system_ctx_v2(),
        agent,
        name,
        marketplace,
        install_source,
        options,
    )
}

pub fn uninstall_plugin_with_v2(
    ctx: &PluginApplyContextV2<'_>,
    agent: AgentId,
    name: &str,
    marketplace: Option<&str>,
    install_source: Option<&str>,
    options: PluginUninstallOptions,
) -> Result<(), String> {
    match agent {
        AgentId::Codex => {
            let source = match install_source {
                Some(source) => codex_plugin_spec(source, None)?,
                None => codex_plugin_spec(name, marketplace)?,
            };
            let expected = codex_plugin_spec(name, marketplace)?;
            if source != expected {
                return Err(
                    "plugin install source does not match the selected Codex plugin".into(),
                );
            }
            let bin = ctx.codex_bin.as_deref().ok_or_else(|| {
                "official Codex command not found; cannot remove plugins".to_string()
            })?;
            run_with_snapshot(
                agent,
                ctx.base.runner,
                bin,
                &["plugin", "remove", source.as_str(), "--json"],
                UNINSTALL_TIMEOUT,
                &ctx.codex_home.join("config.toml"),
                "remove",
            )
        }
        AgentId::Pi => {
            let source = install_source.ok_or_else(|| {
                "Pi removal requires the exact configured install source".to_string()
            })?;
            let source = validate_pi_remove_source(source)?;
            run_pi_package_command(ctx, "remove", source, UNINSTALL_TIMEOUT, "remove", true)
        }
        _ => {
            if install_source.is_some_and(|source| source.trim().is_empty()) {
                return Err("invalid plugin install source".into());
            }
            uninstall_plugin_with(&ctx.base, agent, name, marketplace, options)
        }
    }
}

pub fn uninstall_plugin_with(
    ctx: &PluginApplyContext<'_>,
    agent: AgentId,
    name: &str,
    marketplace: Option<&str>,
    options: PluginUninstallOptions,
) -> Result<(), String> {
    let spec = vendor_spec(agent, name, marketplace)?;
    let (bin, live) = agent_paths(ctx, agent)?;
    let _guard = plugin_write_guard(agent)?;
    let snapshot = snapshot_file(&live)?;
    let args = uninstall_args(agent, &spec, options.keep_data);
    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let run = ctx
        .runner
        .run_plugin_with_timeout(bin, &arg_refs, UNINSTALL_TIMEOUT);
    if run.success() {
        return Ok(());
    }
    let restore_err = restore_file(&snapshot).err();
    let detail = cli_error_detail(&run, "uninstall");
    Err(cli_failure_with_restore(detail, restore_err))
}

enum InstallSource {
    Marketplace {
        name: String,
        marketplace: Option<String>,
    },
    Git(String),
    Local(PathBuf),
}

impl InstallSource {
    fn cli_source(&self) -> String {
        match self {
            Self::Marketplace { name, marketplace } => {
                match (agent_spec_needs_marketplace(name), marketplace) {
                    (true, Some(market)) => format!("{name}@{market}"),
                    _ => name.clone(),
                }
            }
            Self::Git(raw) => raw.clone(),
            Self::Local(path) => path.to_string_lossy().into_owned(),
        }
    }
}

fn agent_spec_needs_marketplace(name: &str) -> bool {
    !name.contains('@')
}

fn classify_install_source(agent: AgentId, raw: &str) -> Result<InstallSource, String> {
    let source = raw.trim();
    if source.is_empty() {
        return Err("plugin source is required".into());
    }
    if source == "mcpServers"
        || source.contains('\0')
        || source.contains(['\n', '\r', ';', '|', '$', '`'])
    {
        return Err("invalid plugin source".into());
    }
    match agent {
        AgentId::Claude => {
            if looks_like_local_path(source) || looks_like_git_source(source) {
                return Err(
                    "Claude install accepts name@marketplace, not a git URL or local path".into(),
                );
            }
            if source.contains(['/', '\\']) {
                return Err("invalid plugin source".into());
            }
            let (name, marketplace) = split_install_name(source);
            if !valid_plugin_token(&name)
                || marketplace
                    .as_deref()
                    .is_some_and(|m| !valid_plugin_token(m))
            {
                return Err("invalid plugin source".into());
            }
            Ok(InstallSource::Marketplace { name, marketplace })
        }
        AgentId::Grok => {
            if looks_like_local_path(source) {
                let path = expand_local_path(source);
                if path_has_parent_dir(&path) {
                    return Err("invalid plugin source".into());
                }
                return Ok(InstallSource::Local(path));
            }
            if looks_like_git_source(source) {
                return Ok(InstallSource::Git(source.to_string()));
            }
            if source.contains(['/', '\\']) {
                return Err("invalid plugin source".into());
            }
            if !valid_plugin_token(source) {
                return Err("invalid plugin source".into());
            }
            Ok(InstallSource::Marketplace {
                name: source.to_string(),
                marketplace: None,
            })
        }
        _ => Err("install is only available for listed Claude and Grok plugin packs".into()),
    }
}

fn codex_plugin_spec(name: &str, marketplace: Option<&str>) -> Result<String, String> {
    let raw = name.trim();
    if raw.is_empty()
        || raw.starts_with('-')
        || raw.chars().any(char::is_control)
        || raw.contains(['/', '\\'])
    {
        return Err("invalid Codex plugin ID".into());
    }
    let (plugin, embedded_marketplace) = match raw.split_once('@') {
        Some((plugin, market)) => (plugin, Some(market)),
        None => (raw, None),
    };
    if raw.matches('@').count() > 1 || !valid_plugin_token(plugin) {
        return Err("invalid Codex plugin ID".into());
    }
    let requested_marketplace = marketplace.map(str::trim).filter(|value| !value.is_empty());
    if embedded_marketplace.is_some()
        && requested_marketplace.is_some()
        && embedded_marketplace != requested_marketplace
    {
        return Err("Codex plugin marketplace does not match the selected plugin".into());
    }
    let market = embedded_marketplace.or(requested_marketplace);
    let Some(market) = market.filter(|value| valid_plugin_token(value)) else {
        return Err("Codex plugins require name@marketplace".into());
    };
    Ok(format!("{plugin}@{market}"))
}

fn normalize_pi_install_source(source: &str, user_home: &Path) -> Result<String, String> {
    let source = source.trim();
    if source.is_empty()
        || source.starts_with('-')
        || source == "mcpServers"
        || source.chars().any(char::is_control)
    {
        return Err("invalid Pi install source".into());
    }
    if source.starts_with("npm:") {
        let package = source.trim_start_matches("npm:");
        if package.is_empty()
            || package.chars().any(char::is_whitespace)
            || package.split(['/', '\\']).any(|part| part == "..")
        {
            return Err("invalid Pi npm source".into());
        }
        return Ok(source.to_string());
    }
    if source.starts_with("git:")
        || source.starts_with("https://")
        || source.starts_with("http://")
        || source.starts_with("ssh://")
        || source.starts_with("git@")
    {
        if source.chars().any(char::is_whitespace)
            || source.split(['/', '\\', ':']).any(|part| part == "..")
        {
            return Err("invalid Pi git source".into());
        }
        return Ok(source.to_string());
    }
    if !looks_like_local_path(source) {
        return Err("Pi install source must be npm:, git, or a local path".into());
    }
    let path = if source == "~" {
        user_home.to_path_buf()
    } else if let Some(rest) = source
        .strip_prefix("~/")
        .or_else(|| source.strip_prefix("~\\"))
    {
        user_home.join(rest)
    } else {
        PathBuf::from(source)
    };
    if !path.is_absolute() {
        return Err("Pi local install source must be an absolute path".into());
    }
    if path_has_parent_dir(&path) {
        return Err("Pi local install source cannot traverse parent directories".into());
    }
    Ok(path.to_string_lossy().into_owned())
}

fn validate_pi_remove_source(source: &str) -> Result<&str, String> {
    let source = source.trim();
    if source.is_empty()
        || source.starts_with('-')
        || source == "mcpServers"
        || source.chars().any(char::is_control)
    {
        return Err("invalid Pi install source".into());
    }
    // Pi may persist an originally absolute local install as a relative path
    // containing `..`. Removal must pass that exact settings.json selector
    // back to Pi; AgentHub itself never resolves or accesses this path.
    Ok(source)
}

fn pi_display_name(source: &str) -> String {
    let base = source
        .strip_prefix("npm:")
        .or_else(|| source.strip_prefix("git:"))
        .unwrap_or(source);
    base.rsplit(['/', ':'])
        .next()
        .unwrap_or(base)
        .split(['@', '#'])
        .next()
        .filter(|value| !value.is_empty())
        .unwrap_or(base)
        .to_string()
}

fn split_install_name(raw: &str) -> (String, Option<String>) {
    if let Some((name, market)) = raw.split_once('@') {
        if !name.is_empty() && !market.is_empty() {
            return (name.to_string(), Some(market.to_string()));
        }
    }
    (raw.to_string(), None)
}

fn valid_plugin_token(raw: &str) -> bool {
    let mut chars = raw.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    first.is_ascii_alphanumeric()
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

fn looks_like_local_path(source: &str) -> bool {
    source.starts_with('/')
        || source.starts_with('.')
        || source.starts_with('~')
        || source.contains('\\')
}

fn looks_like_git_source(source: &str) -> bool {
    source.starts_with("https://")
        || source.starts_with("http://")
        || source.starts_with("git@")
        || source.starts_with("ssh://")
        || {
            let parts: Vec<&str> = source.split('/').collect();
            parts.len() == 2
                && valid_plugin_token(parts[0])
                && valid_plugin_token(parts[1].split(['@', '#']).next().unwrap_or(""))
        }
}

fn expand_local_path(source: &str) -> PathBuf {
    if let Some(rest) = source.strip_prefix("~/") {
        if let Ok(home) = home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(source)
}

fn path_has_parent_dir(path: &Path) -> bool {
    path.components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
}

fn marketplace_matches(row: Option<&str>, wanted: Option<&str>) -> bool {
    match wanted {
        None | Some("") => true,
        Some(market) => row == Some(market),
    }
}

fn stub_preview(agent: AgentId, name: &str, marketplace: Option<&str>) -> PluginEntry {
    let install_source = marketplace.map(|market| format!("{name}@{market}"));
    stub_preview_owned(agent, name, marketplace, install_source)
}

fn stub_preview_with_source(
    agent: AgentId,
    name: &str,
    marketplace: Option<&str>,
    install_source: &str,
) -> PluginEntry {
    stub_preview_owned(agent, name, marketplace, Some(install_source.to_string()))
}

fn stub_preview_owned(
    agent: AgentId,
    name: &str,
    marketplace: Option<&str>,
    install_source: Option<String>,
) -> PluginEntry {
    PluginEntry {
        id: match marketplace {
            Some(m) => format!("{}:{}@{}", agent.as_str(), name, m),
            None => format!("{}:{}", agent.as_str(), name),
        },
        agent,
        name: name.to_string(),
        install_source,
        marketplace: marketplace.map(str::to_string),
        version: None,
        requested_version: None,
        scope: Some("user".into()),
        enabled: None,
        trusted: None,
        path: None,
        description: None,
        source: "available".into(),
        components: Vec::new(),
    }
}

fn install_args(agent: AgentId, spec: &str) -> Vec<String> {
    match agent {
        AgentId::Claude => vec![
            "plugin".into(),
            "install".into(),
            spec.into(),
            "-y".into(),
            "-s".into(),
            "user".into(),
        ],
        AgentId::Grok => vec![
            "plugin".into(),
            "install".into(),
            spec.into(),
            "--trust".into(),
        ],
        _ => vec!["plugin".into(), "install".into(), spec.into()],
    }
}

fn uninstall_args(agent: AgentId, spec: &str, keep_data: bool) -> Vec<String> {
    let mut args = vec!["plugin".into(), "uninstall".into(), spec.into()];
    match agent {
        AgentId::Grok => {
            args.push("--confirm".into());
            if keep_data {
                args.push("--keep-data".into());
            }
        }
        AgentId::Claude => {
            args.extend(["-s".into(), "user".into(), "-y".into()]);
            if keep_data {
                args.push("--keep-data".into());
            }
        }
        _ => {}
    }
    args
}

fn update_args(agent: AgentId, spec: &str) -> Result<Vec<String>, String> {
    match agent {
        AgentId::Claude => Ok(vec![
            "plugin".into(),
            "update".into(),
            spec.into(),
            "-y".into(),
            "-s".into(),
            "user".into(),
        ]),
        AgentId::Grok => Ok(vec!["plugin".into(), "update".into(), spec.into()]),
        _ => Err("individual update is only available for Claude and Grok plugin packs".into()),
    }
}

fn agent_bin<'a>(ctx: &'a PluginApplyContext<'_>, agent: AgentId) -> Result<&'a Path, String> {
    match agent {
        AgentId::Claude => ctx.claude_bin.as_deref().ok_or_else(|| {
            "official Claude command not found; cannot list or install plugins".to_string()
        }),
        AgentId::Grok => ctx.grok_bin.as_deref().ok_or_else(|| {
            "official Grok command not found; cannot list or install plugins".to_string()
        }),
        _ => Err("install is only available for listed Claude and Grok plugin packs".into()),
    }
}

fn set_plugin_enabled_with(
    ctx: &PluginApplyContext<'_>,
    agent: AgentId,
    name: &str,
    marketplace: Option<&str>,
    enabled: bool,
) -> Result<(), String> {
    let spec = vendor_spec(agent, name, marketplace)?;
    let (bin, live) = agent_paths(ctx, agent)?;
    let _guard = plugin_write_guard(agent)?;
    let snapshot = snapshot_file(&live)?;
    let action = if enabled { "enable" } else { "disable" };
    let run = ctx
        .runner
        .run_plugin(bin, &["plugin", action, spec.as_str()]);
    if run.success() {
        return Ok(());
    }
    let restore_err = restore_file(&snapshot).err();
    let detail = cli_error_detail(&run, action);
    Err(cli_failure_with_restore(detail, restore_err))
}

fn agent_paths<'a>(
    ctx: &'a PluginApplyContext<'_>,
    agent: AgentId,
) -> Result<(&'a Path, PathBuf), String> {
    match agent {
        AgentId::Claude => {
            let bin = ctx.claude_bin.as_deref().ok_or_else(|| {
                "official Claude command not found; cannot enable or disable plugins".to_string()
            })?;
            Ok((bin, ctx.claude_home.join("settings.json")))
        }
        AgentId::Grok => {
            let bin = ctx.grok_bin.as_deref().ok_or_else(|| {
                "official Grok command not found; cannot enable or disable plugins".to_string()
            })?;
            Ok((bin, ctx.grok_home.join("config.toml")))
        }
        _ => Err("enable/disable is only available for listed Claude and Grok plugin packs".into()),
    }
}

fn update_agent_paths<'a>(
    ctx: &'a PluginApplyContext<'_>,
    agent: AgentId,
) -> Result<(&'a Path, PathBuf), String> {
    match agent {
        AgentId::Claude => ctx
            .claude_bin
            .as_deref()
            .map(|bin| (bin, ctx.claude_home.join("settings.json")))
            .ok_or_else(|| "official Claude command not found; cannot update plugins".into()),
        AgentId::Grok => ctx
            .grok_bin
            .as_deref()
            .map(|bin| (bin, ctx.grok_home.join("config.toml")))
            .ok_or_else(|| "official Grok command not found; cannot update plugins".into()),
        _ => Err("marketplace refresh and individual update are only available for Claude and Grok plugin packs".into()),
    }
}

fn vendor_spec(agent: AgentId, name: &str, marketplace: Option<&str>) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("plugin name is required".into());
    }
    if name == "mcpServers" || name.starts_with('-') || name.chars().any(char::is_control) {
        return Err("invalid plugin name".into());
    }
    match agent {
        AgentId::Claude => {
            let mut parts = name.split('@');
            let plugin = parts.next().unwrap_or_default();
            let embedded_market = parts.next();
            if parts.next().is_some()
                || !valid_plugin_token(plugin)
                || embedded_market.is_some_and(|market| !valid_plugin_token(market))
            {
                return Err("invalid plugin name".into());
            }
            let market =
                embedded_market.or_else(|| marketplace.map(str::trim).filter(|s| !s.is_empty()));
            if market.is_some_and(|value| !valid_plugin_token(value)) {
                return Err("invalid plugin marketplace".into());
            }
            Ok(match market {
                Some(value) => format!("{plugin}@{value}"),
                None => plugin.to_string(),
            })
        }
        AgentId::Grok if valid_plugin_token(name) => Ok(name.to_string()),
        AgentId::Grok => Err("invalid plugin name".into()),
        _ => Err("enable/disable is only available for listed Claude and Grok plugin packs".into()),
    }
}

fn set_codex_plugin_enabled(
    ctx: &PluginApplyContextV2<'_>,
    name: &str,
    marketplace: Option<&str>,
    enabled: bool,
) -> Result<(), String> {
    let spec = codex_plugin_spec(name, marketplace)?;
    let config = ctx.codex_home.join("config.toml");
    let _guard = plugin_write_guard(AgentId::Codex)?;
    let snapshot = snapshot_file(&config)?;
    let write_result = (|| {
        let text = if config.is_file() {
            fs::read_to_string(&config)
                .map_err(|error| format!("read Codex plugin config: {error}"))?
        } else {
            String::new()
        };
        let mut doc = text
            .parse::<DocumentMut>()
            .map_err(|error| format!("parse Codex plugin config: {error}"))?;
        let plugins = doc
            .get_mut("plugins")
            .ok_or_else(|| "selected Codex plugin is not installed".to_string())?
            .as_table_mut()
            .ok_or_else(|| "Codex plugins config is not a table".to_string())?;
        let plugin = plugins
            .get_mut(&spec)
            .ok_or_else(|| "selected Codex plugin is not installed".to_string())?
            .as_table_mut()
            .ok_or_else(|| "selected Codex plugin config is not a table".to_string())?;
        plugin["enabled"] = value(enabled);
        atomic_write(&config, doc.to_string().as_bytes())
            .map_err(|error| format!("write Codex plugin config: {error}"))?;

        let verified = fs::read_to_string(&config)
            .map_err(|error| format!("verify Codex plugin config: {error}"))?
            .parse::<DocumentMut>()
            .map_err(|error| format!("verify Codex plugin config: {error}"))?;
        if verified
            .get("plugins")
            .and_then(|item| item.get(&spec))
            .and_then(|item| item.get("enabled"))
            .and_then(|item| item.as_bool())
            != Some(enabled)
        {
            return Err("Codex plugin enablement verification failed".into());
        }
        Ok(())
    })();
    match write_result {
        Ok(()) => Ok(()),
        Err(error) => Err(cli_failure_with_restore(
            error,
            restore_file(&snapshot).err(),
        )),
    }
}

fn run_pi_package_command(
    ctx: &PluginApplyContextV2<'_>,
    action: &str,
    source: &str,
    timeout: Duration,
    error_action: &str,
    require_configured_source: bool,
) -> Result<(), String> {
    let bin = ctx
        .pi_bin
        .as_deref()
        .ok_or_else(|| "official Pi command not found; cannot manage extensions".to_string())?;
    let neutral_cwd = if require_configured_source {
        None
    } else {
        Some(
            tempfile::Builder::new()
                .prefix("agenthub-pi-plugin-")
                .tempdir()
                .map_err(|_| "cannot create an isolated Pi plugin directory".to_string())?,
        )
    };
    // Pi persists local sources relative to PI_CODING_AGENT_DIR. Removal must
    // run from that neutral user-config root so the exact persisted selector
    // resolves the same way; installs use a fresh directory to avoid project
    // discovery.
    let cwd = neutral_cwd
        .as_ref()
        .map(|dir| dir.path())
        .unwrap_or(ctx.pi_config.as_path());
    let live = ctx.pi_config.join("settings.json");
    let _guard = plugin_write_guard(AgentId::Pi)?;
    let snapshot = snapshot_file(&live)?;
    if require_configured_source && !pi_settings_contains_exact_source(&live, source)? {
        return Err("Pi install source is not present in the current user settings".into());
    }
    let run = ctx.base.runner.run_plugin_with_timeout_in_cwd(
        bin,
        &[action, source, "--no-approve"],
        timeout,
        cwd,
    );
    if run.success() {
        return Ok(());
    }
    let restore_err = restore_file(&snapshot).err();
    Err(cli_failure_with_restore(
        cli_error_detail(&run, error_action),
        restore_err,
    ))
}

fn pi_settings_contains_exact_source(settings: &Path, expected: &str) -> Result<bool, String> {
    let text = fs::read_to_string(settings)
        .map_err(|error| format!("read Pi plugin settings: {error}"))?;
    let root: JsonValue = serde_json::from_str(&text)
        .map_err(|error| format!("parse Pi plugin settings: {error}"))?;
    let Some(packages) = root.get("packages").and_then(JsonValue::as_array) else {
        return Ok(false);
    };
    Ok(packages.iter().any(|item| match item {
        JsonValue::String(source) => source == expected,
        JsonValue::Object(map) => map.get("source").and_then(JsonValue::as_str) == Some(expected),
        _ => false,
    }))
}

fn snapshot_file(path: &Path) -> Result<FileSnapshot, String> {
    if !path.exists() {
        return Ok(FileSnapshot {
            path: path.to_path_buf(),
            contents: None,
        });
    }
    let contents = fs::read(path).map_err(|e| format!("backup {}: {e}", path.display()))?;
    Ok(FileSnapshot {
        path: path.to_path_buf(),
        contents: Some(contents),
    })
}

fn restore_file(snapshot: &FileSnapshot) -> Result<(), String> {
    match &snapshot.contents {
        Some(bytes) => atomic_write(&snapshot.path, bytes)
            .map_err(|e| format!("restore {}: {e}", snapshot.path.display())),
        None => {
            if snapshot.path.exists() {
                fs::remove_file(&snapshot.path)
                    .map_err(|e| format!("restore {}: {e}", snapshot.path.display()))?;
            }
            Ok(())
        }
    }
}

fn run_with_snapshot(
    agent: AgentId,
    runner: &dyn PluginCliRunner,
    bin: &Path,
    args: &[&str],
    timeout: Duration,
    live: &Path,
    action: &str,
) -> Result<(), String> {
    let _guard = plugin_write_guard(agent)?;
    let snapshot = snapshot_file(live)?;
    let run = runner.run_plugin_with_timeout(bin, args, timeout);
    if run.success() {
        return Ok(());
    }
    let restore_err = restore_file(&snapshot).err();
    let detail = cli_error_detail(&run, action);
    Err(cli_failure_with_restore(detail, restore_err))
}

fn run_pi_update_with_snapshot(bin: &Path, live: &Path) -> Result<(), String> {
    let neutral_cwd = tempfile::Builder::new()
        .prefix("agenthub-pi-update-")
        .tempdir()
        .map_err(|_| "cannot create an isolated Pi update directory".to_string())?;
    let _guard = plugin_write_guard(AgentId::Pi)?;
    let snapshot = snapshot_file(live)?;
    let result = SystemProcessRunner.run(
        &RunSpec {
            agent: AgentId::Pi,
            program: bin.to_path_buf(),
            args: vec![
                "update".into(),
                "--extensions".into(),
                "--no-approve".into(),
            ],
            cwd: Some(neutral_cwd.path().to_path_buf()),
            env: Vec::new(),
        },
        UPDATE_TIMEOUT,
        UPDATE_OUTPUT_LIMIT,
    );
    if result.status == RunStatus::Ok && result.exit_code == Some(0) {
        return Ok(());
    }
    let restore_err = restore_file(&snapshot).err();
    let detail = match result.status {
        RunStatus::Timeout => "plugin update timed out".to_string(),
        _ => "plugin update failed".to_string(),
    };
    Err(cli_failure_with_restore(detail, restore_err))
}

fn plugin_write_guard(agent: AgentId) -> Result<MutexGuard<'static, ()>, String> {
    let lock = match agent {
        AgentId::Claude => &CLAUDE_WRITE_LOCK,
        AgentId::Grok => &GROK_WRITE_LOCK,
        AgentId::Pi => &PI_WRITE_LOCK,
        AgentId::Codex => &CODEX_WRITE_LOCK,
        _ => return Err("plugin writes are not available for this agent".into()),
    };
    lock.lock()
        .map_err(|_| "plugin write lock is unavailable".to_string())
}

fn cli_error_detail(run: &CliRun, action: &str) -> String {
    if run.unavailable() {
        return "official plugin command is unavailable".into();
    }
    if run.timed_out {
        return format!("plugin {action} timed out");
    }
    format!("plugin {action} failed")
}

fn cli_failure_with_restore(detail: String, restore_error: Option<String>) -> String {
    if restore_error.is_some() {
        format!("{detail}; plugin configuration restore failed")
    } else {
        detail
    }
}

#[cfg(test)]
#[path = "plugin_apply/tests.rs"]
mod tests;
