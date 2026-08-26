use super::{
    index_from_member_listings, EffectiveRouteIndex, MemberCapability, MemberCapabilitySnapshot,
    MemberListing, RouteResolveError,
};

fn grant(
    member_id: &str,
    public_model: &str,
    provider: &str,
    dialect: &str,
) -> MemberCapabilitySnapshot {
    MemberCapabilitySnapshot {
        member_id: member_id.into(),
        public_model: public_model.into(),
        endpoint: "responses".into(),
        upstream_provider: provider.into(),
        upstream_dialect: dialect.into(),
        upstream_model: public_model.into(),
        upstream_endpoint: format!("https://{provider}.example/v1"),
        transport_key: format!("{provider}:{dialect}"),
        capability: MemberCapability::Supported,
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
