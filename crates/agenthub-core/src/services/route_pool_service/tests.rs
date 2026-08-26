use crate::models::{
    AdapterProfile, AdapterProfileMode, AdapterProfileStatus, AdapterRoute, AdapterSourceKind,
    AgentId, FEATURE_ROUTE_INDEX_V2, FEATURE_ROUTE_POOL_V2,
};
use crate::services::RoutePoolService;
use crate::storage::{AdapterProfileRepo, Database};

fn tmp() -> (
    tempfile::TempDir,
    Database,
    RoutePoolService,
    AdapterProfileRepo,
) {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(&dir.path().join("route-pool-service.db")).unwrap();
    db.set_setting(FEATURE_ROUTE_POOL_V2, "true").unwrap();
    let service = RoutePoolService::new(db.clone());
    let profiles = AdapterProfileRepo::new(db.clone());
    (dir, db, service, profiles)
}

fn bridge_profile(id: &str, source_id: &str, agent: AgentId, auto_start: bool) -> AdapterProfile {
    AdapterProfile {
        id: id.into(),
        name: id.into(),
        source_kind: AdapterSourceKind::Account,
        source_id: source_id.into(),
        target_agent_id: agent,
        route: AdapterRoute::LocalBridge,
        mode: AdapterProfileMode::Api,
        status: AdapterProfileStatus::Active,
        rule_id: "test-rule".into(),
        rule_version: "v1".into(),
        generated_provider_id: None,
        local_port: Some(43121),
        auto_start,
        last_error_code: None,
        created_at: "t0".into(),
        updated_at: "t0".into(),
    }
}

#[test]
fn flag_off_is_fail_closed() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(&dir.path().join("flag-off.db")).unwrap();
    let service = RoutePoolService::new(db);
    let error = service.list(None, None).unwrap_err();
    assert_eq!(error.code(), "unsupported");
}

#[test]
fn legacy_projection_keeps_id_auto_start_and_single_lead() {
    let (_dir, _db, service, profiles) = tmp();
    let profile = bridge_profile("profile-a", "acc-a", AgentId::Codex, true);
    profiles.create(&profile).unwrap();
    service
        .create_legacy_pool(&profile, "ahb_stable-token", true)
        .unwrap();
    let again = service.project_legacy_local_bridges().unwrap();
    assert_eq!(again.len(), 1);
    assert_eq!(again[0].id, "profile-a");
    assert_eq!(again[0].hub_token, "ahb_stable-token");
    assert!(again[0].auto_start);
    assert!(!again[0].v2_enrolled);
    let members = service.list_members("profile-a").unwrap();
    assert_eq!(members.len(), 1);
    assert_eq!(members[0].source_id, "acc-a");
}

#[test]
fn multiple_old_profiles_are_not_merged_and_default_is_stable() {
    let (_dir, _db, service, profiles) = tmp();
    let a = bridge_profile("b-profile", "acc-b", AgentId::Codex, true);
    let b = bridge_profile("a-profile", "acc-a", AgentId::Codex, false);
    profiles.create(&a).unwrap();
    profiles.create(&b).unwrap();
    let listed = service.project_legacy_local_bridges().unwrap();
    assert_eq!(listed.len(), 2);
    let defaults: Vec<_> = listed.iter().filter(|pool| pool.is_default).collect();
    assert_eq!(defaults.len(), 1);
    assert_eq!(defaults[0].id, "a-profile");
    assert_eq!(service.list_members("a-profile").unwrap().len(), 1);
    assert_eq!(service.list_members("b-profile").unwrap().len(), 1);
}

#[test]
fn second_projection_does_not_rotate_hub_token_or_duplicate_members() {
    let (_dir, _db, service, profiles) = tmp();
    let profile = bridge_profile("profile-a", "acc-a", AgentId::Codex, true);
    profiles.create(&profile).unwrap();
    let first = service.project_legacy_local_bridges().unwrap();
    let token = first[0].hub_token.clone();
    let second = service.project_legacy_local_bridges().unwrap();
    assert_eq!(second[0].hub_token, token);
    assert_eq!(service.list_members("profile-a").unwrap().len(), 1);
}

#[test]
fn member_crud_sort_enabled_priority_and_lead_projection() {
    let (_dir, _db, service, profiles) = tmp();
    let profile = bridge_profile("profile-a", "acc-a", AgentId::Codex, true);
    profiles.create(&profile).unwrap();
    service
        .create_legacy_pool(&profile, "ahb_stable-token", true)
        .unwrap();
    let extra = service
        .add_member("profile-a", AdapterSourceKind::Account, "acc-b")
        .unwrap();
    service.set_member_priority(&extra.id, -1).unwrap();
    let members = service.list_members("profile-a").unwrap();
    assert_eq!(members[0].source_id, "acc-b");
    let projected = profiles.get("profile-a").unwrap().unwrap();
    assert_eq!(projected.source_id, "acc-b");
    service.set_member_enabled(&extra.id, false).unwrap();
    let projected = profiles.get("profile-a").unwrap().unwrap();
    assert_eq!(projected.source_id, "acc-a");
    service.remove_member(&extra.id).unwrap();
    assert_eq!(service.list_members("profile-a").unwrap().len(), 1);
}

#[test]
fn duplicate_fingerprint_is_rejected() {
    let (_dir, _db, service, profiles) = tmp();
    let profile = bridge_profile("profile-a", "acc-a", AgentId::Codex, true);
    profiles.create(&profile).unwrap();
    service
        .create_legacy_pool(&profile, "ahb_stable-token", true)
        .unwrap();
    let error = service
        .add_member("profile-a", AdapterSourceKind::Account, "acc-a")
        .unwrap_err();
    assert!(error.to_string().contains("fingerprint"));
}

#[test]
fn enroll_v2_writes_port_once_and_keeps_token() {
    let (_dir, _db, service, profiles) = tmp();
    let profile = bridge_profile("profile-a", "acc-a", AgentId::Codex, true);
    profiles.create(&profile).unwrap();
    service
        .create_legacy_pool(&profile, "ahb_stable-token", true)
        .unwrap();
    let enrolled = service.enroll_v2("profile-a", 43155).unwrap();
    assert!(enrolled.v2_enrolled);
    assert_eq!(enrolled.gateway_port, Some(43155));
    assert_eq!(enrolled.hub_token, "ahb_stable-token");
    let again = service.enroll_v2("profile-a", 43155).unwrap();
    assert_eq!(again.policy_revision, enrolled.policy_revision);
}

#[test]
fn enroll_v2_rejects_a_different_port() {
    let (_dir, _db, service, profiles) = tmp();
    let profile = bridge_profile("profile-a", "acc-a", AgentId::Codex, true);
    profiles.create(&profile).unwrap();
    service
        .create_legacy_pool(&profile, "ahb_stable-token", true)
        .unwrap();
    let enrolled = service.enroll_v2("profile-a", 43155).unwrap();
    let error = service.enroll_v2("profile-a", 43156).unwrap_err();
    assert_eq!(error.code(), "invalid_arg");
    let stored = service.get("profile-a").unwrap().unwrap();
    assert_eq!(stored.gateway_port, Some(43155));
    assert_eq!(stored.policy_revision, enrolled.policy_revision);
}

#[test]
fn create_legacy_pool_does_not_enroll_v2() {
    let (_dir, _db, service, profiles) = tmp();
    let profile = bridge_profile("profile-a", "acc-a", AgentId::Codex, true);
    profiles.create(&profile).unwrap();
    let pool = service
        .create_legacy_pool(&profile, "ahb_stable-token", true)
        .unwrap();
    assert!(!pool.v2_enrolled);
    assert_eq!(
        profiles.get("profile-a").unwrap().unwrap().local_port,
        Some(43121)
    );
}

#[test]
fn native_endpoint_profiles_are_not_auto_enrolled() {
    let (_dir, _db, service, profiles) = tmp();
    let mut direct = bridge_profile("direct", "acc-direct", AgentId::Codex, false);
    direct.route = AdapterRoute::NativeEndpoint;
    direct.local_port = None;
    direct.auto_start = false;
    profiles.create(&direct).unwrap();
    let listed = service.project_legacy_local_bridges().unwrap();
    assert!(listed.is_empty());
}

#[test]
fn enroll_v2_rejects_native_endpoint_and_config_sync() {
    let (_dir, _db, service, profiles) = tmp();
    for (id, route) in [
        ("native", AdapterRoute::NativeEndpoint),
        ("sync", AdapterRoute::ConfigSync),
    ] {
        let mut profile = bridge_profile(id, "acc", AgentId::Codex, false);
        profile.route = route;
        profile.local_port = None;
        profiles.create(&profile).unwrap();
        service
            .create_legacy_pool(&profile, &format!("ahb_stable-token-{id}"), false)
            .unwrap();
        let error = service.enroll_v2(id, 43155).unwrap_err();
        assert_eq!(error.code(), "unsupported");
        assert!(!service.get(id).unwrap().unwrap().v2_enrolled);
    }
}

#[test]
fn index_enabled_requires_both_flags() {
    let (_dir, db, service, _profiles) = tmp();
    assert!(!service.index_enabled());
    db.set_setting(FEATURE_ROUTE_INDEX_V2, "true").unwrap();
    assert!(service.index_enabled());
    db.set_setting(FEATURE_ROUTE_INDEX_V2, "off").unwrap();
    assert!(!service.index_enabled());
}

#[tokio::test]
async fn occupancy_failure_does_not_enroll_or_rewrite_client() {
    let (_dir, _db, service, profiles) = tmp();
    let profile = bridge_profile("profile-a", "acc-a", AgentId::Codex, true);
    profiles.create(&profile).unwrap();
    service
        .create_legacy_pool(&profile, "ahb_stable-token", true)
        .unwrap();
    let blocker = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let busy = blocker.local_addr().unwrap().port();
    let host = crate::bridge::BridgeRuntimeHost::new();
    assert!(service
        .bind_then_enroll(&host, "profile-a", busy)
        .await
        .is_err());
    let pool = service.get("profile-a").unwrap().unwrap();
    assert!(!pool.v2_enrolled);
    assert_eq!(pool.gateway_port, None);
    assert_eq!(pool.hub_token, "ahb_stable-token");
    assert_eq!(
        profiles.get("profile-a").unwrap().unwrap().local_port,
        Some(43121)
    );
    drop(blocker);
    host.shutdown().await.unwrap();
}

#[tokio::test]
async fn bind_then_enroll_writes_port_only_after_bind() {
    let (_dir, _db, service, profiles) = tmp();
    let profile = bridge_profile("profile-a", "acc-a", AgentId::Codex, true);
    profiles.create(&profile).unwrap();
    service
        .create_legacy_pool(&profile, "ahb_stable-token", true)
        .unwrap();
    let host = crate::bridge::BridgeRuntimeHost::new();
    let port = {
        let probe = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        probe.local_addr().unwrap().port()
    };
    let enrolled = service
        .bind_then_enroll(&host, "profile-a", port)
        .await
        .unwrap();
    assert!(enrolled.v2_enrolled);
    assert_eq!(enrolled.gateway_port, Some(port));
    assert_eq!(enrolled.hub_token, "ahb_stable-token");
    assert_eq!(
        profiles.get("profile-a").unwrap().unwrap().local_port,
        Some(43121),
        "enroll must not rewrite the historical client port"
    );
    host.shutdown().await.unwrap();
}
