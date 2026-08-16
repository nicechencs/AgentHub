use std::path::PathBuf;
use std::sync::Arc;

use crate::platform::install::{InstallContribution, OfficialVersionProbe, ScriptVersionKind};
use crate::platform::AgentKey;
use crate::utils::paths::home_dir;

struct CursorContrib;

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

    fn native_min_runtime_notes(&self) -> Option<&'static str> {
        Some(
            "Windows: irm 'https://cursor.com/install?win32=true' | iex; \
             macOS/Linux: curl https://cursor.com/install -fsS | bash \
             (installs Agent CLI, not Cursor IDE)",
        )
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

pub fn register(ctx: &mut crate::integrations::IntegrationContext<'_>) {
    ctx.install
        .register(Arc::new(CursorContrib))
        .expect("unique built-in install contribution");
}
