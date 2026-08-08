//! Platform install contributions: declarative install specs + registry.
//!
//! Generic executors (npm / native script / setup URL) stay in InstallService.
//! Agent-specific package ids, URLs, flags, and uninstall allowlists live here.
//!
//! Detection continues via [`crate::adapters::AgentAdapter::detect`].
//!
//! TODO(P13): move contributions under integrations/agents/<key>/.

mod contribution;
mod probe;
mod registry;
pub mod sources;

pub use contribution::{InstallContribution, NativeUninstallerSpec};
pub use probe::{OfficialVersionProbe, ScriptVersionKind};
pub use registry::{builtin_install_registry, InstallContributionRegistry};

#[cfg(test)]
mod tests;
