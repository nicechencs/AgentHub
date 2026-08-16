use std::sync::Arc;

use crate::platform::install::{InstallContribution, NativeUninstallerSpec};
use crate::platform::AgentKey;

struct WorkBuddyContrib;

impl InstallContribution for WorkBuddyContrib {
    fn agent_key(&self) -> AgentKey {
        AgentKey::parse("workbuddy").expect("valid built-in agent key")
    }

    fn native_setup_url(&self) -> Option<&'static str> {
        Some(crate::adapters::workbuddy::SETUP_URL)
    }

    fn native_min_runtime_notes(&self) -> Option<&'static str> {
        Some(
            "Download WorkBuddySetup.exe from https://www.codebuddy.cn/work/ (no Node/npm required)",
        )
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

pub fn register(ctx: &mut crate::integrations::IntegrationContext<'_>) {
    ctx.install
        .register(Arc::new(WorkBuddyContrib))
        .expect("unique built-in install contribution");
}
