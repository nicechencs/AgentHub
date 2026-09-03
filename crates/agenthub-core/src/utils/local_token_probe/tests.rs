use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;
use std::time::Duration;

use super::{probe_local_token, LocalTokenProbeOutcome};

fn spawn_seq(replies: Vec<(u16, &'static str)>) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let port = listener.local_addr().expect("addr").port();
    thread::spawn(move || {
        for (status, body) in replies {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            let mut buf = [0u8; 8192];
            let _ = stream.read(&mut buf);
            let reason = if status == 200 {
                "OK"
            } else if status == 401 {
                "Unauthorized"
            } else {
                "Error"
            };
            let response = format!(
                "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();
            thread::sleep(Duration::from_millis(20));
        }
    });
    port
}

#[test]
fn probe_rejects_empty_token_remote_hosts_and_health_path() {
    let empty = probe_local_token("127.0.0.1:1", "  ", "/v1/chat/completions", None);
    assert_eq!(empty.outcome, LocalTokenProbeOutcome::Invalid);
    assert_eq!(
        empty.request_url.as_deref(),
        Some("http://127.0.0.1:1/v1/chat/completions")
    );
    let remote = probe_local_token(
        "https://example.com",
        "ahb_secret",
        "/v1/chat/completions",
        None,
    );
    assert_eq!(remote.outcome, LocalTokenProbeOutcome::Invalid);
    assert!(remote.request_url.is_none());
    let health = probe_local_token("127.0.0.1:1", "ahb_secret", "/health", None);
    assert_eq!(health.outcome, LocalTokenProbeOutcome::Invalid);
}

#[test]
fn probe_posts_chat_completions_after_listing_models() {
    let port = spawn_seq(vec![
        (200, r#"{"object":"list","data":[{"id":"kimi-k2"}]}"#),
        (200, r#"{"choices":[{"message":{"content":"ok"}}]}"#),
    ]);
    let ok = probe_local_token(
        &format!("127.0.0.1:{port}"),
        "ahb_secret",
        "/v1/chat/completions",
        None,
    );
    assert_eq!(ok.outcome, LocalTokenProbeOutcome::Ok);
    assert_eq!(ok.http_status, Some(200));
    assert_eq!(ok.request_method.as_deref(), Some("POST"));
    let expected_url = format!("http://127.0.0.1:{port}/v1/chat/completions");
    assert_eq!(ok.request_url.as_deref(), Some(expected_url.as_str()));
    assert!(ok
        .request_body
        .as_deref()
        .unwrap_or("")
        .contains("\"model\":\"kimi-k2\""));
    assert!(ok
        .request_body
        .as_deref()
        .unwrap_or("")
        .contains("\"content\":\"ping\""));
    assert!(ok.response_body.as_deref().unwrap_or("").contains("ok"));
    assert!(ok.error_message.is_none());
}

#[test]
fn probe_posts_known_model_without_listing() {
    let port = spawn_seq(vec![(200, r#"{"choices":[{"message":{"content":"ok"}}]}"#)]);
    let ok = probe_local_token(
        &format!("127.0.0.1:{port}"),
        "ahb_secret",
        "/v1/chat/completions",
        Some("kimi-k2"),
    );
    assert_eq!(ok.outcome, LocalTokenProbeOutcome::Ok);
    assert_eq!(ok.request_method.as_deref(), Some("POST"));
    assert!(ok
        .request_body
        .as_deref()
        .unwrap_or("")
        .contains("\"model\":\"kimi-k2\""));
}

#[test]
fn probe_posts_messages_and_responses_bodies() {
    let messages_port = spawn_seq(vec![
        (200, r#"{"data":[{"id":"claude-sonnet"}]}"#),
        (200, r#"{"content":[{"type":"text","text":"ok"}]}"#),
    ]);
    let messages = probe_local_token(
        &format!("127.0.0.1:{messages_port}"),
        "ahb_secret",
        "/v1/messages",
        None,
    );
    assert_eq!(messages.outcome, LocalTokenProbeOutcome::Ok);
    assert!(messages
        .request_body
        .as_deref()
        .unwrap_or("")
        .contains("\"max_tokens\":8"));

    let responses_port = spawn_seq(vec![
        (200, r#"{"data":[{"id":"gpt-5"}]}"#),
        (200, r#"{"output":[{"type":"message"}]}"#),
    ]);
    let responses = probe_local_token(
        &format!("127.0.0.1:{responses_port}"),
        "ahb_secret",
        "/v1/responses",
        None,
    );
    assert_eq!(responses.outcome, LocalTokenProbeOutcome::Ok);
    assert!(responses
        .request_body
        .as_deref()
        .unwrap_or("")
        .contains("\"input\":\"ping\""));
}

#[test]
fn probe_reports_unauthorized_empty_models_and_unreachable() {
    let unauth_port = spawn_seq(vec![(401, r#"{"error":{"code":"invalid_api_key"}}"#)]);
    let unauth = probe_local_token(
        &format!("127.0.0.1:{unauth_port}"),
        "ahb_wrong",
        "/v1/chat/completions",
        None,
    );
    assert_eq!(unauth.outcome, LocalTokenProbeOutcome::Unauthorized);
    assert_eq!(unauth.http_status, Some(401));
    assert_eq!(unauth.request_method.as_deref(), Some("GET"));
    assert!(unauth
        .response_body
        .as_deref()
        .unwrap_or("")
        .contains("invalid_api_key"));

    let empty_port = spawn_seq(vec![(200, r#"{"object":"list","data":[]}"#)]);
    let empty = probe_local_token(
        &format!("127.0.0.1:{empty_port}"),
        "ahb_secret",
        "/v1/chat/completions",
        None,
    );
    assert_eq!(empty.outcome, LocalTokenProbeOutcome::Rejected);
    assert_eq!(
        empty.error_message.as_deref(),
        Some("这条路由还没有可用模型")
    );

    let closed = TcpListener::bind("127.0.0.1:0").expect("bind closed");
    let closed_port = closed.local_addr().expect("addr").port();
    drop(closed);
    let unreachable = probe_local_token(
        &format!("127.0.0.1:{closed_port}"),
        "ahb_secret",
        "/v1/chat/completions",
        None,
    );
    assert_eq!(unreachable.outcome, LocalTokenProbeOutcome::Unreachable);
    assert_eq!(unreachable.http_status, None);
    assert!(unreachable.error_message.is_some());
    assert!(!unreachable
        .error_message
        .as_deref()
        .unwrap_or("")
        .contains("ahb_secret"));
}
