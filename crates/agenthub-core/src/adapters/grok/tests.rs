use std::fs;
use std::path::Path;

use tempfile::tempdir;

use crate::adapters::AgentAdapter;
use crate::models::{ProcessMode, RunOptions};

use super::{
    clear_grok_field, grok_auth_state, read_grok_api_key, write_grok_api_key, GrokAdapter,
};
use crate::models::{AgentConfig, AgentId};

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

#[test]
fn grok_api_key_and_oauth_sets_also_present() {
    let dir = tempdir().unwrap();
    let config = dir.path().join("config.toml");
    let auth = dir.path().join("auth.json");
    fs::write(&config, "api_key = \"xai-fixture-key\"\n").unwrap();
    fs::write(
        &auth,
        r#"{"access_token":"grok-access-fixture","refresh_token":"grok-refresh-fixture"}"#,
    )
    .unwrap();

    let state = grok_auth_state(&config, &auth).unwrap();
    assert_eq!(state.kind.as_deref(), Some("api_key"));
    assert!(state.also_present.iter().any(|kind| kind == "oauth"));
    let dumped = serde_json::to_string(&state).unwrap();
    assert!(!dumped.contains("xai-fixture-key"));
    assert!(!dumped.contains("grok-access-fixture"));
    assert!(!dumped.contains("grok-refresh-fixture"));
}

#[test]
fn grok_api_key_and_missing_or_unparseable_auth_leaves_also_present_empty() {
    let dir = tempdir().unwrap();
    let config = dir.path().join("config.toml");
    let auth = dir.path().join("auth.json");
    fs::write(&config, "api_key = \"xai-only-fixture\"\n").unwrap();

    let missing = grok_auth_state(&config, &auth).unwrap();
    assert_eq!(missing.kind.as_deref(), Some("api_key"));
    assert!(missing.also_present.is_empty());
    let dumped = serde_json::to_string(&missing).unwrap();
    assert!(!dumped.contains("xai-only-fixture"));
    assert!(serde_json::to_value(&missing)
        .unwrap()
        .get("alsoPresent")
        .is_none());

    fs::write(&auth, "not-json").unwrap();
    let unparseable = grok_auth_state(&config, &auth).unwrap();
    assert_eq!(unparseable.kind.as_deref(), Some("api_key"));
    assert!(unparseable.also_present.is_empty());
    let dumped = serde_json::to_string(&unparseable).unwrap();
    assert!(!dumped.contains("xai-only-fixture"));
}

#[test]
fn grok_write_config_points_base_url_at_loopback_and_drops_leftover_grok_model() {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _guard = LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let dir = tempdir().unwrap();
    let prev = std::env::var_os("GROK_HOME");
    std::env::set_var("GROK_HOME", dir.path());
    fs::write(
        dir.path().join("config.toml"),
        r#"[models]
default = "grok"

[model."grok"]
model = "grok-4.5"
base_url = "https://api.x.ai/v1"
api_key = "old"
"#,
    )
    .unwrap();
    let result = GrokAdapter.write_config(&AgentConfig {
        agent: AgentId::Grok,
        raw: serde_json::json!({
            "format": "toml",
            "content": "[models]\ndefault = \"agenthub_codex_bridge\"\n\n[model.\"agenthub_codex_bridge\"]\nbase_url = \"http://127.0.0.1:32123/v1\"\napi_key = \"ahb_local\"\napi_backend = \"chat_completions\"\n",
        }),
    });
    match prev {
        Some(value) => std::env::set_var("GROK_HOME", value),
        None => std::env::remove_var("GROK_HOME"),
    }
    result.unwrap();
    let text = fs::read_to_string(dir.path().join("config.toml")).unwrap();
    assert!(text.contains("http://127.0.0.1:32123/v1"));
    assert!(text.contains("api_backend = \"chat_completions\""));
    assert!(!text.contains("grok-4.5"));
    assert!(!text.contains("gpt-"));
}

#[test]
fn grok_oauth_only_leaves_also_present_empty() {
    let dir = tempdir().unwrap();
    let config = dir.path().join("config.toml");
    let auth = dir.path().join("auth.json");
    fs::write(&config, "default = \"grok\"\n").unwrap();
    fs::write(
        &auth,
        r#"{"access_token":"grok-oauth-only-fixture","refresh_token":"grok-refresh-only-fixture"}"#,
    )
    .unwrap();

    let state = grok_auth_state(&config, &auth).unwrap();
    assert_eq!(state.kind.as_deref(), Some("oauth"));
    assert!(state.also_present.is_empty());
    let dumped = serde_json::to_string(&state).unwrap();
    assert!(!dumped.contains("grok-oauth-only-fixture"));
    assert!(!dumped.contains("grok-refresh-only-fixture"));
    assert!(serde_json::to_value(&state)
        .unwrap()
        .get("alsoPresent")
        .is_none());
}

#[test]
fn build_run_spec_guards_auto_update_and_streams_json() {
    let spec = GrokAdapter
        .build_run_spec(
            Path::new("grok"),
            "hi",
            &RunOptions {
                process_mode: ProcessMode::Auto,
                ..RunOptions::default()
            },
        )
        .unwrap();
    assert_eq!(
        spec.args,
        vec![
            "--no-auto-update",
            "-p",
            "hi",
            "--output-format",
            "streaming-json"
        ]
    );
    assert!(spec.env.is_empty());
}

#[test]
fn build_run_spec_dangerous_prefixes_always_approve() {
    let spec = GrokAdapter
        .build_run_spec(
            Path::new("grok"),
            "hi",
            &RunOptions {
                allow_dangerous: true,
                process_mode: ProcessMode::Auto,
                ..RunOptions::default()
            },
        )
        .unwrap();
    assert_eq!(
        spec.args,
        vec![
            "--always-approve",
            "--no-auto-update",
            "-p",
            "hi",
            "--output-format",
            "streaming-json"
        ]
    );
}
