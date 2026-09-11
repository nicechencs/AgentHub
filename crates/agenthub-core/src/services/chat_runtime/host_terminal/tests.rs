use super::*;
use crate::services::chat_runtime::ops::AcpTerminalCreate;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

fn echo_spec(cwd: &std::path::Path) -> AcpTerminalCreate {
    #[cfg(windows)]
    {
        AcpTerminalCreate {
            command: "cmd.exe".into(),
            args: vec!["/C".into(), "echo hello-host".into()],
            cwd: cwd.to_path_buf(),
            env: Vec::new(),
            output_limit: DEFAULT_OUTPUT_LIMIT,
        }
    }
    #[cfg(not(windows))]
    {
        AcpTerminalCreate {
            command: "echo".into(),
            args: vec!["hello-host".into()],
            cwd: cwd.to_path_buf(),
            env: Vec::new(),
            output_limit: DEFAULT_OUTPUT_LIMIT,
        }
    }
}

fn wait_exit(host: &mut HostedTerminals, id: &str) -> i32 {
    for _ in 0..50 {
        let _ = host.poll_exits();
        if let Ok((_, _, Some(code))) = host.output(id) {
            return code;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    panic!("host terminal did not exit");
}

#[test]
fn create_captures_output_and_exit_code() {
    let cwd = std::env::temp_dir();
    let views = Arc::new(Mutex::new(HashMap::new()));
    let mut host = HostedTerminals::new("c-term".into(), Arc::clone(&views));
    let id = host.create(echo_spec(&cwd)).unwrap();
    let code = wait_exit(&mut host, &id);
    assert_eq!(code, 0);
    let (output, _, exit) = host.output(&id).unwrap();
    assert!(output.to_ascii_lowercase().contains("hello-host"), "{output}");
    assert_eq!(exit, Some(0));
    let snap = views.lock().unwrap().get("c-term").cloned().unwrap();
    assert_eq!(snap.len(), 1);
    assert_eq!(snap[0].id, id);
    assert!(!snap[0].running);
}

#[test]
fn missing_terminal_is_not_found() {
    let views = Arc::new(Mutex::new(HashMap::new()));
    let mut host = HostedTerminals::new("c-term".into(), views);
    assert!(host.output("missing").is_err());
}
