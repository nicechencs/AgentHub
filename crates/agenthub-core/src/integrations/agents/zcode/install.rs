use std::sync::Arc;

use crate::platform::install::InstallContribution;
use crate::platform::AgentKey;

struct ZcodeContrib;

impl InstallContribution for ZcodeContrib {
    fn agent_key(&self) -> AgentKey {
        AgentKey::parse("zcode").expect("valid built-in agent key")
    }

    fn native_setup_url(&self) -> Option<&'static str> {
        Some(crate::adapters::zcode::SETUP_URL)
    }

    fn native_min_runtime_notes(&self) -> Option<&'static str> {
        Some("Download ZCode from https://zcode.z.ai/ (no Node/npm required for desktop)")
    }
}

pub fn register(ctx: &mut crate::integrations::IntegrationContext<'_>) {
    ctx.install
        .register(Arc::new(ZcodeContrib))
        .expect("unique built-in install contribution");
}
