use std::sync::Arc;

use crate::integrations::IntegrationContext;
use crate::platform::install::InstallContribution;
use crate::platform::AgentKey;

use super::key;

struct DemoInstallContribution {
    key: AgentKey,
}

impl InstallContribution for DemoInstallContribution {
    fn agent_key(&self) -> AgentKey {
        self.key.clone()
    }

    fn npm_package(&self) -> Option<&'static str> {
        Some("@agenthub/demo-agent")
    }
}

pub fn register(ctx: &mut IntegrationContext<'_>) {
    ctx.install
        .register(Arc::new(DemoInstallContribution { key: key() }))
        .expect("unique demo-agent install contribution");
}
