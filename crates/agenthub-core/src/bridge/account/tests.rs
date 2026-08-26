use super::*;
use crate::bridge::route_index::DispatchCandidate;
use crate::bridge::runtime::ResolvedAuth;

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
    assert!(picker
        .pick_from_candidates(&candidates(&["acc-missing"]), None, &[])
        .is_none());
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

#[test]
#[ignore = "route-scoped sticky is deferred; do not ship a global session map"]
fn affinity_key_is_route_scoped_not_raw_session() {
    let _key = "(route_id, downstream_dialect, hash(session))";
    panic!("sticky affinity is not implemented in this slice");
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
