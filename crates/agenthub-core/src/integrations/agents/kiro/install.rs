use std::path::PathBuf;
use std::sync::Arc;

use crate::platform::install::InstallContribution;
use crate::platform::AgentKey;
use crate::utils::paths::home_dir;

struct KiroContrib;

impl InstallContribution for KiroContrib {
    fn agent_key(&self) -> AgentKey {
        AgentKey::parse("kiro").expect("valid built-in agent key")
    }

    fn native_ps1_url(&self) -> Option<&'static str> {
        Some(crate::adapters::kiro::NATIVE_PS1_URL)
    }

    fn native_sh_url(&self) -> Option<&'static str> {
        Some(crate::adapters::kiro::NATIVE_SH_URL)
    }

    fn native_min_runtime_notes(&self) -> Option<&'static str> {
        Some(
            "Windows: irm 'https://cli.kiro.dev/install.ps1' | iex; \
             macOS/Linux: curl https://cli.kiro.dev/install -fsS | bash \
             (installs kiro-cli, not the Kiro editor)",
        )
    }

    fn native_uninstall_bin_paths(&self) -> Vec<PathBuf> {
        let Ok(home) = home_dir() else {
            return Vec::new();
        };
        crate::adapters::kiro::uninstall_bin_candidates()
            .into_iter()
            .filter(|p| p.starts_with(&home))
            .collect()
    }
}

pub fn register(ctx: &mut crate::integrations::IntegrationContext<'_>) {
    ctx.install
        .register(Arc::new(KiroContrib))
        .expect("unique built-in install contribution");
}
