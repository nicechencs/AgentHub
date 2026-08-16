//! Agent integrations — one directory per agent key.
//!
//! Production platform registries (`builtin_*_registry`) are filled by
//! [`register_integrations`]. Adding a production agent means adding
//! `agents/<key>/` and one call here. Platform services and pages do not
//! grow new `match AgentId` arms.
//!
//! `AgentAdapter` still lives under [`crate::adapters`] (transitional
//! `adapter_facade` re-export). Sidecar / `agenthub-adapterd` is out of scope.

pub mod agents;
pub mod shared;

use std::sync::OnceLock;

use crate::platform::config::ConfigProjectorRegistry;
use crate::platform::detection::DetectorRegistry;
use crate::platform::install::InstallContributionRegistry;
use crate::platform::paths::AgentPathRegistry;
use crate::platform::projects::ProjectSourceRegistry;
use crate::platform::skills::SkillTargetRegistry;
use crate::platform::stream::StreamParserRegistry;
use crate::platform::usage::UsageSourceRegistry;

/// Mutable registries filled by each `agents/<key>::register`.
pub struct IntegrationContext<'a> {
    pub paths: &'a mut AgentPathRegistry,
    pub install: &'a mut InstallContributionRegistry,
    pub config: &'a mut ConfigProjectorRegistry,
    pub usage: &'a mut UsageSourceRegistry,
    pub stream: &'a mut StreamParserRegistry,
    pub projects: &'a mut ProjectSourceRegistry,
    pub detectors: &'a mut DetectorRegistry,
    pub skills: &'a mut SkillTargetRegistry,
}

/// Production contribution set (AgentId::ALL order).
pub struct ProductionIntegrations {
    pub paths: AgentPathRegistry,
    pub install: InstallContributionRegistry,
    pub config: ConfigProjectorRegistry,
    pub usage: UsageSourceRegistry,
    pub stream: StreamParserRegistry,
    pub projects: ProjectSourceRegistry,
    pub detectors: DetectorRegistry,
    pub skills: SkillTargetRegistry,
}

impl ProductionIntegrations {
    pub fn empty() -> Self {
        Self {
            paths: AgentPathRegistry::new(),
            install: InstallContributionRegistry::new(),
            config: ConfigProjectorRegistry::new(),
            usage: UsageSourceRegistry::new(),
            stream: StreamParserRegistry::new(),
            projects: ProjectSourceRegistry::new(),
            detectors: DetectorRegistry::new(),
            skills: SkillTargetRegistry::new(),
        }
    }

    pub fn as_context(&mut self) -> IntegrationContext<'_> {
        IntegrationContext {
            paths: &mut self.paths,
            install: &mut self.install,
            config: &mut self.config,
            usage: &mut self.usage,
            stream: &mut self.stream,
            projects: &mut self.projects,
            detectors: &mut self.detectors,
            skills: &mut self.skills,
        }
    }
}

/// Register every **production** agent into `ctx`.
///
/// Test-only agents (`demo-agent`) call their own `register` and never go
/// through this function.
pub fn register_integrations(ctx: &mut IntegrationContext<'_>) {
    agents::register_production(ctx);
}

/// Process-wide production integrations (single init; no re-entrant builtin_*).
pub fn production_integrations() -> &'static ProductionIntegrations {
    static ONCE: OnceLock<ProductionIntegrations> = OnceLock::new();
    ONCE.get_or_init(|| {
        let mut bundle = ProductionIntegrations::empty();
        let mut ctx = bundle.as_context();
        register_integrations(&mut ctx);
        bundle
    })
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
