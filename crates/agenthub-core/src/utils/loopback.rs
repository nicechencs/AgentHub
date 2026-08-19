//! Loopback URL / host checks shared by the local bridge and live import.

use std::net::IpAddr;

use serde_json::Value;

/// True when `host` is `localhost` or any [`IpAddr::is_loopback`] address.
///
/// `Url::host_str()` keeps IPv6 brackets (`[::1]`); strip them before parsing.
pub(crate) fn is_loopback_host(host: Option<&str>) -> bool {
    let Some(host) = host else {
        return false;
    };
    let host = host
        .strip_prefix('[')
        .and_then(|inner| inner.strip_suffix(']'))
        .unwrap_or(host);
    host == "localhost"
        || host
            .parse::<IpAddr>()
            .is_ok_and(|address| address.is_loopback())
}

/// True when `raw` is an http(s) URL whose host is loopback.
///
/// Parse failures, missing URLs, and remote hosts fail closed.
pub(crate) fn is_loopback_base_url(raw: &str) -> bool {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return false;
    }
    let Ok(url) = reqwest::Url::parse(trimmed) else {
        return false;
    };
    matches!(url.scheme(), "http" | "https") && is_loopback_host(url.host_str())
}

/// True when an account credential object has a loopback `base_url`.
pub(crate) fn credentials_are_loopback(credentials: &Value) -> bool {
    credentials
        .get("base_url")
        .and_then(|value| value.as_str())
        .is_some_and(is_loopback_base_url)
}

#[cfg(test)]
mod tests {
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
}
