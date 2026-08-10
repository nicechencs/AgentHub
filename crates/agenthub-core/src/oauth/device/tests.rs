use super::*;
use serde_json::json;

#[test]
fn rfc_device_pending_body_is_preserved_from_http_400() {
    let body = parse_device_http_response(400, json!({"error": "authorization_pending"})).unwrap();
    assert_eq!(body["error"], "authorization_pending");
}

#[test]
fn rfc_device_slow_down_body_is_preserved_from_http_400() {
    let body =
        parse_device_http_response(400, json!({"error": "slow_down", "interval": 10})).unwrap();
    assert_eq!(body["error"], "slow_down");
    assert_eq!(body["interval"], 10);
}

#[test]
fn rfc_device_access_denied_and_expired_are_not_retried() {
    for error in ["access_denied", "expired_token"] {
        let body = parse_device_http_response(400, json!({"error": error})).unwrap();
        assert_eq!(body["error"], error);
    }
}

#[test]
fn transport_and_server_errors_are_retryable() {
    let server =
        parse_device_http_response(503, json!({"error": "temporarily_unavailable"})).unwrap_err();
    assert_eq!(server.code(), "oauth.device.retry");

    let html_gateway =
        ureq::Response::new(503, "Service Unavailable", "<html>bad gateway</html>").unwrap();
    let non_json = parse_device_status_response(503, html_gateway).unwrap_err();
    assert_eq!(non_json.code(), "oauth.device.retry");
}
