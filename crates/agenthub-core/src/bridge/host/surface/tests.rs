use crate::bridge::runtime::BridgeLocalSurface;

use super::DownstreamSurface;

#[test]
fn from_local_matches_conversation_surfaces() {
    assert_eq!(
        DownstreamSurface::from_local(BridgeLocalSurface::Responses),
        DownstreamSurface::Responses
    );
    assert_eq!(
        DownstreamSurface::from_local(BridgeLocalSurface::Messages),
        DownstreamSurface::Messages
    );
    assert_eq!(
        DownstreamSurface::from_local(BridgeLocalSurface::ChatCompletions),
        DownstreamSurface::ChatCompletions
    );
}

#[test]
fn models_always_served_conversation_surfaces_must_match() {
    for local in [
        BridgeLocalSurface::Responses,
        BridgeLocalSurface::Messages,
        BridgeLocalSurface::ChatCompletions,
    ] {
        assert!(DownstreamSurface::Models.served_by(local));
        assert!(DownstreamSurface::from_local(local).served_by(local));
    }
    assert!(!DownstreamSurface::Responses.served_by(BridgeLocalSurface::Messages));
    assert!(!DownstreamSurface::Responses.served_by(BridgeLocalSurface::ChatCompletions));
    assert!(!DownstreamSurface::Messages.served_by(BridgeLocalSurface::Responses));
    assert!(!DownstreamSurface::Messages.served_by(BridgeLocalSurface::ChatCompletions));
    assert!(!DownstreamSurface::ChatCompletions.served_by(BridgeLocalSurface::Responses));
    assert!(!DownstreamSurface::ChatCompletions.served_by(BridgeLocalSurface::Messages));
}

#[test]
fn op_strings_match_existing_log_contract() {
    assert_eq!(DownstreamSurface::Responses.op(), "responses");
    assert_eq!(DownstreamSurface::Messages.op(), "messages");
    assert_eq!(DownstreamSurface::ChatCompletions.op(), "chat");
    assert_eq!(DownstreamSurface::Models.op(), "models");
}

#[test]
fn surface_mismatch_message_names_served_and_requested_paths() {
    let msg = super::surface_mismatch_message(
        DownstreamSurface::ChatCompletions,
        BridgeLocalSurface::Responses,
    );
    assert!(msg.contains("/v1/responses"), "{msg}");
    assert!(msg.contains("/v1/chat/completions"), "{msg}");
    assert!(msg.contains("This route only serves"), "{msg}");
    assert!(msg.contains("本机路由只提供"), "{msg}");
}

#[test]
fn surface_mismatch_response_is_not_found_with_json_content_type() {
    let response = super::surface_mismatch_response(
        DownstreamSurface::Messages,
        BridgeLocalSurface::ChatCompletions,
    );
    assert_eq!(response.status(), axum::http::StatusCode::NOT_FOUND);
    let ctype = response
        .headers()
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(
        ctype.starts_with("application/json"),
        "expected JSON content-type, got {ctype:?}"
    );
}

#[test]
fn surface_mismatch_error_json_shape_matches_bridge_contract() {
    let message = super::surface_mismatch_message(
        DownstreamSurface::Messages,
        BridgeLocalSurface::ChatCompletions,
    );
    let value = serde_json::json!({
        "error": {
            "code": "surface_mismatch",
            "message": message,
            "type": "invalid_request_error",
        }
    });
    let error = value.get("error").expect("error object");
    assert_eq!(error.get("code").and_then(|v| v.as_str()), Some("surface_mismatch"));
    assert_eq!(
        error.get("type").and_then(|v| v.as_str()),
        Some("invalid_request_error")
    );
    let message = error.get("message").and_then(|v| v.as_str()).unwrap_or("");
    assert!(message.contains("/v1/chat/completions"), "{message}");
    assert!(message.contains("/v1/messages"), "{message}");
}

#[test]
fn method_not_allowed_message_is_bilingual_and_names_path() {
    let msg = super::method_not_allowed_message("/v1/messages");
    assert!(msg.contains("POST /v1/messages"), "{msg}");
    assert!(msg.contains("This endpoint only accepts POST"), "{msg}");
    assert!(msg.contains("本机该路径只接受 POST"), "{msg}");
}

#[test]
fn method_not_allowed_response_is_405_json_with_allow_post() {
    let response = super::method_not_allowed_response("/v1/responses");
    assert_eq!(response.status(), axum::http::StatusCode::METHOD_NOT_ALLOWED);
    let ctype = response
        .headers()
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(
        ctype.starts_with("application/json"),
        "expected JSON content-type, got {ctype:?}"
    );
    let allow = response
        .headers()
        .get(axum::http::header::ALLOW)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(
        allow.split(',').any(|m| m.trim().eq_ignore_ascii_case("POST")),
        "expected Allow to include POST, got {allow:?}"
    );
}

#[test]
fn method_not_allowed_error_json_shape_matches_bridge_contract() {
    let message = super::method_not_allowed_message("/v1/chat/completions");
    let value = serde_json::json!({
        "error": {
            "code": "method_not_allowed",
            "message": message,
            "type": "invalid_request_error",
        }
    });
    let error = value.get("error").expect("error object");
    assert_eq!(
        error.get("code").and_then(|v| v.as_str()),
        Some("method_not_allowed")
    );
    assert_eq!(
        error.get("type").and_then(|v| v.as_str()),
        Some("invalid_request_error")
    );
    let message = error.get("message").and_then(|v| v.as_str()).unwrap_or("");
    assert!(message.contains("/v1/chat/completions"), "{message}");
    assert!(message.contains("POST"), "{message}");
}
