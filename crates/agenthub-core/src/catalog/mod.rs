//! Product constants that must not drift across GUI / CLI / services.
//!
//! - [`limits`]: timeouts, scan caps, default TTL values
//! - [`market`]: remote skill market endpoints / UA
//!
//! **Install channels** (npm packages, native script URLs, GUI catalog DTOs) live in
//! [`crate::platform::install`] — contribution registry is the single allowlist.
//! [`install`] is only a thin `pub use` compatibility façade so existing
//! `catalog::install::*` imports keep compiling.
//!
//! Do not confuse this module with [`crate::platform::agent_catalog`] (Agent
//! descriptor directory). Frontend `src/config/*` may still mirror display
//! strings; install catalog drift is guarded by unit tests.

pub mod install;
pub mod limits;
pub mod market;

pub use limits::*;
pub use market::{
    skill_market_user_agent, skillhub_api_base_url, skillhub_detail_url, skillhub_download_url,
    skillhub_home_url, skillhub_skills_list_url, skills_sh_base_url, skills_sh_detail_url,
    skills_sh_home_url, skills_sh_search_url, skills_sh_user_agent, SkillMarketSource,
    DEFAULT_SKILLHUB_LIMIT, DEFAULT_SKILLS_SH_LIMIT,
};
