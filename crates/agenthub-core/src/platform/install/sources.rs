//! Per-agent InstallContribution values (declarative allowlists).
//!
//! TODO(P13): move next to integrations/agents/<key>/.

use std::path::PathBuf;
use std::sync::Arc;

use super::contribution::{InstallContribution, NativeUninstallerSpec};
use super::probe::{OfficialVersionProbe, ScriptVersionKind};
use super::registry::InstallContributionRegistry;
use crate::platform::AgentKey;
use crate::utils::paths::home_dir;

fn push_named_bins(paths: &mut Vec<PathBuf>, dir: PathBuf, name: &str) {
    #[cfg(windows)]
    {
        paths.push(dir.join(format!("{name}.exe")));
        paths.push(dir.join(format!("{name}.cmd")));
    }
    paths.push(dir.join(name));
}

struct ClaudeContrib;
struct CodexContrib;
struct KimiContrib;
struct GrokContrib;
struct PiContrib;
struct WorkBuddyContrib;
struct CursorContrib;

impl InstallContribution for ClaudeContrib {
    fn agent_key(&self) -> AgentKey {
        AgentKey::parse("claude").expect("valid built-in agent key")
    }
    fn npm_package(&self) -> Option<&'static str> {
        Some("@anthropic-ai/claude-code")
    }
    fn native_ps1_url(&self) -> Option<&'static str> {
        Some("https://claude.ai/install.ps1")
    }
    fn native_sh_url(&self) -> Option<&'static str> {
        Some("https://claude.ai/install.sh")
    }
    fn native_uninstall_bin_paths(&self) -> Vec<PathBuf> {
        let mut paths = Vec::new();
        if let Ok(home) = home_dir() {
            push_named_bins(&mut paths, home.join(".local").join("bin"), "claude");
        }
        paths
    }
}

impl InstallContribution for CodexContrib {
    fn agent_key(&self) -> AgentKey {
        AgentKey::parse("codex").expect("valid built-in agent key")
    }
    fn npm_package(&self) -> Option<&'static str> {
        Some("@openai/codex")
    }
    fn native_ps1_url(&self) -> Option<&'static str> {
        Some("https://openai.com/codex/install.ps1")
    }
    fn prefer_npm_channel_first(&self) -> bool {
        true
    }
    fn native_uninstall_bin_paths(&self) -> Vec<PathBuf> {
        let mut paths = Vec::new();
        if let Ok(home) = home_dir() {
            push_named_bins(&mut paths, home.join(".local").join("bin"), "codex");
            push_named_bins(&mut paths, home.join(".codex").join("bin"), "codex");
        }
        paths
    }
}

impl InstallContribution for KimiContrib {
    fn agent_key(&self) -> AgentKey {
        AgentKey::parse("kimi").expect("valid built-in agent key")
    }
    fn npm_package(&self) -> Option<&'static str> {
        Some("@moonshot-ai/kimi-code")
    }
    fn native_ps1_url(&self) -> Option<&'static str> {
        Some("https://code.kimi.com/kimi-code/install.ps1")
    }
    fn native_sh_url(&self) -> Option<&'static str> {
        Some("https://code.kimi.com/kimi-code/install.sh")
    }
    fn official_version_probe(&self) -> Option<OfficialVersionProbe> {
        Some(OfficialVersionProbe::JsonVersion {
            url: "https://cdn.kimi.com/kimi-code/latest.json",
            source: "cdn",
        })
    }
    fn native_uninstall_bin_paths(&self) -> Vec<PathBuf> {
        let mut paths = Vec::new();
        if let Ok(home) = home_dir() {
            push_named_bins(&mut paths, home.join(".kimi-code").join("bin"), "kimi");
            push_named_bins(&mut paths, home.join(".kimi").join("bin"), "kimi");
        }
        paths
    }
}

impl InstallContribution for GrokContrib {
    fn agent_key(&self) -> AgentKey {
        AgentKey::parse("grok").expect("valid built-in agent key")
    }
    fn native_ps1_url(&self) -> Option<&'static str> {
        Some("https://x.ai/cli/install.ps1")
    }
    fn native_sh_url(&self) -> Option<&'static str> {
        Some("https://x.ai/cli/install.sh")
    }
    fn official_version_probe(&self) -> Option<OfficialVersionProbe> {
        Some(OfficialVersionProbe::PlainVersion {
            url: "https://storage.googleapis.com/grok-build-public-artifacts/cli/stable",
            source: "cdn:stable",
        })
    }
    fn native_uninstall_bin_paths(&self) -> Vec<PathBuf> {
        let mut paths = Vec::new();
        if let Ok(home) = home_dir() {
            push_named_bins(&mut paths, home.join(".grok").join("bin"), "grok");
            push_named_bins(&mut paths, home.join(".local").join("bin"), "grok");
        }
        paths
    }
}

impl InstallContribution for PiContrib {
    fn agent_key(&self) -> AgentKey {
        AgentKey::parse("pi").expect("valid built-in agent key")
    }
    fn npm_package(&self) -> Option<&'static str> {
        Some("@earendil-works/pi-coding-agent")
    }
    fn npm_install_extra_flags(&self) -> &'static [&'static str] {
        &["--ignore-scripts"]
    }
    fn native_uninstall_bin_paths(&self) -> Vec<PathBuf> {
        let mut paths = Vec::new();
        if let Ok(home) = home_dir() {
            push_named_bins(&mut paths, home.join(".local").join("bin"), "pi");
            push_named_bins(&mut paths, home.join(".pi").join("bin"), "pi");
            push_named_bins(&mut paths, home.join(".pi").join("agent").join("bin"), "pi");
        }
        paths
    }
}

impl InstallContribution for WorkBuddyContrib {
    fn agent_key(&self) -> AgentKey {
        AgentKey::parse("workbuddy").expect("valid built-in agent key")
    }
    fn native_setup_url(&self) -> Option<&'static str> {
        Some(crate::adapters::workbuddy::SETUP_URL)
    }
    fn native_uninstaller_specs(&self) -> Vec<NativeUninstallerSpec> {
        let mut out = Vec::new();
        if let Some(u) = crate::adapters::workbuddy::resolve_uninstaller() {
            out.push(NativeUninstallerSpec {
                program: u,
                args: vec!["/currentuser".into(), "/S".into()],
            });
        }
        out
    }
}

impl InstallContribution for CursorContrib {
    fn agent_key(&self) -> AgentKey {
        AgentKey::parse("cursor").expect("valid built-in agent key")
    }
    fn native_ps1_url(&self) -> Option<&'static str> {
        Some(crate::adapters::cursor::NATIVE_PS1_URL)
    }
    fn native_sh_url(&self) -> Option<&'static str> {
        Some(crate::adapters::cursor::NATIVE_SH_URL)
    }
    fn official_version_probe(&self) -> Option<OfficialVersionProbe> {
        Some(OfficialVersionProbe::ScriptVersion {
            url: crate::adapters::cursor::NATIVE_PS1_URL,
            source: "install-script",
            kind: ScriptVersionKind::CursorInstall,
        })
    }
    fn native_uninstall_bin_paths(&self) -> Vec<PathBuf> {
        let Ok(home) = home_dir() else {
            return Vec::new();
        };
        crate::adapters::cursor::uninstall_bin_candidates()
            .into_iter()
            .filter(|p| p.starts_with(&home))
            .collect()
    }
}

pub fn build_registry() -> InstallContributionRegistry {
    let mut reg = InstallContributionRegistry::new();
    reg.register(Arc::new(ClaudeContrib))
        .expect("unique built-in install contribution");
    reg.register(Arc::new(CodexContrib))
        .expect("unique built-in install contribution");
    reg.register(Arc::new(KimiContrib))
        .expect("unique built-in install contribution");
    reg.register(Arc::new(GrokContrib))
        .expect("unique built-in install contribution");
    reg.register(Arc::new(PiContrib))
        .expect("unique built-in install contribution");
    reg.register(Arc::new(WorkBuddyContrib))
        .expect("unique built-in install contribution");
    reg.register(Arc::new(CursorContrib))
        .expect("unique built-in install contribution");
    reg
}
