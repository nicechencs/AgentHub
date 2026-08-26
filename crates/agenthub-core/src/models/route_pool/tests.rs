use crate::models::{
    authorization_fingerprint, choose_default_pool_id, feature_flag_enabled, generate_hub_token,
    AdapterSourceKind, AgentId, RouteDownstreamDialect, RouteDownstreamSurface,
    RouteSchedulePolicy, FEATURE_ROUTE_POOL_V2,
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
