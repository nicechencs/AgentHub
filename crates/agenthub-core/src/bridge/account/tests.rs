use super::*;
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
