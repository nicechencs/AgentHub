use super::*;

#[test]
fn rejects_non_http_urls() {
    assert!(validate_http_url("file:///etc/passwd").is_err());
    assert!(validate_http_url("javascript:alert(1)").is_err());
    assert!(validate_http_url("https://v2.pincc.ai/api/v1/settings/public").is_ok());
}

#[test]
fn safe_path_omits_query() {
    let p = safe_path_for_log("https://v2.pincc.ai/api/v1/auth/login?x=1");
    assert!(p.contains("v2.pincc.ai/api/v1/auth/login"));
    assert!(!p.contains("x=1"));
}
