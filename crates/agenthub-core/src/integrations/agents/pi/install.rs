use std::path::PathBuf;
use std::sync::Arc;

use crate::integrations::shared::install::push_named_bins;
use crate::platform::install::InstallContribution;
use crate::platform::AgentKey;
use crate::utils::paths::home_dir;

struct PiContrib;

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

    fn npm_min_runtime_notes(&self) -> Option<&'static str> {
        Some("Node.js >= 18; install uses --ignore-scripts")
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

pub fn register(ctx: &mut crate::integrations::IntegrationContext<'_>) {
    ctx.install
        .register(Arc::new(PiContrib))
        .expect("unique built-in install contribution");
}
