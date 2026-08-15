//! Declarative install contribution for one agent.

use std::path::PathBuf;

use crate::platform::AgentKey;

use super::probe::OfficialVersionProbe;

/// External uninstaller: program + structured argv (never a shell string).
#[derive(Debug, Clone)]
pub struct NativeUninstallerSpec {
    pub program: PathBuf,
    pub args: Vec<String>,
}

/// Agent-specific install metadata (no process execution).
pub trait InstallContribution: Send + Sync {
    fn agent_key(&self) -> AgentKey;

    fn npm_package(&self) -> Option<&'static str> {
        None
    }

    fn npm_install_extra_flags(&self) -> &'static [&'static str] {
        &[]
    }

    fn native_ps1_url(&self) -> Option<&'static str> {
        None
    }

    fn native_sh_url(&self) -> Option<&'static str> {
        None
    }

    fn native_setup_url(&self) -> Option<&'static str> {
        None
    }

    fn official_version_probe(&self) -> Option<OfficialVersionProbe> {
        None
    }

    /// Prefer npm before native in product channel list (Codex historical order).
    fn prefer_npm_channel_first(&self) -> bool {
        false
    }

    /// Detect / doctor note for the npm channel. Default matches historical adapters.
    fn npm_min_runtime_notes(&self) -> Option<&'static str> {
        Some("Node.js >= 18")
    }

    /// Detect / doctor note for the native channel.
    fn native_min_runtime_notes(&self) -> Option<&'static str> {
        None
    }

    fn native_uninstall_bin_paths(&self) -> Vec<PathBuf> {
        Vec::new()
    }

    fn native_uninstaller_specs(&self) -> Vec<NativeUninstallerSpec> {
        Vec::new()
    }
}
