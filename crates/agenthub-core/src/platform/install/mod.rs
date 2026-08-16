//! Platform install contributions: declarative install specs + registry.
//!
//! Generic executors (npm / native script / setup URL) stay in InstallService.
//! Agent-specific package ids, URLs, flags, and uninstall allowlists live here.
//!
//! Channel list / GUI catalog DTOs: [`catalog`] (same contribution allowlist).
//! Detection continues via [`crate::adapters::AgentAdapter::detect`].
//!
//! Per-agent package ids / URLs live in [`crate::integrations`].

mod catalog;
mod contribution;
mod probe;
mod registry;
pub mod sources;

pub use catalog::{
    adapter_install_channels, channels_for, list_install_catalog, native_ps1_url, native_setup_url,
    native_sh_url, npm_install_extra_flags, npm_package, official_version_probe,
    AgentInstallCatalogEntry, InstallChannelPlan,
};
pub use contribution::{InstallContribution, NativeUninstallerSpec};
pub use probe::{OfficialVersionProbe, ScriptVersionKind};
pub use registry::{builtin_install_registry, InstallContributionRegistry};

#[cfg(test)]
mod tests;
