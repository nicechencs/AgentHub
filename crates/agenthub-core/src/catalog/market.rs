//! Remote skill market endpoints (skills.sh / skillhub.cn).
//!
//! Override bases with env:
//! - `AGENTHUB_SKILLS_SH_BASE` (no trailing slash)
//! - `AGENTHUB_SKILLHUB_API_BASE` (no trailing slash; default `https://api.skillhub.cn`)

use std::sync::OnceLock;

/// Default page size for skills.sh search / leaderboard slice.
pub const DEFAULT_SKILLS_SH_LIMIT: usize = 40;

/// Default page size for skillhub.cn list/search.
pub const DEFAULT_SKILLHUB_LIMIT: usize = 40;

const DEFAULT_SKILLS_SH_BASE: &str = "https://skills.sh";
const DEFAULT_SKILLHUB_API_BASE: &str = "https://api.skillhub.cn";
const DEFAULT_SKILLHUB_HOME: &str = "https://skillhub.cn";
const REPO_URL: &str = "https://github.com/demo_chen/AgentHub";

/// User preference for which remote market to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillMarketSource {
    /// Try skills.sh first; on network/API failure fall back to skillhub.cn.
    Auto,
    /// Fixed: skills.sh only.
    SkillsSh,
    /// Fixed: skillhub.cn only.
    SkillhubCn,
}

impl SkillMarketSource {
    pub const DEFAULT: Self = Self::Auto;

    /// Canonical storage / UI value.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::SkillsSh => "skills.sh",
            Self::SkillhubCn => "skillhub.cn",
        }
    }

    /// Parse whitelist values (`auto` | `skills.sh` | `skillhub.cn` + aliases).
    pub fn parse(raw: &str) -> Result<Self, String> {
        let s = raw.trim().to_ascii_lowercase();
        match s.as_str() {
            "" | "auto" => Ok(Self::Auto),
            "skills.sh" | "skills_sh" | "skillssh" | "skills-sh" => Ok(Self::SkillsSh),
            "skillhub.cn" | "skillhub" | "skillhub_cn" | "skillhub-cn" => Ok(Self::SkillhubCn),
            other => Err(format!(
                "invalid skill_market_source '{other}' (allowed: auto, skills.sh, skillhub.cn)"
            )),
        }
    }
}

impl Default for SkillMarketSource {
    fn default() -> Self {
        Self::DEFAULT
    }
}

fn skills_sh_base_cached() -> &'static str {
    static BASE: OnceLock<String> = OnceLock::new();
    BASE.get_or_init(|| {
        std::env::var("AGENTHUB_SKILLS_SH_BASE")
            .ok()
            .map(|s| s.trim().trim_end_matches('/').to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| DEFAULT_SKILLS_SH_BASE.to_string())
    })
}

fn skillhub_api_base_cached() -> &'static str {
    static BASE: OnceLock<String> = OnceLock::new();
    BASE.get_or_init(|| {
        std::env::var("AGENTHUB_SKILLHUB_API_BASE")
            .ok()
            .map(|s| s.trim().trim_end_matches('/').to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| DEFAULT_SKILLHUB_API_BASE.to_string())
    })
}

/// Base origin for skills.sh (no trailing slash).
pub fn skills_sh_base_url() -> &'static str {
    skills_sh_base_cached()
}

/// `GET {base}/api/search?q=…&limit=…`
pub fn skills_sh_search_url() -> String {
    format!("{}/api/search", skills_sh_base_url())
}

/// Leaderboard / home HTML page.
pub fn skills_sh_home_url() -> String {
    format!("{}/", skills_sh_base_url())
}

/// skills.sh skill detail page: `{base}/{owner}/{repo}/{skill}`.
pub fn skills_sh_detail_url(listing_id: &str) -> String {
    let id = listing_id
        .trim()
        .trim_start_matches("market:skills.sh:")
        .trim_matches('/');
    format!("{}/{}", skills_sh_base_url(), id)
}

/// SkillHub public API origin (no trailing slash). Default `https://api.skillhub.cn`.
pub fn skillhub_api_base_url() -> &'static str {
    skillhub_api_base_cached()
}

/// SkillHub site home (docs / browser).
pub fn skillhub_home_url() -> &'static str {
    DEFAULT_SKILLHUB_HOME
}

/// skillhub.cn skill detail page (SPA routes from skill-hub frontend).
///
/// Matches official `buildSkillDetailHref`:
/// - with namespace handle → `/skills/{handle}/{slug}`
/// - otherwise → `/skills/{slug}`
///
/// Do **not** use bare `/{handle}/{slug}` — that route has no skill page and
/// the SPA renders blank.
pub fn skillhub_detail_url(handle: Option<&str>, slug: &str) -> String {
    let slug = slug.trim().trim_matches('/');
    let handle = handle
        .map(str::trim)
        .map(|h| h.trim_start_matches('@'))
        .filter(|h| !h.is_empty());
    match handle {
        Some(h) => format!(
            "{}/skills/{}/{}",
            skillhub_home_url(),
            // path segments are percent-encoded like the official SPA
            urlencoding::encode(h),
            urlencoding::encode(slug)
        ),
        None => format!(
            "{}/skills/{}",
            skillhub_home_url(),
            urlencoding::encode(slug)
        ),
    }
}

/// `GET {api}/api/skills?page=&pageSize=&keyword=…`
pub fn skillhub_skills_list_url() -> String {
    format!("{}/api/skills", skillhub_api_base_url())
}

/// `GET {api}/api/v1/download?slug=…` (302 → zip).
pub fn skillhub_download_url(slug: &str, version: Option<&str>) -> String {
    let mut url = format!(
        "{}/api/v1/download?slug={}",
        skillhub_api_base_url(),
        urlencoding::encode(slug)
    );
    if let Some(v) = version.map(str::trim).filter(|s| !s.is_empty()) {
        url.push_str("&version=");
        url.push_str(&urlencoding::encode(v));
    }
    url
}

/// User-Agent using the crate package version (not a hardcoded 0.1).
pub fn skills_sh_user_agent() -> String {
    format!(
        "AgentHub/{} (+{REPO_URL})",
        env!("CARGO_PKG_VERSION")
    )
}

/// Alias for shared market HTTP UA (skills.sh + skillhub).
pub fn skill_market_user_agent() -> String {
    skills_sh_user_agent()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_agent_includes_package_version() {
        let ua = skills_sh_user_agent();
        assert!(
            ua.contains(env!("CARGO_PKG_VERSION")),
            "UA should embed CARGO_PKG_VERSION: {ua}"
        );
        assert!(ua.starts_with("AgentHub/"));
    }

    #[test]
    fn default_urls_point_at_skills_sh() {
        // Only assert when env is unset so local overrides do not break CI.
        if std::env::var_os("AGENTHUB_SKILLS_SH_BASE").is_none() {
            assert_eq!(skills_sh_base_url(), "https://skills.sh");
            assert_eq!(skills_sh_search_url(), "https://skills.sh/api/search");
            assert_eq!(skills_sh_home_url(), "https://skills.sh/");
        }
    }

    #[test]
    fn skillhub_urls_and_source_parse() {
        if std::env::var_os("AGENTHUB_SKILLHUB_API_BASE").is_none() {
            assert_eq!(skillhub_api_base_url(), "https://api.skillhub.cn");
            assert!(skillhub_skills_list_url().ends_with("/api/skills"));
            assert!(skillhub_download_url("find-skills", Some("1.0.0")).contains("slug=find-skills"));
        }
        assert_eq!(SkillMarketSource::parse("auto").unwrap(), SkillMarketSource::Auto);
        assert_eq!(
            SkillMarketSource::parse("skills.sh").unwrap(),
            SkillMarketSource::SkillsSh
        );
        assert_eq!(
            SkillMarketSource::parse("skillhub.cn").unwrap(),
            SkillMarketSource::SkillhubCn
        );
        assert!(SkillMarketSource::parse("nope").is_err());
        if std::env::var_os("AGENTHUB_SKILLS_SH_BASE").is_none() {
            assert_eq!(
                skills_sh_detail_url("vercel-labs/agent-skills/foo"),
                "https://skills.sh/vercel-labs/agent-skills/foo"
            );
        }
        assert_eq!(
            skillhub_detail_url(Some("pskoett"), "self-improving-agent"),
            "https://skillhub.cn/skills/pskoett/self-improving-agent"
        );
        assert_eq!(
            skillhub_detail_url(Some("@root"), "find-skills"),
            "https://skillhub.cn/skills/root/find-skills"
        );
        assert_eq!(
            skillhub_detail_url(None, "find-skills"),
            "https://skillhub.cn/skills/find-skills"
        );
    }
}
