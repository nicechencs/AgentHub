use crate::models::{
    AdapterSourceKind, AgentId, ModelRouteRule, RouteDownstreamDialect, RouteDownstreamSurface,
    RouteMember, RoutePool, RouteSchedulePolicy,
};
use crate::storage::{Database, RoutePoolRepo};

fn tmp() -> (tempfile::TempDir, Database, RoutePoolRepo) {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(&dir.path().join("route-pool.db")).unwrap();
    let repo = RoutePoolRepo::new(db.clone());
    (dir, db, repo)
}

fn pool(id: &str, agent: AgentId, is_default: bool, token: &str) -> RoutePool {
    RoutePool {
        id: id.into(),
        target_agent_id: agent,
        downstream_surface: RouteDownstreamSurface::Responses,
        downstream_dialect: RouteDownstreamDialect::for_agent(agent),
        hub_token: token.into(),
        schedule_policy: RouteSchedulePolicy::PriorityFailover,
        is_default,
        v2_enrolled: false,
        policy_revision: 1,
        auto_start: true,
        gateway_port: None,
        created_at: "t0".into(),
        updated_at: "t0".into(),
    }
}

fn rule(
    id: &str,
    pool_id: &str,
    public_model: &str,
    provider: &str,
    upstream_model: &str,
) -> ModelRouteRule {
    ModelRouteRule {
        id: id.into(),
        route_pool_id: pool_id.into(),
        public_model: public_model.into(),
        endpoint_family: "responses".into(),
        upstream_provider: provider.into(),
        upstream_dialect: provider.into(),
        upstream_model: upstream_model.into(),
        priority: 0,
        equivalent_group: None,
        enabled: true,
        created_at: "t0".into(),
        updated_at: "t0".into(),
    }
}

fn member(id: &str, pool_id: &str, source_id: &str, position: i64) -> RouteMember {
    RouteMember {
        id: id.into(),
        route_pool_id: pool_id.into(),
        source_kind: AdapterSourceKind::Account,
        source_id: source_id.into(),
        enabled: true,
        priority: 0,
        position,
        created_at: "t0".into(),
        updated_at: "t0".into(),
    }
}

#[test]
fn migration_creates_normalized_tables_and_indexes() {
    let (_dir, db, _repo) = tmp();
    db.with_conn(|conn| {
        let version: i64 = conn.query_row(
            "SELECT COUNT(*) FROM schema_migrations WHERE version = '00016_route_pools'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(version, 1);
        let indexes: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type = 'index'")?
            .query_map([], |row| row.get(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        assert!(indexes.iter().any(|name| name == "idx_route_pools_default"));
        assert!(indexes.iter().any(|name| name == "idx_route_members_auth"));
        let rules_version: i64 = conn.query_row(
            "SELECT COUNT(*) FROM schema_migrations WHERE version = '00019_model_route_rules'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(rules_version, 1);
        assert!(indexes
            .iter()
            .any(|name| name == "idx_model_route_rules_lane"));
        Ok(())
    })
    .unwrap();
}

#[test]
fn create_get_and_list_round_trip() {
    let (_dir, _db, repo) = tmp();
    let saved = repo
        .create_pool(&pool("pool-a", AgentId::Codex, true, "ahb_token-a"))
        .unwrap();
    assert_eq!(saved.id, "pool-a");
    assert_eq!(saved.hub_token, "ahb_token-a");
    assert_eq!(repo.get_pool("pool-a").unwrap().unwrap().id, "pool-a");
    assert_eq!(
        repo.list_pools(
            Some(AgentId::Codex),
            Some(RouteDownstreamSurface::Responses)
        )
        .unwrap()
        .len(),
        1
    );
}

#[test]
fn default_pool_is_unique_per_agent_and_surface() {
    let (_dir, _db, repo) = tmp();
    repo.create_pool(&pool("pool-a", AgentId::Codex, true, "ahb_token-a"))
        .unwrap();
    let error = repo
        .create_pool(&pool("pool-b", AgentId::Codex, true, "ahb_token-b"))
        .unwrap_err();
    assert_eq!(error.code(), "invalid_arg");
    repo.create_pool(&pool("pool-b", AgentId::Codex, false, "ahb_token-b"))
        .unwrap();
    repo.set_default("pool-b").unwrap();
    let listed = repo.list_pools(Some(AgentId::Codex), None).unwrap();
    let defaults: Vec<_> = listed.into_iter().filter(|pool| pool.is_default).collect();
    assert_eq!(defaults.len(), 1);
    assert_eq!(defaults[0].id, "pool-b");
}

#[test]
fn hub_token_is_unique_and_not_rotated_on_update() {
    let (_dir, _db, repo) = tmp();
    repo.create_pool(&pool("pool-a", AgentId::Codex, true, "ahb_token-a"))
        .unwrap();
    let error = repo
        .create_pool(&pool("pool-b", AgentId::Grok, true, "ahb_token-a"))
        .unwrap_err();
    assert_eq!(error.code(), "invalid_arg");
    let mut updated = repo.get_pool("pool-a").unwrap().unwrap();
    updated.hub_token = "ahb_rotated".into();
    updated.auto_start = false;
    let saved = repo.update_pool(&updated).unwrap();
    assert_eq!(saved.hub_token, "ahb_token-a");
    assert!(!saved.auto_start);
}

#[test]
fn set_hub_token_replaces_the_loopback_bearer() {
    let (_dir, _db, repo) = tmp();
    repo.create_pool(&pool("pool-a", AgentId::Codex, true, "ahb_token-a"))
        .unwrap();
    let saved = repo
        .set_hub_token("pool-a", "ahb_token-b", "t1")
        .unwrap();
    assert_eq!(saved.hub_token, "ahb_token-b");
    assert_eq!(saved.updated_at, "t1");
}

#[test]
fn duplicate_authorization_fingerprint_is_rejected() {
    let (_dir, _db, repo) = tmp();
    repo.create_pool(&pool("pool-a", AgentId::Codex, true, "ahb_token-a"))
        .unwrap();
    repo.add_member(&member("m1", "pool-a", "acc-1", 0))
        .unwrap();
    let error = repo
        .add_member(&member("m2", "pool-a", "acc-1", 1))
        .unwrap_err();
    assert!(error.to_string().contains("fingerprint"));
}

#[test]
fn members_sort_by_priority_position_and_id() {
    let (_dir, _db, repo) = tmp();
    repo.create_pool(&pool("pool-a", AgentId::Codex, true, "ahb_token-a"))
        .unwrap();
    let mut late = member("m-late", "pool-a", "acc-late", 0);
    late.priority = 10;
    repo.add_member(&late).unwrap();
    repo.add_member(&member("m-b", "pool-a", "acc-b", 1))
        .unwrap();
    repo.add_member(&member("m-a", "pool-a", "acc-a", 0))
        .unwrap();
    let ids: Vec<_> = repo
        .list_members("pool-a")
        .unwrap()
        .into_iter()
        .map(|row| row.id)
        .collect();
    assert_eq!(ids, vec!["m-a", "m-b", "m-late"]);
}

#[test]
fn member_mutations_bump_revision_and_reorder() {
    let (_dir, _db, repo) = tmp();
    repo.create_pool(&pool("pool-a", AgentId::Codex, true, "ahb_token-a"))
        .unwrap();
    repo.add_member(&member("m1", "pool-a", "acc-1", 0))
        .unwrap();
    repo.add_member(&member("m2", "pool-a", "acc-2", 1))
        .unwrap();
    assert_eq!(repo.get_pool("pool-a").unwrap().unwrap().policy_revision, 3);
    let reordered = repo
        .reorder_members("pool-a", &["m2".into(), "m1".into()])
        .unwrap();
    assert_eq!(reordered[0].id, "m2");
    assert_eq!(reordered[0].position, 0);
    assert_eq!(reordered[1].id, "m1");
    let mut disabled = repo.get_member("m1").unwrap().unwrap();
    disabled.enabled = false;
    repo.update_member(&disabled).unwrap();
    repo.remove_member("m2").unwrap();
    assert_eq!(repo.list_members("pool-a").unwrap().len(), 1);
    assert_eq!(repo.get_pool("pool-a").unwrap().unwrap().policy_revision, 6);
}

#[test]
fn enroll_v2_is_idempotent_for_the_same_port() {
    let (_dir, _db, repo) = tmp();
    repo.create_pool(&pool("pool-a", AgentId::Codex, true, "ahb_token-a"))
        .unwrap();
    let first = repo.enroll_v2("pool-a", 43121, "t1").unwrap();
    assert!(first.v2_enrolled);
    assert_eq!(first.gateway_port, Some(43121));
    let revision = first.policy_revision;
    let second = repo.enroll_v2("pool-a", 43121, "t2").unwrap();
    assert_eq!(second.policy_revision, revision);
    assert_eq!(second.gateway_port, Some(43121));
}

#[test]
fn enroll_v2_rejects_a_different_port() {
    let (_dir, _db, repo) = tmp();
    repo.create_pool(&pool("pool-a", AgentId::Codex, true, "ahb_token-a"))
        .unwrap();
    let first = repo.enroll_v2("pool-a", 43121, "t1").unwrap();
    let error = repo.enroll_v2("pool-a", 43122, "t2").unwrap_err();
    assert_eq!(error.code(), "invalid_arg");
    let stored = repo.get_pool("pool-a").unwrap().unwrap();
    assert_eq!(stored.gateway_port, Some(43121));
    assert_eq!(stored.policy_revision, first.policy_revision);
}

#[test]
fn old_profiles_are_not_merged_into_one_pool() {
    let (_dir, _db, repo) = tmp();
    repo.create_pool(&pool("profile-a", AgentId::Codex, true, "ahb_token-a"))
        .unwrap();
    repo.create_pool(&pool("profile-b", AgentId::Codex, false, "ahb_token-b"))
        .unwrap();
    repo.add_member(&member("m-a", "profile-a", "acc-a", 0))
        .unwrap();
    repo.add_member(&member("m-b", "profile-b", "acc-b", 0))
        .unwrap();
    assert_eq!(
        repo.list_pools(Some(AgentId::Codex), None).unwrap().len(),
        2
    );
    assert_eq!(repo.list_members("profile-a").unwrap().len(), 1);
    assert_eq!(repo.list_members("profile-b").unwrap().len(), 1);
}

#[test]
fn model_route_rule_crud_bumps_revision_and_rejects_duplicates() {
    let (_dir, _db, repo) = tmp();
    repo.create_pool(&pool("pool-a", AgentId::Codex, true, "ahb_token-a"))
        .unwrap();
    assert_eq!(repo.get_pool("pool-a").unwrap().unwrap().policy_revision, 1);
    let saved = repo
        .add_rule(&rule("r1", "pool-a", "m1", "grok", "grok-4"))
        .unwrap();
    assert_eq!(saved.public_model, "m1");
    assert_eq!(saved.upstream_model, "grok-4");
    assert!(saved.equivalent_group.is_none());
    assert_eq!(repo.get_pool("pool-a").unwrap().unwrap().policy_revision, 2);
    let error = repo
        .add_rule(&rule("r2", "pool-a", "m1", "grok", "grok-4"))
        .unwrap_err();
    assert_eq!(error.code(), "invalid_arg");
    let mut updated = repo.get_rule("r1").unwrap().unwrap();
    updated.priority = 3;
    updated.equivalent_group = Some("shared".into());
    repo.update_rule(&updated).unwrap();
    let stored = repo.get_rule("r1").unwrap().unwrap();
    assert_eq!(stored.priority, 3);
    assert_eq!(stored.equivalent_group.as_deref(), Some("shared"));
    assert_eq!(repo.get_pool("pool-a").unwrap().unwrap().policy_revision, 3);
    repo.remove_rule("r1").unwrap();
    assert!(repo.list_rules("pool-a").unwrap().is_empty());
    assert_eq!(repo.get_pool("pool-a").unwrap().unwrap().policy_revision, 4);
}

#[test]
fn model_route_rule_rejects_glob_ids() {
    let (_dir, _db, repo) = tmp();
    repo.create_pool(&pool("pool-a", AgentId::Codex, true, "ahb_token-a"))
        .unwrap();
    let error = repo
        .add_rule(&rule("r1", "pool-a", "gpt-*", "openai", "gpt-4"))
        .unwrap_err();
    assert_eq!(error.code(), "invalid_arg");
}
