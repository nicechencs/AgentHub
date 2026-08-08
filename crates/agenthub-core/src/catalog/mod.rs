//! Single source of truth for product constants that must not drift.
//!
//! - [`install`]: npm packages, native installer URLs, setup pages
//! - [`limits`]: timeouts, scan caps, default TTL values
//! - [`market`]: remote skill market endpoints / UA
//!
//! Frontend `src/config/*` may still mirror display strings until GUI is
//! fully wired to core APIs; install catalog drift is guarded by unit tests
//! that read `src/config/agents.ts`.

pub mod install;
pub mod limits;
pub mod market;

pub use install::{
    channels_for, list_install_catalog, native_ps1_url, native_setup_url, native_sh_url,
    npm_install_extra_flags, npm_package, official_version_probe, AgentInstallCatalogEntry,
    InstallChannelPlan, OfficialVersionProbe, ScriptVersionKind,
};
pub use limits::*;
pub use market::{
    skill_market_user_agent, skillhub_api_base_url, skillhub_detail_url, skillhub_download_url,
    skillhub_home_url, skillhub_skills_list_url, skills_sh_base_url, skills_sh_detail_url,
    skills_sh_home_url, skills_sh_search_url, skills_sh_user_agent, SkillMarketSource,
    DEFAULT_SKILLHUB_LIMIT, DEFAULT_SKILLS_SH_LIMIT,
};
