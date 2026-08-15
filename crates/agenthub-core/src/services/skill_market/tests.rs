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
    let (_dir, skills) = test_skills();
    let shared = skills.list_shared_ids().unwrap();
    assert!(shared.is_empty());

    let mut items = vec![listing("owner/repo/hello"), listing("plain")];
    mark_listings_installed(&mut items, &skills);
    assert!(items.iter().all(|item| !item.installed));
}

#[test]
fn install_market_listing_rejects_unsupported_id() {
    let (_dir, skills) = test_skills();
    let err = install_market_listing(&skills, "not-a-market-id", false).unwrap_err();
    assert_eq!(err.code(), "skill.market");
    assert!(err.to_string().contains("unsupported market skill id"));
}
