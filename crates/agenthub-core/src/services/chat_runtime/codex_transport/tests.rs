use super::*;

use serde_json::json;
use tempfile::tempdir;

#[test]
fn classify_distinguishes_numeric_and_string_ids_and_message_kinds() {
    assert!(matches!(
        classify_message(json!({"id": 1, "result": {"ok": true}})),
        Ok(WireMessage::Response { id, .. }) if id == json!(1)
    ));
    assert!(matches!(
        classify_message(json!({"id": "1", "result": {"ok": true}})),
        Ok(WireMessage::Response { id, .. }) if id == json!("1")
    ));
    assert!(matches!(
        classify_message(json!({"id": 1, "method": "approve", "params": {}})),
        Ok(WireMessage::Request { id, method, .. }) if id == json!(1) && method == "approve"
    ));
    assert!(matches!(
        classify_message(json!({"method": "notice", "params": {}})),
        Ok(WireMessage::Notification { method, .. }) if method == "notice"
    ));
}

#[test]
fn classify_rejects_ambiguous_or_incomplete_messages() {
    for value in [
        json!({"id": 1}),
        json!({"result": {}}),
        json!({"id": 1, "result": {}, "error": {}}),
        json!(null),
    ] {
        assert!(matches!(
            classify_message(value),
            Err(CodexTransportError::Protocol(_))
        ));
    }
}

#[cfg(unix)]
#[test]
fn fake_app_server_preserves_half_line_notifications_and_answers_requests() {
    use std::os::unix::fs::PermissionsExt;

    let directory = tempdir().expect("temp directory");
    let program = directory.path().join("fake-codex");
    std::fs::write(
        &program,
        r##"#!/bin/sh
IFS= read -r initialize
printf '%s' '{"method":"half-line","params":{"ready":'
sleep 0.02
printf '%s\n' 'true}}'
printf '%s\n' '{"id":1,"result":{"initialized":true}}'
IFS= read -r initialized
printf '%s\n' '{"id":"server-1","method":"approval/request","params":{"kind":"test"}}'
IFS= read -r response
case "$response" in
  *'"id":"server-1"'*'"result":{"approved":false}'*) ;;
  *) exit 9 ;;
esac
IFS= read -r echo_request
printf '%s\n' '{"id":2,"result":{"echo":true}}'
"##,
    )
    .expect("fake app-server script");
    let mut permissions = std::fs::metadata(&program)
        .expect("fake metadata")
        .permissions();
    permissions.set_mode(0o700);
    std::fs::set_permissions(&program, permissions).expect("fake executable");

    let mut transport = CodexTransport::spawn(&program, directory.path()).expect("spawn fake");
    assert_eq!(
        transport
            .recv_timeout(Duration::from_secs(1))
            .expect("notification receive"),
        Some(CodexEvent::Notification {
            method: "half-line".into(),
            params: json!({"ready": true}),
        })
    );
    let request = transport
        .recv_timeout(Duration::from_secs(1))
        .expect("server request receive")
        .expect("server request");
    assert_eq!(
        request,
        CodexEvent::Request {
            id: json!("server-1"),
            method: "approval/request".into(),
            params: json!({"kind": "test"}),
        }
    );
    transport
        .respond(json!("server-1"), Ok(json!({"approved": false})))
        .expect("respond to server request");
    assert_eq!(
        transport
            .request("echo", json!({}), Duration::from_secs(1))
            .expect("echo response"),
        json!({"echo": true})
    );
    transport.shutdown();
}

#[cfg(unix)]
#[test]
fn fake_app_server_exit_is_reported_after_last_response() {
    use std::os::unix::fs::PermissionsExt;

    let directory = tempdir().expect("temp directory");
    let program = directory.path().join("fake-codex-exit");
    std::fs::write(
        &program,
        r##"#!/bin/sh
IFS= read -r initialize
printf '%s\n' '{"id":1,"result":{"initialized":true}}'
IFS= read -r initialized
exit 0
"##,
    )
    .expect("fake app-server script");
    let mut permissions = std::fs::metadata(&program)
        .expect("fake metadata")
        .permissions();
    permissions.set_mode(0o700);
    std::fs::set_permissions(&program, permissions).expect("fake executable");

    let mut transport = CodexTransport::spawn(&program, directory.path()).expect("spawn fake");
    assert_eq!(
        transport
            .recv_timeout(Duration::from_secs(1))
            .expect("exit receive"),
        Some(CodexEvent::Exited)
    );
    assert!(matches!(
        transport.request("after-exit", json!({}), Duration::from_millis(20)),
        Err(CodexTransportError::Exited)
    ));
}

#[test]
fn stderr_capture_is_bounded() {
    let capture = Arc::new(Mutex::new(Vec::new()));
    let stop = Arc::new(AtomicBool::new(false));
    read_stderr(
        &vec![b'x'; MAX_STDERR_BYTES * 2][..],
        Arc::clone(&capture),
        stop,
    );
    assert_eq!(
        capture.lock().expect("capture lock").len(),
        MAX_STDERR_BYTES
    );
}
