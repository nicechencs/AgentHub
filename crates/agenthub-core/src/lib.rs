//! agenthub-core — shared business logic for GUI and CLI.
//! No Tauri dependency.

pub mod adapter_control;
pub mod adapters;
pub mod bridge;
pub mod catalog;
pub mod domain;
pub mod error;
pub mod integrations;
pub mod logging;
pub mod models;
pub mod oauth;
pub mod platform;
pub mod presets;
pub mod runtime;
pub mod services;
pub mod storage;
pub mod usage;
pub mod utils;

mod startup;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use adapters::AdapterRegistry;
use error::Result;
use models::{
    AdapterSourceKind, AgentId, AgentUpdateInfo, ConnectionTrashKind, InstallOutcome,
    MultiRunReport, RouteMembershipTrashPayload, RunOptions, RuntimeId, RuntimeUpdateInfo, Skill,
    SkillListing, SwitchConfirmKind, SwitchConfirmPreview, TRASH_HOME_ROUTE_POOL,
};
use platform::{LifecycleCoordinator, LifecycleResult};
use services::{
    check_agent_updates as probe_agent_updates, check_runtime_updates as probe_runtime_updates,
    install_runtime_system, invalidate_latest_cache, invalidate_runtime_latest_cache,
    AccountService, AdapterApplyService, AdapterBridgeService, AdapterRouteService,
    AdapterSecretResolver, AgentService, AgentVisibilityService, BackupService, ChatService,
    ConnectionService, EnvService, ProjectService, ProviderService, RoutePoolService, RunService,
    SettingsService, SkillService, TicketBindService, TicketReadService, UsageService,
};
use storage::Database;
use utils::command_exec::SystemCommandExecutor;

// Re-export catalog + configuration types for GUI and CLI shells.
pub use platform::{
    AgentCatalogService, AgentConfigSchema, AgentDescriptor, AgentKey, ConfigApplyResult,
    ConfigChangePlan, ConfigValidationResult, ConfigurationService, InstallChannelDescriptor,
    NormalizedConfigDocument, SECRET_REDACTED,
};

/// Application facade shared by CLI and (future) GUI.
pub struct AgentHub {
    pub(crate) data_dir: PathBuf,
    pub(crate) db: Database,
    pub(crate) registry: AdapterRegistry,
    /// Read-only agent directory (key / capabilities / install channels).
    pub(crate) catalog: AgentCatalogService,
    /// Install-family lifecycle (operation records + redetect).
    pub(crate) lifecycle: LifecycleCoordinator,
    /// Native config schema / read / validate / apply (projectors).
    pub(crate) configuration: ConfigurationService,
    /// Active account/provider binding (unique current pointer per agent).
    pub(crate) connections: ConnectionService,
    pub(crate) env: EnvService,
    pub(crate) agents: AgentService,
    pub(crate) providers: ProviderService,
    pub(crate) accounts: AccountService,
    /// Read-only compatibility route analysis. Never applies configuration.
    pub(crate) adapter_routes: AdapterRouteService,
    /// Applies the one supported Kimi membership -> Claude adapter projection.
    pub(crate) adapter_apply: AdapterApplyService,
    /// Prepares/persists the Kimi membership -> Codex bridge saga. The desktop
    /// host owns listener lifetime and live configuration switching.
    pub(crate) adapter_bridge: AdapterBridgeService,
    /// Read-only Ticket / Binding wallet aggregation + plan(ticket, agent).
    pub(crate) tickets: TicketReadService,
    /// Ticket bind / unbind write API. Codex bridge bind stays on the host.
    pub(crate) ticket_bind: TicketBindService,
    pub(crate) backups: BackupService,
    pub(crate) skills: SkillService,
    pub(crate) settings: SettingsService,
    pub(crate) run: Arc<RunService>,
    pub(crate) chat: ChatService,
    pub(crate) projects: ProjectService,
    pub(crate) usage: UsageService,
    /// Soft-hide preference (UI only; detect / install unchanged).
    pub(crate) agent_visibility: AgentVisibilityService,
    /// Default-on RoutePool persistence (`feature.route_pool_v2`).
    /// Resolver attach is `feature.route_index_v2` (also default on).
    /// Mixed-provider composite routes stay `feature.mixed_provider_pool`
    /// (fail-closed).
    pub(crate) route_pools: RoutePoolService,
}

impl AgentHub {
    /// Open hub with optional data-dir override (`--data-dir` / `AGENTHUB_HOME`).
    ///
    /// Skills stay at `~/.agents/skills` regardless of data-dir.
    pub fn open(data_dir_override: Option<&Path>) -> Result<Self> {
        Self::open_with_skills_root(data_dir_override, None)
    }

    /// Same as [`open`], with an optional skills source root.
    ///
    /// `skills_root == None` uses `~/.agents/skills` (production).
    pub fn open_with_skills_root(
        data_dir_override: Option<&Path>,
        skills_root: Option<&Path>,
    ) -> Result<Self> {
        crate::startup::open_with_skills_root(data_dir_override, skills_root)
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    pub fn registry(&self) -> &AdapterRegistry {
        &self.registry
    }

    /// Storage handle for composition and tests.
    /// Domain writes go through the corresponding service.
    pub fn db(&self) -> &Database {
        &self.db
    }

    pub fn catalog(&self) -> &AgentCatalogService {
        &self.catalog
    }

    pub fn lifecycle(&self) -> &LifecycleCoordinator {
        &self.lifecycle
    }

    pub fn configuration(&self) -> &ConfigurationService {
        &self.configuration
    }

    pub fn connections(&self) -> &ConnectionService {
        &self.connections
    }

    /// Restore a recycle-bin row into its original home (Connections or the pool).
    pub fn restore_connection_trash(&self, id: &str) -> Result<()> {
        let row = self.connections.load_trash_payload(id)?;
        match row.kind {
            ConnectionTrashKind::Membership => {
                let payload: RouteMembershipTrashPayload = serde_json::from_value(row.payload)?;
                self.route_pools.restore_membership_trash(&payload)?;
                self.connections.delete_trash(id)?;
            }
            ConnectionTrashKind::Account => {
                let home = row.home.clone();
                let agent_id = row.agent_id;
                let source_id = row.source_id.clone();
                self.connections.restore_trash(id)?;
                if home == TRASH_HOME_ROUTE_POOL {
                    self.route_pools.reattach_restored_pool_owned(
                        agent_id,
                        AdapterSourceKind::Account,
                        &source_id,
                    )?;
                }
            }
            ConnectionTrashKind::Provider => {
                let home = row.home.clone();
                let agent_id = row.agent_id;
                let source_id = row.source_id.clone();
                self.connections.restore_trash(id)?;
                if home == TRASH_HOME_ROUTE_POOL {
                    self.route_pools.reattach_restored_pool_owned(
                        agent_id,
                        AdapterSourceKind::Provider,
                        &source_id,
                    )?;
                }
            }
        }
        Ok(())
    }

    pub fn env(&self) -> &EnvService {
        &self.env
    }

    pub fn agents(&self) -> &AgentService {
        &self.agents
    }

    pub fn providers(&self) -> &ProviderService {
        &self.providers
    }

    pub fn accounts(&self) -> &AccountService {
        &self.accounts
    }

    pub fn adapter_routes(&self) -> &AdapterRouteService {
        &self.adapter_routes
    }

    pub fn adapter_apply(&self) -> &AdapterApplyService {
        &self.adapter_apply
    }

    pub fn adapter_bridge(&self) -> &AdapterBridgeService {
        &self.adapter_bridge
    }

    pub fn tickets(&self) -> &TicketReadService {
        &self.tickets
    }

    pub fn ticket_bind(&self) -> &TicketBindService {
        &self.ticket_bind
    }

    pub fn backups(&self) -> &BackupService {
        &self.backups
    }

    pub fn skills(&self) -> &SkillService {
        &self.skills
    }

    pub fn settings(&self) -> &SettingsService {
        &self.settings
    }

    pub fn run(&self) -> &Arc<RunService> {
        &self.run
    }

    pub fn chat(&self) -> &ChatService {
        &self.chat
    }

    /// Write the live default model for Chat. Pi stores it in settings.json;
    /// other agents keep using the current login's config write path.
    pub fn set_live_chat_model(&self, agent: AgentId, model: &str) -> Result<()> {
        match agent {
            AgentId::Pi => {
                let _guard = self.backups.acquire_live_write(agent)?;
                crate::adapters::pi::set_pi_default_model(model)
            }
            _ => Err(error::AppError::Unsupported(
                "换模型请用当前登录的配置".into(),
            )),
        }
    }

    /// Read the live Chat model chip + picker for this agent.
    pub fn live_chat_model(&self, agent: AgentId) -> Result<models::LiveChatModel> {
        match agent {
            AgentId::Pi => Ok(crate::adapters::pi::pi_live_chat_model()),
            _ => Err(error::AppError::Unsupported(
                "换模型请用当前登录的配置".into(),
            )),
        }
    }

    pub fn projects(&self) -> &ProjectService {
        &self.projects
    }

    pub fn usage(&self) -> &UsageService {
        &self.usage
    }

    pub fn agent_visibility(&self) -> &AgentVisibilityService {
        &self.agent_visibility
    }

    pub fn route_pools(&self) -> &RoutePoolService {
        &self.route_pools
    }

    /// Isolated-adapter tests swap live-switch services after [`Self::open`].
    pub fn set_providers(&mut self, providers: ProviderService) {
        self.providers = providers;
    }

    /// Isolated-adapter tests swap live-switch services after [`Self::open`].
    pub fn set_accounts(&mut self, accounts: AccountService) {
        self.accounts = accounts;
    }

    fn live_backup_dir(&self, agent: AgentId) -> PathBuf {
        self.backups()
            .backups_root()
            .join("live")
            .join(agent.as_str())
    }

    /// Read-only account switch confirm facts. Does not snapshot, lock, or switch.
    pub fn account_switch_preview(
        &self,
        agent: AgentId,
        id_or_label: &str,
    ) -> Result<SwitchConfirmPreview> {
        let current = self
            .accounts()
            .list(Some(agent))?
            .into_iter()
            .find(|a| a.is_current);
        Ok(SwitchConfirmPreview {
            agent,
            target: id_or_label.to_string(),
            kind: SwitchConfirmKind::Account,
            current_label: current.map(|c| c.label),
            backup_dir: self.live_backup_dir(agent),
        })
    }

    /// Read-only provider switch confirm facts. Does not snapshot, lock, or switch.
    pub fn provider_switch_preview(
        &self,
        agent: AgentId,
        id_or_name: &str,
    ) -> Result<SwitchConfirmPreview> {
        let current = self
            .providers()
            .list(Some(agent))?
            .into_iter()
            .find(|p| p.is_current);
        Ok(SwitchConfirmPreview {
            agent,
            target: id_or_name.to_string(),
            kind: SwitchConfirmKind::Provider,
            current_label: current.map(|c| c.name),
            backup_dir: self.live_backup_dir(agent),
        })
    }

    pub fn adapter_secret_resolver(&self) -> AdapterSecretResolver {
        AdapterSecretResolver::new(self.db.clone())
    }

    /// Fan-out the same prompt to one or more agents (parallel or sequential).
    pub fn run_agents(
        &self,
        agents: &[AgentId],
        prompt: &str,
        opts: &RunOptions,
    ) -> Result<MultiRunReport> {
        self.run.run(agents, prompt, opts)
    }

    /// Search the configured skill market and mark already-installed listings.
    pub fn search_skill_market(&self, query: &str) -> Result<Vec<SkillListing>> {
        let source = self
            .settings
            .load()
            .map(|s| s.skill_market_source_parsed())
            .unwrap_or_default();
        crate::services::search_market(&self.skills, source, query)
    }

    /// Install a skills.sh / skillhub.cn listing into the shared library.
    pub fn install_market_listing(&self, skill_id: &str, overwrite: bool) -> Result<Skill> {
        crate::services::install_market_listing(&self.skills, skill_id, overwrite)
    }

    pub fn version() -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    /// Install a shared runtime (Node via winget/brew, or Linux remediations).
    /// Never fakes success without redetect.
    pub fn install_runtime(&self, id: RuntimeId, channel: &str) -> Result<InstallOutcome> {
        let outcome = install_runtime_system(id, channel)?;
        if outcome.ok {
            invalidate_runtime_latest_cache(&self.data_dir, id);
            if matches!(id, RuntimeId::NodeJs | RuntimeId::Npm) {
                invalidate_runtime_latest_cache(&self.data_dir, RuntimeId::NodeJs);
                invalidate_runtime_latest_cache(&self.data_dir, RuntimeId::Npm);
            }
        }
        Ok(outcome)
    }

    /// Install an agent via allowlisted channel (`npm` / `native`).
    ///
    /// Goes through [`LifecycleCoordinator`] (operation record + redetect).
    pub fn install_agent(
        &self,
        agent: AgentId,
        channel: &str,
        install_deps: bool,
    ) -> Result<InstallOutcome> {
        let key = AgentKey::from_agent_id(agent);
        self.install_agent_key(&key, channel, install_deps)
    }

    pub fn install_agent_key(
        &self,
        key: &AgentKey,
        channel: &str,
        install_deps: bool,
    ) -> Result<InstallOutcome> {
        self.lifecycle
            .install_agent_key(key, channel, install_deps, &SystemCommandExecutor, None)
    }

    /// Install an agent and retain operation id plus key-native observation.
    pub fn install_agent_detailed(
        &self,
        agent: AgentId,
        channel: &str,
        install_deps: bool,
    ) -> Result<LifecycleResult> {
        let key = AgentKey::from_agent_id(agent);
        self.install_agent_key_detailed(&key, channel, install_deps)
    }

    pub fn install_agent_key_detailed(
        &self,
        key: &AgentKey,
        channel: &str,
        install_deps: bool,
    ) -> Result<LifecycleResult> {
        self.lifecycle.install_agent_key_detailed(
            key,
            channel,
            install_deps,
            &SystemCommandExecutor,
            None,
        )
    }

    /// Upgrade an installed agent.
    pub fn upgrade_agent(&self, agent: AgentId) -> Result<InstallOutcome> {
        let key = AgentKey::from_agent_id(agent);
        self.upgrade_agent_key(&key)
    }

    pub fn upgrade_agent_key(&self, key: &AgentKey) -> Result<InstallOutcome> {
        let outcome = self
            .lifecycle
            .upgrade_agent_key(key, &SystemCommandExecutor, None)?;
        // Drop stale latest so the next check re-queries registry.
        if let Some(agent) = legacy_builtin_agent_id(key) {
            invalidate_latest_cache(&self.data_dir, agent);
        }
        Ok(outcome)
    }

    /// Upgrade an agent and retain operation id plus key-native observation.
    pub fn upgrade_agent_detailed(&self, agent: AgentId) -> Result<LifecycleResult> {
        let key = AgentKey::from_agent_id(agent);
        self.upgrade_agent_key_detailed(&key)
    }

    pub fn upgrade_agent_key_detailed(&self, key: &AgentKey) -> Result<LifecycleResult> {
        let result =
            self.lifecycle
                .upgrade_agent_key_detailed(key, &SystemCommandExecutor, None)?;
        if let Some(agent) = legacy_builtin_agent_id(key) {
            invalidate_latest_cache(&self.data_dir, agent);
        }
        Ok(result)
    }

    /// Uninstall agent when possible (`npm` global); optional config purge.
    pub fn uninstall_agent(&self, agent: AgentId, purge_config: bool) -> Result<InstallOutcome> {
        let key = AgentKey::from_agent_id(agent);
        self.uninstall_agent_key(&key, purge_config)
    }

    pub fn uninstall_agent_key(
        &self,
        key: &AgentKey,
        purge_config: bool,
    ) -> Result<InstallOutcome> {
        self.lifecycle
            .uninstall_agent_key(key, purge_config, &SystemCommandExecutor, None)
    }

    /// Uninstall an agent and retain operation id plus key-native observation.
    pub fn uninstall_agent_detailed(
        &self,
        agent: AgentId,
        purge_config: bool,
    ) -> Result<LifecycleResult> {
        let key = AgentKey::from_agent_id(agent);
        self.uninstall_agent_key_detailed(&key, purge_config)
    }

    pub fn uninstall_agent_key_detailed(
        &self,
        key: &AgentKey,
        purge_config: bool,
    ) -> Result<LifecycleResult> {
        self.lifecycle
            .uninstall_agent_key_detailed(key, purge_config, &SystemCommandExecutor, None)
    }

    /// Redetect-only repair lifecycle operation.
    pub fn repair_agent_detect(&self, agent: AgentId) -> Result<InstallOutcome> {
        let key = AgentKey::from_agent_id(agent);
        let mut outcome = self.repair_agent_detect_key(&key)?;
        // Legacy DTO compatibility: key-native observed state has no AgentId,
        // while the old facade promised a populated DetectResult.
        if outcome.agent.is_none() {
            outcome.agent = self.registry.get(agent).map(|adapter| adapter.detect());
        }
        Ok(outcome)
    }

    pub fn repair_agent_detect_key(&self, key: &AgentKey) -> Result<InstallOutcome> {
        Ok(self.lifecycle.repair_detect_key(key, None)?.outcome)
    }

    /// Run `f` while install log lines are forwarded to `hook` (GUI streaming).
    pub fn with_install_log_hook<R>(
        &self,
        hook: crate::services::InstallLogHook,
        f: impl FnOnce() -> R,
    ) -> R {
        crate::services::with_install_log_hook(hook, f)
    }

    /// Probe remote latest versions for agents (npm dist-tags, disk-cached).
    ///
    /// `agents = None` or empty → all known agents. `force` bypasses disk TTL cache.
    /// Agents with an npm package are compared even when installed via native channel.
    pub fn check_agent_updates(
        &self,
        agents: Option<&[AgentId]>,
        force: bool,
    ) -> Result<Vec<AgentUpdateInfo>> {
        probe_agent_updates(&self.registry, &self.data_dir, agents, force)
    }

    /// Probe official latest versions for shared runtimes (disk-cached).
    /// `runtimes = None` or empty means all runtimes detected on this host.
    pub fn check_runtime_updates(
        &self,
        runtimes: Option<&[RuntimeId]>,
        force: bool,
    ) -> Result<Vec<RuntimeUpdateInfo>> {
        probe_runtime_updates(&self.data_dir, &self.env.detect_all(), runtimes, force)
    }
}

fn legacy_builtin_agent_id(key: &AgentKey) -> Option<AgentId> {
    AgentId::ALL
        .iter()
        .copied()
        .find(|agent| agent.as_str() == key.as_str())
}

/// Doctor report payload (CLI / GUI shared shape).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DoctorReport {
    pub data_dir: String,
    pub runtimes: Vec<models::EnvStatus>,
    pub agents: Vec<models::DetectResult>,
    /// Static capability matrix (from adapters); not dependent on install state.
    pub capabilities: std::collections::BTreeMap<
        models::AgentId,
        std::collections::BTreeMap<models::Capability, models::CapabilityStateDto>,
    >,
    /// Usage parser health (DB row counts / support flags) — same shape as Dashboard footer.
    pub usage_health: Vec<models::ParserHealth>,
    pub paths: models::PathInfo,
    pub db_ok: bool,
    pub ok: bool,
    pub warnings: Vec<String>,
    pub version: String,
    /// Live-write lock files under `{data_dir}/locks` (`held` / `stale` / `malformed`).
    #[serde(default)]
    pub locks: Vec<utils::agent_lock::LockInspection>,
}

impl AgentHub {
    pub fn doctor(&self) -> DoctorReport {
        let runtimes = self.env.detect_all();
        let agents = self.agents.detect_all();
        let paths = self.settings.path_info();
        let db_ok = self.settings.db_ok().is_ok();
        let usage_health = self.usage.parser_health().unwrap_or_default();

        let mut warnings = Vec::new();
        for rt in &runtimes {
            if rt.status != models::EnvStatusKind::Ok {
                warnings.push(format!("runtime {} is {:?}", rt.id.as_str(), rt.status));
            }
        }
        for ag in &agents {
            if ag.status == models::DetectStatus::NotFound {
                warnings.push(format!("agent {} not installed", ag.agent));
            } else if !ag.env_ready {
                warnings.push(format!(
                    "agent {} installed but default channel env not ready",
                    ag.agent
                ));
            }
        }
        if !db_ok {
            warnings.push("database not writable/readable".into());
        }
        let locks = utils::agent_lock::inspect_locks(&self.data_dir.join("locks"));
        for lock in &locks {
            match lock.status.as_str() {
                "stale" => warnings.push(format!(
                    "stale live-write lock for {} (pid={})",
                    lock.agent,
                    lock.pid
                        .map(|p| p.to_string())
                        .unwrap_or_else(|| "-".into())
                )),
                "malformed" => warnings.push(format!(
                    "malformed live-write lock for {} ({})",
                    lock.agent,
                    lock.note.as_deref().unwrap_or("unreadable")
                )),
                "held" => warnings.push(format!(
                    "live-write lock held for {} (pid={})",
                    lock.agent,
                    lock.pid
                        .map(|p| p.to_string())
                        .unwrap_or_else(|| "-".into())
                )),
                _ => {}
            }
        }
        // Soft usage notes (never fail doctor)
        let supported = usage_health.iter().filter(|h| h.supported).count();
        let with_rows = usage_health.iter().filter(|h| h.records > 0).count();
        if supported > 0 && with_rows == 0 {
            warnings.push(
                "usage parsers ready but no rows collected yet — run `agenthub usage collect`"
                    .into(),
            );
        }
        for h in &usage_health {
            if let Some(rate) = h.fail_rate_pct {
                if rate >= 20.0 {
                    warnings.push(format!(
                        "usage parser {} fail rate ~{rate}% — log format may have drifted",
                        h.agent_id.as_str()
                    ));
                }
            }
        }

        let ok = db_ok; // hard failure only for db; runtime missing = warning
        let capabilities: std::collections::BTreeMap<_, _> = self
            .registry
            .matrix()
            .into_iter()
            .map(|(agent, row)| {
                (
                    agent,
                    row.into_iter()
                        .map(|(cap, state)| (cap, models::CapabilityStateDto::from(state)))
                        .collect(),
                )
            })
            .collect();
        tracing::debug!(
            target: logging::targets::CAPABILITY,
            module = logging::targets::CAPABILITY,
            op = "doctor_matrix",
            agents = capabilities.len(),
            capabilities = models::Capability::ALL.len(),
            "capability matrix attached to doctor report"
        );
        let report = DoctorReport {
            data_dir: self.data_dir.display().to_string(),
            runtimes,
            agents,
            capabilities,
            usage_health,
            paths,
            db_ok,
            ok,
            warnings,
            version: Self::version().into(),
            locks,
        };
        services::doctor_snapshot::save(&self.data_dir, &report);
        report
    }
}

#[cfg(test)]
mod tests;
