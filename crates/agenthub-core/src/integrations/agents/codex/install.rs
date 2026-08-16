use std::path::PathBuf;
use std::sync::Arc;

use crate::integrations::shared::install::push_named_bins;
use crate::platform::install::InstallContribution;
use crate::platform::AgentKey;
use crate::utils::paths::home_dir;

struct CodexContrib;

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

pub fn register(ctx: &mut crate::integrations::IntegrationContext<'_>) {
    ctx.install
        .register(Arc::new(CodexContrib))
        .expect("unique built-in install contribution");
}
