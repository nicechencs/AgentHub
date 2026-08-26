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

use agenthub_core::bridge::BridgeRuntimeHost;
use agenthub_core::models::{
    AdapterProfile, AdapterProfileMode, AdapterProfileStatus, AdapterRoute, AdapterSourceKind,
    AgentId, FEATURE_ROUTE_POOL_V2,
};
use agenthub_core::storage::AdapterProfileRepo;
use agenthub_core::AgentHub;

fn native_or_bridge_profile(
    id: &str,
    source_id: &str,
    route: AdapterRoute,
    port: Option<u16>,
) -> AdapterProfile {
    AdapterProfile {
        id: id.into(),
        name: id.into(),
        source_kind: AdapterSourceKind::Provider,
        source_id: source_id.into(),
        target_agent_id: AgentId::Codex,
        route,
        mode: AdapterProfileMode::Api,
        status: AdapterProfileStatus::Active,
        rule_id: "kimi-membership-to-codex-v1".into(),
        rule_version: "v1".into(),
        generated_provider_id: None,
        local_port: port,
        auto_start: true,
        last_error_code: None,
        created_at: "t0".into(),
        updated_at: "t0".into(),
    }
}

#[test]
fn persist_enroll_native_if_bound_skips_enroll_on_bind_error() {
    let dir = tempfile::tempdir().unwrap();
    let hub = AgentHub::open(Some(dir.path())).unwrap();
    hub.db().set_setting(FEATURE_ROUTE_POOL_V2, "true").unwrap();
    let profile = native_or_bridge_profile(
        "bound-skip",
        "src-1",
        AdapterRoute::LocalBridge,
        Some(43121),
    );
    AdapterProfileRepo::new(hub.db().clone())
        .create(&profile)
        .unwrap();
    hub.route_pools()
        .create_legacy_pool(&profile, "ahb_secret-token", true)
        .unwrap();
    let host = BridgeRuntimeHost::new();
    let error =
        persist_enroll_native_if_bound(&hub, &host, Err("adapter.port_in_use".into())).unwrap_err();
    assert!(error.contains("adapter.port_in_use"));
    let pool = hub.route_pools().get("bound-skip").unwrap().unwrap();
    assert!(!pool.v2_enrolled);
    assert_eq!(pool.gateway_port, None);
    assert_eq!(pool.hub_token, "ahb_secret-token");
}

#[test]
fn persist_enroll_native_if_bound_success_omits_hub_token() {
    let dir = tempfile::tempdir().unwrap();
    let hub = AgentHub::open(Some(dir.path())).unwrap();
    hub.db().set_setting(FEATURE_ROUTE_POOL_V2, "true").unwrap();
    let profile =
        native_or_bridge_profile("bound-ok", "src-1", AdapterRoute::LocalBridge, Some(43121));
    AdapterProfileRepo::new(hub.db().clone())
        .create(&profile)
        .unwrap();
    let host = BridgeRuntimeHost::new();
    let binding = TicketBinding {
        ticket_id: "provider:src-1".into(),
        agent_id: AgentId::Codex,
        route: TicketBindingRoute::Bridge,
        active: true,
        profile_id: Some("bound-ok".into()),
        bridge: Some(agenthub_core::models::TicketBridgeRuntime {
            port: Some(43155),
            running: true,
        }),
    };
    let overview = persist_enroll_native_if_bound(&hub, &host, Ok(binding)).unwrap();
    assert!(overview.v2_enrolled);
    assert_eq!(overview.gateway_port, Some(43155));
    let json = serde_json::to_string(&overview).unwrap();
    assert!(!json.contains("hubToken"));
    assert!(!json.contains("ahb_"));
}
