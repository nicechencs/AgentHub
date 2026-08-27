//! AgentHub startup composition behind [`crate::AgentHub::open`].
//!
//! Path layout, storage open, crash recovery, one [`register_all`], catalog
//! snapshot, then service assembly. Does not create a loopback
//! [`crate::bridge::BridgeRuntimeHost`]. Install / upgrade / uninstall stay on
//! the [`crate::AgentHub`] façade.

use std::path::Path;
use std::sync::Arc;

use crate::adapters::register_all;
use crate::error::Result;
use crate::logging::{self, targets};
use crate::platform::{AgentCatalogService, ConfigurationService, LifecycleCoordinator};
use crate::services::{
    AccountService, AdapterApplyService, AdapterBridgeService, AdapterRouteService, AgentService,
    AgentVisibilityService, BackupService, ChatService, ConnectionService, EnvService,
    ProjectService, ProviderService, RoutePoolService, RunService, SettingsService, SkillService,
    TicketBindService, TicketReadService, UsageService,
};
use crate::storage::{ChatRepo, Database};
use crate::utils::paths::{
    backups_dir, db_path, ensure_data_layout, home_dir, normalize_data_dir, resolve_data_dir,
};
use crate::AgentHub;

pub(crate) fn open_with_skills_root(
    data_dir_override: Option<&Path>,
    skills_root: Option<&Path>,
) -> Result<AgentHub> {
    let data_dir = normalize_data_dir(&resolve_data_dir(data_dir_override)?)?;
    ensure_data_layout(&data_dir)?;
    // STORAGE module logs open success/failure (including migrate).
    let db = Database::open(&db_path(&data_dir))?;
    recover_stale_lifecycle(&db);
    recover_stale_chat(&db);
    let registry = register_all();
    let catalog = AgentCatalogService::from_registry(&registry)?;
    let lifecycle =
        LifecycleCoordinator::new_with_data_dir(db.clone(), registry.clone(), data_dir.clone());
    let configuration = ConfigurationService::new(db.clone());
    let connections = ConnectionService::new(db.clone());
    // AgentService keeps a cheap Arc clone of the same adapters; do not call register_all twice.
    let agents = AgentService::new(registry.clone());
    let run = Arc::new(RunService::new(registry.clone()));
    let chat = ChatService::new(db.clone(), Arc::clone(&run));
    let providers =
        ProviderService::with_live(db.clone(), registry.clone(), backups_dir(&data_dir));
    let accounts = AccountService::with_live(db.clone(), registry.clone(), backups_dir(&data_dir));
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
    recover_pending_skill_commit(&skills)?;
    let settings = SettingsService::new(data_dir.clone(), db.clone());
    let projects = ProjectService::new(registry.clone(), data_dir.clone());
    let agent_visibility = AgentVisibilityService::new(data_dir.clone());
    let usage = UsageService::with_live_scope(db.clone(), agent_visibility.clone(), agents.clone());
    let route_pools = RoutePoolService::new(db.clone());
    tracing::info!(
        target: logging::targets::BOOT,
        module = logging::targets::BOOT,
        op = "open",
        data_dir = %data_dir.display(),
        "AgentHub opened"
    );
    Ok(AgentHub {
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
        route_pools,
    })
}

fn recover_stale_lifecycle(db: &Database) {
    // Recover audit rows left running after crash (never auto-retry dangerous steps).
    let _ = LifecycleCoordinator::interrupt_stale_running(db);
}

fn recover_stale_chat(db: &Database) {
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
}

fn recover_pending_skill_commit(skills: &SkillService) -> Result<()> {
    // Recover a durable package commit before exposing any skill operation.
    // This is deliberately narrower than bootstrap_assignments(): startup
    // must not import projections or mutate assignment intent implicitly.
    skills.recover_pending_commit()
}
