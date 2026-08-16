use std::path::PathBuf;
use std::sync::Arc;

use crate::integrations::shared::install::push_named_bins;
use crate::platform::install::InstallContribution;
use crate::platform::AgentKey;
use crate::utils::paths::home_dir;

struct ClaudeContrib;

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

pub fn register(ctx: &mut crate::integrations::IntegrationContext<'_>) {
    ctx.install
        .register(Arc::new(ClaudeContrib))
        .expect("unique built-in install contribution");
}
