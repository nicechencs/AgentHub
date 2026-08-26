use super::{
    index_from_member_listings, EffectiveRouteIndex, MemberCapability, MemberCapabilitySnapshot,
    MemberListing, RouteRejectionReason, RouteResolveError,
};
use crate::models::ModelRouteRule;

fn grant(
    member_id: &str,
    public_model: &str,
    provider: &str,
    dialect: &str,
) -> MemberCapabilitySnapshot {
    grant_mapped(member_id, public_model, provider, dialect, public_model)
}

fn grant_mapped(
    member_id: &str,
    public_model: &str,
    provider: &str,
    dialect: &str,
    upstream_model: &str,
) -> MemberCapabilitySnapshot {
    MemberCapabilitySnapshot {
        member_id: member_id.into(),
        public_model: public_model.into(),
        endpoint: "responses".into(),
        upstream_provider: provider.into(),
        upstream_dialect: dialect.into(),
        upstream_model: upstream_model.into(),
        upstream_endpoint: format!("https://{provider}.example/v1"),
        transport_key: format!("{provider}:{dialect}"),
        capability: MemberCapability::Supported,
    }
}

fn mixed_index() -> EffectiveRouteIndex {
    EffectiveRouteIndex::build(
        "mixed",
        1,
        &[
            grant_mapped("grok-member", "m1", "grok", "grok", "grok-4"),
            grant_mapped("codex-member", "m1", "codex", "codex", "gpt-5"),
        ],
    )
}

fn rule(
    id: &str,
    provider: &str,
    dialect: &str,
    upstream_model: &str,
    priority: i64,
    equivalent: Option<&str>,
) -> ModelRouteRule {
    ModelRouteRule {
        id: id.into(),
        route_pool_id: "mixed".into(),
        public_model: "m1".into(),
        endpoint_family: "responses".into(),
        upstream_provider: provider.into(),
        upstream_dialect: dialect.into(),
        upstream_model: upstream_model.into(),
        priority,
        equivalent_group: equivalent.map(ToOwned::to_owned),
        enabled: true,
        created_at: "t0".into(),
        updated_at: "t0".into(),
    }
}

fn index_ab() -> EffectiveRouteIndex {
    EffectiveRouteIndex::build(
        "pool-codex",
        1,
        &[
            grant("member-a", "m1", "codex", "codex"),
            grant("member-b", "m2", "codex", "codex"),
        ],
    )
}

#[test]
fn exclusive_model_m1_does_not_select_member_b() {
    let index = index_ab();
    let candidates = index.resolve("responses", "m1").expect("m1 resolves");
    assert_eq!(
        candidates
            .iter()
            .map(|candidate| candidate.member_id.as_str())
            .collect::<Vec<_>>(),
        vec!["member-a"]
    );
    assert!(candidates
        .iter()
        .all(|candidate| candidate.member_id != "member-b"));
}

#[test]
fn exclusive_model_m2_does_not_select_member_a() {
    let index = index_ab();
    let candidates = index.resolve("responses", "m2").expect("m2 resolves");
    assert_eq!(
        candidates
            .iter()
            .map(|candidate| candidate.member_id.as_str())
            .collect::<Vec<_>>(),
        vec!["member-b"]
    );
}

#[test]
fn unknown_model_fails_closed() {
    let index = index_ab();
    assert_eq!(
        index.resolve("responses", "m-unknown"),
        Err(RouteResolveError::UnknownModel)
    );
    assert_eq!(
        index.resolve("responses", ""),
        Err(RouteResolveError::UnknownModel)
    );
}

#[test]
fn empty_index_fails_closed() {
    let index = EffectiveRouteIndex::build("empty", 1, &[]);
    assert_eq!(
        index.resolve("responses", "m1"),
        Err(RouteResolveError::EmptyIndex)
    );
    assert!(index.list_models("responses").is_empty());
}

#[test]
fn unsupported_and_unknown_capability_are_not_candidates() {
    let index = EffectiveRouteIndex::build(
        "pool",
        2,
        &[
            MemberCapabilitySnapshot {
                capability: MemberCapability::Unsupported,
                ..grant("member-a", "m1", "codex", "codex")
            },
            MemberCapabilitySnapshot {
                capability: MemberCapability::Unknown,
                ..grant("member-b", "m1", "codex", "codex")
            },
        ],
    );
    assert_eq!(
        index.resolve("responses", "m1"),
        Err(RouteResolveError::EmptyIndex)
    );
}

#[test]
fn ambiguous_cross_provider_model_fails_closed() {
    let index = EffectiveRouteIndex::build(
        "mixed",
        1,
        &[
            grant("codex-member", "shared", "codex", "codex"),
            grant("grok-member", "shared", "grok", "grok"),
        ],
    );
    assert_eq!(
        index.resolve("responses", "shared"),
        Err(RouteResolveError::AmbiguousModel)
    );
    assert!(
        index.list_models("responses").is_empty(),
        "ambiguous models must not appear in the public catalog"
    );
}

#[test]
fn list_models_is_the_union_of_exclusive_grants() {
    let index = index_ab();
    assert_eq!(index.list_models("responses"), vec!["m1", "m2"]);
    assert!(index.list_models("messages").is_empty());
}

#[test]
fn listed_models_each_have_a_resolver_candidate() {
    let index = index_ab();
    for model in index.list_models("responses") {
        let candidates = index.resolve("responses", &model).expect("listed model");
        assert!(!candidates.is_empty());
        assert!(candidates
            .iter()
            .all(|candidate| candidate.capability_generation == index.generation));
    }
}

#[test]
fn index_from_member_listings_unions_exclusive_models() {
    let index = index_from_member_listings(
        "pool",
        7,
        "responses",
        &[
            listing("acc-a", &["m1"], true),
            listing("acc-b", &["m2"], true),
        ],
        None,
    );
    assert_eq!(index.generation, 7);
    assert_eq!(index.list_models("responses"), vec!["m1", "m2"]);
    assert_eq!(
        index
            .resolve("responses", "m1")
            .expect("m1")
            .iter()
            .map(|candidate| candidate.member_id.as_str())
            .collect::<Vec<_>>(),
        vec!["acc-a"]
    );
    assert_eq!(
        index
            .resolve("responses", "m2")
            .expect("m2")
            .iter()
            .map(|candidate| candidate.member_id.as_str())
            .collect::<Vec<_>>(),
        vec!["acc-b"]
    );
}

#[test]
fn partial_member_snapshot_failure_keeps_last_successful() {
    let prior = vec![
        grant("acc-a", "m1", "codex", "codex"),
        grant("acc-b", "m2", "codex", "codex"),
    ];
    let index = index_from_member_listings(
        "pool",
        3,
        "responses",
        &[
            listing("acc-a", &["m1"], true),
            listing("acc-b", &[], false),
        ],
        Some(&prior),
    );
    assert_eq!(index.list_models("responses"), vec!["m1", "m2"]);
    assert_eq!(
        index
            .resolve("responses", "m2")
            .expect("kept B")
            .iter()
            .map(|candidate| candidate.member_id.as_str())
            .collect::<Vec<_>>(),
        vec!["acc-b"]
    );
}

fn listing(member_id: &str, models: &[&str], snapshot_ok: bool) -> MemberListing {
    MemberListing {
        member_id: member_id.into(),
        listed_models: models.iter().map(|model| (*model).to_owned()).collect(),
        upstream_provider: "codex".into(),
        upstream_dialect: "codex".into(),
        upstream_endpoint: "https://codex.example/v1".into(),
        transport_key: "codex:codex".into(),
        snapshot_ok,
    }
}

#[test]
fn endpoint_isolation_does_not_leak_models() {
    let index = EffectiveRouteIndex::build(
        "pool",
        1,
        &[
            MemberCapabilitySnapshot {
                endpoint: "messages".into(),
                ..grant("claude-member", "m1", "anthropic", "claude")
            },
            grant("codex-member", "m2", "codex", "codex"),
        ],
    );
    assert_eq!(
        index.resolve("messages", "m2"),
        Err(RouteResolveError::UnknownModel)
    );
    assert_eq!(
        index.resolve("responses", "m1"),
        Err(RouteResolveError::UnknownModel)
    );
}

#[test]
fn mixed_provider_flag_off_is_ambiguous_and_omitted_from_models() {
    let index = mixed_index();
    assert_eq!(
        index.resolve("responses", "m1"),
        Err(RouteResolveError::AmbiguousModel)
    );
    assert!(index.list_models("responses").is_empty());
    assert_eq!(
        index.explain("responses", "m1"),
        Some(RouteRejectionReason::AmbiguousNoRule)
    );
}

#[test]
fn mixed_provider_flag_on_without_rules_does_not_guess_from_model_prefix() {
    let index = EffectiveRouteIndex::build(
        "mixed",
        1,
        &[
            grant("grok-member", "gpt-4", "grok", "grok"),
            grant("codex-member", "gpt-4", "codex", "codex"),
        ],
    )
    .with_mixed_provider_rules(true, vec![]);
    assert_eq!(
        index.resolve("responses", "gpt-4"),
        Err(RouteResolveError::AmbiguousModel)
    );
    assert!(index.list_models("responses").is_empty());
    assert_eq!(
        index.explain("responses", "gpt-4"),
        Some(RouteRejectionReason::AmbiguousNoRule)
    );
}

#[test]
fn mixed_provider_rule_keeps_only_declared_lane() {
    let index = mixed_index().with_mixed_provider_rules(
        true,
        vec![rule("r-grok", "grok", "grok", "grok-4", 0, None)],
    );
    let candidates = index
        .resolve("responses", "m1")
        .expect("declared grok lane");
    assert_eq!(
        candidates
            .iter()
            .map(|candidate| candidate.member_id.as_str())
            .collect::<Vec<_>>(),
        vec!["grok-member"]
    );
    assert!(candidates
        .iter()
        .all(|candidate| candidate.upstream_provider == "grok"));
    assert_eq!(index.list_models("responses"), vec!["m1"]);
}

#[test]
fn mixed_provider_equivalent_rules_list_model_and_pick_first_lane() {
    let index = mixed_index().with_mixed_provider_rules(
        true,
        vec![
            rule("r-grok", "grok", "grok", "grok-4", 0, Some("shared")),
            rule("r-codex", "codex", "codex", "gpt-5", 10, Some("shared")),
        ],
    );
    assert_eq!(index.list_models("responses"), vec!["m1"]);
    let candidates = index.resolve("responses", "m1").expect("composite");
    assert_eq!(candidates.len(), 2);
    let first_lane = index.schedule_lane("responses", "m1", &candidates, &[], None);
    assert_eq!(first_lane.len(), 1);
    assert_eq!(first_lane[0].member_id, "grok-member");
}

#[test]
fn mixed_provider_without_equivalence_does_not_cross_lanes() {
    let index = mixed_index().with_mixed_provider_rules(
        true,
        vec![
            rule("r-grok", "grok", "grok", "grok-4", 0, None),
            rule("r-codex", "codex", "codex", "gpt-5", 10, None),
        ],
    );
    let candidates = index.resolve("responses", "m1").expect("two lanes");
    let after_grok = index.schedule_lane(
        "responses",
        "m1",
        &candidates,
        &["grok-member".into()],
        Some("grok-member"),
    );
    assert!(
        after_grok.is_empty(),
        "5xx on lane A must not hop to lane B without equivalence"
    );
}

#[test]
fn mixed_provider_equivalent_lanes_failover_after_exclusion() {
    let index = mixed_index().with_mixed_provider_rules(
        true,
        vec![
            rule("r-grok", "grok", "grok", "grok-4", 0, Some("shared")),
            rule("r-codex", "codex", "codex", "gpt-5", 10, Some("shared")),
        ],
    );
    let candidates = index.resolve("responses", "m1").expect("two lanes");
    let next = index.schedule_lane(
        "responses",
        "m1",
        &candidates,
        &["grok-member".into()],
        Some("grok-member"),
    );
    assert_eq!(next.len(), 1);
    assert_eq!(next[0].member_id, "codex-member");
}

#[test]
fn unknown_model_and_unmatched_rule_are_distinct_rejections() {
    let index = mixed_index().with_mixed_provider_rules(
        true,
        vec![rule("r-other", "openai", "generic", "gpt-4", 0, None)],
    );
    assert_eq!(
        index.resolve("responses", "missing"),
        Err(RouteResolveError::UnknownModel)
    );
    assert_eq!(
        index.explain("responses", "missing"),
        Some(RouteRejectionReason::UnknownModel)
    );
    assert_eq!(
        index.resolve("responses", "m1"),
        Err(RouteResolveError::UnknownModel)
    );
    assert_eq!(
        index.explain("responses", "m1"),
        Some(RouteRejectionReason::NoMatchingRule)
    );
    assert_eq!(
        index.explain("responses", "m1"),
        Some(RouteRejectionReason::NoMatchingRule)
    );
    assert_ne!(
        index.explain("responses", "missing"),
        index.explain("responses", "m1")
    );
}

#[test]
fn disabled_lane_rule_is_lane_disabled_not_silent_drop() {
    let mut disabled = rule("r-codex", "codex", "codex", "gpt-5", 10, None);
    disabled.enabled = false;
    let index = mixed_index().with_mixed_provider_rules(
        true,
        vec![rule("r-grok", "grok", "grok", "grok-4", 0, None), disabled],
    );
    let candidates = index.resolve("responses", "m1").expect("grok still live");
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].member_id, "grok-member");
    assert_eq!(
        index.explain_lane("responses", "m1", "codex", "codex"),
        Some(RouteRejectionReason::LaneDisabled)
    );
    assert!(index.explain("responses", "m1").is_none());
}

#[test]
fn mixed_rules_project_public_model_from_upstream_listings() {
    let index = EffectiveRouteIndex::build(
        "mixed",
        1,
        &[
            grant_mapped("grok-member", "grok-4", "grok", "grok", "grok-4"),
            grant_mapped("codex-member", "gpt-5", "codex", "codex", "gpt-5"),
        ],
    )
    .with_mixed_provider_rules(
        true,
        vec![
            rule("r-grok", "grok", "grok", "grok-4", 0, Some("shared")),
            rule("r-codex", "codex", "codex", "gpt-5", 10, Some("shared")),
        ],
    );
    let candidates = index.resolve("responses", "m1").expect("public m1");
    assert_eq!(
        candidates
            .iter()
            .map(|candidate| (
                candidate.member_id.as_str(),
                candidate.upstream_model.as_str()
            ))
            .collect::<Vec<_>>(),
        vec![("codex-member", "gpt-5"), ("grok-member", "grok-4")]
    );
    assert!(index.list_models("responses").contains(&"m1".to_owned()));
}

#[test]
fn exclusive_single_provider_models_stay_listed_with_mixed_flag_on() {
    let index = index_ab().with_mixed_provider_rules(true, vec![]);
    assert_eq!(index.list_models("responses"), vec!["m1", "m2"]);
    assert_eq!(
        index
            .resolve("responses", "m1")
            .expect("m1")
            .iter()
            .map(|candidate| candidate.member_id.as_str())
            .collect::<Vec<_>>(),
        vec!["member-a"]
    );
}
