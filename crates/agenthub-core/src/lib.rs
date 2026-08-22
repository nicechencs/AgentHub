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

use std::path::{Path, PathBuf};
use std::sync::Arc;

use adapters::{register_all, AdapterRegistry};
use error::Result;
use models::{
    AgentId, AgentUpdateInfo, InstallOutcome, MultiRunReport, RunOptions, RuntimeId, Skill,
    SkillListing,
};
use platform::{LifecycleCoordinator, LifecycleResult};
use services::{
    check_agent_updates as probe_agent_updates, install_runtime_system, invalidate_latest_cache,
    AccountService, AdapterApplyService, AdapterBridgeService, AdapterRouteService, AgentService,
    AgentVisibilityService, BackupService, ChatService, ConnectionService, EnvService,
    ProjectService, ProviderService, RunService, SettingsService, SkillService, TicketBindService,
    TicketReadService, UsageService,
};
use logging::targets;
use storage::{ChatRepo, Database};
use utils::command_exec::SystemCommandExecutor;
use utils::paths::{
    backups_dir, db_path, ensure_data_layout, home_dir, normalize_data_dir, resolve_data_dir,
};

// Re-export catalog + configuration types for GUI and CLI shells.
pub use platform::{
    AgentCatalogService, AgentConfigSchema, AgentDescriptor, AgentKey, ConfigApplyResult,
    ConfigChangePlan, ConfigValidationResult, ConfigurationService, InstallChannelDescriptor,
    NormalizedConfigDocument, SECRET_REDACTED,
};

/// Application facade shared by CLI and (future) GUI.
pub struct AgentHub {
    pub data_dir: PathBuf,
    pub db: Database,
    pub registry: AdapterRegistry,
    /// Read-only agent directory (key / capabilities / install channels).
    pub catalog: AgentCatalogService,
    /// Install-family lifecycle (operation records + redetect).
    pub lifecycle: LifecycleCoordinator,
    /// Native config schema / read / validate / apply (projectors).
    pub configuration: ConfigurationService,
    /// Active account/provider binding (unique current pointer per agent).
    pub connections: ConnectionService,
    pub env: EnvService,
    pub agents: AgentService,
    pub providers: ProviderService,
    pub accounts: AccountService,
    /// Read-only compatibility route analysis. Never applies configuration.
    pub adapter_routes: AdapterRouteService,
    /// Applies the one supported Kimi membership -> Claude adapter projection.
    pub adapter_apply: AdapterApplyService,
    /// Prepares/persists the Kimi membership -> Codex bridge saga. The desktop
    /// host owns listener lifetime and live configuration switching.
    pub adapter_bridge: AdapterBridgeService,
    /// Read-only Ticket / Binding wallet aggregation + plan(ticket, agent).
    pub tickets: TicketReadService,
    /// Ticket bind / unbind write API. Codex bridge bind stays on the host.
    pub ticket_bind: TicketBindService,
    pub backups: BackupService,
    pub skills: SkillService,
    pub settings: SettingsService,
    pub run: Arc<RunService>,
    pub chat: ChatService,
    pub projects: ProjectService,
    pub usage: UsageService,
    /// Soft-hide preference (UI only; detect / install unchanged).
    pub agent_visibility: AgentVisibilityService,
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
        let data_dir = normalize_data_dir(&resolve_data_dir(data_dir_override)?)?;
        ensure_data_layout(&data_dir)?;
        // STORAGE module logs open success/failure (including migrate).
        let db = Database::open(&db_path(&data_dir))?;
        // Recover audit rows left running after crash (never auto-retry dangerous steps).
        let _ = LifecycleCoordinator::interrupt_stale_running(&db);
        // Recover chat placeholders left running after crash. Cancelled is not a hard failure.
        if let Ok(n) = ChatRepo::new(db.clone()).interrupt_stale_running() {
            if n > 0 {
                logging::log_warn(
                    targets::CHAT,
                    "chat_interrupt",
                    &format!("marked {n} running chat messages as cancelled after restart"),
                );
            }
        }
        let registry = register_all();
        let catalog = AgentCatalogService::from_registry(&registry)?;
        let lifecycle = LifecycleCoordinator::new_with_data_dir(
            db.clone(),
            registry.clone(),
            data_dir.clone(),
        );
        let configuration = ConfigurationService::new(db.clone());
        let connections = ConnectionService::new(db.clone());
        // AgentService keeps a cheap Arc clone of the same adapters; do not call register_all twice.
        let agents = AgentService::new(registry.clone());
        let run = Arc::new(RunService::new(registry.clone()));
        let chat = ChatService::new(db.clone(), Arc::clone(&run));
        let providers =
            ProviderService::with_live(db.clone(), registry.clone(), backups_dir(&data_dir));
        let accounts =
            AccountService::with_live(db.clone(), registry.clone(), backups_dir(&data_dir));
        let adapter_routes = AdapterRouteService::new(db.clone());
        let adapter_apply = AdapterApplyService::from_parts(
            adapter_routes.clone(),
            crate::storage::AdapterProfileRepo::new(db.clone()),
            providers.clone(),
            crate::services::AdapterSecretResolver::new(db.clone()),
        );
        let adapter_bridge = AdapterBridgeService::new(db.clone());
        let tickets = TicketReadService::new(db.clone());
        let ticket_bind = TicketBindService::from_parts(
            tickets.clone(),
            adapter_apply.clone(),
            crate::storage::AdapterProfileRepo::new(db.clone()),
            providers.clone(),
            accounts.clone(),
        );
        let backups = BackupService::new(db.clone(), registry.clone(), backups_dir(&data_dir));
        let skills_root = match skills_root {
            Some(path) => path.to_path_buf(),
            None => home_dir()?.join(".agents").join("skills"),
        };
        let skills = SkillService::with_db_and_target_registry(
            skills_root,
            registry.clone(),
            db.clone(),
            crate::platform::skills::builtin_skill_target_registry().clone(),
        );
        // Recover a durable package commit before exposing any skill operation.
        // This is deliberately narrower than bootstrap_assignments(): startup
        // must not import projections or mutate assignment intent implicitly.
        skills.recover_pending_commit()?;
        let settings = SettingsService::new(data_dir.clone(), db.clone());
        let projects = ProjectService::new(registry.clone(), data_dir.clone());
        let agent_visibility = AgentVisibilityService::new(data_dir.clone());
        let usage =
            UsageService::with_live_scope(db.clone(), agent_visibility.clone(), agents.clone());
        tracing::info!(
            target: logging::targets::BOOT,
            module = logging::targets::BOOT,
            op = "open",
            data_dir = %data_dir.display(),
            "AgentHub opened"
        );
        Ok(Self {
            data_dir,
            db,
            registry,
            catalog,
            lifecycle,
            configuration,
            connections,
            env: EnvService::new(),
            agents,
            providers,
            accounts,
            adapter_routes,
            adapter_apply,
            adapter_bridge,
            tickets,
            ticket_bind,
            backups,
            skills,
            settings,
            run,
            chat,
            projects,
            usage,
            agent_visibility,
        })
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
        install_runtime_system(id, channel)
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
        DoctorReport {
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
        }
    }
}

#[cfg(test)]
mod tests;
