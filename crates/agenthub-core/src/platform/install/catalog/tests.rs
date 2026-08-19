use super::*;
use crate::models::RuntimeId;

#[test]
fn adapter_install_channels_match_catalog_ids_requires_and_notes() {
    let registry = crate::adapters::register_all();
    for agent in AgentId::ALL {
        let adapter = registry
            .get(agent)
            .unwrap_or_else(|| panic!("missing adapter {}", agent.as_str()));
        let adapter_channels = adapter.install_channels();
        let catalog_channels = channels_for(agent);
        let expected = adapter_install_channels(agent);
        assert_eq!(
            adapter_channels,
            expected,
            "adapter.install_channels drifted from catalog for {}",
            agent.as_str()
        );
        let adapter_ids: Vec<&str> = adapter_channels.iter().map(|ch| ch.id.as_str()).collect();
        let catalog_ids: Vec<&str> = catalog_channels.iter().map(|ch| ch.id.as_str()).collect();
        assert_eq!(
            adapter_ids,
            catalog_ids,
            "install channel ids drifted for {}",
            agent.as_str()
        );
        for (adapter_ch, catalog_ch) in adapter_channels.iter().zip(catalog_channels.iter()) {
            assert_eq!(
                adapter_ch.requires,
                catalog_ch.requires,
                "{}",
                agent.as_str()
            );
            assert_eq!(adapter_ch.label, catalog_ch.label, "{}", agent.as_str());
        }
    }
}

#[test]
fn adapter_install_channel_notes_come_from_contribution() {
    let pi = adapter_install_channels(AgentId::Pi);
    assert_eq!(pi.len(), 1);
    assert_eq!(pi[0].id, "npm");
    assert_eq!(
        pi[0].min_runtime_notes.as_deref(),
        Some("Node.js >= 18; install uses --ignore-scripts")
    );

    let cursor = adapter_install_channels(AgentId::Cursor);
    assert_eq!(cursor.len(), 1);
    assert_eq!(cursor[0].id, "native");
    assert!(cursor[0]
        .min_runtime_notes
        .as_deref()
        .unwrap_or_default()
        .contains("cursor.com/install"));

    let workbuddy = adapter_install_channels(AgentId::WorkBuddy);
    assert_eq!(workbuddy[0].id, "native");
    assert!(workbuddy[0].requires.is_empty());
    assert!(workbuddy[0]
        .min_runtime_notes
        .as_deref()
        .unwrap_or_default()
        .contains("codebuddy.cn"));

    let dsh = adapter_install_channels(AgentId::Dsh);
    assert_eq!(dsh[0].id, "npm");
    assert_eq!(
        dsh[0].min_runtime_notes.as_deref(),
        Some("Node.js required; install uses the published dsh CLI")
    );
}

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
    let adapter = adapter_install_channels(AgentId::Codex);
    assert_eq!(adapter[0].id, "npm");
    #[cfg(windows)]
    {
        assert!(ch.len() >= 2);
        assert_eq!(ch[1].id, "native");
        assert_eq!(adapter.len(), ch.len());
    }
    #[cfg(not(windows))]
    {
        // Codex currently publishes only a Windows PowerShell script;
        // don't expose that as a Unix/macOS native channel.
        assert_eq!(ch.len(), 1);
        assert_eq!(adapter.len(), 1);
    }
}
