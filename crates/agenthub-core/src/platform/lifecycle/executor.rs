//! Narrow execution boundary for install-family lifecycle operations.

use crate::adapters::AdapterRegistry;
use crate::error::{AppError, Result};
use crate::models::{AgentId, InstallOutcome};
use crate::platform::install::InstallContribution;
use crate::platform::AgentKey;
use crate::utils::command_exec::CommandExecutor;

use super::{LifecycleError, OperationKind};

/// Executes lifecycle mutations after key-native resolve/capability checks.
///
/// Production currently delegates only the seven built-in agents to the
/// existing InstallService. Tests and future integrations can inject a narrow
/// implementation without requiring a full legacy AgentAdapter.
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
        command_executor: &dyn CommandExecutor,
    ) -> Result<InstallOutcome>;
}

#[derive(Clone)]
pub struct BuiltinLifecycleInstallExecutor {
    adapters: AdapterRegistry,
}

impl BuiltinLifecycleInstallExecutor {
    pub fn new(adapters: AdapterRegistry) -> Self {
        Self { adapters }
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

fn require_builtin(key: &AgentKey, kind: OperationKind) -> Result<AgentId> {
    legacy_builtin_agent_id(key)
        .ok_or_else(|| AppError::from(LifecycleError::unsupported(key, kind)))
}

impl LifecycleInstallExecutor for BuiltinLifecycleInstallExecutor {
    fn install(
        &self,
        key: &AgentKey,
        _contribution: &dyn InstallContribution,
        channel: &str,
        install_deps: bool,
        command_executor: &dyn CommandExecutor,
    ) -> Result<InstallOutcome> {
        let agent = require_builtin(key, OperationKind::Install)?;
        crate::services::install_service::install_agent(
            &self.adapters,
            agent,
            channel,
            install_deps,
            command_executor,
        )
    }

    fn upgrade(
        &self,
        key: &AgentKey,
        _contribution: &dyn InstallContribution,
        command_executor: &dyn CommandExecutor,
    ) -> Result<InstallOutcome> {
        let agent = require_builtin(key, OperationKind::Upgrade)?;
        crate::services::install_service::upgrade_agent(&self.adapters, agent, command_executor)
    }

    fn uninstall(
        &self,
        key: &AgentKey,
        _contribution: &dyn InstallContribution,
        purge_config: bool,
        command_executor: &dyn CommandExecutor,
    ) -> Result<InstallOutcome> {
        let agent = require_builtin(key, OperationKind::Uninstall)?;
        crate::services::install_service::uninstall_agent(
            &self.adapters,
            agent,
            purge_config,
            command_executor,
        )
    }
}
