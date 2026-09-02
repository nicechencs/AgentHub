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
