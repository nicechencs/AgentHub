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
    assert_eq!(
        SkillMarketSource::parse("auto").unwrap(),
        SkillMarketSource::Auto
    );
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
