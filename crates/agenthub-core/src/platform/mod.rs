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
    bootstrap_skill_assignments, builtin_skill_target_registry, AdapterSkillTarget,
    AgentSkillTarget, SkillAssignmentService, SkillBootstrapReport, SkillPackageService,
    SkillReconciler, SkillSourceService, SkillTargetRegistry, StaticSkillTarget,
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
pub use detection::{
    builtin_detector_registry, AdapterDetector, AgentDetector, DetectorRegistry, FnDetector,
};
pub use install::{
    adapter_install_channels, builtin_install_registry, channels_for, list_install_catalog,
    native_ps1_url, native_setup_url, native_sh_url, npm_install_extra_flags, npm_package,
    official_version_probe, AgentInstallCatalogEntry, InstallChannelPlan, InstallContribution,
    InstallContributionRegistry, OfficialVersionProbe, ScriptVersionKind,
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
    builtin_project_registry, empty_registry as empty_project_registry, ProjectScanContext,
    ProjectSource, ProjectSourceRegistry,
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
