use super::*;
use serde_json::json;

#[test]
fn is_loopback_base_url_accepts_ipv4_localhost_and_ipv6() {
    assert!(is_loopback_base_url("http://127.0.0.1:43081"));
    assert!(is_loopback_base_url("http://localhost:44227/v1"));
    assert!(is_loopback_base_url("http://[::1]:8080"));
    assert!(is_loopback_base_url("  http://127.0.0.1  "));
    assert!(credentials_are_loopback(&json!({
        "format": "api_key",
        "api_key": "tok",
        "base_url": "http://127.0.0.1:43081"
    })));
}

#[test]
fn is_loopback_base_url_rejects_remote_or_invalid() {
    assert!(!is_loopback_base_url("https://api.anthropic.com"));
    assert!(!is_loopback_base_url("https://relay.example.com"));
    assert!(!is_loopback_base_url("https://api.anthropic.com/v1"));
    assert!(!is_loopback_base_url(""));
    assert!(!is_loopback_base_url("not-a-url"));
    assert!(!is_loopback_base_url("127.0.0.1:43081"));
    assert!(!credentials_are_loopback(&json!({
        "format": "api_key",
        "api_key": "tok"
    })));
    assert!(!credentials_are_loopback(&json!({
        "format": "api_key",
        "api_key": "tok",
        "base_url": "https://api.anthropic.com"
    })));
}

#[test]
fn validate_upstream_base_url_rejects_non_loopback_http() {
    assert!(validate_upstream_base_url("http://169.254.169.254").is_err());
    assert!(validate_upstream_base_url("http://example.com").is_err());
    assert!(validate_upstream_base_url("http://0.0.0.0:8080").is_err());
    assert!(validate_upstream_base_url("ftp://127.0.0.1").is_err());
    assert!(validate_upstream_base_url("http://user:pass@127.0.0.1:8080").is_err());
    assert!(validate_upstream_base_url("https://example.com/#fragment").is_err());
}

#[test]
fn validate_upstream_base_url_accepts_https_and_loopback_http_and_normalizes_slash() {
    let loopback =
        validate_upstream_base_url("http://127.0.0.1:9/coding/v1").expect("loopback http");
    assert_eq!(loopback.path(), "/coding/v1/");
    let https = validate_upstream_base_url("https://api.example.com/v1").expect("https");
    assert_eq!(https.path(), "/v1/");
    assert!(validate_upstream_base_url("http://localhost/v1").is_ok());
    assert!(validate_upstream_base_url("http://[::1]/v1").is_ok());
}
