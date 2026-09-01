use crate::models::{
    authorization_fingerprint, choose_default_pool_id, feature_flag_enabled, generate_hub_token,
    model_route_id_is_exact, product_flag_enabled, AdapterSourceKind, AgentId,
    RouteDownstreamDialect, RouteDownstreamSurface, RouteSchedulePolicy,
    FEATURE_CODEX_INGRESS_GROK_UPSTREAM, FEATURE_GROK_INGRESS_CODEX_UPSTREAM,
    FEATURE_MIXED_PROVIDER_POOL, FEATURE_ROUTE_INDEX_V2, FEATURE_ROUTE_POOL_V2,
    SHARE_CHAT_COMPLETIONS,
};

#[test]
fn feature_flags_are_fail_closed() {
    assert!(!feature_flag_enabled(None));
    assert!(!feature_flag_enabled(Some("")));
    assert!(!feature_flag_enabled(Some("false")));
    assert!(!feature_flag_enabled(Some("off")));
    assert!(feature_flag_enabled(Some("true")));
    assert!(feature_flag_enabled(Some("1")));
    assert!(feature_flag_enabled(Some("ON")));
    assert_eq!(FEATURE_ROUTE_POOL_V2, "feature.route_pool_v2");
    assert_eq!(FEATURE_ROUTE_INDEX_V2, "feature.route_index_v2");
    assert_eq!(
        FEATURE_CODEX_INGRESS_GROK_UPSTREAM,
        "feature.codex_ingress_grok_upstream"
    );
    assert_eq!(
        FEATURE_GROK_INGRESS_CODEX_UPSTREAM,
        "feature.grok_ingress_codex_upstream"
    );
    assert_eq!(FEATURE_MIXED_PROVIDER_POOL, "feature.mixed_provider_pool");
    assert_eq!(SHARE_CHAT_COMPLETIONS, "share_chat_completions");
    assert!(feature_flag_enabled(Some("yes")));
}

#[test]
fn product_flags_default_on() {
    assert!(product_flag_enabled(None));
    assert!(product_flag_enabled(Some("")));
    assert!(product_flag_enabled(Some("true")));
    assert!(product_flag_enabled(Some("1")));
    assert!(product_flag_enabled(Some("ON")));
    assert!(!product_flag_enabled(Some("0")));
    assert!(!product_flag_enabled(Some("false")));
    assert!(!product_flag_enabled(Some("OFF")));
    assert!(!product_flag_enabled(Some("no")));
}

#[test]
fn model_route_ids_are_exact_only() {
    assert!(model_route_id_is_exact("m1"));
    assert!(model_route_id_is_exact("grok-4"));
    assert!(!model_route_id_is_exact(""));
    assert!(!model_route_id_is_exact("gpt-*"));
    assert!(!model_route_id_is_exact("gpt-?"));
    assert!(!model_route_id_is_exact("gpt-[4]"));
}

#[test]
fn surface_and_dialect_follow_target_agent() {
    assert_eq!(
        RouteDownstreamSurface::for_agent(AgentId::Claude),
        Some(RouteDownstreamSurface::Messages)
    );
    assert_eq!(
        RouteDownstreamSurface::for_agent(AgentId::Codex),
        Some(RouteDownstreamSurface::Responses)
    );
    assert_eq!(
        RouteDownstreamSurface::for_agent(AgentId::Grok),
        Some(RouteDownstreamSurface::Responses)
    );
    assert_eq!(
        RouteDownstreamDialect::for_agent(AgentId::Codex),
        RouteDownstreamDialect::Codex
    );
    assert_eq!(
        RouteDownstreamDialect::for_agent(AgentId::Grok),
        RouteDownstreamDialect::Grok
    );
    assert!(RouteDownstreamSurface::for_agent(AgentId::Cursor).is_none());
}

#[test]
fn default_schedule_is_priority_failover() {
    assert_eq!(
        RouteSchedulePolicy::default(),
        RouteSchedulePolicy::PriorityFailover
    );
    assert_eq!(
        RouteSchedulePolicy::parse("round_robin"),
        Some(RouteSchedulePolicy::RoundRobin)
    );
}

#[test]
fn default_pool_prefers_active_binding_then_stable_id() {
    assert_eq!(
        choose_default_pool_id(["b-pool", "a-pool"], Some("b-pool")).as_deref(),
        Some("b-pool")
    );
    assert_eq!(
        choose_default_pool_id(["b-pool", "a-pool"], None).as_deref(),
        Some("a-pool")
    );
    assert_eq!(
        choose_default_pool_id(["b-pool", "a-pool"], Some("missing")).as_deref(),
        Some("a-pool")
    );
    assert_eq!(choose_default_pool_id(Vec::<&str>::new(), None), None);
}

#[test]
fn hub_token_is_generated_and_redacted() {
    let token = generate_hub_token().expect("token");
    assert!(token.starts_with("ahb_"));
    assert_ne!(token, generate_hub_token().expect("other"));
    assert_eq!(
        authorization_fingerprint(AdapterSourceKind::Account, "acc-1"),
        "account:acc-1"
    );
}

#[test]
fn default_overview_json_never_includes_hub_token() {
    let overview = crate::models::DefaultRoutePoolOverview {
        id: "pool-1".into(),
        target_agent_id: AgentId::Codex,
        surface: RouteDownstreamSurface::Responses,
        dialect: RouteDownstreamDialect::Codex,
        v2_enrolled: true,
        gateway_port: Some(43121),
        members: vec![crate::models::RouteMemberOverview {
            id: "member-1".into(),
            source_kind: AdapterSourceKind::Account,
            source_id: "acc-1".into(),
            display_label: None,
            refresh_token_tail: None,
            enabled: true,
            priority: 0,
            availability: None,
        }],
        listed_models: vec!["gpt-4o".into()],
    };
    let json = serde_json::to_string(&overview).expect("json");
    assert!(!json.contains("hubToken"));
    assert!(!json.contains("hub_token"));
    assert!(!json.contains("ahb_"));
    assert!(json.contains("v2Enrolled"));
    assert!(json.contains("gatewayPort"));
    assert!(!json.contains("127.0.0.1"));
}

#[test]
fn debug_redacts_hub_token() {
    let pool = crate::models::RoutePool {
        id: "pool-1".into(),
        target_agent_id: AgentId::Codex,
        downstream_surface: RouteDownstreamSurface::Responses,
        downstream_dialect: RouteDownstreamDialect::Codex,
        hub_token: "ahb_secret-token-value".into(),
        schedule_policy: RouteSchedulePolicy::PriorityFailover,
        is_default: true,
        v2_enrolled: false,
        policy_revision: 1,
        auto_start: true,
        gateway_port: None,
        created_at: "t0".into(),
        updated_at: "t0".into(),
    };
    let debug = format!("{pool:?}");
    assert!(debug.contains("REDACTED"));
    assert!(!debug.contains("ahb_secret-token-value"));
}
