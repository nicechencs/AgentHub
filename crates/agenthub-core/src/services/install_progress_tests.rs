use super::{emit_install_log, with_install_log_hook};
use std::sync::{Arc, Mutex};

#[test]
fn hook_receives_empty_and_whitespace_lines_verbatim() {
    let seen = Arc::new(Mutex::new(Vec::<String>::new()));
    let sink = Arc::clone(&seen);

    with_install_log_hook(
        Arc::new(move |line| {
            sink.lock()
                .expect("test hook mutex poisoned")
                .push(line.to_owned())
        }),
        || {
            emit_install_log("");
            emit_install_log("   ");
            emit_install_log("\n");
            emit_install_log("line\n");
        },
    );

    assert_eq!(
        *seen.lock().expect("test hook mutex poisoned"),
        vec!["", "   ", "\n", "line\n"]
    );
}
