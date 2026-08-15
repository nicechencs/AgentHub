//! Compatibility façade for install channel lookups and GUI/CLI DTOs.
//!
//! True source: [`crate::platform::install`] (`InstallContribution` + catalog API).
//! Prefer `platform::install::{npm_package, list_install_catalog, …}` for new code.

pub use crate::platform::install::{
    adapter_install_channels, channels_for, list_install_catalog, native_ps1_url, native_setup_url,
    native_sh_url, npm_install_extra_flags, npm_package, official_version_probe,
    AgentInstallCatalogEntry, InstallChannelPlan, OfficialVersionProbe, ScriptVersionKind,
};
