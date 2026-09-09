use serde_json::Value;

use super::extract_file_changes;

fn fixture(name: &str) -> Value {
    let local = format!(
        "{}/src/services/chat_runtime/file_change/fixtures/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    let text = std::fs::read_to_string(&local)
        .unwrap_or_else(|error| panic!("read {local}: {error}"));
    serde_json::from_str(&text).unwrap_or_else(|error| panic!("parse {name}: {error}"))
}

#[test]
fn item_started_fixture_copies_diff_not_invented_hunk() {
    let changes = extract_file_changes(&fixture("item_started_with_diff.json"));
    assert_eq!(changes.len(), 1);
    assert_eq!(
        changes[0].path,
        "/workspace/qa-codex-filechange-scratch/probe.txt"
    );
    assert_eq!(changes[0].kind.as_deref(), Some("add"));
    assert_eq!(changes[0].preview.as_deref(), Some("FILECHANGE_OK\n"));
}

#[test]
fn path_only_fixture_stays_empty_instead_of_faking_a_diff() {
    let started = extract_file_changes(&fixture("item_started_path_only.json"));
    assert_eq!(started[0].path, "/workspace/notes.md");
    assert_eq!(started[0].kind.as_deref(), Some("update"));
    assert_eq!(started[0].preview, None);

    let approval = extract_file_changes(&fixture("request_approval_path_only.json"));
    assert!(approval.is_empty());
}

#[test]
fn apply_patch_fixture_uses_content_field() {
    let changes = extract_file_changes(&fixture("apply_patch_with_content.json"));
    assert_eq!(changes[0].path, "/tmp/example.txt");
    assert_eq!(changes[0].kind.as_deref(), Some("add"));
    assert_eq!(changes[0].preview.as_deref(), Some("ok"));
}

#[test]
fn before_after_fixture_joins_protocol_snippets() {
    let changes = extract_file_changes(&fixture("before_after.json"));
    assert_eq!(changes[0].path, "src/app.ts");
    assert_eq!(
        changes[0].preview.as_deref(),
        Some("const name = 'old';\n\nconst name = 'new';")
    );
}

#[test]
fn grok_operation_fixture_reads_nested_diff() {
    let changes = extract_file_changes(&fixture("grok_operation_diff.json"));
    assert_eq!(changes[0].path, "README.md");
    assert_eq!(changes[0].kind.as_deref(), Some("update"));
    assert_eq!(
        changes[0].preview.as_deref(),
        Some("@@ -1,2 +1,3 @@\n hello\n+world\n")
    );
}
