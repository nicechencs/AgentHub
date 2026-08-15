use super::{install_market_listing, mark_listings_installed};
use crate::adapters::register_all;
use crate::models::SkillListing;
use crate::services::SkillService;
use crate::storage::Database;

fn test_skills() -> (tempfile::TempDir, SkillService) {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(&dir.path().join("skills.db")).unwrap();
    let skills = SkillService::with_db(dir.path().join("skills"), register_all(), db);
    (dir, skills)
}

fn listing(id: &str) -> SkillListing {
    SkillListing {
        id: id.into(),
        name: id.into(),
        description: String::new(),
        version: None,
        provider_id: "test".into(),
        installed: false,
        detail_url: None,
    }
}

#[test]
fn mark_listings_installed_matches_shared_or_local_id() {
    let (dir, skills) = test_skills();
    let shared = skills.list_shared_ids().unwrap();
    assert!(shared.is_empty());

    let mut items = vec![listing("owner/repo/hello"), listing("plain")];
    mark_listings_installed(&mut items, &skills);
    assert!(items.iter().all(|item| !item.installed));

    let hello = dir.path().join("skills").join("hello");
    std::fs::create_dir_all(&hello).unwrap();
    std::fs::write(hello.join("SKILL.md"), "---\nname: Hello\n---\nbody\n").unwrap();
    skills.invalidate_list_cache();

    let mut items = vec![
        listing("owner/repo/hello"),
        listing("hello"),
        listing("owner/repo/other"),
        listing("skillhub:hello"),
    ];
    mark_listings_installed(&mut items, &skills);
    assert!(items[0].installed, "skills.sh listing should match local slug");
    assert!(items[1].installed, "exact shared id should match");
    assert!(!items[2].installed, "unrelated listing must stay unmarked");
    assert!(
        items[3].installed,
        "skillhub listing with same slug should match local id"
    );
}

#[test]
fn install_market_listing_rejects_unsupported_id() {
    let (_dir, skills) = test_skills();
    let err = install_market_listing(&skills, "not-a-market-id", false).unwrap_err();
    assert_eq!(err.code(), "skill.market");
    assert!(err.to_string().contains("unsupported market skill id"));
}
