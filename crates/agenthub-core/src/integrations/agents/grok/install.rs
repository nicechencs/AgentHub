use std::path::PathBuf;
use std::sync::Arc;

use crate::integrations::shared::install::push_named_bins;
use crate::platform::install::{InstallContribution, OfficialVersionProbe};
use crate::platform::AgentKey;
use crate::utils::paths::home_dir;

struct GrokContrib;

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

pub fn register(ctx: &mut crate::integrations::IntegrationContext<'_>) {
    ctx.install
        .register(Arc::new(GrokContrib))
        .expect("unique built-in install contribution");
}
