//! Allowlisted install channels: npm package ids and official script URLs.
//!
//! Product data is owned by [`crate::platform::install::InstallContribution`].
//! This module is a compatibility façade for GUI/CLI catalog DTOs.

use serde::{Deserialize, Serialize};

use crate::models::{AgentId, InstallChannel, RuntimeId};
use crate::platform::install::builtin_install_registry;

// Re-export types under catalog path for existing imports.
pub use crate::platform::install::{OfficialVersionProbe, ScriptVersionKind};

/// Fixed npm packages — façade over install contributions.
pub fn npm_package(agent: AgentId) -> Option<&'static str> {
    builtin_install_registry()
        .get_agent_id(agent)
        .and_then(|c| c.npm_package())
}

/// Official version endpoint for agents without a usable public npm latest.
pub fn official_version_probe(agent: AgentId) -> Option<OfficialVersionProbe> {
    builtin_install_registry()
        .get_agent_id(agent)
        .and_then(|c| c.official_version_probe())
}

/// Extra npm install flags (after `install -g`).
pub fn npm_install_extra_flags(agent: AgentId) -> &'static [&'static str] {
    builtin_install_registry()
        .get_agent_id(agent)
        .map(|c| c.npm_install_extra_flags())
        .unwrap_or(&[])
}

/// Fixed official install.ps1 URLs (Windows native allowlist only).
pub fn native_ps1_url(agent: AgentId) -> Option<&'static str> {
    builtin_install_registry()
        .get_agent_id(agent)
        .and_then(|c| c.native_ps1_url())
}

/// Fixed official install.sh URLs (macOS/Linux native allowlist only).
#[cfg_attr(windows, allow(dead_code))]
pub fn native_sh_url(agent: AgentId) -> Option<&'static str> {
    builtin_install_registry()
        .get_agent_id(agent)
        .and_then(|c| c.native_sh_url())
}

/// Official Setup landing page for agents without scripted installers.
pub fn native_setup_url(agent: AgentId) -> Option<&'static str> {
    builtin_install_registry()
        .get_agent_id(agent)
        .and_then(|c| c.native_setup_url())
}

/// One install channel shown in the GUI (command is display / copy-paste only).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct InstallChannelPlan {
    pub id: String,
    pub label: String,
    /// Human-facing install command or setup URL (platform-aware).
    pub command: String,
    pub requires: Vec<RuntimeId>,
}

/// Per-agent install catalog row for GUI / CLI.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentInstallCatalogEntry {
    pub agent_id: AgentId,
    pub channels: Vec<InstallChannelPlan>,
}

/// Build the full install catalog (stable order = [`AgentId::ALL`]).
pub fn list_install_catalog() -> Vec<AgentInstallCatalogEntry> {
    AgentId::ALL
        .iter()
        .copied()
        .map(|agent_id| AgentInstallCatalogEntry {
            agent_id,
            channels: channels_for(agent_id),
        })
        .collect()
}

/// Adapter detect/doctor channels — same id order as [`channels_for`].
///
/// Labels and presence come from the install catalog; runtime notes come from
/// [`crate::platform::install::InstallContribution`].
pub fn adapter_install_channels(agent: AgentId) -> Vec<InstallChannel> {
    let contrib = builtin_install_registry().get_agent_id(agent);
    channels_for(agent)
        .into_iter()
        .map(|plan| {
            let notes = match plan.id.as_str() {
                "npm" => contrib.as_ref().and_then(|c| c.npm_min_runtime_notes()),
                "native" => contrib.as_ref().and_then(|c| c.native_min_runtime_notes()),
                _ => None,
            };
            InstallChannel {
                id: plan.id,
                label: plan.label,
                requires: plan.requires,
                min_runtime_notes: notes.map(str::to_string),
            }
        })
        .collect()
}

/// Install channels for one agent, product order (matches historical GUI).
pub fn channels_for(agent: AgentId) -> Vec<InstallChannelPlan> {
    let mut channels = Vec::new();
    let prefer_npm = builtin_install_registry()
        .get_agent_id(agent)
        .map(|c| c.prefer_npm_channel_first())
        .unwrap_or(false);
    if prefer_npm {
        push_npm(&mut channels, agent);
        push_native(&mut channels, agent);
    } else {
        push_native(&mut channels, agent);
        push_npm(&mut channels, agent);
    }
    channels
}

fn push_npm(out: &mut Vec<InstallChannelPlan>, agent: AgentId) {
    let Some(pkg) = npm_package(agent) else {
        return;
    };
    let extra = npm_install_extra_flags(agent);
    let flags = if extra.is_empty() {
        String::new()
    } else {
        format!("{} ", extra.join(" "))
    };
    out.push(InstallChannelPlan {
        id: "npm".into(),
        label: format!("npm {pkg}"),
        command: format!("npm i -g {flags}{pkg}"),
        requires: vec![RuntimeId::NodeJs, RuntimeId::Npm],
    });
}

fn push_native(out: &mut Vec<InstallChannelPlan>, agent: AgentId) {
    if let Some(url) = native_setup_url(agent) {
        out.push(InstallChannelPlan {
            id: "native".into(),
            label: "官网 Setup（打开安装页）".into(),
            command: url.into(),
            requires: vec![],
        });
        return;
    }

    let (label, command) = native_command_display(agent);
    let Some(command) = command else {
        return;
    };
    out.push(InstallChannelPlan {
        id: "native".into(),
        label: label.into(),
        command,
        requires: native_runtime_requirements(agent),
    });
}

/// Runtime requirements for the native channel are platform-specific.
///
/// A Unix/macOS `install.sh` is executed directly by the POSIX shell and must
/// never advertise PowerShell as a prerequisite.  Windows keeps the historical
/// PowerShell requirement for the allowlisted `.ps1` installers.
fn native_runtime_requirements(agent: AgentId) -> Vec<RuntimeId> {
    let _ = agent;
    crate::runtime::native_install_requires()
}

fn native_command_display(agent: AgentId) -> (&'static str, Option<String>) {
    let label = if agent == AgentId::Cursor {
        "Cursor Agent CLI 官方脚本"
    } else {
        "native 官方脚本"
    };
    #[cfg(windows)]
    {
        if let Some(url) = native_ps1_url(agent) {
            let cmd = if url.contains('?') || url.contains('\'') {
                format!("irm '{url}' | iex")
            } else {
                format!("irm {url} | iex")
            };
            return (label, Some(cmd));
        }
        (label, None)
    }
    #[cfg(not(windows))]
    {
        if let Some(url) = native_sh_url(agent) {
            return (label, Some(format!("curl -fsS {url} | bash")));
        }
        // A Windows-only ps1 URL is not a Unix/macOS install channel.  Do not
        // expose a PowerShell command in the native catalog on these platforms.
        (label, None)
    }
}

#[cfg(test)]
mod tests;
