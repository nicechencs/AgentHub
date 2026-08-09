use std::fs;

use tempfile::tempdir;

use super::{clear_grok_field, read_grok_api_key, write_grok_api_key};

#[test]
fn grok_account_key_reads_and_writes_active_nested_model() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("config.toml");
    fs::write(
        &path,
        r#"[models]
default = "grok"
web_search = "grok"

[model."grok"]
model = "grok-4.5"
base_url = "https://relay.example.com/v1"
api_backend = "responses"
"#,
    )
    .unwrap();

    write_grok_api_key(&path, "xai-test-key-123456").unwrap();
    assert_eq!(
        read_grok_api_key(&path).unwrap().as_deref(),
        Some("xai-test-key-123456")
    );
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.contains("[model."));
    assert!(text.contains("api_backend = \"responses\""));
    assert!(text.contains("api_key = \"xai-test-key-123456\""));

    clear_grok_field(&path, "api_key").unwrap();
    assert_eq!(read_grok_api_key(&path).unwrap(), None);
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.contains("api_backend = \"responses\""));
    assert!(!text.contains("xai-test-key-123456"));
}
