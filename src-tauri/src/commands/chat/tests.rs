use super::*;
use agenthub_core::models::AgentId;
use tempfile::tempdir;

#[test]
fn create_list_send_and_delete() {
    let dir = tempdir().unwrap();
    let hub = AgentHub::open(Some(dir.path())).unwrap();
    let conv = create_conversation_inner(&hub, vec!["claude".into()], None).unwrap();
    assert!(!conv.id.is_empty());
    let list = list_conversations_inner(&hub).unwrap();
    assert_eq!(list.len(), 1);

    hub.chat()
        .send(&conv.id, "hello from test", &|_ev| {})
        .unwrap();
    let after_send = list_conversations_inner(&hub).unwrap();
    assert_eq!(
        after_send[0].first_user_content.as_deref(),
        Some("hello from test")
    );
    let msgs = list_chat_messages_inner(&hub, &conv.id).unwrap();
    assert!(msgs
        .iter()
        .any(|m| matches!(m.role, agenthub_core::models::ChatRole::User)));
    delete_conversation_inner(&hub, &conv.id).unwrap();
    assert!(list_conversations_inner(&hub).unwrap().is_empty());
}

#[test]
fn parse_agent_ids_dedupes_and_rejects_empty() {
    let ids = parse_agent_ids(vec!["claude".into(), "claude".into(), "codex".into()]).unwrap();
    assert_eq!(ids, vec![AgentId::Claude, AgentId::Codex]);

    let err = parse_agent_ids(vec![]).unwrap_err();
    assert!(err.contains("empty"));

    let err = parse_agent_ids(vec!["nope".into()]).unwrap_err();
    assert!(!err.is_empty());
}

#[test]
fn create_dedupes_agents_and_update_roundtrip() {
    let dir = tempdir().unwrap();
    let hub = AgentHub::open(Some(dir.path())).unwrap();
    let conv =
        create_conversation_inner(&hub, vec!["claude".into(), "claude".into()], None).unwrap();
    assert_eq!(conv.agent_ids, vec![AgentId::Claude]);

    let err =
        create_conversation_inner(&hub, vec!["claude".into(), "grok".into()], None).unwrap_err();
    assert!(err.contains("only one agent"));

    let updated = update_conversation_inner(
        &hub,
        &conv.id,
        Some("title".into()),
        Some(vec!["codex".into()]),
        Some(String::new()), // clear cwd
        Some(true),
    )
    .unwrap();
    assert_eq!(updated.title, "title");
    assert_eq!(updated.agent_ids, vec![AgentId::Codex]);
    assert!(updated.cwd.is_none());
    assert!(updated.allow_dangerous);
}

#[test]
fn ensure_default_is_idempotent_but_create_remains_explicit() {
    let dir = tempdir().unwrap();
    let hub = AgentHub::open(Some(dir.path())).unwrap();
    let first = ensure_default_conversation_inner(&hub, vec!["claude".into()], None).unwrap();
    let second = ensure_default_conversation_inner(&hub, vec!["claude".into()], None).unwrap();
    assert_eq!(first.id, second.id);
    assert_eq!(list_conversations_inner(&hub).unwrap().len(), 1);

    let explicit_a = create_conversation_inner(&hub, vec!["claude".into()], None).unwrap();
    let explicit_b = create_conversation_inner(&hub, vec!["claude".into()], None).unwrap();
    assert_ne!(explicit_a.id, explicit_b.id);
    assert_eq!(list_conversations_inner(&hub).unwrap().len(), 3);
}

#[test]
fn cancel_is_noop_without_inflight_send() {
    let dir = tempdir().unwrap();
    let hub = AgentHub::open(Some(dir.path())).unwrap();
    let conv = create_conversation_inner(&hub, vec!["claude".into()], None).unwrap();
    chat_cancel_inner(&hub, &conv.id).unwrap();
}

#[test]
fn empty_agent_list_rejected_on_create() {
    let dir = tempdir().unwrap();
    let hub = AgentHub::open(Some(dir.path())).unwrap();
    let err = create_conversation_inner(&hub, vec![], None).unwrap_err();
    assert!(err.contains("empty") || err.contains("at least"));
}

#[test]
fn refresh_chat_agent_title_returns_null_without_an_agent_title() {
    let dir = tempdir().unwrap();
    let hub = AgentHub::open(Some(dir.path())).unwrap();
    let conv = create_conversation_inner(&hub, vec!["claude".into()], None).unwrap();

    // No session id and no Claude title source: the command stays a no-op.
    assert_eq!(
        refresh_chat_agent_title_inner(&hub, &conv.id).unwrap(),
        None
    );
}

#[test]
fn refresh_chat_agent_title_rejects_an_unknown_conversation() {
    let dir = tempdir().unwrap();
    let hub = AgentHub::open(Some(dir.path())).unwrap();

    let err = refresh_chat_agent_title_inner(&hub, "conv-missing").unwrap_err();
    assert!(!err.is_empty());
}

#[test]
fn cwd_or_home_keeps_a_folder_and_fills_home_when_missing() {
    assert_eq!(
        cwd_or_home(Some("/tmp/app".into())).as_deref(),
        Some("/tmp/app")
    );
    let Ok(home) = agenthub_core::utils::paths::home_dir() else {
        return;
    };
    let home = home.to_string_lossy().into_owned();
    assert_eq!(cwd_or_home(None).as_deref(), Some(home.as_str()));
    assert_eq!(
        cwd_or_home(Some("  ".into())).as_deref(),
        Some(home.as_str())
    );
}

// 1x1 PNG
const TINY_PNG_B64: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";

#[test]
fn paste_image_rejects_unknown_extension() {
    let err = save_chat_paste_image_inner("AQID", "exe", None).unwrap_err();
    assert!(err.contains("unsupported"));
}

#[test]
fn paste_image_writes_small_png_bytes() {
    let path = save_chat_paste_image_inner(TINY_PNG_B64, "png", Some(68)).unwrap();
    assert!(path.ends_with(".png"));
    assert!(std::path::Path::new(&path).is_file());
    let _ = std::fs::remove_file(path);
}

#[test]
fn paste_image_consecutive_saves_get_distinct_paths() {
    let a = save_chat_paste_image_inner(TINY_PNG_B64, "png", None).unwrap();
    let b = save_chat_paste_image_inner(TINY_PNG_B64, "png", None).unwrap();
    assert_ne!(a, b);
    assert!(std::path::Path::new(&a).is_file());
    assert!(std::path::Path::new(&b).is_file());
    let _ = std::fs::remove_file(a);
    let _ = std::fs::remove_file(b);
}

#[test]
fn paste_image_write_never_overwrites_and_names_match_cleanup_rule() {
    let dir = tempdir().unwrap();
    let a = write_new_paste_image(dir.path(), "jpg", b"one").unwrap();
    let b = write_new_paste_image(dir.path(), "jpg", b"two").unwrap();
    assert_ne!(a, b);
    assert_eq!(std::fs::read(&a).unwrap(), b"one");
    assert_eq!(std::fs::read(&b).unwrap(), b"two");
    for p in [&a, &b] {
        let name = p.file_name().unwrap().to_str().unwrap();
        assert!(is_generated_paste_name(name), "{name}");
    }
}

#[test]
fn paste_image_cleanup_removes_only_expired_generated_files() {
    use std::time::{Duration, SystemTime};
    let dir = tempdir().unwrap();
    let now = SystemTime::now();
    let max_age = Duration::from_secs(7 * 24 * 60 * 60);
    let old = now - Duration::from_secs(8 * 24 * 60 * 60);

    let write_with_mtime = |name: &str, mtime: SystemTime| {
        let path = dir.path().join(name);
        std::fs::write(&path, b"x").unwrap();
        std::fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(mtime)
            .unwrap();
        path
    };

    let expired = write_with_mtime("paste-1700000000000-42-0.png", old);
    let expired_legacy = write_with_mtime("paste-1700000000000-42.jpg", old);
    let fresh = write_with_mtime("paste-1700000000001-42-1.png", now);
    let foreign = write_with_mtime("notes.png", old);
    let foreign_ext = write_with_mtime("paste-1700000000000-42-2.txt", old);
    let foreign_stem = write_with_mtime("paste-abc-42-3.png", old);
    let subdir = dir.path().join("paste-1700000000000-42-4.png");
    std::fs::create_dir(&subdir).unwrap();

    cleanup_stale_paste_images(dir.path(), now, max_age);

    assert!(!expired.exists());
    assert!(!expired_legacy.exists());
    assert!(fresh.is_file());
    assert!(foreign.is_file());
    assert!(foreign_ext.is_file());
    assert!(foreign_stem.is_file());
    assert!(subdir.is_dir());
}

#[cfg(unix)]
#[test]
fn paste_image_cleanup_does_not_follow_symlinks() {
    use std::time::{Duration, SystemTime};
    let target_dir = tempdir().unwrap();
    let target = target_dir.path().join("paste-1700000000000-42-0.png");
    std::fs::write(&target, b"keep").unwrap();
    std::fs::File::options()
        .write(true)
        .open(&target)
        .unwrap()
        .set_modified(SystemTime::now() - Duration::from_secs(30 * 24 * 60 * 60))
        .unwrap();

    let dir = tempdir().unwrap();
    let link = dir.path().join("paste-1700000000000-42-1.png");
    std::os::unix::fs::symlink(&target, &link).unwrap();

    cleanup_stale_paste_images(
        dir.path(),
        SystemTime::now(),
        Duration::from_secs(7 * 24 * 60 * 60),
    );

    assert!(target.is_file());
    assert!(std::fs::symlink_metadata(&link).is_ok());
}

#[test]
fn paste_image_cleanup_ignores_missing_dir() {
    let dir = tempdir().unwrap();
    cleanup_stale_paste_images(
        &dir.path().join("missing"),
        std::time::SystemTime::now(),
        std::time::Duration::from_secs(1),
    );
}
