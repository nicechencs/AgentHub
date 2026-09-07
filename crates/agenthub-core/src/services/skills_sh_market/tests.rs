use super::*;

#[test]
fn parse_search_payload() {
    let v: Value = serde_json::from_str(
            r#"{"skills":[{"id":"vercel-labs/agent-skills/foo","skillId":"foo","name":"Foo","installs":12,"source":"vercel-labs/agent-skills"}]}"#,
        )
        .unwrap();
    let arr = v.get("skills").unwrap().as_array().unwrap();
    let skills = parse_skills_array(arr);
    assert_eq!(skills.len(), 1);
    assert_eq!(skills[0].id, "vercel-labs/agent-skills/foo");
    assert_eq!(skills[0].skill_id, "foo");
}

#[test]
fn parse_next_data_fixture() {
    let html = r#"
        <html><script id="__NEXT_DATA__" type="application/json">
        {"props":{"pageProps":{"initialSkills":[{"source":"antfu/skills","skillId":"vite","name":"vite","installs":152}]}}}
        </script></html>
        "#;
    let skills = parse_next_data(html).unwrap();
    assert_eq!(skills.len(), 1);
    assert_eq!(skills[0].id, "antfu/skills/vite");
}

#[test]
fn listing_id_parse() {
    let s = parse_listing_id("vercel-labs/agent-skills/vercel-react-best-practices").unwrap();
    assert_eq!(s.source, "vercel-labs/agent-skills");
    assert_eq!(s.skill_id, "vercel-react-best-practices");
}
