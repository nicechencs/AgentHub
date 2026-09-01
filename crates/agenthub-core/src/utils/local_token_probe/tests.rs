use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;
use std::time::Duration;

use super::{loopback_health_url, probe_local_token, LocalTokenProbeOutcome};

fn spawn_http(status_line: &'static str, body: &'static str) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let port = listener.local_addr().expect("addr").port();
    thread::spawn(move || {
        let Ok((mut stream, _)) = listener.accept() else {
            return;
        };
        let mut buf = [0u8; 2048];
        let _ = stream.read(&mut buf);
        let response = format!(
            "HTTP/1.1 {status_line}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let _ = stream.write_all(response.as_bytes());
        let _ = stream.flush();
        thread::sleep(Duration::from_millis(20));
    });
    port
}

#[test]
fn loopback_health_url_rewrites_origin_and_rejects_remote() {
    assert_eq!(
        loopback_health_url("127.0.0.1:8123").as_deref(),
        Some("http://127.0.0.1:8123/health")
    );
    assert_eq!(
        loopback_health_url("http://127.0.0.1:8123/v1/chat/completions").as_deref(),
        Some("http://127.0.0.1:8123/health")
    );
    assert!(loopback_health_url("https://api.anthropic.com").is_none());
    assert!(loopback_health_url("").is_none());
    assert!(loopback_health_url("not-a-host").is_none());
}

#[test]
fn probe_rejects_empty_token_and_remote_hosts() {
    let empty = probe_local_token("127.0.0.1:1", "  ");
    assert_eq!(empty.outcome, LocalTokenProbeOutcome::Invalid);
    assert_eq!(
        empty.request_url.as_deref(),
        Some("http://127.0.0.1:1/health")
    );
    let remote = probe_local_token("https://example.com", "ahb_secret");
    assert_eq!(remote.outcome, LocalTokenProbeOutcome::Invalid);
    assert!(remote.request_url.is_none());
}

#[test]
fn probe_reports_ok_unauthorized_and_unreachable() {
    let ok_port = spawn_http("200 OK", r#"{"ok":true,"upstream_status":"unknown"}"#);
    let ok = probe_local_token(&format!("127.0.0.1:{ok_port}"), "ahb_secret");
    assert_eq!(ok.outcome, LocalTokenProbeOutcome::Ok);
    assert_eq!(ok.http_status, Some(200));
    assert_eq!(ok.upstream_status.as_deref(), Some("unknown"));
    let expected_url = format!("http://127.0.0.1:{ok_port}/health");
    assert_eq!(ok.request_url.as_deref(), Some(expected_url.as_str()));
    assert!(ok.response_body.as_deref().unwrap_or("").contains("unknown"));
    assert!(ok.error_message.is_none());

    let unauth_port = spawn_http(
        "401 Unauthorized",
        r#"{"error":{"code":"invalid_api_key"}}"#,
    );
    let unauth = probe_local_token(&format!("127.0.0.1:{unauth_port}"), "ahb_wrong");
    assert_eq!(unauth.outcome, LocalTokenProbeOutcome::Unauthorized);
    assert_eq!(unauth.http_status, Some(401));
    assert!(unauth
        .response_body
        .as_deref()
        .unwrap_or("")
        .contains("invalid_api_key"));

    let closed = TcpListener::bind("127.0.0.1:0").expect("bind closed");
    let closed_port = closed.local_addr().expect("addr").port();
    drop(closed);
    let unreachable = probe_local_token(&format!("127.0.0.1:{closed_port}"), "ahb_secret");
    assert_eq!(unreachable.outcome, LocalTokenProbeOutcome::Unreachable);
    assert_eq!(unreachable.http_status, None);
    assert!(unreachable.error_message.is_some());
    assert!(!unreachable
        .error_message
        .as_deref()
        .unwrap_or("")
        .contains("ahb_secret"));
}
