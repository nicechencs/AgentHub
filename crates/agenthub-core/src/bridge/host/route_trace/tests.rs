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
fn sanitize_upstream_url_strips_userinfo_query_and_fragment() {
    assert_eq!(
        sanitize_upstream_url("https://user:secret@api.example.com/v1/chat/completions?foo=1#bar"),
        "https://api.example.com/v1/chat/completions"
    );
    assert_eq!(sanitize_upstream_url("not a url with secret"), "");
}

#[test]
fn stage_ids_accept_legacy_aliases_and_unknown_values() {
    assert_eq!(
        serde_json::from_str::<RouteTraceStageId>("\"conversion\"").unwrap(),
        RouteTraceStageId::RequestConversion
    );
    assert_eq!(
        serde_json::from_str::<RouteTraceStageId>("\"upstream_auth\"").unwrap(),
        RouteTraceStageId::UpstreamResponse
    );
    assert_eq!(
        serde_json::from_str::<RouteTraceStageId>("\"future_stage\"").unwrap(),
        RouteTraceStageId::Unknown
    );
}

#[test]
fn real_attempt_records_sanitized_url_status_auth_and_duration() {
    let member = PickedMember::new(
        "",
        "account",
        "acct-1",
        "acct-1",
        ResolvedAuth::bearer("sk-super-secret"),
        None,
        MemberHealth::Renewable,
    );
    let mut builder = RouteTraceBuilder::begin("req-attempt", "POST", "/v1/messages");
    builder.pool_selected(&member, None);
    builder.conversion_prepared(
        DownstreamSurface::Messages,
        UpstreamChannel::Anthropic,
        false,
    );
    let attempt_id = builder.upstream_attempt_started(
        "https://api.example.com/v1/messages?token=secret#fragment",
        &member,
        Some("claude-sonnet"),
    );
    builder.upstream_attempt_response(attempt_id, 401, 27, Some("unauthorized"));

    let attempt = &builder.trace.pool.attempts[0];
    assert_eq!(attempt.attempt_id, 1);
    assert_eq!(
        attempt.url.as_deref(),
        Some("https://api.example.com/v1/messages")
    );
    assert_eq!(attempt.request_status, TraceStageStatus::Ok);
    assert_eq!(attempt.response_status, TraceStageStatus::Failed);
    assert_eq!(attempt.auth_result.as_deref(), Some("rejected"));
    assert_eq!(attempt.http_status, Some(401));
    assert_eq!(attempt.duration_ms, Some(27));
    let json = serde_json::to_string(&builder.trace).unwrap();
    assert!(!json.contains("sk-super-secret"));
    assert!(!json.contains("token=secret"));
}

#[test]
fn redirect_attempt_is_recorded_as_http_failure() {
    let member = PickedMember::new(
        "",
        "account",
        "acct-1",
        "acct-1",
        ResolvedAuth::bearer("sk-test"),
        None,
        MemberHealth::Renewable,
    );
    let mut builder = RouteTraceBuilder::begin("req-redirect", "POST", "/v1/messages");
    let attempt =
        builder.upstream_attempt_started("https://api.example.com/v1/messages", &member, None);
    builder.upstream_attempt_response(attempt, 302, 4, Some("upstream_error"));

    let attempt = &builder.trace.pool.attempts[0];
    assert_eq!(attempt.status, TraceStageStatus::Failed);
    assert_eq!(attempt.request_status, TraceStageStatus::Ok);
    assert_eq!(attempt.response_status, TraceStageStatus::Failed);
    assert_eq!(attempt.result.as_deref(), Some("http_error"));
}

#[test]
fn upstream_request_failure_closes_all_later_nodes() {
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
    let mut builder = RouteTraceBuilder::begin("req-url", "POST", "/v1/messages");
    builder.local_auth_ok("profile-a", None);
    builder.local_endpoint_ok();
    builder.admission_ok();
    builder.route_resolution_ok();
    builder.pool_selected(&member, None);
    builder.conversion_prepared(
        DownstreamSurface::Messages,
        UpstreamChannel::Anthropic,
        false,
    );
    builder.upstream_request_failed(None, &member, None, "invalid_upstream_url");
    builder.finalize(502, &log);

    let trace = log.get("req-url").unwrap();
    assert_eq!(
        trace.failure_stage,
        Some(RouteTraceStageId::UpstreamRequest)
    );
    assert_eq!(trace.upstream_request.status, TraceStageStatus::Failed);
    assert!(pending_stage_ids(&trace).is_empty());
}

#[test]
fn response_read_stopping_closes_all_later_nodes() {
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
    let mut builder = RouteTraceBuilder::begin("req-stopping", "POST", "/v1/messages");
    builder.local_auth_ok("profile-a", None);
    builder.local_endpoint_ok();
    builder.admission_ok();
    builder.route_resolution_ok();
    builder.pool_selected(&member, None);
    builder.conversion_prepared(
        DownstreamSurface::Messages,
        UpstreamChannel::Anthropic,
        false,
    );
    let attempt =
        builder.upstream_attempt_started("https://api.example.com/v1/messages", &member, None);
    builder.upstream_attempt_response(attempt, 500, 4, Some("upstream_error"));
    builder.upstream_failed(
        "https://api.example.com/v1/messages",
        &member,
        Some(500),
        "stopping",
        "stopped",
    );
    builder.finalize(503, &log);

    let trace = log.get("req-stopping").unwrap();
    assert_eq!(
        trace.failure_stage,
        Some(RouteTraceStageId::UpstreamResponse)
    );
    assert!(pending_stage_ids(&trace).is_empty());
}

#[test]
fn route_trace_records_only_key_last4_for_local_and_upstream_auth() {
    let mut builder = RouteTraceBuilder::begin("req-key-hints", "POST", "/v1/chat/completions");
    builder.local_auth_ok("profile-a", Some(17034));
    builder.local_auth_key_last4("ahb_local_1234");
    let member = PickedMember::new(
        "account:workbuddy",
        "account",
        "workbuddy",
        "WorkBuddy Grok",
        ResolvedAuth::bearer("sk-upstream-627a"),
        None,
        MemberHealth::Renewable,
    );
    builder.pool_selected(&member, None);
    builder.upstream_model(Some("grok-4.6"));
    assert_eq!(builder.trace.local_auth.key_last4.as_deref(), Some("1234"));
    assert_eq!(
        builder
            .trace
            .pool
            .selected_member
            .as_ref()
            .and_then(|item| item.key_last4.as_deref()),
        Some("627a")
    );
    assert_eq!(
        builder.trace.upstream.upstream_model.as_deref(),
        Some("grok-4.6")
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
    assert_eq!(trace.failure_stage, Some(RouteTraceStageId::Pool));
    assert_eq!(trace.pool.status, TraceStageStatus::Failed);
    assert_eq!(trace.conversion.status, TraceStageStatus::Skipped);
}

#[test]
fn conversion_failed_skips_upstream_stages() {
    let log = RouteTraceLog::new();
    let mut builder = RouteTraceBuilder::begin("req-conv", "POST", "/v1/messages");
    builder.local_auth_ok("profile-a", Some(8787));
    builder.conversion_failed(
        "conversion_failed",
        "Could not convert this request for the upstream.",
    );
    builder.finalize(400, &log);
    let trace = log.get("req-conv").expect("trace stored");
    assert_eq!(
        trace.failure_stage,
        Some(RouteTraceStageId::RequestConversion)
    );
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
    assert_eq!(trace.failure_stage, Some(RouteTraceStageId::LocalEndpoint));
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
    let fallback = PickedMember::new(
        "",
        "account",
        "acct-2",
        "acct-2",
        ResolvedAuth::bearer("sk-fallback"),
        None,
        MemberHealth::Renewable,
    );
    let mut builder = RouteTraceBuilder::begin("req-pool", "POST", "/v1/messages");
    builder.local_auth_ok("profile-a", None);
    builder.pool_selected(&member, None);
    let first_attempt =
        builder.upstream_attempt_started("https://api.example.com/v1/messages", &member, None);
    builder.upstream_attempt_response(first_attempt, 500, 10, Some("upstream_error"));
    builder.pool_attempt_failed(&member, "upstream_error", "Upstream attempt failed");
    let second_attempt =
        builder.upstream_attempt_started("https://api.example.com/v1/messages", &fallback, None);
    builder.upstream_attempt_response(second_attempt, 200, 8, None);
    builder.upstream_success("https://api.example.com/v1/messages", &fallback, 200, None);
    builder.finalize(200, &log);
    let trace = log.get("req-pool").expect("trace stored");
    assert_eq!(trace.pool.attempts.len(), 2);
    assert_eq!(trace.pool.attempts[0].member.source_id, "acct-1");
    assert_eq!(trace.pool.attempts[0].status, TraceStageStatus::Failed);
    assert_eq!(trace.pool.attempts[1].member.source_id, "acct-2");
    assert_eq!(trace.pool.attempts[1].status, TraceStageStatus::Ok);
    assert_eq!(
        trace
            .pool
            .selected_member
            .as_ref()
            .map(|row| row.source_id.as_str()),
        Some("acct-2")
    );
    assert_eq!(trace.upstream.member, trace.pool.selected_member);
    assert_eq!(trace.upstream.status, TraceStageStatus::Ok);
}

#[test]
fn recovered_auth_attempt_does_not_leave_request_failure() {
    let log = RouteTraceLog::new();
    let first = PickedMember::new(
        "",
        "account",
        "acct-1",
        "acct-1",
        ResolvedAuth::bearer("sk-first"),
        None,
        MemberHealth::Renewable,
    );
    let second = PickedMember::new(
        "",
        "account",
        "acct-2",
        "acct-2",
        ResolvedAuth::bearer("sk-second"),
        None,
        MemberHealth::Renewable,
    );
    let mut builder = RouteTraceBuilder::begin("req-auth-recovered", "POST", "/v1/messages");
    builder.local_auth_ok("profile-a", None);
    builder.pool_selected(&first, None);
    let first_attempt =
        builder.upstream_attempt_started("https://api.example.com/v1/messages", &first, None);
    builder.upstream_attempt_response(first_attempt, 401, 10, Some("unauthorized"));
    builder.upstream_auth_result(false, Some(401), Some("unauthorized"), None);
    builder.pool_attempt_failed(&first, "unauthorized", "Unauthorized");
    let second_attempt =
        builder.upstream_attempt_started("https://api.example.com/v1/messages", &second, None);
    builder.upstream_attempt_response(second_attempt, 200, 8, None);
    builder.upstream_auth_result(true, Some(200), None, None);
    builder.upstream_success("https://api.example.com/v1/messages", &second, 200, None);
    builder.finalize(200, &log);

    let trace = log.get("req-auth-recovered").expect("trace stored");
    assert!(trace.ok);
    assert_eq!(trace.failure_stage, None);
    assert_eq!(trace.upstream_auth.status, TraceStageStatus::Ok);
    assert_eq!(trace.pool.attempts.len(), 2);
}

#[test]
fn exhausted_attempts_preserve_executed_conversion_and_upstream_failure() {
    let log = RouteTraceLog::new();
    let member = PickedMember::new(
        "",
        "account",
        "acct-1",
        "acct-1",
        ResolvedAuth::bearer("sk-first"),
        None,
        MemberHealth::Renewable,
    );
    let mut builder = RouteTraceBuilder::begin("req-exhausted", "POST", "/v1/messages");
    builder.local_auth_ok("profile-a", None);
    builder.route_resolution_ok();
    builder.pool_selected(&member, None);
    builder.conversion_prepared(
        DownstreamSurface::Messages,
        UpstreamChannel::Anthropic,
        false,
    );
    let attempt =
        builder.upstream_attempt_started("https://api.example.com/v1/messages", &member, None);
    builder.upstream_attempt_response(attempt, 429, 12, Some("quota_account"));
    builder.pool_attempt_failed(&member, "quota_account", "Rate limited");
    builder.attempts_exhausted(
        Some("https://api.example.com/v1/messages"),
        &member,
        Some(429),
        "quota_account",
        "Rate limited",
    );
    builder.finalize(503, &log);

    let trace = log.get("req-exhausted").expect("trace stored");
    assert_eq!(trace.pool.status, TraceStageStatus::Ok);
    assert_eq!(trace.conversion.status, TraceStageStatus::Ok);
    assert_eq!(trace.upstream.status, TraceStageStatus::Failed);
    assert_eq!(trace.upstream.http_status, Some(429));
    assert_eq!(trace.response_conversion.status, TraceStageStatus::Skipped);
    assert_eq!(
        trace.failure_stage,
        Some(RouteTraceStageId::UpstreamResponse)
    );
}

fn pending_stage_ids(trace: &RouteRequestTrace) -> Vec<RouteTraceStageId> {
    [
        (RouteTraceStageId::LocalAuth, trace.local_auth.status),
        (
            RouteTraceStageId::LocalEndpoint,
            trace.local_endpoint.status,
        ),
        (RouteTraceStageId::Admission, trace.admission.status),
        (
            RouteTraceStageId::RouteResolution,
            trace.route_resolution.status,
        ),
        (RouteTraceStageId::Pool, trace.pool.status),
        (
            RouteTraceStageId::RequestConversion,
            trace.conversion.status,
        ),
        (
            RouteTraceStageId::UpstreamRequest,
            trace.upstream_request.status,
        ),
        (RouteTraceStageId::UpstreamResponse, trace.upstream.status),
        (
            RouteTraceStageId::ResponseConversion,
            trace.response_conversion.status,
        ),
        (RouteTraceStageId::Delivery, trace.delivery.status),
    ]
    .into_iter()
    .filter_map(|(id, status)| (status == TraceStageStatus::Pending).then_some(id))
    .collect()
}

#[test]
fn new_non_stream_terminal_trace_has_no_pending_nodes() {
    let log = RouteTraceLog::new();
    let member = PickedMember::new(
        "",
        "account",
        "acct-1",
        "acct-1",
        ResolvedAuth::bearer("sk-first"),
        None,
        MemberHealth::Renewable,
    );
    let mut builder = RouteTraceBuilder::begin("req-terminal", "POST", "/v1/messages");
    builder.local_auth_ok("profile-a", None);
    builder.local_endpoint_ok();
    builder.admission_ok();
    builder.route_resolution_ok();
    builder.pool_selected(&member, None);
    builder.conversion_prepared(
        DownstreamSurface::Messages,
        UpstreamChannel::Anthropic,
        false,
    );
    let attempt = builder.upstream_attempt_started(
        "https://api.example.com/v1/messages",
        &member,
        Some("claude-sonnet"),
    );
    builder.upstream_attempt_response(attempt, 200, 5, None);
    builder.upstream_auth_result(true, Some(200), None, None);
    builder.upstream_success("https://api.example.com/v1/messages", &member, 200, None);
    builder.response_conversion_result(
        false,
        200,
        DownstreamSurface::Messages,
        UpstreamChannel::Anthropic,
    );
    builder.finalize(200, &log);

    let trace = log.get("req-terminal").unwrap();
    assert_eq!(trace.trace_version, 2);
    assert!(pending_stage_ids(&trace).is_empty());
    assert!(trace.failure_stage.is_none());
}

#[test]
fn trace_serializes_camel_case_for_frontend() {
    let log = RouteTraceLog::new();
    let mut builder = RouteTraceBuilder::begin("req-json", "POST", "/v1/messages");
    builder.local_auth_ok("profile-a", Some(8787));
    builder.local_endpoint_ok();
    builder.admission_ok();
    builder.route_resolution_ok();
    builder.pool_selected(
        &PickedMember::new(
            "",
            "account",
            "acct-1",
            "acct-1",
            ResolvedAuth::bearer("x"),
            None,
            MemberHealth::Renewable,
        ),
        None,
    );
    builder.conversion_prepared(
        DownstreamSurface::Messages,
        UpstreamChannel::Anthropic,
        false,
    );
    builder.upstream_auth_result(true, Some(200), None, None);
    builder.upstream_success(
        "https://api.anthropic.com/v1/messages",
        &PickedMember::new(
            "",
            "account",
            "acct-1",
            "acct-1",
            ResolvedAuth::bearer("x"),
            None,
            MemberHealth::Renewable,
        ),
        200,
        Some("claude-sonnet"),
    );
    builder.response_conversion_result(
        false,
        200,
        DownstreamSurface::Messages,
        UpstreamChannel::Anthropic,
    );
    builder.finalize(200, &log);
    let trace = log.get("req-json").expect("trace");
    let json = serde_json::to_string(&trace).expect("json");
    assert!(json.contains("\"requestId\""));
    assert!(json.contains("\"localEndpoint\""));
    assert!(json.contains("\"localAuth\""));
    assert!(json.contains("\"admission\""));
    assert!(json.contains("\"routeResolution\""));
    assert!(json.contains("\"upstreamAuth\""));
    assert!(json.contains("\"upstreamRequest\""));
    assert!(json.contains("\"traceVersion\":2"));
    assert!(json.contains("\"responseConversion\""));
    assert!(json.contains("anthropic_to_messages"));
    assert!(json.contains("\"delivery\""));
    assert!(!json.contains("local_auth"));
}

#[test]
fn old_persisted_trace_without_lifecycle_fields_still_loads() {
    let log = RouteTraceLog::new();
    let mut builder = RouteTraceBuilder::begin("req-old", "POST", "/v1/messages");
    builder.local_auth_ok("profile-a", Some(8787));
    builder.finalize(200, &log);
    let trace = log.get("req-old").expect("trace");
    let mut value = serde_json::to_value(trace).expect("serialize trace");
    let object = value.as_object_mut().expect("trace object");
    for key in [
        "traceVersion",
        "localEndpoint",
        "admission",
        "routeResolution",
        "upstreamRequest",
        "responseConversion",
        "delivery",
    ] {
        object.remove(key);
    }

    let restored: RouteRequestTrace = serde_json::from_value(value).expect("old trace loads");
    assert_eq!(restored.trace_version, 1);
    assert_eq!(restored.local_endpoint.status, TraceStageStatus::Pending);
    assert_eq!(restored.admission.status, TraceStageStatus::Pending);
    assert_eq!(restored.route_resolution.status, TraceStageStatus::Pending);
    assert_eq!(restored.upstream_request.status, TraceStageStatus::Pending);
    assert_eq!(
        restored.response_conversion.status,
        TraceStageStatus::Pending
    );
    assert_eq!(restored.delivery.status, TraceStageStatus::Pending);
}

#[test]
fn patch_usage_applies_before_and_after_push() {
    let log = RouteTraceLog::new();
    log.patch_usage(
        "req-pending",
        Some(120),
        Some(11),
        Some(7),
        Some(3),
        Some(2),
    );
    let mut builder = RouteTraceBuilder::begin("req-pending", "POST", "/v1/messages");
    builder.local_auth_ok("profile-a", None);
    builder.finalize(200, &log);
    let first = log.get("req-pending").expect("stored");
    assert_eq!(first.ttft_ms, Some(120));
    assert_eq!(first.input_tokens, Some(11));
    assert_eq!(first.output_tokens, Some(7));
    assert_eq!(first.cached_input_tokens, Some(3));
    assert_eq!(first.reasoning_tokens, Some(2));

    log.patch_usage(
        "req-pending",
        Some(180),
        Some(20),
        Some(9),
        Some(4),
        Some(5),
    );
    let second = log.get("req-pending").expect("updated");
    assert_eq!(second.ttft_ms, Some(180));
    assert_eq!(second.input_tokens, Some(20));
    assert_eq!(second.output_tokens, Some(9));
    assert_eq!(second.cached_input_tokens, Some(4));
    assert_eq!(second.reasoning_tokens, Some(5));
}

#[test]
fn stream_completion_patches_response_and_delivery() {
    let log = RouteTraceLog::new();
    let mut builder = RouteTraceBuilder::begin("req-stream", "POST", "/v1/messages");
    builder.local_auth_ok("profile-a", Some(8787));
    builder.response_conversion_result(
        true,
        200,
        DownstreamSurface::Messages,
        UpstreamChannel::Anthropic,
    );
    builder.finalize(200, &log);
    let pending = log.get("req-stream").expect("pending stream");
    assert_eq!(pending.delivery.status, TraceStageStatus::Pending);

    log.patch_stream_completed("req-stream", 420);
    let complete = log.get("req-stream").expect("completed stream");
    assert_eq!(complete.response_conversion.status, TraceStageStatus::Ok);
    assert_eq!(complete.delivery.status, TraceStageStatus::Ok);
    assert_eq!(
        complete.delivery.completion.as_deref(),
        Some("stream_completed")
    );
    assert_eq!(complete.latency_ms, Some(420));
}

#[test]
fn stream_failure_patches_response_and_delivery() {
    let log = RouteTraceLog::new();
    let mut builder = RouteTraceBuilder::begin("req-stream-fail", "POST", "/v1/messages");
    builder.local_auth_ok("profile-a", Some(8787));
    builder.response_conversion_result(
        true,
        200,
        DownstreamSurface::Messages,
        UpstreamChannel::Anthropic,
    );
    builder.finalize(200, &log);

    log.patch_stream_conversion_failed("req-stream-fail", 510);
    let failed = log.get("req-stream-fail").expect("failed stream");
    assert_eq!(failed.response_conversion.status, TraceStageStatus::Failed);
    assert_eq!(failed.delivery.status, TraceStageStatus::Failed);
    assert_eq!(
        failed.failure_stage,
        Some(RouteTraceStageId::ResponseConversion)
    );
    assert_eq!(failed.latency_ms, Some(510));
}

#[test]
fn stream_disconnect_only_fails_delivery() {
    let log = RouteTraceLog::new();
    let mut builder = RouteTraceBuilder::begin("req-disconnect", "POST", "/v1/messages");
    builder.local_auth_ok("profile-a", Some(8787));
    builder.response_conversion_result(
        true,
        200,
        DownstreamSurface::Messages,
        UpstreamChannel::Anthropic,
    );
    builder.finalize(200, &log);

    log.patch_stream_disconnected("req-disconnect", 300);
    let failed = log.get("req-disconnect").expect("disconnected stream");
    assert_eq!(
        failed.response_conversion.status,
        TraceStageStatus::Interrupted
    );
    assert_eq!(
        failed.response_conversion.result.as_deref(),
        Some("interrupted")
    );
    assert_eq!(failed.delivery.status, TraceStageStatus::Failed);
    assert_eq!(
        failed.delivery.completion.as_deref(),
        Some("client_disconnected")
    );
    assert_eq!(failed.failure_stage, Some(RouteTraceStageId::Delivery));
}

#[test]
fn route_trace_log_persists_and_restores_across_enable() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("route-traces.db");
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
    let path = dir.path().join("route-traces.db");
    std::fs::write(&path, b"{not-json").expect("write corrupt");
    let log = RouteTraceLog::new();
    log.enable_persist(path.clone());
    assert!(log.recent("profile-a").is_empty());

    let mut builder = RouteTraceBuilder::begin("req-after-corrupt", "POST", "/v1/messages");
    builder.local_auth_ok("profile-a", None);
    builder.finalize(200, &log);
    assert_eq!(log.recent("profile-a").len(), 1);

    let restored = RouteTraceLog::new();
    restored.enable_persist(path);
    assert_eq!(
        restored.recent("profile-a")[0].request_id,
        "req-after-corrupt"
    );
}

#[test]
fn route_trace_persist_second_enable_is_ignored() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path_a = dir.path().join("a.db");
    let path_b = dir.path().join("b.db");
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
    assert!(
        !path_b.exists(),
        "second enable_persist must not retarget writes"
    );
}

#[test]
fn route_trace_persist_keeps_ring_cap_on_reload() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("route-traces.db");
    let log = RouteTraceLog::new();
    log.enable_persist(path.clone());
    for index in 0..(ROUTE_TRACE_CAP + 8) {
        let mut builder = RouteTraceBuilder::begin(format!("req-{index}"), "POST", "/v1/messages");
        builder.local_auth_ok("profile-a", None);
        builder.finalize(200, &log);
    }
    assert_eq!(log.persist_count("profile-a"), ROUTE_TRACE_CAP + 8);
    let restored = RouteTraceLog::new();
    restored.enable_persist(path);
    let recent = restored.recent("profile-a");
    assert_eq!(recent.len(), ROUTE_TRACE_CAP);
    assert_eq!(recent[0].request_id, format!("req-{}", ROUTE_TRACE_CAP + 7));
    assert_eq!(restored.persist_count("profile-a"), ROUTE_TRACE_CAP + 8);
}

#[test]
fn route_trace_persist_imports_legacy_json_then_removes_it() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("route-traces.db");
    let json_path = dir.path().join("route-traces.json");
    let payload = serde_json::json!({
        "version": 1,
        "byProfile": {
            "profile-a": [{
                "traceVersion": 2,
                "requestId": "req-legacy",
                "atUnixMs": super::now_unix_ms(),
                "profileId": "profile-a",
                "method": "POST",
                "path": "/v1/messages",
                "httpStatus": 200,
                "ok": true,
                "localAuth": { "status": "ok", "profileId": "profile-a" },
                "pool": { "status": "ok" },
                "conversion": { "status": "ok", "path": "passthrough" },
                "upstreamAuth": { "status": "ok" },
                "upstream": { "status": "ok" }
            }]
        },
        "unauthenticated": []
    });
    std::fs::write(&json_path, serde_json::to_vec(&payload).expect("json")).expect("write json");

    let log = RouteTraceLog::new();
    log.enable_persist(db_path);
    let recent = log.recent("profile-a");
    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0].request_id, "req-legacy");
    assert!(
        !json_path.exists(),
        "legacy json should be removed after import"
    );
}

#[test]
fn route_trace_persist_keeps_tokens_across_reload() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("route-traces.db");
    let log = RouteTraceLog::new();
    log.enable_persist(path.clone());
    let mut builder = RouteTraceBuilder::begin("req-tokens", "POST", "/v1/messages");
    builder.local_auth_ok("profile-a", None);
    builder.finalize(200, &log);
    log.patch_usage("req-tokens", Some(90), Some(11), Some(7), Some(3), Some(2));

    let restored = RouteTraceLog::new();
    restored.enable_persist(path);
    let row = restored.get("req-tokens").expect("restored");
    assert_eq!(row.input_tokens, Some(11));
    assert_eq!(row.output_tokens, Some(7));
    assert_eq!(row.cached_input_tokens, Some(3));
    assert_eq!(row.reasoning_tokens, Some(2));
}

#[test]
fn route_trace_persist_prunes_by_retention_days() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("route-traces.db");
    let log = RouteTraceLog::new();
    log.enable_persist_with_retention(path.clone(), 2);
    let now = super::now_unix_ms();
    let day_ms = 86_400_000u128;

    let mut old = RouteTraceBuilder::begin("req-old", "POST", "/v1/messages");
    old.local_auth_ok("profile-a", None);
    old.set_at_unix_ms(now.saturating_sub(day_ms * 3));
    old.finalize(200, &log);

    let mut recent = RouteTraceBuilder::begin("req-recent", "POST", "/v1/messages");
    recent.local_auth_ok("profile-a", None);
    recent.set_at_unix_ms(now.saturating_sub(day_ms));
    recent.finalize(200, &log);

    assert_eq!(log.persist_count("profile-a"), 1);

    let restored = RouteTraceLog::new();
    restored.enable_persist_with_retention(path, 2);
    let recent_rows = restored.recent("profile-a");
    assert_eq!(recent_rows.len(), 1);
    assert_eq!(recent_rows[0].request_id, "req-recent");
}

#[test]
fn route_trace_persist_uses_settings_retention_days() {
    let dir = tempfile::tempdir().expect("tempdir");
    let data_dir = dir.path();
    let db = crate::storage::Database::open(&data_dir.join("agenthub.db")).expect("settings db");
    db.set_setting("log_retention_days", "1").expect("set days");
    let path = data_dir.join("cache").join("route-traces.db");
    let log = RouteTraceLog::new();
    log.enable_persist(path.clone());
    let now = super::now_unix_ms();
    let day_ms = 86_400_000u128;

    let mut old = RouteTraceBuilder::begin("req-old", "POST", "/v1/messages");
    old.local_auth_ok("profile-a", None);
    old.set_at_unix_ms(now.saturating_sub(day_ms * 2));
    old.finalize(200, &log);

    let mut recent = RouteTraceBuilder::begin("req-recent", "POST", "/v1/messages");
    recent.local_auth_ok("profile-a", None);
    recent.finalize(200, &log);

    assert_eq!(log.persist_count("profile-a"), 1);
    assert_eq!(log.recent("profile-a")[0].request_id, "req-recent");
}

#[test]
fn trace_stage_status_as_str_matches_serde() {
    assert_eq!(TraceStageStatus::Pending.as_str(), "pending");
    assert_eq!(TraceStageStatus::Ok.as_str(), "ok");
    assert_eq!(TraceStageStatus::Failed.as_str(), "failed");
    assert_eq!(TraceStageStatus::Skipped.as_str(), "skipped");
    assert_eq!(TraceStageStatus::Interrupted.as_str(), "interrupted");
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
