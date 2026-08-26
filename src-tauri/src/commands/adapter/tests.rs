use super::*;
use crate::commands::{adapter_error_from_string, is_adapter_error_retryable};

#[test]
fn command_input_only_accepts_connection_table_kinds() {
    assert_eq!(
        parse_source_kind("account").unwrap(),
        AdapterSourceKind::Account
    );
    assert_eq!(
        parse_source_kind("provider").unwrap(),
        AdapterSourceKind::Provider
    );
    assert!(parse_source_kind("credential").is_err());
}

#[test]
fn optional_source_kind_preserves_absent_list_filter() {
    assert_eq!(parse_source_kind_opt(None).unwrap(), None);
    assert_eq!(
        parse_source_kind_opt(Some("provider")).unwrap(),
        Some(AdapterSourceKind::Provider)
    );
    assert!(parse_source_kind_opt(Some("credential")).is_err());
}

#[test]
fn optional_mode_and_route_filters_fail_closed() {
    assert_eq!(parse_mode_opt(None).unwrap(), None);
    assert_eq!(
        parse_mode_opt(Some("api")).unwrap(),
        Some(AdapterProfileMode::Api)
    );
    assert_eq!(
        parse_mode_opt(Some("oauth")).unwrap(),
        Some(AdapterProfileMode::Oauth)
    );
    assert!(parse_mode_opt(Some("bridge")).is_err());

    assert_eq!(parse_route_opt(None).unwrap(), None);
    assert_eq!(
        parse_route_opt(Some("local_bridge")).unwrap(),
        Some(AdapterRoute::LocalBridge)
    );
    assert!(parse_route_opt(Some("unsupported")).is_err());
}

#[test]
fn adapter_error_from_string_keeps_bracketed_code_and_retryable() {
    let error = adapter_error_from_string("本机路由无法启动或停止 [adapter.port_in_use]".into());
    assert_eq!(error.code, "adapter.port_in_use");
    assert!(error.message.contains("本机路由"));
    assert!(error.retryable);
    assert!(error.details.is_none());
}

#[test]
fn adapter_error_from_string_marks_rollback_and_stop_as_not_retryable() {
    let rollback = adapter_error_from_string(
        "finalize failed and compensation was incomplete [adapter.bridge_rollback]".into(),
    );
    assert_eq!(rollback.code, "adapter.bridge_rollback");
    assert!(!rollback.retryable);

    let stop =
        adapter_error_from_string("listener compensation failed [adapter.bridge_stop]".into());
    assert_eq!(stop.code, "adapter.bridge_stop");
    assert!(!stop.retryable);
}

#[test]
fn adapter_retryable_classification_covers_restore_and_retryable_prefix() {
    // Keep in lockstep with `isAdapterErrorCodeRetryable` in
    // `src/lib/backend/contracts/adapter.test.ts`.
    for code in [
        "retryable:adapter.port_in_use",
        "adapter.port_in_use",
        "adapter.bridge_start",
        "adapter.bridge_upstream_auth",
        "adapter.bridge_restore_source",
        "adapter.bridge_restore_port",
    ] {
        assert!(is_adapter_error_retryable(code), "{code}");
    }
    for code in [
        "needs_attention",
        "adapter.bridge_rollback",
        "adapter.bridge_stop",
        "not_found",
    ] {
        assert!(!is_adapter_error_retryable(code), "{code}");
    }
}
