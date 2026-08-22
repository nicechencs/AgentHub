use super::*;
use crate::bridge::types::IrEvent;

fn fsm_on() -> RequestFsm {
    RequestFsm::new(true)
}

#[test]
fn switches_before_first_event() {
    let fsm = fsm_on();
    assert_eq!(
        fsm.on_failure(false, SwitchClass::AccountFailure, true),
        RequestDecision::SwitchAccount
    );
}

#[test]
fn refuses_switch_after_first_event() {
    let mut fsm = fsm_on();
    fsm.observe(&IrEvent::TextDelta {
        text: "hi".into(),
    });
    assert_eq!(fsm.emission, EmissionState::Emitted);
    assert_eq!(
        fsm.on_failure(true, SwitchClass::AccountFailure, true),
        RequestDecision::Fail
    );
}

#[test]
fn one_switch_per_request() {
    let mut fsm = fsm_on();
    assert_eq!(
        fsm.on_failure(false, SwitchClass::AccountFailure, true),
        RequestDecision::SwitchAccount
    );
    fsm.record_switch();
    assert_eq!(
        fsm.on_failure(false, SwitchClass::AccountFailure, true),
        RequestDecision::Fail
    );
}

#[test]
fn retry_gate_then_switch_then_new_account_retry() {
    let mut fsm = fsm_on();
    assert_eq!(
        fsm.on_failure(true, SwitchClass::AccountFailure, true),
        RequestDecision::ReloadSameAccount
    );
    fsm.record_retry();
    assert_eq!(
        fsm.on_failure(true, SwitchClass::AccountFailure, true),
        RequestDecision::SwitchAccount
    );
    fsm.record_switch();
    assert!(!fsm.retry_used());
    assert_eq!(
        fsm.on_failure(true, SwitchClass::AccountFailure, true),
        RequestDecision::ReloadSameAccount
    );
    fsm.record_retry();
    assert_eq!(
        fsm.on_failure(true, SwitchClass::AccountFailure, false),
        RequestDecision::Fail
    );
}

#[test]
fn closed_gate_never_switches() {
    let fsm = RequestFsm::new(false);
    assert_eq!(
        fsm.on_failure(true, SwitchClass::AccountFailure, true),
        RequestDecision::ReloadSameAccount
    );
    let mut fsm = RequestFsm::new(false);
    fsm.record_retry();
    assert_eq!(
        fsm.on_failure(true, SwitchClass::AccountFailure, true),
        RequestDecision::Fail
    );
}

#[test]
fn rate_limit_and_server_errors_do_not_switch() {
    let fsm = fsm_on();
    assert_eq!(
        fsm.on_failure(false, SwitchClass::NotAccountFailure, true),
        RequestDecision::Fail
    );
}

#[test]
fn api_key_401_skips_retry_gate_and_may_switch() {
    let fsm = fsm_on();
    assert_eq!(
        fsm.on_failure(false, SwitchClass::AccountFailure, true),
        RequestDecision::SwitchAccount
    );
}

#[test]
fn no_failover_member_fails_closed() {
    let mut fsm = fsm_on();
    fsm.record_retry();
    assert_eq!(
        fsm.on_failure(true, SwitchClass::AccountFailure, false),
        RequestDecision::Fail
    );
}
