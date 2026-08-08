//! Platform capability modules (modular monolith boundaries).
//!
//! New platform services land here as they are extracted from the legacy
//! adapter/service layout. See `docs/platform-capability-refactor.md`.

pub mod agent_catalog;
pub mod config;
pub mod detection;
pub mod install;
pub mod lifecycle;
pub mod paths;
pub mod projects;
pub mod skills;
pub mod stream;
pub mod usage;

pub use skills::{
    bootstrap_skill_assignments, AdapterSkillTarget, AgentSkillTarget, SkillAssignmentService,
    SkillBootstrapReport, SkillPackageService, SkillReconciler, SkillSourceService,
    SkillTargetRegistry, StaticSkillTarget,
};

pub use agent_catalog::{
    parse_agent_key, AgentCatalogService, AgentDescriptor, AgentKey, AgentKeyError,
    InstallChannelDescriptor,
};
pub use config::{
    builtin_config_registry, AgentConfigProjector, AgentConfigSchema, ConfigApplyResult,
    ConfigChangePlan, ConfigProjectorRegistry, ConfigValidationResult, ConfigurationService,
    NormalizedConfigDocument, SECRET_REDACTED,
};
pub use detection::{builtin_detector_registry, AdapterDetector, AgentDetector, DetectorRegistry};
pub use install::{
    builtin_install_registry, InstallContribution, InstallContributionRegistry,
    OfficialVersionProbe, ScriptVersionKind,
};
pub use lifecycle::{
    LifecycleCoordinator, LifecycleResult, OperationId, OperationKind, OperationRecord,
    OperationStatus, ProgressEvent, ProgressSink,
};
pub use paths::{
    builtin_path_registry, resolve_agent_config_dir, resolve_agent_home, AgentPathContribution,
    AgentPathRegistry,
};
pub use projects::{
    empty_registry as empty_project_registry, ProjectScanContext, ProjectSource,
    ProjectSourceRegistry,
};
pub use stream::{
    builtin_stream_registry, has_stream_parser, StreamParseError, StreamParser,
    StreamParserRegistry,
};
pub use usage::{
    builtin_usage_registry, supports_usage_agent, TokenAccounting, UsageSource, UsageSourceRegistry,
};

#[cfg(test)]
#[path = "demo_agent_tests.rs"]
mod demo_agent_tests;
