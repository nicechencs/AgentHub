use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::integrations::IntegrationContext;
use crate::models::DetectStatus;
use crate::platform::detection::AgentDetector;
use crate::platform::lifecycle::InstallationObserved;
use crate::platform::AgentKey;

use super::key;

struct DemoDetector {
    key: AgentKey,
    installed: Arc<AtomicBool>,
}

impl AgentDetector for DemoDetector {
    fn agent_key(&self) -> AgentKey {
        self.key.clone()
    }

    fn detect(&self) -> InstallationObserved {
        let installed = self.installed.load(Ordering::SeqCst);
        InstallationObserved {
            status: if installed {
                DetectStatus::Installed
            } else {
                DetectStatus::NotFound
            },
            version: installed.then(|| "0.0.1-demo".into()),
            binary_path: None,
            channel: Some("npm".into()),
            notes: vec!["demo-agent detector".into()],
        }
    }
}

pub fn register(ctx: &mut IntegrationContext<'_>, installed: Arc<AtomicBool>) {
    ctx.detectors
        .register(Arc::new(DemoDetector {
            key: key(),
            installed,
        }))
        .expect("unique demo-agent detector");
}
