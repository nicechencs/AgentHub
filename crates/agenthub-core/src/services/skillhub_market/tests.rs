use super::*;

#[test]
fn parse_list_payload() {
    let body = r#"{
          "code":0,
          "message":"success",
          "data":{
            "skills":[{
              "slug":"find-skills",
              "name":"Find Skills",
              "description":"discover skills",
              "description_zh":"发现技能",
              "downloads":1000,
              "source":"clawhub",
              "version":"1.0.0"
            }],
            "total":1
          }
        }"#;
    let skills = parse_skills_page(body).unwrap();
    assert_eq!(skills.len(), 1);
    assert_eq!(skills[0].id, "skillhub:find-skills@1.0.0");
    assert_eq!(skills[0].slug, "find-skills");
    assert_eq!(skills[0].name, "Find Skills");
    assert_eq!(skills[0].description, "发现技能");
    let listing = to_listing(skills[0].clone());
    assert_eq!(
        listing.detail_url.as_deref(),
        Some("https://skillhub.cn/skills/find-skills")
    );
}

#[test]
fn detail_url_prefers_namespace_handle() {
    let body = r#"{
          "code":0,
          "data":{
            "skills":[{
              "slug":"self-improving-agent",
              "name":"self-improving agent",
              "description":"x",
              "downloads":10,
              "source":"clawhub",
              "version":"3.0.24",
              "namespace":{"handle":"pskoett","publicSlug":"self-improving-agent"}
            }],
            "total":1
          }
        }"#;
    let skills = parse_skills_page(body).unwrap();
    assert_eq!(skills[0].handle.as_deref(), Some("pskoett"));
    let listing = to_listing(skills[0].clone());
    assert_eq!(
        listing.detail_url.as_deref(),
        Some("https://skillhub.cn/skills/pskoett/self-improving-agent")
    );
}

#[test]
fn parse_ids() {
    let (s, v) = parse_skillhub_id("skillhub:find-skills@1.0.0").unwrap();
    assert_eq!(s, "find-skills");
    assert_eq!(v.as_deref(), Some("1.0.0"));
    let (s2, v2) = parse_skillhub_id("skillhub:find-skills").unwrap();
    assert_eq!(s2, "find-skills");
    assert!(v2.is_none());
    assert!(is_skillhub_listing_id("skillhub:x"));
    assert!(!is_skillhub_listing_id("vercel-labs/agent-skills/foo"));
}
