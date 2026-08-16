use std::path::PathBuf;
use std::sync::Arc;

use crate::integrations::shared::install::push_named_bins;
use crate::platform::install::InstallContribution;
use crate::platform::AgentKey;
use crate::utils::paths::home_dir;

struct DshContrib;

impl InstallContribution for DshContrib {
    fn agent_key(&self) -> AgentKey {
        AgentKey::parse("dsh").expect("valid built-in agent key")
    }

    fn npm_package(&self) -> Option<&'static str> {
        Some(crate::adapters::dsh::NPM_PACKAGE)
    }

    fn npm_min_runtime_notes(&self) -> Option<&'static str> {
        Some("Node.js required; install uses the published dsh CLI")
    }

    fn native_uninstall_bin_paths(&self) -> Vec<PathBuf> {
        let mut paths = Vec::new();
        if let Ok(home) = home_dir() {
            push_named_bins(&mut paths, home.join(".local").join("bin"), "dsh");
            push_named_bins(&mut paths, home.join(".dsh").join("bin"), "dsh");
        }
        paths
    }
}

pub fn register(ctx: &mut crate::integrations::IntegrationContext<'_>) {
    ctx.install
        .register(Arc::new(DshContrib))
        .expect("unique built-in install contribution");
}
