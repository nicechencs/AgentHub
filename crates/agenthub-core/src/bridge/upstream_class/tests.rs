use axum::http::{HeaderValue, StatusCode};

use std::time::{Duration, SystemTime};

use super::{
    classify_connect_timeout, classify_connect_unavailable, classify_http, cooldown_for_class,
    cooldown_from_retry_after, parse_retry_after, FailoverDecision, UpstreamErrorClass,
    CREDIT_DEFAULT_COOLDOWN, DEFAULT_COOLDOWN, MAX_COOLDOWN, QUOTA_DEFAULT_COOLDOWN,
};

#[test]
fn ordinary_400_and_422_are_request_scoped() {
    assert_eq!(
        classify_http(StatusCode::BAD_REQUEST, Some("bad schema"), false),
        UpstreamErrorClass::Request
    );
    assert_eq!(
        classify_http(StatusCode::UNPROCESSABLE_ENTITY, None, false),
        UpstreamErrorClass::Request
    );
    assert_eq!(
        UpstreamErrorClass::Request.decision(false),
        FailoverDecision::ReturnToClient
    );
}

#[test]
fn grok_reasoning_400_is_same_member_retry() {
    assert_eq!(
        classify_http(
            StatusCode::BAD_REQUEST,
            Some("encrypted reasoning decode"),
            true
        ),
        UpstreamErrorClass::GrokReasoningRecoverable
    );
    assert_eq!(
        classify_http(StatusCode::BAD_REQUEST, Some("encrypted reasoning"), false),
        UpstreamErrorClass::Request
    );
}

#[test]
fn unauthorized_is_auth_reload_then_failover() {
    assert_eq!(
        classify_http(StatusCode::UNAUTHORIZED, None, false),
        UpstreamErrorClass::Auth
    );
    assert_eq!(
        UpstreamErrorClass::Auth.decision(false),
        FailoverDecision::ReloadThenFailover
    );
}

#[test]
fn not_found_is_entitlement_not_whole_account() {
    assert_eq!(
        classify_http(StatusCode::NOT_FOUND, Some("no such model"), false),
        UpstreamErrorClass::Entitlement
    );
}

#[test]
fn entitlement_403_needs_model_evidence() {
    let body = r#"{"error":{"code":"model_not_found","message":"The model does not exist or you do not have access to it."}}"#;
    assert_eq!(
        classify_http(StatusCode::FORBIDDEN, Some(body), false),
        UpstreamErrorClass::Entitlement
    );
}

#[test]
fn policy_403_stays_request_scoped() {
    let body = r#"{"error":{"message":"content policy violation","type":"invalid_request_error"}}"#;
    assert_eq!(
        classify_http(StatusCode::FORBIDDEN, Some(body), false),
        UpstreamErrorClass::Request
    );
    let access = r#"{"error":{"message":"you do not have access to generate this content"}}"#;
    assert_eq!(
        classify_http(StatusCode::FORBIDDEN, Some(access), false),
        UpstreamErrorClass::Request
    );
    assert!(!UpstreamErrorClass::Request.allows_member_switch(false));
}

#[test]
fn quota_429_defaults_to_account_and_can_be_model() {
    assert_eq!(
        classify_http(StatusCode::TOO_MANY_REQUESTS, Some("rate limit"), false),
        UpstreamErrorClass::QuotaAccount
    );
    assert_eq!(
        classify_http(
            StatusCode::TOO_MANY_REQUESTS,
            Some("exceeded token rate limit for this model"),
            false
        ),
        UpstreamErrorClass::QuotaModel
    );
}

#[test]
fn five_xx_and_connect_errors_are_transient() {
    assert_eq!(
        classify_http(StatusCode::INTERNAL_SERVER_ERROR, None, false),
        UpstreamErrorClass::Transient
    );
    assert_eq!(
        classify_http(StatusCode::BAD_GATEWAY, None, false),
        UpstreamErrorClass::Transient
    );
    assert_eq!(classify_connect_timeout(), UpstreamErrorClass::Transient);
    assert_eq!(
        classify_connect_unavailable(),
        UpstreamErrorClass::Transient
    );
}

#[test]
fn committed_downstream_never_failovers() {
    for class in [
        UpstreamErrorClass::Auth,
        UpstreamErrorClass::Entitlement,
        UpstreamErrorClass::QuotaAccount,
        UpstreamErrorClass::Transient,
        UpstreamErrorClass::Request,
    ] {
        assert_eq!(class.decision(true), FailoverDecision::ReturnToClient);
        assert!(!class.allows_member_switch(true));
    }
}

#[test]
fn retry_after_delta_seconds_and_missing_default() {
    let header = HeaderValue::from_static("1");
    assert_eq!(
        parse_retry_after(&header),
        Some(std::time::Duration::from_secs(1))
    );
    assert_eq!(cooldown_from_retry_after(None), DEFAULT_COOLDOWN);
    assert_eq!(
        cooldown_from_retry_after(Some(&HeaderValue::from_static("0"))),
        DEFAULT_COOLDOWN
    );
}

#[test]
fn quota_retry_after_wins_over_body_and_reset() {
    let header = HeaderValue::from_static("30");
    let reset = SystemTime::now() + Duration::from_secs(20 * 60);
    assert_eq!(
        cooldown_for_class(
            UpstreamErrorClass::QuotaAccount,
            Some(&header),
            Some(r#"{"error":{"resets_in_seconds":90}}"#),
            Some(reset),
            true,
        ),
        Duration::from_secs(30)
    );
}

#[test]
fn quota_missing_retry_after_uses_reset_at() {
    let reset = SystemTime::now() + Duration::from_secs(20 * 60);
    let got = cooldown_for_class(
        UpstreamErrorClass::QuotaModel,
        None,
        None,
        Some(reset),
        false,
    );
    assert!(got >= Duration::from_secs(19 * 60));
    assert!(got <= Duration::from_secs(21 * 60));
    let zero = HeaderValue::from_static("0");
    let from_zero = cooldown_for_class(
        UpstreamErrorClass::QuotaAccount,
        Some(&zero),
        None,
        Some(reset),
        false,
    );
    assert!(from_zero >= Duration::from_secs(19 * 60));
}

#[test]
fn quota_body_reset_beats_snapshot_reset() {
    let reset = SystemTime::now() + Duration::from_secs(20 * 60);
    assert_eq!(
        cooldown_for_class(
            UpstreamErrorClass::QuotaAccount,
            None,
            Some(r#"{"resets_in_seconds":90}"#),
            Some(reset),
            false,
        ),
        Duration::from_secs(90)
    );
}

#[test]
fn quota_body_accepts_reset_after_seconds_spelling() {
    let reset = SystemTime::now() + Duration::from_secs(20 * 60);
    assert_eq!(
        cooldown_for_class(
            UpstreamErrorClass::QuotaAccount,
            None,
            Some(r#"{"error":{"reset_after_seconds":90}}"#),
            Some(reset),
            false,
        ),
        Duration::from_secs(90)
    );
    assert_eq!(
        cooldown_for_class(
            UpstreamErrorClass::QuotaModel,
            None,
            Some(r#"{"resetAfterSeconds":45}"#),
            Some(reset),
            false,
        ),
        Duration::from_secs(45)
    );
}

#[test]
fn quota_body_accepts_reset_at_spelling() {
    let far = SystemTime::now() + Duration::from_secs(20 * 60);
    let near_secs = (SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("epoch")
        + Duration::from_secs(120))
    .as_secs();
    let got = cooldown_for_class(
        UpstreamErrorClass::QuotaAccount,
        None,
        Some(&format!(r#"{{"error":{{"reset_at":{near_secs}}}}}"#)),
        Some(far),
        false,
    );
    assert!(
        got >= Duration::from_secs(60) && got <= Duration::from_secs(150),
        "reset_at unix seconds should beat snapshot, got {got:?}"
    );
}

#[test]
fn quota_missing_hints_uses_capped_class_default() {
    assert_eq!(
        cooldown_for_class(UpstreamErrorClass::QuotaAccount, None, None, None, false),
        QUOTA_DEFAULT_COOLDOWN
    );
    assert_eq!(
        cooldown_for_class(UpstreamErrorClass::QuotaModel, None, None, None, true),
        CREDIT_DEFAULT_COOLDOWN
    );
    assert_eq!(
        cooldown_for_class(
            UpstreamErrorClass::QuotaAccount,
            None,
            Some("insufficient credit"),
            None,
            false,
        ),
        CREDIT_DEFAULT_COOLDOWN
    );
    assert!(QUOTA_DEFAULT_COOLDOWN <= MAX_COOLDOWN);
    assert!(CREDIT_DEFAULT_COOLDOWN <= MAX_COOLDOWN);
    assert_eq!(
        cooldown_for_class(
            UpstreamErrorClass::QuotaAccount,
            Some(&HeaderValue::from_static("7200")),
            None,
            None,
            false,
        ),
        MAX_COOLDOWN
    );
}

#[test]
fn non_quota_cooldown_stays_short() {
    let reset = SystemTime::now() + Duration::from_secs(20 * 60);
    assert_eq!(
        cooldown_for_class(
            UpstreamErrorClass::Transient,
            None,
            Some(r#"{"resets_in_seconds":90}"#),
            Some(reset),
            true,
        ),
        DEFAULT_COOLDOWN
    );
    assert_eq!(
        cooldown_for_class(UpstreamErrorClass::Request, None, None, None, false),
        cooldown_from_retry_after(None)
    );
    assert_eq!(
        cooldown_for_class(
            UpstreamErrorClass::Transient,
            Some(&HeaderValue::from_static("30")),
            None,
            None,
            false,
        ),
        Duration::from_secs(30)
    );
}

#[test]
fn retry_after_http_date_in_the_future() {
    let when = chrono::Utc::now() + chrono::Duration::seconds(30);
    let raw = when.format("%a, %d %b %Y %H:%M:%S GMT").to_string();
    let header = HeaderValue::from_str(&raw).expect("header");
    let parsed = parse_retry_after(&header).expect("future http-date");
    assert!(parsed >= std::time::Duration::from_secs(20));
    assert!(parsed <= std::time::Duration::from_secs(40));
}
