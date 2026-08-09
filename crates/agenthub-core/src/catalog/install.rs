//! Allowlisted install channels: npm package ids and official script URLs.
//!
//! Product data is owned by [`crate::platform::install::InstallContribution`].
//! This module is a compatibility façade for GUI/CLI catalog DTOs.

use serde::{Deserialize, Serialize};

use crate::models::{AgentId, RuntimeId};
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
    #[cfg(windows)]
    {
        let _ = agent;
        vec![RuntimeId::PowerShell]
    }
    #[cfg(not(windows))]
    {
        // Setup pages and official sh installers are self-contained.  A
        // Windows-only ps1 URL is intentionally not offered on Unix/macOS by
        // `native_command_display`, so it cannot leak a PowerShell dependency.
        let _ = agent;
        vec![]
    }
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
mod tests {
    use super::*;

    #[test]
    fn catalog_covers_every_agent_with_a_plan() {
        for agent in AgentId::ALL {
            let has_plan = npm_package(agent).is_some()
                || native_ps1_url(agent).is_some()
                || native_sh_url(agent).is_some()
                || native_setup_url(agent).is_some();
            assert!(has_plan, "agent {} has no install channel", agent.as_str());
            assert!(
                !channels_for(agent).is_empty(),
                "agent {} has empty channel list",
                agent.as_str()
            );
        }
        assert_eq!(list_install_catalog().len(), AgentId::ALL.len());
    }

    #[test]
    fn list_install_catalog_commands_reference_allowlist() {
        for entry in list_install_catalog() {
            for ch in &entry.channels {
                assert!(
                    !ch.command.trim().is_empty(),
                    "{:?} empty command",
                    entry.agent_id
                );
                if ch.id == "npm" {
                    let pkg = npm_package(entry.agent_id).expect("npm channel needs package");
                    assert!(ch.command.contains(pkg));
                }
                if ch.id == "native" {
                    if let Some(url) = native_setup_url(entry.agent_id) {
                        assert_eq!(ch.command, url);
                    } else if let Some(url) = native_ps1_url(entry.agent_id) {
                        assert!(
                            ch.command.contains(url) || native_sh_url(entry.agent_id).is_some(),
                            "command {} should reference allowlisted URL for {:?}",
                            ch.command,
                            entry.agent_id
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn pi_and_cursor_and_workbuddy_edges() {
        assert_eq!(
            npm_package(AgentId::Pi),
            Some("@earendil-works/pi-coding-agent")
        );
        assert_eq!(npm_install_extra_flags(AgentId::Pi), &["--ignore-scripts"]);
        assert!(npm_package(AgentId::WorkBuddy).is_none());
        assert!(native_ps1_url(AgentId::WorkBuddy).is_none());
        assert!(native_setup_url(AgentId::WorkBuddy).is_some());
        assert!(npm_package(AgentId::Cursor).is_none());
        assert_eq!(
            native_ps1_url(AgentId::Cursor),
            Some(crate::adapters::cursor::NATIVE_PS1_URL)
        );
        let cursor_probe = official_version_probe(AgentId::Cursor).expect("cursor install script");
        assert!(matches!(
            cursor_probe,
            OfficialVersionProbe::ScriptVersion {
                kind: ScriptVersionKind::CursorInstall,
                ..
            }
        ));
    }

    #[cfg(not(windows))]
    #[test]
    fn unix_native_shell_channels_do_not_require_powershell() {
        for agent in AgentId::ALL {
            for channel in channels_for(agent) {
                if channel.id == "native" && native_sh_url(agent).is_some() {
                    assert!(!channel.requires.contains(&RuntimeId::PowerShell));
                    assert!(!channel.command.contains("irm "));
                    assert!(!channel.command.contains("PowerShell"));
                }
            }
        }
    }

    #[test]
    fn kimi_and_grok_version_sources() {
        assert_eq!(npm_package(AgentId::Kimi), Some("@moonshot-ai/kimi-code"));
        assert!(npm_package(AgentId::Grok).is_none());
        let kimi_probe = official_version_probe(AgentId::Kimi).expect("kimi cdn");
        assert!(matches!(
            kimi_probe,
            OfficialVersionProbe::JsonVersion { .. }
        ));
        let grok_probe = official_version_probe(AgentId::Grok).expect("grok cdn");
        assert!(matches!(
            grok_probe,
            OfficialVersionProbe::PlainVersion { .. }
        ));
        assert_eq!(
            native_ps1_url(AgentId::Kimi),
            Some("https://code.kimi.com/kimi-code/install.ps1")
        );
        assert!(!native_ps1_url(AgentId::Kimi).unwrap().contains("kimi-cli"));
    }

    #[test]
    fn codex_orders_npm_before_native() {
        let ch = channels_for(AgentId::Codex);
        assert_eq!(ch[0].id, "npm");
        #[cfg(windows)]
        {
            assert!(ch.len() >= 2);
            assert_eq!(ch[1].id, "native");
        }
        #[cfg(not(windows))]
        {
            // Codex currently publishes only a Windows PowerShell script;
            // don't expose that as a Unix/macOS native channel.
            assert_eq!(ch.len(), 1);
        }
    }
}
