use std::fs;
use std::path::Path;

use tempfile::tempdir;

use crate::adapters::AgentAdapter;
use crate::models::{ProcessMode, RunOptions};

use super::{
    clear_grok_field, grok_auth_state, grok_cli_args, grok_supports_no_auto_update,
    read_grok_api_key, write_grok_api_key, GrokAdapter,
};
use crate::models::{AccountKind, AgentConfig, AgentId, LiveAccount};

static GROK_HOME_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

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
    let _guard = GROK_HOME_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
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
            "content": "[models]\ndefault = \"agenthub_codex_bridge\"\n\n[model.\"agenthub_codex_bridge\"]\nbase_url = \"http://127.0.0.1:32123/v1\"\napi_key = \"ahb_local\"\napi_backend = \"responses\"\n",
        }),
    });
    match prev {
        Some(value) => std::env::set_var("GROK_HOME", value),
        None => std::env::remove_var("GROK_HOME"),
    }
    result.unwrap();
    let text = fs::read_to_string(dir.path().join("config.toml")).unwrap();
    assert!(text.contains("http://127.0.0.1:32123/v1"));
    assert!(text.contains("api_backend = \"responses\""));
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
fn grok_supports_no_auto_update_gates_old_semver() {
    assert!(grok_supports_no_auto_update(None));
    assert!(grok_supports_no_auto_update(Some("")));
    assert!(grok_supports_no_auto_update(Some("not-a-version")));
    assert!(grok_supports_no_auto_update(Some("0.2.117")));
    assert!(grok_supports_no_auto_update(Some("0.2.118")));
    assert!(grok_supports_no_auto_update(Some(
        "grok 0.2.118 (1e1687c1cf)"
    )));
    assert!(grok_supports_no_auto_update(Some("1.0.0")));
    assert!(!grok_supports_no_auto_update(Some("0.2.116")));
    assert!(!grok_supports_no_auto_update(Some("0.2.0")));
    assert!(!grok_supports_no_auto_update(Some(
        "grok 0.2.116 (deadbeef)"
    )));
    assert!(
        !grok_supports_no_auto_update(Some("0.2.117-beta.1")),
        "semver prerelease of the min version is still older than 0.2.117"
    );
}

#[test]
fn grok_cli_args_include_no_auto_update_for_unknown_and_modern() {
    let opts = RunOptions {
        process_mode: ProcessMode::Auto,
        ..RunOptions::default()
    };
    assert_eq!(
        grok_cli_args("hi", &opts, None),
        vec![
            "--no-auto-update",
            "-p",
            "hi",
            "--output-format",
            "streaming-json"
        ]
    );
    assert_eq!(
        grok_cli_args("hi", &opts, Some("0.2.117")),
        vec![
            "--no-auto-update",
            "-p",
            "hi",
            "--output-format",
            "streaming-json"
        ]
    );
}

#[test]
fn grok_cli_args_omit_no_auto_update_for_old_cli() {
    let opts = RunOptions {
        process_mode: ProcessMode::Auto,
        ..RunOptions::default()
    };
    assert_eq!(
        grok_cli_args("hi", &opts, Some("0.2.116")),
        vec!["-p", "hi", "--output-format", "streaming-json"]
    );
}

#[test]
fn grok_cli_args_dangerous_prefixes_always_approve() {
    let opts = RunOptions {
        allow_dangerous: true,
        process_mode: ProcessMode::Auto,
        ..RunOptions::default()
    };
    assert_eq!(
        grok_cli_args("hi", &opts, None),
        vec![
            "--always-approve",
            "--no-auto-update",
            "-p",
            "hi",
            "--output-format",
            "streaming-json"
        ]
    );
    assert_eq!(
        grok_cli_args("hi", &opts, Some("0.2.116")),
        vec![
            "--always-approve",
            "-p",
            "hi",
            "--output-format",
            "streaming-json"
        ]
    );
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
    let detected = GrokAdapter.detect().version;
    let expected = grok_cli_args(
        "hi",
        &RunOptions {
            process_mode: ProcessMode::Auto,
            ..RunOptions::default()
        },
        detected.as_deref(),
    );
    assert_eq!(spec.args, expected);
    assert!(spec.env.is_empty());
}

#[test]
fn apply_account_writes_pkce_bundle_into_auth_json() {
    let _guard = GROK_HOME_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let dir = tempdir().unwrap();
    let prev = std::env::var_os("GROK_HOME");
    std::env::set_var("GROK_HOME", dir.path());
    fs::write(
        dir.path().join("auth.json"),
        r#"{"https://auth.x.ai::client":{"email":"a@example.com","user_id":"uid-1","key":"old-at","refresh_token":"old-rt"}}"#,
    )
    .unwrap();
    let result = GrokAdapter.apply_account(&LiveAccount {
        agent: AgentId::Grok,
        kind: AccountKind::Oauth,
        credentials: serde_json::json!({
            "type": "oauth",
            "provider": "xai",
            "access_token": "new-at",
            "refresh_token": "new-rt",
            "sub": "uid-1"
        }),
        label_hint: Some("hub".into()),
        extra: serde_json::json!({ "source": "oauth_refresh" }),
    });
    match prev {
        Some(value) => std::env::set_var("GROK_HOME", value),
        None => std::env::remove_var("GROK_HOME"),
    }
    result.unwrap();
    let body: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(dir.path().join("auth.json")).unwrap()).unwrap();
    assert_eq!(body["https://auth.x.ai::client"]["refresh_token"], "new-rt");
    assert_eq!(body["https://auth.x.ai::client"]["key"], "new-at");
    assert_eq!(body["https://auth.x.ai::client"]["email"], "a@example.com");
}

#[test]
fn apply_account_patches_only_matching_nested_profile() {
    let _guard = GROK_HOME_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let dir = tempdir().unwrap();
    let prev = std::env::var_os("GROK_HOME");
    std::env::set_var("GROK_HOME", dir.path());
    fs::write(
        dir.path().join("auth.json"),
        r#"{
  "https://auth.x.ai::client": {
    "email": "a@example.com",
    "user_id": "uid-1",
    "key": "old-at-1",
    "refresh_token": "old-rt-1"
  },
  "https://auth.x.ai::https://api.x.ai": {
    "email": "b@example.com",
    "user_id": "uid-2",
    "key": "old-at-2",
    "refresh_token": "old-rt-2"
  }
}"#,
    )
    .unwrap();
    let result = GrokAdapter.apply_account(&LiveAccount {
        agent: AgentId::Grok,
        kind: AccountKind::Oauth,
        credentials: serde_json::json!({
            "type": "oauth",
            "provider": "xai",
            "access_token": "new-at-1",
            "refresh_token": "new-rt-1",
            "sub": "uid-1"
        }),
        label_hint: Some("hub".into()),
        extra: serde_json::json!({ "source": "oauth_refresh" }),
    });
    match prev {
        Some(value) => std::env::set_var("GROK_HOME", value),
        None => std::env::remove_var("GROK_HOME"),
    }
    result.unwrap();
    let body: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(dir.path().join("auth.json")).unwrap()).unwrap();
    assert_eq!(body["https://auth.x.ai::client"]["key"], "new-at-1");
    assert_eq!(
        body["https://auth.x.ai::client"]["refresh_token"],
        "new-rt-1"
    );
    assert_eq!(
        body["https://auth.x.ai::https://api.x.ai"]["key"],
        "old-at-2"
    );
    assert_eq!(
        body["https://auth.x.ai::https://api.x.ai"]["refresh_token"],
        "old-rt-2"
    );
    assert!(body
        .as_object()
        .is_some_and(|obj| obj.contains_key("https://auth.x.ai::https://api.x.ai")));
}

#[test]
fn apply_account_wraps_empty_auth_json_as_nested_client_slot() {
    let _guard = GROK_HOME_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let dir = tempdir().unwrap();
    let prev = std::env::var_os("GROK_HOME");
    std::env::set_var("GROK_HOME", dir.path());
    let result = GrokAdapter.apply_account(&LiveAccount {
        agent: AgentId::Grok,
        kind: AccountKind::Oauth,
        credentials: serde_json::json!({
            "type": "oauth",
            "access_token": "new-at",
            "refresh_token": "new-rt",
            "sub": "uid-1",
            "email": "a@example.com"
        }),
        label_hint: Some("hub".into()),
        extra: serde_json::json!({}),
    });
    match prev {
        Some(value) => std::env::set_var("GROK_HOME", value),
        None => std::env::remove_var("GROK_HOME"),
    }
    result.unwrap();
    let body: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(dir.path().join("auth.json")).unwrap()).unwrap();
    assert_eq!(body["https://auth.x.ai::client"]["key"], "new-at");
    assert_eq!(body["https://auth.x.ai::client"]["refresh_token"], "new-rt");
    assert_eq!(body["https://auth.x.ai::client"]["user_id"], "uid-1");
    assert!(body.get("access_token").is_none());
}
