use serde_json::json;

use super::{
    sanitize_method, sanitize_path, InboundRequestLog, InboundRequestRecord, INBOUND_LOG_CAP,
};
use crate::bridge::runtime::{
    BridgeLocalSurface, BridgeStartSpec, BridgeUpstreamConfig, BridgeUpstreamProtocol, ResolvedAuth,
};
use crate::bridge::BridgeRuntimeHost;

#[test]
fn ring_keeps_last_20_newest_first() {
    let log = InboundRequestLog::new();
    for status in 1..=25_u16 {
        log.push(
            "profile-a",
            InboundRequestRecord::new("POST", "/v1/responses", 200 + status),
        );
    }
    let recent = log.recent("profile-a");
    assert_eq!(recent.len(), INBOUND_LOG_CAP);
    assert_eq!(recent[0].status, 225);
    assert_eq!(recent[19].status, 206);
    assert!(log.recent("profile-b").is_empty());
}

#[test]
fn stats_survive_ring_truncation_and_count_failures() {
    let log = InboundRequestLog::new();
    for i in 0..25_u16 {
        let status = if i % 5 == 0 { 500 } else { 200 };
        log.push(
            "profile-a",
            InboundRequestRecord::new("POST", "/v1/responses", status),
        );
    }
    let stats = log.stats("profile-a");
    assert_eq!(stats.total_request_count, 25);
    assert_eq!(stats.failed_request_count, 5);
    assert!(stats.last_request_at_unix_ms.is_some());
    assert_eq!(log.recent("profile-a").len(), INBOUND_LOG_CAP);
    assert_eq!(log.stats("profile-b").total_request_count, 0);
}

#[test]
fn record_struct_never_serializes_secrets() {
    let record = InboundRequestRecord::new(
        "POST",
        "/v1/responses?api_key=sk-secret&token=abc#Authorization=Bearer%20x",
        200,
    );
    let value = serde_json::to_value(&record).expect("json");
    let object = value.as_object().expect("object");
    let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
    keys.sort();
    assert_eq!(keys, ["atUnixMs", "method", "ok", "path", "status"]);
    assert_eq!(value["method"], "POST");
    assert_eq!(value["path"], "/v1/responses");
    assert_eq!(value["status"], 200);
    assert_eq!(value["ok"], true);
    let text = serde_json::to_string(&record).expect("text");
    assert!(!text.contains("sk-secret"));
    assert!(!text.contains("Bearer"));
    assert!(!text.contains("Authorization"));
    assert!(!text.contains("api_key"));
    assert!(!text.contains("token"));
    assert!(!text.contains('?'));
    assert_eq!(
        serde_json::to_value(&record).unwrap(),
        json!({
            "atUnixMs": record.at_unix_ms,
            "method": "POST",
            "path": "/v1/responses",
            "status": 200,
            "ok": true,
        })
    );
}

#[test]
fn sanitize_drops_query_and_unknown_methods() {
    assert_eq!(sanitize_path("/v1/messages?foo=bar"), "/v1/messages");
    assert_eq!(sanitize_path("/models#token=x"), "/models");
    assert_eq!(
        sanitize_path("/v1/chat/completions"),
        "/v1/chat/completions"
    );
    assert_eq!(sanitize_method("POST\nAuthorization: Bearer x"), "POST");
    assert_eq!(sanitize_method("TRACE"), "OTHER");
}

fn start_spec(profile_id: &str, local_token: &str) -> BridgeStartSpec {
    BridgeStartSpec::new(
        profile_id,
        0,
        local_token,
        BridgeUpstreamConfig {
            base_url: "https://example.invalid/v1".into(),
            model: None,
            source_id: Some("connection-test".into()),
            auth: ResolvedAuth::bearer("upstream-test-token"),
            protocol: BridgeUpstreamProtocol::OpenAiChatCompletions,
            local_surface: BridgeLocalSurface::Responses,
        },
    )
}

#[tokio::test]
async fn http_health_and_models_are_logged_without_query_or_secrets() {
    let host = BridgeRuntimeHost::new();
    let started = host
        .start(start_spec("inbound-profile", "local-test-token"))
        .await
        .expect("start");
    let client = reqwest::Client::builder().build().expect("client");
    let origin = format!("http://127.0.0.1:{}", started.port);

    let health = client
        .get(format!("{origin}/health?token=sk-secret"))
        .header("authorization", "Bearer local-test-token")
        .send()
        .await
        .expect("health");
    assert_eq!(health.status(), reqwest::StatusCode::OK);

    let models = client
        .get(format!("{origin}/models"))
        .header("authorization", "Bearer local-test-token")
        .send()
        .await
        .expect("models");
    assert_eq!(models.status(), reqwest::StatusCode::OK);

    let unauthorized = client
        .get(format!("{origin}/health"))
        .send()
        .await
        .expect("unauthorized");
    assert_eq!(unauthorized.status(), reqwest::StatusCode::UNAUTHORIZED);

    let recent = host.recent_inbound("inbound-profile");
    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0].method, "GET");
    assert_eq!(recent[0].path, "/models");
    assert_eq!(recent[0].status, 200);
    assert!(recent[0].ok);
    assert!(recent.iter().all(|row| row.path != "/health"));
    let stats = host.inbound_stats("inbound-profile");
    assert_eq!(stats.total_request_count, 1);
    assert_eq!(stats.failed_request_count, 0);
    assert_eq!(stats.last_request_at_unix_ms, Some(recent[0].at_unix_ms));
    let json = serde_json::to_string(&recent).expect("json");
    assert!(!json.contains("sk-secret"));
    assert!(!json.contains("local-test-token"));
    assert!(!json.contains("upstream-test-token"));
    assert!(!json.contains("Authorization"));
    assert!(!json.contains("authorization"));
    host.shutdown().await.expect("shutdown");
}
