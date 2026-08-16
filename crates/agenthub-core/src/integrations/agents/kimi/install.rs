use std::path::PathBuf;
use std::sync::Arc;

use crate::integrations::shared::install::push_named_bins;
use crate::platform::install::{InstallContribution, OfficialVersionProbe};
use crate::platform::AgentKey;
use crate::utils::paths::home_dir;

struct KimiContrib;

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

pub fn register(ctx: &mut crate::integrations::IntegrationContext<'_>) {
    ctx.install
        .register(Arc::new(KimiContrib))
        .expect("unique built-in install contribution");
}
