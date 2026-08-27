use super::*;
use crate::bridge::route_index::DispatchCandidate;
use crate::bridge::runtime::ResolvedAuth;
use crate::models::RouteSchedulePolicy;

fn member(id: &str, token: &str, health: MemberHealth) -> PickedMember {
    PickedMember::new(
        format!("account:{id}"),
        "account",
        id,
        id,
        ResolvedAuth::bearer(token),
        None,
        health,
    )
}

fn picker(health_a: MemberHealth, health_b: MemberHealth) -> AccountPicker {
    AccountPicker::new(
        vec![
            member("acc-a", "token-a", health_a),
            member("acc-b", "token-b", health_b),
        ],
        true,
    )
}

#[test]
fn pick_new_walks_ticket_id_order_and_advances_cursor() {
    let picker = picker(MemberHealth::Renewable, MemberHealth::Renewable);
    assert_eq!(picker.pick_new().expect("a").source_id, "acc-a");
    assert_eq!(picker.pick_new().expect("b").source_id, "acc-b");
    assert_eq!(picker.pick_new().expect("a again").source_id, "acc-a");
}

#[test]
fn pick_new_skips_needs_login_and_empty_token() {
    let picker = AccountPicker::new(
        vec![
            member("acc-a", "token-a", MemberHealth::NeedsLogin),
            member("acc-b", "", MemberHealth::Renewable),
            member("acc-c", "token-c", MemberHealth::TryOnce),
        ],
        true,
    );
    assert_eq!(picker.pick_new().expect("c").source_id, "acc-c");
}

#[test]
fn failover_does_not_advance_new_request_cursor() {
    let picker = picker(MemberHealth::Renewable, MemberHealth::Renewable);
    let first = picker.pick_new().expect("a");
    assert_eq!(first.source_id, "acc-a");
    let fail = picker.failover("acc-a").expect("b");
    assert_eq!(fail.source_id, "acc-b");
    // Cursor was already advanced by pick_new to B; next new request is B.
    assert_eq!(picker.pick_new().expect("still b").source_id, "acc-b");
}

#[test]
fn isolate_only_marks_that_member() {
    let picker = picker(MemberHealth::Renewable, MemberHealth::Renewable);
    picker.isolate("acc-a");
    assert_eq!(picker.health_of("acc-a"), Some(MemberHealth::NeedsLogin));
    assert_eq!(picker.health_of("acc-b"), Some(MemberHealth::Renewable));
    assert_eq!(picker.pick_new().expect("b").source_id, "acc-b");
    assert_eq!(picker.pick_new().expect("still b").source_id, "acc-b");
}

#[test]
fn restore_makes_member_eligible_without_rebuild() {
    let picker = picker(MemberHealth::NeedsLogin, MemberHealth::Renewable);
    assert_eq!(picker.pick_new().expect("b").source_id, "acc-b");
    picker.restore("acc-a", MemberHealth::Renewable);
    assert_eq!(picker.health_of("acc-a"), Some(MemberHealth::Renewable));
    assert_eq!(picker.pick_new().expect("a").source_id, "acc-a");
}

#[test]
fn closed_gate_keeps_only_lead() {
    let picker = AccountPicker::from_members(
        vec![
            member("acc-a", "token-a", MemberHealth::Renewable),
            member("acc-b", "token-b", MemberHealth::Renewable),
        ],
        false,
        None,
    );
    assert_eq!(picker.len(), 1);
    assert!(!picker.multi_account());
    assert_eq!(picker.pick_new().expect("lead").source_id, "acc-a");
    assert_eq!(picker.pick_new().expect("lead again").source_id, "acc-a");
    assert!(picker.failover("acc-a").is_none());
}

#[test]
fn partition_account_id_only_when_polling() {
    let single = AccountPicker::new(
        vec![member("acc-a", "token-a", MemberHealth::Renewable)],
        true,
    );
    let a = single.pick_new().expect("a");
    assert_eq!(single.partition_account_id(&a), None);

    let multi = picker(MemberHealth::Renewable, MemberHealth::Renewable);
    let a = multi.pick_new().expect("a");
    assert_eq!(multi.partition_account_id(&a), Some("acc-a"));
}

fn candidate() -> DispatchCandidate {
    DispatchCandidate {
        member_id: "acc-b".into(),
        upstream_endpoint: "http://127.0.0.1/v1".into(),
        upstream_model: "m1".into(),
        upstream_provider: "openai".into(),
        upstream_dialect: "generic".into(),
        transport_key: "openai:generic".into(),
        capability_generation: 1,
    }
}

fn candidates(ids: &[&str]) -> Vec<DispatchCandidate> {
    ids.iter()
        .map(|id| DispatchCandidate {
            member_id: (*id).into(),
            ..candidate()
        })
        .collect()
}

#[test]
fn pick_from_candidates_cannot_select_member_absent_from_resolve() {
    let picker = AccountPicker::with_sink(
        vec![
            member("acc-b", "token-b", MemberHealth::Renewable).with_schedule(0, 0),
            member("acc-a", "token-a", MemberHealth::Renewable).with_schedule(0, 1),
        ],
        false,
        None,
    );
    assert_eq!(picker.pick_new().expect("lead-first").source_id, "acc-b");
    let picked = picker
        .pick_from_candidates(&candidates(&["acc-a"]), None, &[])
        .expect("A");
    assert_eq!(picked.source_id, "acc-a");
    assert!(
        picker
            .pick_from_candidates(&candidates(&["acc-missing"]), None, &[])
            .is_none()
    );
}

#[test]
fn pick_from_candidates_only_shrinks_and_respects_exclusion_and_priority() {
    let picker = AccountPicker::with_sink(
        vec![
            member("acc-a", "token-a", MemberHealth::Renewable).with_schedule(1, 0),
            member("acc-b", "token-b", MemberHealth::Renewable).with_schedule(0, 0),
            member("acc-c", "token-c", MemberHealth::NeedsLogin).with_schedule(-1, 0),
        ],
        false,
        None,
    );
    let picked = picker
        .pick_from_candidates(&candidates(&["acc-a", "acc-b", "acc-c"]), None, &[])
        .expect("b wins priority");
    assert_eq!(picked.source_id, "acc-b");
    let after_exclude = picker
        .pick_from_candidates(
            &candidates(&["acc-a", "acc-b"]),
            None,
            &["acc-b".to_owned()],
        )
        .expect("a after exclude");
    assert_eq!(after_exclude.source_id, "acc-a");
}

#[test]
fn scheduler_picks_are_always_in_the_resolver_set() {
    let picker = AccountPicker::with_sink(
        vec![
            member("acc-a", "token-a", MemberHealth::Renewable),
            member("acc-b", "token-b", MemberHealth::Renewable),
            member("acc-c", "token-c", MemberHealth::Renewable),
        ],
        false,
        None,
    );
    for set in [
        vec!["acc-a"],
        vec!["acc-b"],
        vec!["acc-a", "acc-c"],
        vec!["acc-b", "acc-c"],
        vec!["acc-a", "acc-b", "acc-c"],
    ] {
        let resolved = candidates(&set);
        let picked = picker
            .pick_from_candidates(&resolved, None, &[])
            .expect("eligible");
        assert!(
            set.contains(&picked.source_id.as_str()),
            "pick {} not in {set:?}",
            picked.source_id
        );
    }
}

fn candidate_at(
    id: &str,
    provider: &str,
    dialect: &str,
    transport_key: &str,
    generation: u64,
) -> DispatchCandidate {
    DispatchCandidate {
        member_id: id.into(),
        upstream_provider: provider.into(),
        upstream_dialect: dialect.into(),
        transport_key: transport_key.into(),
        capability_generation: generation,
        ..candidate()
    }
}

fn pf_picker() -> AccountPicker {
    AccountPicker::with_policy(
        vec![
            member("acc-a", "token-a", MemberHealth::Renewable).with_schedule(1, 0),
            member("acc-b", "token-b", MemberHealth::Renewable).with_schedule(0, 0),
        ],
        false,
        None,
        RouteSchedulePolicy::PriorityFailover,
    )
}

fn rr_picker(priority_a: i64, priority_b: i64) -> AccountPicker {
    AccountPicker::with_policy(
        vec![
            member("acc-a", "token-a", MemberHealth::Renewable).with_schedule(priority_a, 0),
            member("acc-b", "token-b", MemberHealth::Renewable).with_schedule(priority_b, 1),
        ],
        false,
        None,
        RouteSchedulePolicy::RoundRobin,
    )
}

#[test]
fn affinity_key_is_route_scoped_not_raw_session() {
    let session = "shared-session-id";
    let key_a = route_scoped_affinity_key("route-a", "codex", session);
    let key_b = route_scoped_affinity_key("route-b", "codex", session);
    assert_ne!(key_a, session);
    assert!(
        !key_a.contains(session),
        "raw session must not appear in the sticky map key"
    );
    assert_ne!(key_a, key_b, "different route_id must not share a key");

    let picker = pf_picker();
    let only_a = candidates(&["acc-a"]);
    let both = candidates(&["acc-a", "acc-b"]);

    assert_eq!(
        picker
            .pick_from_candidates(&only_a, Some(&key_a), &[])
            .expect("bind A")
            .source_id,
        "acc-a"
    );
    assert_eq!(
        picker
            .pick_from_candidates(&both, Some(&key_b), &[])
            .expect("other route")
            .source_id,
        "acc-b",
        "same session string on a different route_id must not reuse sticky"
    );
    assert_eq!(
        picker
            .pick_from_candidates(&both, Some(&key_a), &[])
            .expect("sticky A")
            .source_id,
        "acc-a",
        "same route + hashed session must reuse the bound member"
    );
    assert_eq!(
        picker
            .pick_from_candidates(&both, Some(session), &[])
            .expect("raw session is not a key")
            .source_id,
        "acc-b",
        "raw session id must not hit the hashed route-scoped record"
    );

    let other_bearer = pf_picker();
    assert_eq!(
        other_bearer
            .pick_from_candidates(&both, Some(&key_a), &[])
            .expect("other pool")
            .source_id,
        "acc-b",
        "same session hash on another Hub bearer / pool must not hit this record"
    );
}

#[test]
fn sticky_reuses_member_until_invalidated() {
    let picker = pf_picker();
    let key = route_scoped_affinity_key("route-a", "codex", "conv-1");
    let only_a = candidates(&["acc-a"]);
    let both = candidates(&["acc-a", "acc-b"]);
    assert_eq!(
        picker
            .pick_from_candidates(&only_a, Some(&key), &[])
            .expect("bind")
            .source_id,
        "acc-a"
    );
    assert_eq!(
        picker
            .pick_from_candidates(&both, Some(&key), &[])
            .expect("reuse")
            .source_id,
        "acc-a"
    );

    assert_eq!(
        picker
            .pick_from_candidates(&candidates(&["acc-b"]), Some(&key), &[])
            .expect("member left resolver set")
            .source_id,
        "acc-b"
    );
}

#[test]
fn sticky_invalidates_on_disable_and_falls_back_to_policy() {
    let picker = pf_picker();
    let key = route_scoped_affinity_key("route-a", "codex", "conv-disable");
    assert_eq!(
        picker
            .pick_from_candidates(&candidates(&["acc-a"]), Some(&key), &[])
            .expect("bind")
            .source_id,
        "acc-a"
    );
    picker.isolate("acc-a");
    assert_eq!(
        picker
            .pick_from_candidates(&candidates(&["acc-a", "acc-b"]), Some(&key), &[])
            .expect("policy after disable")
            .source_id,
        "acc-b"
    );
}

#[test]
fn sticky_invalidates_on_fingerprint_change() {
    let picker = pf_picker();
    let key = route_scoped_affinity_key("route-a", "codex", "conv-fp");
    assert_eq!(
        picker
            .pick_from_candidates(&candidates(&["acc-a"]), Some(&key), &[])
            .expect("bind")
            .source_id,
        "acc-a"
    );
    picker.rewrite_sticky_fingerprint(&key, "stale-fingerprint");
    assert_eq!(
        picker
            .pick_from_candidates(&candidates(&["acc-a", "acc-b"]), Some(&key), &[])
            .expect("policy after fingerprint change")
            .source_id,
        "acc-b"
    );
}

#[test]
fn sticky_invalidates_on_dialect_change_keeps_generation_if_still_valid() {
    let picker = pf_picker();
    let key = route_scoped_affinity_key("route-a", "codex", "conv-dialect");
    let gen1_a = vec![candidate_at(
        "acc-a",
        "openai",
        "generic",
        "openai:generic",
        1,
    )];
    assert_eq!(
        picker
            .pick_from_candidates(&gen1_a, Some(&key), &[])
            .expect("bind")
            .source_id,
        "acc-a"
    );

    let gen2_same_lane = vec![
        candidate_at("acc-a", "openai", "generic", "openai:generic", 2),
        candidate_at("acc-b", "openai", "generic", "openai:generic", 2),
    ];
    assert_eq!(
        picker
            .pick_from_candidates(&gen2_same_lane, Some(&key), &[])
            .expect("still valid for current generation")
            .source_id,
        "acc-a"
    );

    let dialect_changed = vec![
        candidate_at("acc-a", "openai", "codex", "openai:codex", 3),
        candidate_at("acc-b", "openai", "generic", "openai:generic", 3),
    ];
    assert_eq!(
        picker
            .pick_from_candidates(&dialect_changed, Some(&key), &[])
            .expect("dialect change drops sticky")
            .source_id,
        "acc-b"
    );
}

#[test]
fn unbound_sticky_falls_back_to_policy_pick() {
    let picker = pf_picker();
    let key = route_scoped_affinity_key("route-a", "codex", "fresh-header");
    let both = candidates(&["acc-a", "acc-b"]);
    assert_eq!(
        picker
            .pick_from_candidates(&both, Some(&key), &[])
            .expect("no binding yet")
            .source_id,
        "acc-b",
        "a session header without a recorded binding must not freeze failover"
    );
    assert_eq!(
        picker
            .pick_from_candidates(&both, Some(&key), &["acc-b".to_owned()])
            .expect("first-turn failover")
            .source_id,
        "acc-a"
    );
}

#[test]
fn sticky_exclusion_does_not_steal_binding() {
    let picker = pf_picker();
    let key = route_scoped_affinity_key("route-a", "codex", "conv-exclude");
    let only_a = candidates(&["acc-a"]);
    let both = candidates(&["acc-a", "acc-b"]);
    assert_eq!(
        picker
            .pick_from_candidates(&only_a, Some(&key), &[])
            .expect("bind A")
            .source_id,
        "acc-a"
    );
    assert_eq!(
        picker
            .pick_from_candidates(&both, Some(&key), &["acc-a".to_owned()])
            .expect("this request skips A")
            .source_id,
        "acc-b"
    );
    assert_eq!(
        picker
            .pick_from_candidates(&both, Some(&key), &[])
            .expect("sticky A still held")
            .source_id,
        "acc-a",
        "cooldown / this-request exclusion must not rebind sticky to the failover member"
    );
}

#[test]
fn sticky_cooldown_skips_member_but_keeps_binding() {
    let picker = pf_picker();
    let key = route_scoped_affinity_key("route-a", "codex", "conv-cool");
    let both = candidates(&["acc-a", "acc-b"]);
    assert_eq!(
        picker
            .pick_from_candidates(&candidates(&["acc-a"]), Some(&key), &[])
            .expect("bind A")
            .source_id,
        "acc-a"
    );
    picker.set_cooldown("acc-a", Some("m1"), std::time::Duration::from_secs(60));
    let excluded = picker.cooldown_exclusions("m1");
    assert_eq!(
        picker
            .pick_from_candidates(&both, Some(&key), &excluded)
            .expect("skip cooling A")
            .source_id,
        "acc-b"
    );
    assert_eq!(
        picker
            .pick_from_candidates(&both, Some(&key), &[])
            .expect("sticky kept after skip")
            .source_id,
        "acc-a"
    );
}

#[test]
fn priority_failover_keeps_stable_order() {
    let picker = AccountPicker::with_sink(
        vec![
            member("acc-a", "token-a", MemberHealth::Renewable).with_schedule(0, 1),
            member("acc-b", "token-b", MemberHealth::Renewable).with_schedule(0, 0),
        ],
        false,
        None,
    );
    let both = candidates(&["acc-a", "acc-b"]);
    assert_eq!(
        picker
            .pick_from_candidates(&both, None, &[])
            .expect("b by position")
            .source_id,
        "acc-b"
    );
    assert_eq!(
        picker
            .pick_from_candidates(&both, None, &[])
            .expect("still b")
            .source_id,
        "acc-b",
        "default priority_failover must not rotate"
    );
}

#[test]
fn round_robin_alternates_isomorphic_members() {
    let picker = rr_picker(0, 0);
    let both = candidates(&["acc-a", "acc-b"]);
    assert_eq!(
        picker
            .pick_from_candidates(&both, None, &[])
            .expect("first")
            .source_id,
        "acc-a"
    );
    assert_eq!(
        picker
            .pick_from_candidates(&both, None, &[])
            .expect("second")
            .source_id,
        "acc-b"
    );
    assert_eq!(
        picker
            .pick_from_candidates(&both, None, &[])
            .expect("third")
            .source_id,
        "acc-a"
    );
}

#[test]
fn round_robin_does_not_advance_v1_pick_new_cursor() {
    let picker = rr_picker(0, 0);
    let both = candidates(&["acc-a", "acc-b"]);
    assert_eq!(
        picker
            .pick_from_candidates(&both, None, &[])
            .expect("rr a")
            .source_id,
        "acc-a"
    );
    assert_eq!(
        picker.pick_new().expect("v1 cursor untouched").source_id,
        "acc-a",
        "v2 round-robin must not reuse the v1 pick_new cursor"
    );
}

#[test]
fn round_robin_sticky_beats_rotation() {
    let picker = rr_picker(0, 0);
    let both = candidates(&["acc-a", "acc-b"]);
    let key = route_scoped_affinity_key("route-a", "codex", "sticky-rr");
    assert_eq!(
        picker
            .pick_from_candidates(&both, Some(&key), &[])
            .expect("first")
            .source_id,
        "acc-a"
    );
    assert_eq!(
        picker
            .pick_from_candidates(&both, Some(&key), &[])
            .expect("sticky")
            .source_id,
        "acc-a"
    );
}

#[test]
fn round_robin_prefers_higher_priority_over_worse_rotation() {
    let picker = AccountPicker::with_policy(
        vec![
            member("acc-a", "token-a", MemberHealth::Renewable).with_schedule(0, 0),
            member("acc-b", "token-b", MemberHealth::Renewable).with_schedule(1, 0),
            member("acc-c", "token-c", MemberHealth::Renewable).with_schedule(1, 1),
        ],
        false,
        None,
        RouteSchedulePolicy::RoundRobin,
    );
    let all = candidates(&["acc-a", "acc-b", "acc-c"]);
    assert_eq!(
        picker
            .pick_from_candidates(&all, None, &[])
            .expect("lead")
            .source_id,
        "acc-a"
    );
    assert_eq!(
        picker
            .pick_from_candidates(&all, None, &[])
            .expect("still lead")
            .source_id,
        "acc-a"
    );
    assert_eq!(
        picker
            .pick_from_candidates(&all, None, &["acc-a".to_owned()])
            .expect("b")
            .source_id,
        "acc-b"
    );
    assert_eq!(
        picker
            .pick_from_candidates(&all, None, &["acc-a".to_owned()])
            .expect("c")
            .source_id,
        "acc-c"
    );
}

#[test]
fn round_robin_does_not_cross_transport_dialect() {
    let picker = rr_picker(0, 0);
    let mixed = vec![
        candidate_at("acc-a", "openai", "generic", "openai:generic", 1),
        candidate_at("acc-b", "anthropic", "claude", "anthropic:claude", 1),
    ];
    assert_eq!(
        picker
            .pick_from_candidates(&mixed, None, &[])
            .expect("lead dialect")
            .source_id,
        "acc-a"
    );
    assert_eq!(
        picker
            .pick_from_candidates(&mixed, None, &[])
            .expect("must not rotate into the other dialect")
            .source_id,
        "acc-a"
    );
    assert_eq!(
        picker
            .pick_from_candidates(&mixed, None, &["acc-a".to_owned()])
            .expect("other dialect only after lead excluded")
            .source_id,
        "acc-b"
    );
}

#[test]
fn member_cooldown_skips_pick_but_does_not_mark_needs_login() {
    let picker = picker(MemberHealth::Renewable, MemberHealth::Renewable);
    picker.set_cooldown("acc-a", None, std::time::Duration::from_secs(60));
    assert!(picker.is_cooling("acc-a", Some("m1")));
    assert!(!picker.is_cooling("acc-b", Some("m1")));
    assert_eq!(picker.health_of("acc-a"), Some(MemberHealth::Renewable));
    assert_eq!(picker.cooldown_exclusions("m1"), vec!["acc-a".to_owned()]);
    let picked = picker
        .pick_from_candidates(
            &candidates(&["acc-a", "acc-b"]),
            None,
            &picker.cooldown_exclusions("m1"),
        )
        .expect("b");
    assert_eq!(picked.source_id, "acc-b");
}

#[test]
fn member_model_cooldown_does_not_block_other_models() {
    let picker = picker(MemberHealth::Renewable, MemberHealth::Renewable);
    picker.set_cooldown("acc-a", Some("m1"), std::time::Duration::from_secs(60));
    assert!(picker.is_cooling("acc-a", Some("m1")));
    assert!(!picker.is_cooling("acc-a", Some("m2")));
    assert_eq!(picker.cooldown_exclusions("m2"), Vec::<String>::new());
}

#[test]
fn set_cooldown_keeps_the_later_deadline() {
    let picker = picker(MemberHealth::Renewable, MemberHealth::Renewable);
    picker.set_cooldown("acc-a", None, std::time::Duration::from_secs(60));
    picker.set_cooldown("acc-a", None, std::time::Duration::from_millis(10));
    std::thread::sleep(std::time::Duration::from_millis(40));
    assert!(
        picker.is_cooling("acc-a", Some("m1")),
        "shorter Retry-After must not replace a later member cooldown"
    );

    picker.set_cooldown("acc-b", Some("m1"), std::time::Duration::from_secs(60));
    picker.set_cooldown("acc-b", Some("m1"), std::time::Duration::from_millis(10));
    std::thread::sleep(std::time::Duration::from_millis(40));
    assert!(
        picker.is_cooling("acc-b", Some("m1")),
        "shorter Retry-After must not replace a later member-model cooldown"
    );
}

#[test]
fn isolate_sink_receives_only_that_account_id() {
    let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let seen_cb = seen.clone();
    let picker = AccountPicker::with_sink(
        vec![
            member("acc-a", "token-a", MemberHealth::Renewable),
            member("acc-b", "token-b", MemberHealth::Renewable),
        ],
        true,
        Some(std::sync::Arc::new(move |id: &str| {
            seen_cb.lock().expect("lock").push(id.to_owned());
        })),
    );
    picker.isolate("acc-a");
    assert_eq!(*seen.lock().expect("lock"), vec!["acc-a".to_owned()]);
}
