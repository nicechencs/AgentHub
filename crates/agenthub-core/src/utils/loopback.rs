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

/// HTTPS anywhere, or HTTP only to a loopback host. Rejects userinfo and
/// fragments. Normalizes a trailing slash so `Url::join` keeps the last path
/// segment.
pub(crate) fn validate_upstream_base_url(raw: &str) -> Result<reqwest::Url, ()> {
    let Ok(mut url) = reqwest::Url::parse(raw.trim()) else {
        return Err(());
    };
    if url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(());
    }
    match url.scheme() {
        "https" => {}
        "http" if is_loopback_host(url.host_str()) => {}
        _ => return Err(()),
    }
    if !url.path().ends_with('/') {
        let path = format!("{}/", url.path());
        url.set_path(&path);
    }
    Ok(url)
}

#[cfg(test)]
mod tests;
