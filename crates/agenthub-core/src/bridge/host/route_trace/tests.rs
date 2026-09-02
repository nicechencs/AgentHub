use super::*;
use crate::bridge::account::{MemberHealth, PickedMember};
use crate::bridge::host::surface::DownstreamSurface;
use crate::bridge::host::transport::UpstreamChannel;
use crate::bridge::ResolvedAuth;

#[test]
fn conversion_path_id_maps_surfaces() {
    assert_eq!(
        conversion_path_id(
            DownstreamSurface::Messages,
            UpstreamChannel::OpenAiChat,
            false,
        ),
        "messages_to_openai_chat"
    );
    assert_eq!(
        conversion_path_id(
            DownstreamSurface::Responses,
            UpstreamChannel::CodexResponses,
            true,
        ),
        "passthrough"
    );
}

#[test]
fn sanitize_upstream_url_strips_query_and_fragment() {
    assert_eq!(
        sanitize_upstream_url("https://api.example.com/v1/chat/completions?foo=1#bar"),
        "https://api.example.com/v1/chat/completions"
    );
}

#[test]
fn route_trace_log_caps_per_profile() {
    let log = RouteTraceLog::new();
    for index in 0..(ROUTE_TRACE_CAP + 5) {
        let mut builder = RouteTraceBuilder::begin(format!("req-{index}"), "POST", "/v1/messages");
        builder.local_auth_ok("profile-a", None);
        builder.finalize(200, &log);
    }
    let recent = log.recent("profile-a");
    assert_eq!(recent.len(), ROUTE_TRACE_CAP);
    assert_eq!(recent[0].request_id, format!("req-{}", ROUTE_TRACE_CAP + 4));
}

#[test]
fn failure_stage_records_first_failed_node() {
    let log = RouteTraceLog::new();
    let mut builder = RouteTraceBuilder::begin("req-fail", "POST", "/v1/messages");
    builder.local_auth_ok("profile-a", Some(8787));
    builder.pool_failed("pool_exhausted", "No eligible member");
    builder.finalize(503, &log);
    let trace = log.get("req-fail").expect("trace stored");
    assert_eq!(trace.failure_stage.as_deref(), Some("pool"));
    assert_eq!(trace.pool.status, TraceStageStatus::Failed);
    assert_eq!(trace.conversion.status, TraceStageStatus::Skipped);
}

#[test]
fn conversion_failed_skips_upstream_stages() {
    let log = RouteTraceLog::new();
    let mut builder = RouteTraceBuilder::begin("req-conv", "POST", "/v1/messages");
    builder.local_auth_ok("profile-a", Some(8787));
    builder.conversion_failed("conversion_failed", "Could not convert this request for the upstream.");
    builder.finalize(400, &log);
    let trace = log.get("req-conv").expect("trace stored");
    assert_eq!(trace.failure_stage.as_deref(), Some("conversion"));
    assert_eq!(trace.conversion.status, TraceStageStatus::Failed);
    assert_eq!(trace.conversion.code.as_deref(), Some("conversion_failed"));
    assert_eq!(trace.upstream_auth.status, TraceStageStatus::Skipped);
    assert_eq!(trace.upstream.status, TraceStageStatus::Skipped);
    assert_eq!(trace.local_auth.status, TraceStageStatus::Ok);
}

#[test]
fn surface_mismatch_keeps_local_auth_ok() {
    let log = RouteTraceLog::new();
    let mut builder = RouteTraceBuilder::begin("req-surface", "POST", "/v1/responses");
    builder.local_auth_ok("profile-a", Some(8787));
    builder.local_path_failed(
        "profile-a",
        Some(8787),
        "surface_mismatch",
        "This route only serves /v1/messages",
    );
    builder.finalize(404, &log);
    let trace = log.get("req-surface").expect("trace stored");
    assert_eq!(trace.local_auth.status, TraceStageStatus::Ok);
    assert_eq!(trace.local_auth.port, Some(8787));
    assert_eq!(trace.failure_stage.as_deref(), Some("local_endpoint"));
    assert_eq!(trace.pool.status, TraceStageStatus::Skipped);
    assert_eq!(trace.conversion.status, TraceStageStatus::Skipped);
    assert_eq!(trace.conversion.code.as_deref(), Some("surface_mismatch"));
    assert!(trace
        .conversion
        .message
        .as_deref()
        .unwrap_or("")
        .contains("/v1/messages"));
    assert_eq!(trace.upstream.status, TraceStageStatus::Skipped);
}

#[test]
fn pool_attempts_record_failover() {
    let log = RouteTraceLog::new();
    let member = PickedMember::new(
        "",
        "account",
        "acct-1",
        "acct-1",
        ResolvedAuth::bearer("sk-test"),
        None,
        MemberHealth::Renewable,
    );
    let mut builder = RouteTraceBuilder::begin("req-pool", "POST", "/v1/messages");
    builder.local_auth_ok("profile-a", None);
    builder.pool_selected(&member, None);
    builder.pool_attempt_failed(&member, "upstream_error", "401 from upstream");
    builder.upstream_success("https://api.example.com/v1/messages", &member, 200, None);
    builder.finalize(200, &log);
    let trace = log.get("req-pool").expect("trace stored");
    assert_eq!(trace.pool.attempts.len(), 2);
    assert_eq!(trace.upstream.status, TraceStageStatus::Ok);
}

#[test]
fn trace_serializes_camel_case_for_frontend() {
    let log = RouteTraceLog::new();
    let mut builder = RouteTraceBuilder::begin("req-json", "POST", "/v1/messages");
    builder.local_auth_ok("profile-a", Some(8787));
    builder.pool_selected(
        &PickedMember::new("", "account", "acct-1", "acct-1", ResolvedAuth::bearer("x"), None, MemberHealth::Renewable),
        None,
    );
    builder.conversion_prepared(DownstreamSurface::Messages, UpstreamChannel::Anthropic, false);
    builder.upstream_auth_result(true, Some(200), None, None);
    builder.upstream_success(
        "https://api.anthropic.com/v1/messages",
        &PickedMember::new("", "account", "acct-1", "acct-1", ResolvedAuth::bearer("x"), None, MemberHealth::Renewable),
        200,
        Some("claude-sonnet"),
    );
    builder.finalize(200, &log);
    let trace = log.get("req-json").expect("trace");
    let json = serde_json::to_string(&trace).expect("json");
    assert!(json.contains("\"requestId\""));
    assert!(json.contains("\"localAuth\""));
    assert!(json.contains("\"upstreamAuth\""));
    assert!(!json.contains("local_auth"));
}

#[test]
fn patch_usage_applies_before_and_after_push() {
    let log = RouteTraceLog::new();
    log.patch_usage("req-pending", Some(120), Some(11), Some(7));
    let mut builder = RouteTraceBuilder::begin("req-pending", "POST", "/v1/messages");
    builder.local_auth_ok("profile-a", None);
    builder.finalize(200, &log);
    let first = log.get("req-pending").expect("stored");
    assert_eq!(first.ttft_ms, Some(120));
    assert_eq!(first.input_tokens, Some(11));
    assert_eq!(first.output_tokens, Some(7));

    log.patch_usage("req-pending", Some(180), Some(20), Some(9));
    let second = log.get("req-pending").expect("updated");
    assert_eq!(second.ttft_ms, Some(180));
    assert_eq!(second.input_tokens, Some(20));
    assert_eq!(second.output_tokens, Some(9));
}


#[test]
fn route_trace_log_persists_and_restores_across_enable() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("route-traces.json");
    let log = RouteTraceLog::new();
    log.enable_persist(path.clone());

    let mut builder = RouteTraceBuilder::begin("req-persist", "POST", "/v1/messages");
    builder.local_auth_ok("profile-a", Some(8787));
    builder.set_model(Some("claude-sonnet".into()));
    builder.finalize(200, &log);

    let mut other = RouteTraceBuilder::begin("req-other", "POST", "/v1/chat/completions");
    other.local_auth_ok("profile-b", None);
    other.finalize(201, &log);

    assert!(path.is_file(), "persist file should exist after finalize");

    let restored = RouteTraceLog::new();
    restored.enable_persist(path);
    let a = restored.recent("profile-a");
    assert_eq!(a.len(), 1);
    assert_eq!(a[0].request_id, "req-persist");
    assert_eq!(a[0].http_status, 200);
    assert_eq!(a[0].model.as_deref(), Some("claude-sonnet"));
    assert_eq!(a[0].local_auth.port, Some(8787));
    let b = restored.recent("profile-b");
    assert_eq!(b.len(), 1);
    assert_eq!(b[0].request_id, "req-other");
}

#[test]
fn route_trace_persist_ignores_corrupt_file() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("route-traces.json");
    std::fs::write(&path, b"{not-json").expect("write corrupt");
    let log = RouteTraceLog::new();
    log.enable_persist(path);
    assert!(log.recent("profile-a").is_empty());
}

#[test]
fn route_trace_persist_second_enable_is_ignored() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path_a = dir.path().join("a.json");
    let path_b = dir.path().join("b.json");
    let log = RouteTraceLog::new();
    log.enable_persist(path_a.clone());
    let mut builder = RouteTraceBuilder::begin("req-a", "POST", "/v1/messages");
    builder.local_auth_ok("profile-a", None);
    builder.finalize(200, &log);
    log.enable_persist(path_b.clone());
    let mut builder = RouteTraceBuilder::begin("req-b", "POST", "/v1/messages");
    builder.local_auth_ok("profile-a", None);
    builder.finalize(200, &log);
    assert!(path_a.is_file());
    assert!(!path_b.exists(), "second enable_persist must not retarget writes");
}

#[test]
fn route_trace_persist_keeps_ring_cap_on_reload() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("route-traces.json");
    let log = RouteTraceLog::new();
    log.enable_persist(path.clone());
    for index in 0..(ROUTE_TRACE_CAP + 8) {
        let mut builder = RouteTraceBuilder::begin(format!("req-{index}"), "POST", "/v1/messages");
        builder.local_auth_ok("profile-a", None);
        builder.finalize(200, &log);
    }
    let restored = RouteTraceLog::new();
    restored.enable_persist(path);
    let recent = restored.recent("profile-a");
    assert_eq!(recent.len(), ROUTE_TRACE_CAP);
    assert_eq!(
        recent[0].request_id,
        format!("req-{}", ROUTE_TRACE_CAP + 7)
    );
}

#[test]
fn trace_stage_status_as_str_matches_serde() {
    assert_eq!(TraceStageStatus::Pending.as_str(), "pending");
    assert_eq!(TraceStageStatus::Ok.as_str(), "ok");
    assert_eq!(TraceStageStatus::Failed.as_str(), "failed");
    assert_eq!(TraceStageStatus::Skipped.as_str(), "skipped");
}

#[test]
fn finalize_keeps_five_stage_statuses_for_log_alignment() {
    let log = RouteTraceLog::new();
    let mut builder = RouteTraceBuilder::begin("req-log-align", "POST", "/v1/messages");
    builder.local_auth_ok("profile-a", Some(44227));
    builder.finalize(200, &log);
    let trace = log.get("req-log-align").expect("trace stored");
    assert_eq!(trace.local_auth.status.as_str(), "ok");
    // pool/conversion/upstream remain pending or get filled by happy path helpers;
    // ensure labels stay stable for `core.adapter.route_trace` grepping.
    for status in [
        trace.pool.status,
        trace.conversion.status,
        trace.upstream_auth.status,
        trace.upstream.status,
    ] {
        assert!(
            matches!(
                status,
                TraceStageStatus::Pending
                    | TraceStageStatus::Ok
                    | TraceStageStatus::Failed
                    | TraceStageStatus::Skipped
            ),
            "unexpected stage {:?}",
            status
        );
    }
}

