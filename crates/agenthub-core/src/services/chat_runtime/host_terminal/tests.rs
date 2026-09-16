use super::*;
use crate::services::chat_runtime::ops::AcpTerminalCreate;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

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

fn exit_spec(code: i32) -> AcpTerminalCreate {
    let cwd = std::env::temp_dir();
    #[cfg(windows)]
    {
        AcpTerminalCreate {
            command: "cmd.exe".into(),
            args: vec!["/C".into(), format!("exit {code}")],
            cwd,
            env: Vec::new(),
            output_limit: DEFAULT_OUTPUT_LIMIT,
        }
    }
    #[cfg(not(windows))]
    {
        AcpTerminalCreate {
            command: "sh".into(),
            args: vec!["-c".into(), format!("exit {code}")],
            cwd,
            env: Vec::new(),
            output_limit: DEFAULT_OUTPUT_LIMIT,
        }
    }
}

#[test]
fn create_records_nonzero_exit_code() {
    let views = Arc::new(Mutex::new(HashMap::new()));
    let mut host = HostedTerminals::new("c-term".into(), views);
    let id = host.create(exit_spec(7)).unwrap();
    assert_eq!(wait_exit(&mut host, &id), 7);
}

#[test]
fn missing_terminal_is_not_found() {
    let views = Arc::new(Mutex::new(HashMap::new()));
    let mut host = HostedTerminals::new("c-term".into(), views);
    assert!(host.output("missing").is_err());
}
