use crate::models::{
    authorization_is_route_pool_home, enroll_native_plan_is_open, Account, AccountKind,
    AdapterApplyPlan, AdapterGateKind, AdapterMaturity, AdapterProfile, AdapterProfileMode,
    AdapterProfileStatus, AdapterReusePath, AdapterRoute, AdapterRouteAnalysis,
    AdapterServiceImpact, AdapterSourceKind, AdapterSupport, AgentId, Provider,
    RouteDownstreamSurface, FEATURE_CODEX_INGRESS_GROK_UPSTREAM, FEATURE_GROK_INGRESS_CODEX_UPSTREAM,
    FEATURE_MIXED_PROVIDER_POOL, FEATURE_ROUTE_INDEX_V2, FEATURE_ROUTE_POOL_V2,
};
use crate::services::RoutePoolService;
use crate::storage::{AccountRepo, AdapterProfileRepo, Database, ProviderRepo};
use serde_json::json;

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
    db.set_setting(FEATURE_ROUTE_POOL_V2, "off").unwrap();
    let service = RoutePoolService::new(db);
    let error = service.list(None, None).unwrap_err();
    assert_eq!(error.code(), "unsupported");
    assert_eq!(
        service
            .add_rule("pool", "m1", "responses", "grok", "grok", "grok-4", 0, None)
            .unwrap_err()
            .code(),
        "unsupported"
    );
}

#[test]
fn product_flags_default_on() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(&dir.path().join("flag-default-on.db")).unwrap();
    let service = RoutePoolService::new(db);
    assert!(service.enabled().unwrap());
    assert!(service.index_enabled());
    assert_eq!(service.pair_adapter_flags(), (false, false));
    assert!(!service.mixed_provider_enabled());
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
    let flipped = service
        .set_authorization_enabled(AdapterSourceKind::Account, "acc-b", true)
        .unwrap();
    assert_eq!(flipped, 1);
    assert!(service
        .list_members("profile-a")
        .unwrap()
        .iter()
        .any(|member| member.source_id == "acc-b" && member.enabled));
    service.remove_member(&extra.id).unwrap();
    assert_eq!(service.list_members("profile-a").unwrap().len(), 1);
}

#[test]
fn remove_route_authorization_drops_membership_while_source_still_exists() {
    let (_dir, db, service, _profiles) = tmp();
    ProviderRepo::new(db.clone())
        .create(&Provider {
            id: "codex-api".into(),
            agent_id: AgentId::Codex,
            name: "Codex API".into(),
            settings_config: json!({"apiKey": "secret"}),
            meta: json!({"preset": "custom"}),
            is_current: false,
            created_at: "t0".into(),
            updated_at: "t0".into(),
        })
        .unwrap();
    service
        .attach_pool_owned_authorization(
            AgentId::Codex,
            RouteDownstreamSurface::Responses,
            AdapterSourceKind::Provider,
            "codex-api",
        )
        .unwrap();

    let removed = service
        .remove_route_authorization(AdapterSourceKind::Provider, "codex-api")
        .unwrap();
    assert_eq!(removed, 1);
    assert!(service
        .list_default_overviews()
        .unwrap()
        .pools
        .iter()
        .all(|pool| pool.members.is_empty()));
    assert!(ProviderRepo::new(db)
        .get_by_id("codex-api")
        .unwrap()
        .is_some());
}

#[test]
fn remove_route_authorization_removes_missing_source_from_all_default_pools() {
    let (_dir, db, service, _profiles) = tmp();
    let stale_source = "missing-connection";
    let codex_pool = service
        .ensure_default_pool(AgentId::Codex, RouteDownstreamSurface::Responses)
        .unwrap();
    let claude_pool = service
        .ensure_default_pool(AgentId::Claude, RouteDownstreamSurface::Messages)
        .unwrap();
    service
        .add_member(&codex_pool.id, AdapterSourceKind::Account, stale_source)
        .unwrap();
    service
        .add_member(&claude_pool.id, AdapterSourceKind::Account, stale_source)
        .unwrap();

    let removed = service
        .remove_route_authorization(AdapterSourceKind::Account, stale_source)
        .unwrap();
    assert_eq!(removed, 2);
    assert!(service
        .list_default_overviews()
        .unwrap()
        .pools
        .iter()
        .all(|pool| pool.members.is_empty()));

    // The membership removal is persisted, so a fresh service cannot
    // resurrect the unavailable authorization on the next load.
    drop(service);
    let reopened = RoutePoolService::new(db);
    assert!(reopened
        .list_default_overviews()
        .unwrap()
        .pools
        .iter()
        .all(|pool| pool.members.is_empty()));
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
    assert!(service.index_enabled());
    db.set_setting(FEATURE_ROUTE_INDEX_V2, "off").unwrap();
    assert!(!service.index_enabled());
    db.set_setting(FEATURE_ROUTE_INDEX_V2, "true").unwrap();
    assert!(service.index_enabled());
    db.set_setting(FEATURE_ROUTE_POOL_V2, "off").unwrap();
    assert!(!service.index_enabled());
}

#[test]
fn pair_adapter_flags_are_independent_and_fail_closed() {
    let (_dir, db, service, _profiles) = tmp();
    assert_eq!(service.pair_adapter_flags(), (false, false));
    db.set_setting(FEATURE_CODEX_INGRESS_GROK_UPSTREAM, "on")
        .unwrap();
    assert_eq!(service.pair_adapter_flags(), (true, false));
    db.set_setting(FEATURE_GROK_INGRESS_CODEX_UPSTREAM, "yes")
        .unwrap();
    assert_eq!(service.pair_adapter_flags(), (true, true));
    db.set_setting(FEATURE_CODEX_INGRESS_GROK_UPSTREAM, "off")
        .unwrap();
    assert_eq!(service.pair_adapter_flags(), (false, true));
}

#[test]
fn mixed_provider_requires_index_and_mixed_flags() {
    let (_dir, db, service, _profiles) = tmp();
    assert!(!service.mixed_provider_enabled());
    db.set_setting(FEATURE_ROUTE_INDEX_V2, "true").unwrap();
    assert!(!service.mixed_provider_enabled());
    db.set_setting(FEATURE_MIXED_PROVIDER_POOL, "true").unwrap();
    assert!(service.mixed_provider_enabled());
    db.set_setting(FEATURE_MIXED_PROVIDER_POOL, "off").unwrap();
    assert!(!service.mixed_provider_enabled());
}

#[test]
fn rule_crud_is_gated_by_pool_flag_and_does_not_copy_member_models() {
    let (_dir, _db, service, profiles) = tmp();
    let profile = bridge_profile("profile-a", "acc-a", AgentId::Codex, true);
    profiles.create(&profile).unwrap();
    service
        .create_legacy_pool(&profile, "ahb_stable-token", true)
        .unwrap();
    assert!(service.list_rules("profile-a").unwrap().is_empty());
    let grok = service
        .add_rule(
            "profile-a",
            "m1",
            "responses",
            "grok",
            "grok",
            "grok-4",
            0,
            None,
        )
        .unwrap();
    let _codex = service
        .add_rule(
            "profile-a",
            "m1",
            "responses",
            "codex",
            "codex",
            "gpt-5",
            1,
            Some("shared"),
        )
        .unwrap();
    service.set_rule_enabled(&grok.id, false).unwrap();
    let listed = service.list_rules("profile-a").unwrap();
    assert_eq!(listed.len(), 2);
    assert!(!listed.iter().any(|rule| rule.public_model == "acc-a"));
    assert!(!listed[0].enabled);
    assert_eq!(listed[1].equivalent_group.as_deref(), Some("shared"));
    service.remove_rule(&listed[0].id).unwrap();
    assert_eq!(service.list_rules("profile-a").unwrap().len(), 1);
    let revision = service.get("profile-a").unwrap().unwrap().policy_revision;
    assert!(revision > 1);
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

fn native_profile(id: &str, source_id: &str, agent: AgentId) -> AdapterProfile {
    let mut profile = bridge_profile(id, source_id, agent, false);
    profile.source_kind = AdapterSourceKind::Provider;
    profile.route = AdapterRoute::NativeEndpoint;
    profile.local_port = None;
    profile.auto_start = false;
    profile
}

fn sample_plan(route: AdapterRoute, can_apply: bool, reason: &str) -> AdapterApplyPlan {
    AdapterApplyPlan {
        analysis: AdapterRouteAnalysis {
            route,
            support: AdapterSupport::Stable,
            reason: reason.into(),
            actions: Vec::new(),
            limitations: Vec::new(),
            evidence: Vec::new(),
            rule_id: None,
            gate_kind: AdapterGateKind::None,
        },
        target_agent_id: AgentId::Codex,
        can_apply,
        maturity: AdapterMaturity::Stable,
        reuse_path: AdapterReusePath::LocalBridge,
        reason: reason.into(),
        service_impact: AdapterServiceImpact::RequiresLocalBridge,
        changes: Vec::new(),
    }
}

fn kimi_provider(id: &str) -> Provider {
    Provider {
        id: id.into(),
        agent_id: AgentId::Kimi,
        name: "Kimi membership".into(),
        settings_config: json!({"apiKey": "kimi-secret"}),
        meta: json!({"preset": "kimi-code-membership"}),
        is_current: false,
        created_at: "t0".into(),
        updated_at: "t0".into(),
    }
}

#[test]
fn list_default_overviews_is_empty_when_flag_off() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(&dir.path().join("flag-off-overview.db")).unwrap();
    db.set_setting(FEATURE_ROUTE_POOL_V2, "false").unwrap();
    let service = RoutePoolService::new(db);
    let listed = service.list_default_overviews().unwrap();
    assert!(!listed.enabled);
    assert!(listed.pools.is_empty());
}

#[test]
fn list_default_overviews_omits_non_default_and_hub_token() {
    let (_dir, _db, service, profiles) = tmp();
    let default_profile = bridge_profile("profile-a", "acc-a", AgentId::Codex, true);
    let extra = bridge_profile("profile-b", "acc-b", AgentId::Codex, false);
    profiles.create(&default_profile).unwrap();
    profiles.create(&extra).unwrap();
    service
        .create_legacy_pool(&default_profile, "ahb_secret-must-not-leak", true)
        .unwrap();
    service
        .create_legacy_pool(&extra, "ahb_other-secret", false)
        .unwrap();
    let listed = service.list_default_overviews().unwrap();
    assert!(listed.enabled);
    assert_eq!(listed.pools.len(), 1);
    assert_eq!(listed.pools[0].id, "profile-a");
    assert_eq!(listed.pools[0].members.len(), 1);
    assert_eq!(listed.pools[0].members[0].source_id, "acc-a");
    assert!(listed.pools[0].members[0].enabled);
    let json = serde_json::to_string(&listed).unwrap();
    assert!(!json.contains("hubToken"));
    assert!(!json.contains("ahb_secret-must-not-leak"));
    assert!(!json.contains("ahb_other-secret"));
}

#[test]
fn enroll_native_plan_rejects_closed_matrix_and_non_native_routes() {
    let native = native_profile("native-1", "acc-1", AgentId::Codex);
    let closed = sample_plan(AdapterRoute::LocalBridge, false, "matrix closed");
    let error = enroll_native_plan_is_open(&native, &closed).unwrap_err();
    assert_eq!(error.code(), "unsupported");
    assert!(error.to_string().contains("matrix closed"));

    let official = sample_plan(AdapterRoute::NativeEndpoint, true, "official login");
    let error = enroll_native_plan_is_open(&native, &official).unwrap_err();
    assert_eq!(error.code(), "unsupported");

    let bridge = bridge_profile("bridge-1", "acc-1", AgentId::Codex, true);
    let open = sample_plan(AdapterRoute::LocalBridge, true, "ok");
    let error = enroll_native_plan_is_open(&bridge, &open).unwrap_err();
    assert_eq!(error.code(), "unsupported");

    enroll_native_plan_is_open(&native, &open).unwrap();
}

#[test]
fn evaluate_enroll_native_rejects_when_plan_is_not_local_bridge() {
    let (_dir, db, service, profiles) = tmp();
    ProviderRepo::new(db)
        .create(&kimi_provider("kimi-native"))
        .unwrap();
    let profile = native_profile("native-claude", "kimi-native", AgentId::Claude);
    profiles.create(&profile).unwrap();
    let error = service.evaluate_enroll_native(&profile).unwrap_err();
    assert_eq!(error.code(), "unsupported");
}

#[test]
fn evaluate_enroll_native_allows_kimi_to_codex_local_bridge() {
    let (_dir, db, service, profiles) = tmp();
    ProviderRepo::new(db)
        .create(&kimi_provider("kimi-gateway"))
        .unwrap();
    let profile = native_profile("native-codex", "kimi-gateway", AgentId::Codex);
    profiles.create(&profile).unwrap();
    let plan = service.evaluate_enroll_native(&profile).unwrap();
    assert!(plan.can_apply);
    assert_eq!(plan.analysis.route, AdapterRoute::LocalBridge);
}

#[test]
fn persist_enroll_after_native_bind_sets_v2_without_hub_token() {
    let (_dir, _db, service, profiles) = tmp();
    let bound = bridge_profile("bound-1", "acc-a", AgentId::Codex, true);
    profiles.create(&bound).unwrap();
    let overview = service
        .persist_enroll_after_native_bind(&bound, 43155)
        .unwrap();
    assert!(overview.v2_enrolled);
    assert_eq!(overview.gateway_port, Some(43155));
    assert_eq!(overview.id, "bound-1");
    assert!(service.get("bound-1").unwrap().unwrap().is_default);
    let json = serde_json::to_string(&overview).unwrap();
    assert!(!json.contains("hubToken"));
    assert!(!json.contains("ahb_"));
}

#[test]
fn persist_enroll_after_native_bind_promotes_over_sibling_default() {
    let (_dir, _db, service, profiles) = tmp();
    let previous = bridge_profile("old-default", "acc-old", AgentId::Codex, true);
    let bound = bridge_profile("bound-new", "acc-a", AgentId::Codex, true);
    profiles.create(&previous).unwrap();
    profiles.create(&bound).unwrap();
    service
        .create_legacy_pool(&previous, "ahb_old-token", true)
        .unwrap();
    let overview = service
        .persist_enroll_after_native_bind(&bound, 43155)
        .unwrap();
    assert_eq!(overview.id, "bound-new");
    assert!(service.get("bound-new").unwrap().unwrap().is_default);
    assert!(!service.get("old-default").unwrap().unwrap().is_default);
    let listed = service.list_default_overviews().unwrap();
    assert_eq!(listed.pools.len(), 1);
    assert_eq!(listed.pools[0].id, "bound-new");
    assert!(listed.pools[0].members.iter().any(|member| member.source_id == "acc-old"));
    assert!(listed.pools[0].members.iter().any(|member| member.source_id == "acc-a"));
}

#[test]
fn attach_pool_owned_authorization_creates_default_pool_and_hides_from_home_stamp() {
    let (_dir, db, service, _profiles) = tmp();
    ProviderRepo::new(db.clone())
        .create(&Provider {
            id: "codex-api".into(),
            agent_id: AgentId::Codex,
            name: "Codex API".into(),
            settings_config: json!({"apiKey": "secret"}),
            meta: json!({"preset": "custom"}),
            is_current: false,
            created_at: "t0".into(),
            updated_at: "t0".into(),
        })
        .unwrap();
    let overview = service
        .attach_pool_owned_authorization(
            AgentId::Codex,
            RouteDownstreamSurface::Responses,
            AdapterSourceKind::Provider,
            "codex-api",
        )
        .unwrap();
    assert_eq!(overview.target_agent_id, AgentId::Codex);
    assert_eq!(overview.surface, RouteDownstreamSurface::Responses);
    assert_eq!(overview.members.len(), 1);
    assert_eq!(overview.members[0].source_id, "codex-api");
    let stored = ProviderRepo::new(db).get_by_id("codex-api").unwrap().unwrap();
    assert!(authorization_is_route_pool_home(&stored.meta));
    let again = service
        .attach_pool_owned_authorization(
            AgentId::Codex,
            RouteDownstreamSurface::Responses,
            AdapterSourceKind::Provider,
            "codex-api",
        )
        .unwrap();
    assert_eq!(again.id, overview.id);
    assert_eq!(again.members.len(), 1);
}

#[test]
fn attach_pool_owned_authorization_reuses_existing_default_pool() {
    let (_dir, db, service, profiles) = tmp();
    let profile = bridge_profile("profile-a", "acc-a", AgentId::Codex, true);
    profiles.create(&profile).unwrap();
    service
        .create_legacy_pool(&profile, "ahb_stable-token", true)
        .unwrap();
    ProviderRepo::new(db)
        .create(&Provider {
            id: "codex-api".into(),
            agent_id: AgentId::Codex,
            name: "Codex API".into(),
            settings_config: json!({"apiKey": "secret"}),
            meta: json!({"preset": "custom"}),
            is_current: false,
            created_at: "t0".into(),
            updated_at: "t0".into(),
        })
        .unwrap();
    let overview = service
        .attach_pool_owned_authorization(
            AgentId::Codex,
            RouteDownstreamSurface::Responses,
            AdapterSourceKind::Provider,
            "codex-api",
        )
        .unwrap();
    assert_eq!(overview.id, "profile-a");
    assert!(overview
        .members
        .iter()
        .any(|member| member.source_id == "acc-a"));
    assert!(overview
        .members
        .iter()
        .any(|member| member.source_id == "codex-api"));
}

#[test]
fn sync_connection_authorizations_enrolls_connection_logins_without_hiding_them() {
    let (_dir, db, service, _profiles) = tmp();
    ProviderRepo::new(db.clone())
        .create(&Provider {
            id: "conn-codex".into(),
            agent_id: AgentId::Codex,
            name: "Connection API".into(),
            settings_config: json!({"apiKey": "secret"}),
            meta: json!({"preset": "custom"}),
            is_current: false,
            created_at: "t0".into(),
            updated_at: "t0".into(),
        })
        .unwrap();
    AccountRepo::new(db.clone())
        .create(&Account {
            id: "conn-oauth".into(),
            agent_id: AgentId::Claude,
            kind: AccountKind::Oauth,
            label: "Claude login".into(),
            credentials: json!({"format": "auth_json", "tokens": {"access_token": "x"}}),
            extra: json!({}),
            status: "active".into(),
            is_current: false,
            created_at: "t0".into(),
            updated_at: "t0".into(),
        })
        .unwrap();
    ProviderRepo::new(db.clone())
        .create(&Provider {
            id: "pool-only".into(),
            agent_id: AgentId::Grok,
            name: "Pool API".into(),
            settings_config: json!({"apiKey": "secret"}),
            meta: json!({"preset": "custom", "home": "route_pool"}),
            is_current: false,
            created_at: "t0".into(),
            updated_at: "t0".into(),
        })
        .unwrap();
    let result = service.sync_connection_authorizations().unwrap();
    assert_eq!(result.added, 2);
    assert!(result.skipped >= 1);
    let listed = service.list_default_overviews().unwrap();
    let members: Vec<_> = listed
        .pools
        .iter()
        .flat_map(|pool| pool.members.iter().map(|member| member.source_id.as_str()))
        .collect();
    assert!(members.contains(&"conn-codex"));
    assert!(members.contains(&"conn-oauth"));
    assert!(!members.contains(&"pool-only"));
    let stored = ProviderRepo::new(db).get_by_id("conn-codex").unwrap().unwrap();
    assert!(!authorization_is_route_pool_home(&stored.meta));
    let again = service.sync_connection_authorizations().unwrap();
    assert_eq!(again.added, 0);
}

#[test]
fn persist_enroll_after_native_bind_rejects_native_profile() {
    let (_dir, _db, service, profiles) = tmp();
    let native = native_profile("native-1", "acc-a", AgentId::Codex);
    profiles.create(&native).unwrap();
    let error = service
        .persist_enroll_after_native_bind(&native, 43155)
        .unwrap_err();
    assert_eq!(error.code(), "unsupported");
    assert!(service.get("native-1").unwrap().is_none());
}

#[test]
fn occupancy_fail_skips_persist_and_leaves_unenrolled() {
    let (_dir, _db, service, profiles) = tmp();
    let bound = bridge_profile("bound-occ", "acc-a", AgentId::Codex, true);
    profiles.create(&bound).unwrap();
    service
        .create_legacy_pool(&bound, "ahb_stable-token", true)
        .unwrap();
    let bind_failed: Result<(AdapterProfile, u16), String> = Err("adapter.port_in_use".into());
    assert!(bind_failed.is_err());
    let stored = service.get("bound-occ").unwrap().unwrap();
    assert!(!stored.v2_enrolled);
    assert_eq!(stored.gateway_port, None);
}
#[test]
fn selected_connection_sync_enrolls_only_requested_sources() {
    use crate::models::{
        Account, AccountKind, AdapterSourceKind, AgentId, Provider, SyncConnectionSource,
        FEATURE_ROUTE_POOL_V2,
    };
    use crate::services::RoutePoolService;
    use crate::storage::{AccountRepo, Database, ProviderRepo};
    use serde_json::json;

    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(&dir.path().join("route-pool-selected-sync.db")).unwrap();
    db.set_setting(FEATURE_ROUTE_POOL_V2, "true").unwrap();

    let account = |id: &str| Account {
        id: id.into(),
        agent_id: AgentId::Codex,
        kind: AccountKind::ApiKey,
        label: id.into(),
        credentials: json!({"format": "api_key", "api_key": "redacted-test-secret"}),
        extra: json!({}),
        status: "active".into(),
        is_current: false,
        created_at: "t0".into(),
        updated_at: "t0".into(),
    };
    AccountRepo::new(db.clone()).create(&account("account-selected")).unwrap();
    AccountRepo::new(db.clone()).create(&account("account-unselected")).unwrap();
    ProviderRepo::new(db.clone())
        .create(&Provider {
            id: "provider-unselected".into(),
            agent_id: AgentId::Codex,
            name: "Provider unselected".into(),
            settings_config: json!({"api_key": "redacted-test-secret"}),
            meta: json!({}),
            is_current: false,
            created_at: "t0".into(),
            updated_at: "t0".into(),
        })
        .unwrap();

    let service = RoutePoolService::new(db);
    let result = service
        .sync_connection_authorizations_selected(Some(&[SyncConnectionSource {
            source_kind: AdapterSourceKind::Account,
            source_id: "account-selected".into(),
        }]))
        .unwrap();
    assert_eq!(result.added, 1);
    assert_eq!(result.skipped, 0);

    let pools = service.list_default_overviews().unwrap();
    let members = &pools.pools[0].members;
    assert_eq!(members.len(), 1);
    assert_eq!(members[0].source_kind, AdapterSourceKind::Account);
    assert_eq!(members[0].source_id, "account-selected");
}

#[test]
fn pool_member_overview_exposes_safe_source_label_and_oauth_refresh_tail() {
    let (_dir, db, service, _profiles) = tmp();
    AccountRepo::new(db.clone())
        .create(&Account {
            id: "codex-oauth".into(),
            agent_id: AgentId::Codex,
            kind: AccountKind::Oauth,
            label: "internal-oauth-row".into(),
            credentials: json!({
                "tokens": {"refresh_token": "refresh-token-12345678", "access_token": "access-secret"}
            }),
            extra: json!({"identityLabel": "user@example.com"}),
            status: "active".into(),
            is_current: false,
            created_at: "t0".into(),
            updated_at: "t0".into(),
        })
        .unwrap();
    ProviderRepo::new(db.clone())
        .create(&Provider {
            id: "codex-provider".into(),
            agent_id: AgentId::Codex,
            name: "Team API".into(),
            settings_config: json!({"apiKey": "api-secret"}),
            meta: json!({}),
            is_current: false,
            created_at: "t0".into(),
            updated_at: "t0".into(),
        })
        .unwrap();

    let first = service
        .attach_pool_owned_authorization(
            AgentId::Codex,
            RouteDownstreamSurface::Responses,
            AdapterSourceKind::Account,
            "codex-oauth",
        )
        .unwrap();
    let overview = service
        .attach_pool_owned_authorization(
            AgentId::Codex,
            RouteDownstreamSurface::Responses,
            AdapterSourceKind::Provider,
            "codex-provider",
        )
        .unwrap();
    assert_eq!(overview.id, first.id);
    let oauth = overview
        .members
        .iter()
        .find(|member| member.source_id == "codex-oauth")
        .unwrap();
    assert_eq!(oauth.display_label.as_deref(), Some("user@example.com"));
    assert_eq!(oauth.refresh_token_tail.as_deref(), Some("**5678"));
    let api = overview
        .members
        .iter()
        .find(|member| member.source_id == "codex-provider")
        .unwrap();
    assert_eq!(api.display_label.as_deref(), Some("Team API"));
    assert!(api.refresh_token_tail.is_none());
    let encoded = serde_json::to_string(&overview).unwrap();
    assert!(!encoded.contains("refresh-token-12345678"));
    assert!(!encoded.contains("access-secret"));
    assert!(!encoded.contains("api-secret"));
}
