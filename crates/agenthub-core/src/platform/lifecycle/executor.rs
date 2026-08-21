//! Narrow execution boundary for install-family lifecycle operations.

use crate::adapters::AdapterRegistry;
use crate::error::{AppError, Result};
use crate::models::{AgentId, BackupKind, InstallOutcome};
use crate::platform::install::InstallContribution;
use crate::platform::AgentKey;
use crate::services::{BackupService, LiveWriteAuthority};
use crate::storage::Database;
use crate::utils::command_exec::CommandExecutor;
use std::path::Path;

/// Executes lifecycle mutations after key-native resolve/capability checks.
///
/// Production consumes [`InstallContribution`] allowlists for npm / native URLs /
/// flags / uninstallers. Built-in agents still redetect via adapters; unknown
/// keys execute contribution-driven allowlists without requiring `AgentId`.
pub trait LifecycleInstallExecutor: Send + Sync {
    fn install(
        &self,
        key: &AgentKey,
        contribution: &dyn InstallContribution,
        channel: &str,
        install_deps: bool,
        command_executor: &dyn CommandExecutor,
    ) -> Result<InstallOutcome>;

    fn upgrade(
        &self,
        key: &AgentKey,
        contribution: &dyn InstallContribution,
        command_executor: &dyn CommandExecutor,
    ) -> Result<InstallOutcome>;

    fn uninstall(
        &self,
        key: &AgentKey,
        contribution: &dyn InstallContribution,
        purge_config: bool,
        actual_data_dir: &Path,
        command_executor: &dyn CommandExecutor,
    ) -> Result<InstallOutcome>;
}

#[derive(Clone)]
pub struct BuiltinLifecycleInstallExecutor {
    adapters: AdapterRegistry,
    authority: LiveWriteAuthority,
    backups: BackupService,
}

impl BuiltinLifecycleInstallExecutor {
    pub fn new(db: &Database, adapters: AdapterRegistry) -> Self {
        let authority = LiveWriteAuthority::from_database(db);
        Self {
            backups: BackupService::new(
                db.clone(),
                adapters.clone(),
                authority.data_root().join("backups"),
            ),
            adapters,
            authority,
        }
    }
}

/// Explicit compatibility boundary. It intentionally performs exact matching
/// over the closed built-in set and never parses an arbitrary AgentKey.
pub(super) fn legacy_builtin_agent_id(key: &AgentKey) -> Option<AgentId> {
    AgentId::ALL
        .iter()
        .copied()
        .find(|agent| agent.as_str() == key.as_str())
}

fn require_contribution_matches(
    key: &AgentKey,
    contribution: &dyn InstallContribution,
) -> Result<()> {
    if contribution.agent_key() != *key {
        return Err(AppError::InvalidArg(format!(
            "install contribution key mismatch: expected {}, got {}",
            key.as_str(),
            contribution.agent_key().as_str()
        )));
    }
    Ok(())
}

impl LifecycleInstallExecutor for BuiltinLifecycleInstallExecutor {
    fn install(
        &self,
        key: &AgentKey,
        contribution: &dyn InstallContribution,
        channel: &str,
        install_deps: bool,
        command_executor: &dyn CommandExecutor,
    ) -> Result<InstallOutcome> {
        require_contribution_matches(key, contribution)?;
        if let Some(agent) = legacy_builtin_agent_id(key) {
            return crate::services::install_service::install_agent_with_contribution(
                &self.adapters,
                agent,
                contribution,
                channel,
                install_deps,
                command_executor,
            );
        }
        // Non-AgentId keys: contribution is the sole allowlist source.
        crate::services::install_service::install_from_contribution(
            key,
            contribution,
            channel,
            install_deps,
            command_executor,
        )
    }

    fn upgrade(
        &self,
        key: &AgentKey,
        contribution: &dyn InstallContribution,
        command_executor: &dyn CommandExecutor,
    ) -> Result<InstallOutcome> {
        require_contribution_matches(key, contribution)?;
        if let Some(agent) = legacy_builtin_agent_id(key) {
            return crate::services::install_service::upgrade_agent_with_contribution(
                &self.adapters,
                agent,
                contribution,
                command_executor,
            );
        }
        crate::services::install_service::upgrade_from_contribution(
            key,
            contribution,
            command_executor,
        )
    }

    fn uninstall(
        &self,
        key: &AgentKey,
        contribution: &dyn InstallContribution,
        purge_config: bool,
        actual_data_dir: &Path,
        command_executor: &dyn CommandExecutor,
    ) -> Result<InstallOutcome> {
        require_contribution_matches(key, contribution)?;
        if let Some(agent) = legacy_builtin_agent_id(key) {
            if purge_config {
                let guard = self.authority.acquire(agent)?;
                match self.backups.snapshot_with_guard(
                    &guard,
                    agent,
                    BackupKind::PreUninstall,
                    Some("pre-uninstall"),
                ) {
                    Ok(_) | Err(crate::error::AppError::NotFound(_)) => {}
                    Err(error) => return Err(error),
                }
                return crate::services::install_service::uninstall_agent_with_contribution_and_guard_at_data_dir(
                    &self.adapters,
                    &self.authority,
                    &guard,
                    actual_data_dir,
                    agent,
                    contribution,
                    true,
                    command_executor,
                );
            }
            return crate::services::install_service::uninstall_agent_with_contribution_and_authority_at_data_dir(
                &self.adapters,
                &self.authority,
                actual_data_dir,
                agent,
                contribution,
                purge_config,
                command_executor,
            );
        }
        crate::services::install_service::uninstall_from_contribution(
            key,
            contribution,
            purge_config,
            command_executor,
        )
    }
}
