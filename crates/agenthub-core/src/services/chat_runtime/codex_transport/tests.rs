use super::*;

use serde_json::json;
#[cfg(unix)]
use tempfile::tempdir;

#[test]
fn classify_distinguishes_numeric_and_string_ids_and_message_kinds() {
    assert!(matches!(
        classify_message(json!({"id": 1, "result": {"ok": true}})),
        Ok(Some(WireMessage::Response { id, .. })) if id == json!(1)
    ));
    assert!(matches!(
        classify_message(json!({"id": "1", "result": {"ok": true}})),
        Ok(Some(WireMessage::Response { id, .. })) if id == json!("1")
    ));
    assert!(matches!(
        classify_message(json!({"id": 1, "method": "approve", "params": {}})),
        Ok(Some(WireMessage::Request { id, method, .. })) if id == json!(1) && method == "approve"
    ));
    assert!(matches!(
        classify_message(json!({"method": "notice", "params": {}})),
        Ok(Some(WireMessage::Notification { method, .. })) if method == "notice"
    ));
}

#[test]
fn classify_skips_unshaped_json_instead_of_failing() {
    for value in [
        json!({"id": 1}),
        json!({"result": {}}),
        json!({"type": "text", "content": "hi"}),
        json!({"error": "missing field `prompt`", "phase": "deserialization"}),
        json!({
            "jsonrpc": "2.0",
            "error": {
                "code": -32601,
                "message": "Method not found",
                "data": "initialized"
            }
        }),
        json!(null),
    ] {
        assert!(matches!(classify_message(value), Ok(None)));
    }
}

#[test]
fn classify_still_rejects_response_with_both_result_and_error() {
    assert!(matches!(
        classify_message(json!({"id": 1, "result": {}, "error": {}})),
        Err(CodexTransportError::Protocol(_))
    ));
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
fn zero_timeout_try_recv_drains_queued_notifications() {
    use std::os::unix::fs::PermissionsExt;

    let directory = tempdir().expect("temp directory");
    let ready = directory.path().join("ready");
    let ready_path = ready.display().to_string();
    let program = directory.path().join("fake-codex-burst");
    std::fs::write(
        &program,
        format!(
            r##"#!/bin/sh
IFS= read -r initialize
printf '%s\n' '{{"id":1,"result":{{"initialized":true}}}}'
i=0
while [ "$i" -lt 8 ]; do
  printf '%s\n' '{{"method":"notice","params":{{"n":'"$i"'}}}}'
  i=$((i + 1))
done
printf '%s\n' ready > "{ready_path}"
while IFS= read -r _; do
  :
done
"##
        ),
    )
    .expect("fake app-server script");
    let mut permissions = std::fs::metadata(&program)
        .expect("fake metadata")
        .permissions();
    permissions.set_mode(0o700);
    std::fs::set_permissions(&program, permissions).expect("fake executable");

    let mut transport = CodexTransport::spawn(&program, directory.path()).expect("spawn fake");
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline && !ready.is_file() {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(ready.is_file(), "burst notifications were not flushed");
    let mut got = 0;
    while got < 8 {
        match transport
            .recv_timeout(Duration::ZERO)
            .expect("zero-timeout recv")
        {
            Some(CodexEvent::Notification { method, .. }) if method == "notice" => got += 1,
            Some(CodexEvent::Exited) => panic!("process exited before draining notices"),
            Some(_) => {}
            None => break,
        }
    }
    assert_eq!(got, 8, "zero timeout must try_recv queued lines");
    assert_eq!(
        transport
            .recv_timeout(Duration::ZERO)
            .expect("empty try_recv"),
        None
    );
    transport.shutdown();
}

#[cfg(unix)]
#[test]
fn handshake_skips_bare_json_and_outgoing_lines_include_jsonrpc() {
    use std::os::unix::fs::PermissionsExt;

    let directory = tempdir().expect("temp directory");
    let program = directory.path().join("fake-kiro-acp");
    std::fs::write(
        &program,
        r##"#!/bin/sh
log="$(dirname "$0")/wire.log"
IFS= read -r initialize
printf '%s\n' "$initialize" >> "$log"
printf '%s\n' '{"type":"text","content":"hi"}'
printf '%s\n' '{"id":1,"result":{"initialized":true}}'
IFS= read -r initialized
printf '%s\n' "$initialized" >> "$log"
"##,
    )
    .expect("fake acp script");
    let mut permissions = std::fs::metadata(&program)
        .expect("fake metadata")
        .permissions();
    permissions.set_mode(0o700);
    std::fs::set_permissions(&program, permissions).expect("fake executable");

    let mut transport = CodexTransport::spawn(&program, directory.path()).expect("spawn fake");
    // Wait for the fake peer to read+log `initialized` and exit; shutting down
    // immediately races the shell `read` and flakes on busy CI runners.
    assert_eq!(
        transport
            .recv_timeout(Duration::from_secs(2))
            .expect("peer exit receive"),
        Some(CodexEvent::Exited)
    );
    let wire = std::fs::read_to_string(directory.path().join("wire.log")).expect("wire log");
    assert!(
        wire.contains(r#""jsonrpc":"2.0""#) && wire.contains(r#""method":"initialize""#),
        "outgoing initialize must be JSON-RPC 2.0: {wire}"
    );
    assert!(
        wire.contains(r#""method":"initialized""#),
        "outgoing initialized notification missing: {wire}"
    );
}

#[cfg(unix)]
#[test]
fn spawn_kiro_skips_initialized_and_id_less_errors() {
    use std::os::unix::fs::PermissionsExt;

    let directory = tempdir().expect("temp directory");
    let program = directory.path().join("fake-kiro-acp");
    std::fs::write(
        &program,
        r##"#!/bin/sh
log="$(dirname "$0")/wire.log"
IFS= read -r initialize
printf '%s\n' "$initialize" >> "$log"
printf '%s\n' '{"jsonrpc":"2.0","error":{"code":-32601,"message":"Method not found","data":"initialized"}}'
printf '%s\n' '{"id":1,"result":{"initialized":true}}'
IFS= read -r extra
printf '%s\n' "$extra" >> "$log"
"##,
    )
    .expect("fake acp script");
    let mut permissions = std::fs::metadata(&program)
        .expect("fake metadata")
        .permissions();
    permissions.set_mode(0o700);
    std::fs::set_permissions(&program, permissions).expect("fake executable");

    let mut transport = CodexTransport::spawn_kiro(&program, directory.path(), None, None, false)
        .expect("spawn kiro");
    transport.shutdown();
    let wire = std::fs::read_to_string(directory.path().join("wire.log")).expect("wire log");
    assert!(
        wire.contains(r#""jsonrpc":"2.0""#) && wire.contains(r#""method":"initialize""#),
        "outgoing initialize must be JSON-RPC 2.0: {wire}"
    );
    assert!(
        !wire.contains(r#""method":"initialized""#),
        "Kiro handshake must not send initialized: {wire}"
    );
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

#[cfg(windows)]
#[test]
fn powershell_restore_discards_history_without_filling_event_queue() {
    let directory = tempfile::tempdir().expect("temp directory");
    let script = directory.path().join("fake-acp.ps1");
    std::fs::write(
        &script,
        r#"
$OutputEncoding = [Console]::OutputEncoding = [Text.UTF8Encoding]::new($false)
$log = Join-Path (Split-Path -Parent $PSCommandPath) 'wire.log'
while ($null -ne ($line = [Console]::In.ReadLine())) {
  if ($line -like '*"method":"initialize"*') {
    [Console]::Out.WriteLine('{"jsonrpc":"2.0","id":1,"result":{"agentCapabilities":{"loadSession":true}}}')
    [Console]::Out.Flush()
  } elseif ($line -like '*"method":"session/load"*') {
    [Console]::Out.WriteLine('{"jsonrpc":"2.0","id":"replay-permission","method":"session/request_permission","params":{}}')
    0..299 | ForEach-Object {
      [Console]::Out.WriteLine('{"jsonrpc":"2.0","method":"session/update","params":{"delta":"old"}}')
    }
    [Console]::Out.WriteLine('{"jsonrpc":"2.0","id":2,"result":{"sessionId":"restored"}}')
    [Console]::Out.Flush()
    $permissionReply = [Console]::In.ReadLine()
    Add-Content -LiteralPath $log -Value $permissionReply -Encoding utf8
  }
}
"#,
    )
    .expect("fake ACP script");
    let powershell = std::env::var_os("SystemRoot")
        .map(std::path::PathBuf::from)
        .map(|root| root.join("System32\\WindowsPowerShell\\v1.0\\powershell.exe"))
        .unwrap_or_else(|| "powershell.exe".into());
    let args = vec![
        "-NoProfile".into(),
        "-ExecutionPolicy".into(),
        "Bypass".into(),
        "-File".into(),
        script.to_string_lossy().into_owned(),
    ];
    let mut transport = CodexTransport::spawn_test_with(
        &powershell,
        &args,
        directory.path(),
        json!({"protocolVersion": 1}),
        false,
    )
    .expect("spawn powershell ACP");
    assert_eq!(
        transport
            .request_discarding_history(
                "session/load",
                json!({"sessionId":"old"}),
                Duration::from_secs(2),
            )
            .expect("restore response")["sessionId"],
        "restored"
    );
    assert_eq!(
        transport
            .recv_timeout(Duration::from_millis(100))
            .expect("history queue check"),
        None
    );
    transport.shutdown();
    let wire = std::fs::read_to_string(directory.path().join("wire.log"))
        .expect("permission replay wire log");
    assert!(
        wire.contains(r#""id":"replay-permission","result":{"outcome":{"outcome":"cancelled"}}"#)
    );
}

#[cfg(windows)]
#[test]
fn powershell_cancel_notification_has_no_id_and_does_not_wait_for_response() {
    let directory = tempfile::tempdir().expect("temp directory");
    let script = directory.path().join("fake-cancel.ps1");
    std::fs::write(
        &script,
        r#"
$OutputEncoding = [Console]::OutputEncoding = [Text.UTF8Encoding]::new($false)
$log = Join-Path (Split-Path -Parent $PSCommandPath) 'wire.log'
$line = [Console]::In.ReadLine()
[Console]::Out.WriteLine('{"jsonrpc":"2.0","id":1,"result":{"agentCapabilities":{}}}')
[Console]::Out.Flush()
while ($null -ne ($line = [Console]::In.ReadLine())) {
  Add-Content -LiteralPath $log -Value $line -Encoding utf8
  [Console]::Out.WriteLine('{"jsonrpc":"2.0","method":"test/cancel_seen","params":{}}')
  [Console]::Out.Flush()
}
"#,
    )
    .expect("fake ACP script");
    let powershell = std::env::var_os("SystemRoot")
        .map(std::path::PathBuf::from)
        .map(|root| root.join("System32\\WindowsPowerShell\\v1.0\\powershell.exe"))
        .unwrap_or_else(|| "powershell.exe".into());
    let args = vec![
        "-NoProfile".into(),
        "-ExecutionPolicy".into(),
        "Bypass".into(),
        "-File".into(),
        script.to_string_lossy().into_owned(),
    ];
    let mut transport = CodexTransport::spawn_test_with(
        &powershell,
        &args,
        directory.path(),
        json!({"protocolVersion": 1}),
        false,
    )
    .expect("spawn powershell ACP");
    let started = Instant::now();
    transport
        .notify("session/cancel", Some(json!({"sessionId":"session-1"})))
        .expect("send cancel notification");
    assert!(started.elapsed() < Duration::from_secs(1));
    assert_eq!(
        transport
            .recv_timeout(Duration::from_secs(1))
            .expect("cancel barrier"),
        Some(CodexEvent::Notification {
            method: "test/cancel_seen".into(),
            params: json!({}),
        })
    );
    transport.shutdown();
    let wire = std::fs::read_to_string(directory.path().join("wire.log")).expect("cancel wire log");
    let cancel = wire
        .lines()
        .find_map(|line| {
            let value: Value = serde_json::from_str(line.trim_start_matches('\u{feff}')).ok()?;
            (value.get("method").and_then(Value::as_str) == Some("session/cancel")).then_some(value)
        })
        .expect("session/cancel wire message");
    assert!(cancel.get("id").is_none(), "cancel must be a notification");
}
