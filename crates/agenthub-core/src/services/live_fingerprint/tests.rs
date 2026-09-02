use std::path::Path;

use super::{classify, hash_existing, record_changed, record_written, LiveFileState};
use crate::models::AgentId;
use crate::storage::Database;

fn tmp() -> (tempfile::TempDir, Database) {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(&dir.path().join("fingerprints.db")).unwrap();
    (dir, db)
}

fn write_file(path: &Path, content: &[u8]) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, content).unwrap();
}

#[test]
fn classify_without_fingerprint_is_unknown() {
    let (dir, db) = tmp();
    let file = dir.path().join("settings.json");
    write_file(&file, b"whatever");
    assert_eq!(
        classify(&db, AgentId::Claude, &file).unwrap(),
        LiveFileState::Unknown
    );
    // A missing file without a fingerprint is also Unknown, not Missing:
    // AgentHub never wrote it, so it was not lost by us either.
    assert_eq!(
        classify(&db, AgentId::Claude, &dir.path().join("nope.json")).unwrap(),
        LiveFileState::Unknown
    );
}

#[test]
fn record_written_fingerprints_only_existing_files() {
    let (dir, db) = tmp();
    let present = dir.path().join("settings.json");
    let absent = dir.path().join("auth.json");
    write_file(&present, b"v1");

    record_written(&db, AgentId::Claude, &[present.clone(), absent.clone()]);

    assert_eq!(
        classify(&db, AgentId::Claude, &present).unwrap(),
        LiveFileState::Managed
    );
    assert_eq!(
        classify(&db, AgentId::Claude, &absent).unwrap(),
        LiveFileState::Unknown
    );
}

#[test]
fn classify_detects_edited_and_missing() {
    let (dir, db) = tmp();
    let file = dir.path().join("settings.json");
    write_file(&file, b"v1");
    record_written(&db, AgentId::Claude, &[file.clone()]);

    // Hand edit after the managed write.
    write_file(&file, b"hand edit");
    assert_eq!(
        classify(&db, AgentId::Claude, &file).unwrap(),
        LiveFileState::Edited
    );

    std::fs::remove_file(&file).unwrap();
    assert_eq!(
        classify(&db, AgentId::Claude, &file).unwrap(),
        LiveFileState::Missing
    );
}

#[test]
fn fingerprints_are_scoped_per_agent_and_rerecorded() {
    let (dir, db) = tmp();
    let file = dir.path().join("config.toml");
    write_file(&file, b"v1");
    record_written(&db, AgentId::Claude, &[file.clone()]);

    // Another agent never wrote this path.
    assert_eq!(
        classify(&db, AgentId::Codex, &file).unwrap(),
        LiveFileState::Unknown
    );

    // A new managed write refreshes the fingerprint.
    write_file(&file, b"v2");
    record_written(&db, AgentId::Claude, &[file.clone()]);
    assert_eq!(
        classify(&db, AgentId::Claude, &file).unwrap(),
        LiveFileState::Managed
    );
}

#[test]
fn record_changed_skips_unchanged_siblings() {
    let (dir, db) = tmp();
    let settings = dir.path().join("settings.json");
    let auth = dir.path().join("auth.json");
    write_file(&settings, b"v1");
    write_file(&auth, b"user-login");

    let before = hash_existing(&[settings.clone(), auth.clone()]);
    write_file(&settings, b"v2");
    record_changed(
        &db,
        AgentId::Claude,
        &before,
        &[settings.clone(), auth.clone()],
    );

    assert_eq!(
        classify(&db, AgentId::Claude, &settings).unwrap(),
        LiveFileState::Managed
    );
    assert_eq!(
        classify(&db, AgentId::Claude, &auth).unwrap(),
        LiveFileState::Unknown
    );
}

#[test]
fn record_changed_fingerprints_new_and_rewritten_files() {
    let (dir, db) = tmp();
    let settings = dir.path().join("settings.json");
    let created = dir.path().join("auth.json");
    write_file(&settings, b"v1");

    let before = hash_existing(&[settings.clone(), created.clone()]);
    write_file(&created, b"agenthub-bytes");
    record_changed(
        &db,
        AgentId::Claude,
        &before,
        &[settings.clone(), created.clone()],
    );

    assert_eq!(
        classify(&db, AgentId::Claude, &settings).unwrap(),
        LiveFileState::Unknown
    );
    assert_eq!(
        classify(&db, AgentId::Claude, &created).unwrap(),
        LiveFileState::Managed
    );
}
