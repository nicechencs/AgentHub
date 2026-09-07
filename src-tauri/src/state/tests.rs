use super::*;
use agenthub_core::AgentHub;
use tempfile::tempdir;

fn hub_tmp() -> (tempfile::TempDir, Arc<AgentHub>) {
    let dir = tempdir().unwrap();
    let hub = Arc::new(AgentHub::open(Some(dir.path())).unwrap());
    (dir, hub)
}

#[test]
fn defaults_close_to_tray_true_and_not_exiting() {
    let (_dir, hub) = hub_tmp();
    let state = AppState::from_hub(Ok(hub));
    assert!(state.close_to_tray());
    assert!(!state.should_exit());
}

#[test]
fn loads_close_to_tray_false_from_settings() {
    let (_dir, hub) = hub_tmp();
    hub.settings().set("close_to_tray", "false").unwrap();
    let state = AppState::from_hub(Ok(hub));
    assert!(!state.close_to_tray());
}

#[test]
fn hub_error_still_defaults_close_to_tray_true() {
    let state = AppState::from_hub(Err("open failed".into()));
    assert!(state.close_to_tray());
    assert!(state.hub().is_err());
    assert!(state.hub_arc().is_err());
}

#[test]
fn request_exit_and_set_close_to_tray_flags() {
    let (_dir, hub) = hub_tmp();
    let state = AppState::from_hub(Ok(hub));
    state.set_close_to_tray(false);
    assert!(!state.close_to_tray());
    state.set_close_to_tray(true);
    assert!(state.close_to_tray());
    state.request_exit();
    assert!(state.should_exit());
}

#[test]
fn exit_confirmation_gate_is_idempotent() {
    let (_dir, hub) = hub_tmp();
    let state = AppState::from_hub(Ok(hub));

    assert!(state.begin_exit_confirmation());
    assert!(state.exit_confirmation_pending());
    assert!(!state.begin_exit_confirmation());

    state.finish_exit_confirmation();
    assert!(!state.exit_confirmation_pending());
    assert!(state.begin_exit_confirmation());
}

#[test]
fn pending_open_chat_cwd_is_taken_once() {
    let (_dir, hub) = hub_tmp();
    let state = AppState::from_hub(Ok(hub));
    assert_eq!(state.take_pending_open_chat_cwd(), None);
    state.set_pending_open_chat_cwd(r"D:\work\app".into());
    assert_eq!(
        state.take_pending_open_chat_cwd().as_deref(),
        Some(r"D:\work\app")
    );
    assert_eq!(state.take_pending_open_chat_cwd(), None);
}

#[test]
fn sync_setting_flag_only_reacts_to_close_to_tray() {
    let (_dir, hub) = hub_tmp();
    let state = AppState::from_hub(Ok(hub));
    assert!(state.close_to_tray());

    state.sync_setting_flag("theme", "dark");
    assert!(state.close_to_tray());

    state.sync_setting_flag("close_to_tray", "false");
    assert!(!state.close_to_tray());

    state.sync_setting_flag("close_to_tray", "true");
    assert!(state.close_to_tray());

    state.sync_setting_flag("close_to_tray", "0");
    assert!(!state.close_to_tray());
}
