use super::*;
use crate::models::{
    AccountKind, AgentConfig, AgentId, AuthHealth, Capability, CapabilityLevel, RuntimeId,
};
use crate::utils::paths::home_dir;
use serde_json::json;
use std::sync::Mutex;

static ENV_LOCK: Mutex<()> = Mutex::new(());

fn restore_env(key: &str, prev: Option<std::ffi::OsString>) {
    match prev {
        Some(v) => std::env::set_var(key, v),
        None => std::env::remove_var(key),
    }
}

fn with_dsh_home<T>(dir: &std::path::Path, f: impl FnOnce() -> T) -> T {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let prev = std::env::var_os("DSH_HOME");
    std::env::set_var("DSH_HOME", dir);
    let out = f();
    restore_env("DSH_HOME", prev);
    out
}

#[test]
fn capability_is_exhaustive_and_honest() {
    let adapter = DshAdapter;
    assert_eq!(adapter.id(), AgentId::Dsh);
    assert_eq!(
        adapter.capability(Capability::ApiKeyAccount).level,
        CapabilityLevel::Full
    );
    assert_eq!(
        adapter.capability(Capability::Skills).level,
        CapabilityLevel::Full
    );
    assert_eq!(
        adapter.capability(Capability::ConfigWrite).level,
        CapabilityLevel::Partial
    );
    assert_eq!(
        adapter.capability(Capability::StructuredStream).level,
        CapabilityLevel::Planned
    );
    assert_eq!(
        adapter.capability(Capability::Usage).level,
        CapabilityLevel::Full
    );
    for cap in Capability::ALL {
        let state = adapter.capability(cap);
        if state.level != CapabilityLevel::Full {
            assert!(
                state.reason.is_some(),
                "{cap:?} non-full cell must explain itself"
            );
        }
    }
}

#[test]
fn install_channel_is_npm_only() {
    let channels = DshAdapter.install_channels();
    assert_eq!(channels.len(), 1);
    assert_eq!(channels[0].id, "npm");
    assert!(channels[0].requires.contains(&RuntimeId::NodeJs));
    assert!(channels[0].requires.contains(&RuntimeId::Npm));
}

#[test]
fn skills_dir_is_user_dsh_root() {
    let dir = tempfile::tempdir().unwrap();
    with_dsh_home(dir.path(), || {
        assert_eq!(
            DshAdapter.skills_dir().as_deref(),
            Some(dir.path().join("skills").as_path())
        );
    });
}

#[test]
fn build_api_key_account_does_not_write_live() {
    let account = DshAdapter.build_api_key_account("sk-test-key").unwrap();
    assert_eq!(account.agent, AgentId::Dsh);
    assert_eq!(account.kind, AccountKind::ApiKey);
    assert_eq!(account.credentials["format"], "api_key");
    assert_eq!(account.credentials["provider"], "deepseek");
}

#[test]
fn apply_account_writes_credential_ref_not_key_into_patch() {
    let dir = tempfile::tempdir().unwrap();
    with_dsh_home(dir.path(), || {
        let account = DshAdapter.build_api_key_account("sk-live-secret").unwrap();
        DshAdapter.apply_account(&account).unwrap();
        let creds = std::fs::read_to_string(dir.path().join(CREDENTIALS_FILE)).unwrap();
        assert!(creds.contains("sk-live-secret"));
        let patch = std::fs::read_to_string(dir.path().join(HOME_PATCH_FILE)).unwrap();
        assert!(patch.contains(LLM_PLUGIN_ID));
        assert!(patch.contains(DEFAULT_API_KEY_ENV));
        assert!(!patch.contains("sk-live-secret"));
    });
}

#[test]
fn write_config_can_point_base_url_at_loopback_without_inventing_chatgpt_model() {
    let dir = tempfile::tempdir().unwrap();
    with_dsh_home(dir.path(), || {
        write_dsh_config(&AgentConfig {
            agent: AgentId::Dsh,
            raw: json!({
                "baseURL": "http://127.0.0.1:32123",
                "api_key": "ahb_local"
            }),
        })
        .unwrap();
        let text = std::fs::read_to_string(dir.path().join(HOME_PATCH_FILE)).unwrap();
        assert!(text.contains("http://127.0.0.1:32123"));
        assert!(!text.contains("gpt-"));
        assert!(!text.contains("grok-"));
        let creds = std::fs::read_to_string(dir.path().join(CREDENTIALS_FILE)).unwrap();
        assert!(creds.contains("ahb_local"));
        assert!(!text.contains("ahb_local"));
    });
}

#[test]
fn write_config_merges_llm_row_and_preserves_other_rows() {
    let dir = tempfile::tempdir().unwrap();
    with_dsh_home(dir.path(), || {
        let patch = dir.path().join(HOME_PATCH_FILE);
        std::fs::write(&patch, "- id: example.other\n  config:\n    keep: yes\n").unwrap();
        write_dsh_config(&AgentConfig {
            agent: AgentId::Dsh,
            raw: json!({
                "model": "deepseek-v4-pro",
                "thinking": "disabled",
                "maxTokens": 1024
            }),
        })
        .unwrap();
        let text = std::fs::read_to_string(&patch).unwrap();
        assert!(text.contains("example.other"));
        assert!(text.contains("keep: yes"));
        assert!(text.contains(LLM_PLUGIN_ID));
        assert!(text.contains("deepseek-v4-pro"));
        assert!(text.contains("thinking: disabled"));
        assert!(text.contains("maxTokens: 1024"));
        let fields = read_llm_fields(&patch).unwrap();
        assert_eq!(fields.model, "deepseek-v4-pro");
        assert_eq!(fields.thinking, "disabled");
        assert_eq!(fields.max_tokens, Some(1024));
    });
}

#[test]
fn write_config_peels_api_key_into_credentials() {
    let dir = tempfile::tempdir().unwrap();
    with_dsh_home(dir.path(), || {
        write_dsh_config(&AgentConfig {
            agent: AgentId::Dsh,
            raw: json!({ "apiKey": "sk-should-not-land" }),
        })
        .unwrap();
        let patch = std::fs::read_to_string(dir.path().join(HOME_PATCH_FILE)).unwrap();
        assert!(!patch.contains("sk-should-not-land"));
        let creds = std::fs::read_to_string(dir.path().join(CREDENTIALS_FILE)).unwrap();
        assert!(creds.contains("sk-should-not-land"));
    });
}

#[test]
fn build_run_spec_uses_headless_profile() {
    let dir = tempfile::tempdir().unwrap();
    with_dsh_home(dir.path(), || {
        let spec = DshAdapter
            .build_run_spec(
                std::path::Path::new("/usr/bin/dsh"),
                "fix tests",
                &RunOptions {
                    allow_dangerous: true,
                    ..RunOptions::default()
                },
            )
            .unwrap();
        assert_eq!(spec.args, vec!["--profile", "headless", "fix tests"]);
        assert!(
            !spec
                .args
                .iter()
                .any(|a| a.contains("yolo") || a.contains("danger")),
            "must not invent danger flags"
        );
        assert!(
            spec.env
                .iter()
                .any(|(k, v)| k == "DSH_HOME" && v == &dir.path().to_string_lossy()),
            "run spec must pin DSH_HOME: {:?}",
            spec.env
        );
    });
}

#[test]
fn write_config_skips_connection_secret_marker() {
    let dir = tempfile::tempdir().unwrap();
    with_dsh_home(dir.path(), || {
        write_dsh_config(&AgentConfig {
            agent: AgentId::Dsh,
            raw: json!({ "apiKey": "$AGENTHUB_CONNECTION_SECRET$" }),
        })
        .unwrap();
        let creds = dir.path().join(CREDENTIALS_FILE);
        if creds.exists() {
            let text = std::fs::read_to_string(&creds).unwrap();
            assert!(
                !text.contains("$AGENTHUB_CONNECTION_SECRET$"),
                "marker must not land in credentials: {text}"
            );
        }
        let patch = std::fs::read_to_string(dir.path().join(HOME_PATCH_FILE)).unwrap();
        assert!(!patch.contains("$AGENTHUB_CONNECTION_SECRET$"));
        assert!(!patch.to_ascii_lowercase().contains("sk-"));
    });
}

#[test]
fn write_config_rejects_wrong_agent_and_embedded_secret_patch() {
    let err = write_dsh_config(&AgentConfig {
        agent: AgentId::Claude,
        raw: json!({ "model": "x" }),
    })
    .unwrap_err();
    assert_eq!(err.code(), "invalid_arg");

    let dir = tempfile::tempdir().unwrap();
    with_dsh_home(dir.path(), || {
        std::fs::write(
            dir.path().join(HOME_PATCH_FILE),
            "- id: @deepseek-ai/dsh-llm-deepseek\n  config:\n    apiKey: sk-already-in-patch\n",
        )
        .unwrap();
        let err = write_dsh_config(&AgentConfig {
            agent: AgentId::Dsh,
            raw: json!({ "model": "deepseek-v4-flash" }),
        })
        .unwrap_err();
        assert_eq!(err.code(), "invalid_arg");
        let patch = std::fs::read_to_string(dir.path().join(HOME_PATCH_FILE)).unwrap();
        assert!(patch.contains("sk-already-in-patch"));
    });
}

#[test]
fn read_auth_reports_missing_file_and_env() {
    let dir = tempfile::tempdir().unwrap();
    with_dsh_home(dir.path(), || {
        // Isolate from a host DEEPSEEK_API_KEY (QA shells often export one).
        let prev_key = std::env::var_os(DEFAULT_API_KEY_ENV);
        std::env::remove_var(DEFAULT_API_KEY_ENV);
        let missing = DshAdapter.read_auth().unwrap();
        assert!(!missing.has_credentials);
        assert_eq!(missing.health, AuthHealth::Missing);

        write_credential_value(
            &dir.path().join(CREDENTIALS_FILE),
            DEFAULT_API_KEY_ENV,
            "sk-file-only",
        )
        .unwrap();
        let file = DshAdapter.read_auth().unwrap();
        assert!(file.has_credentials);
        assert_eq!(file.source.as_deref(), Some("dsh:credentials"));
        restore_env(DEFAULT_API_KEY_ENV, prev_key);
    });
}

#[test]
fn live_backup_paths_cover_patch_and_credentials() {
    let dir = tempfile::tempdir().unwrap();
    with_dsh_home(dir.path(), || {
        let paths = DshAdapter.live_backup_paths();
        assert!(paths.iter().any(|p| p.ends_with(HOME_PATCH_FILE)));
        assert!(paths.iter().any(|p| p.ends_with(CREDENTIALS_FILE)));
    });
}

#[test]
fn read_config_does_not_surface_credential_value() {
    let dir = tempfile::tempdir().unwrap();
    with_dsh_home(dir.path(), || {
        write_dsh_config(&AgentConfig {
            agent: AgentId::Dsh,
            raw: json!({
                "model": "deepseek-v4-pro",
                "apiKey": "sk-hidden-from-read"
            }),
        })
        .unwrap();
        let cfg = DshAdapter.read_config().unwrap();
        let dumped = serde_json::to_string(&cfg.raw).unwrap();
        assert!(!dumped.contains("sk-hidden-from-read"));
        assert_eq!(cfg.raw["model"], "deepseek-v4-pro");
        assert_eq!(cfg.raw["apiKeyEnv"], DEFAULT_API_KEY_ENV);
    });
}

#[test]
fn resolve_dsh_home_honors_env() {
    let expected = if cfg!(windows) {
        std::path::PathBuf::from(r"D:\tmp\agenthub-dsh-home-test")
    } else {
        std::path::PathBuf::from("/tmp/agenthub-dsh-home-test")
    };
    with_dsh_home(&expected, || {
        assert_eq!(resolve_dsh_home().unwrap(), expected);
    });
}

#[test]
fn default_home_is_dot_dsh() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let prev = std::env::var_os("DSH_HOME");
    std::env::remove_var("DSH_HOME");
    let home = resolve_dsh_home().unwrap();
    restore_env("DSH_HOME", prev);
    assert_eq!(home, home_dir().unwrap().join(".dsh"));
}

#[test]
fn credentials_yaml_roundtrips_quotes_backslashes_and_markers() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(CREDENTIALS_FILE);
    let value = r#"say "hi" \ : # spaced"#;
    write_credential_value(&path, "keep", "plain").unwrap();
    write_credential_value(&path, DEFAULT_API_KEY_ENV, value).unwrap();
    assert_eq!(
        read_credential_value(&path, DEFAULT_API_KEY_ENV)
            .unwrap()
            .as_deref(),
        Some(value)
    );
    assert_eq!(
        read_credential_value(&path, "keep").unwrap().as_deref(),
        Some("plain")
    );
}

#[test]
fn credentials_yaml_reads_escaped_double_quotes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(CREDENTIALS_FILE);
    std::fs::write(&path, r#"key: "a\"b""#).unwrap();
    assert_eq!(
        read_credential_value(&path, "key").unwrap().as_deref(),
        Some(r#"a"b"#)
    );
}

#[test]
fn credentials_yaml_newline_roundtrips_or_rejects() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(CREDENTIALS_FILE);
    let value = "line1\nline2";
    match write_credential_value(&path, DEFAULT_API_KEY_ENV, value) {
        Ok(()) => {
            assert_eq!(
                read_credential_value(&path, DEFAULT_API_KEY_ENV)
                    .unwrap()
                    .as_deref(),
                Some(value)
            );
        }
        Err(err) => {
            assert_eq!(err.code(), "invalid_arg");
            assert!(!path.exists() || !std::fs::read_to_string(&path).unwrap().contains("line1"));
        }
    }
}

#[test]
fn credentials_yaml_rejects_nested_maps() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(CREDENTIALS_FILE);
    std::fs::write(&path, "nested:\n  inner: secret\n").unwrap();
    let err = read_credential_value(&path, "nested").unwrap_err();
    assert_eq!(err.code(), "invalid_arg");
    let err = write_credential_value(&path, DEFAULT_API_KEY_ENV, "sk-new").unwrap_err();
    assert_eq!(err.code(), "invalid_arg");
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("inner: secret"));
    assert!(!text.contains("sk-new"));
}

/// The shape `dsh` 0.1.5+ writes: `version` / `records` / `refs`. A sibling
/// integer (`version: 1`) used to make the whole file unreadable, which blocked
/// "import the local login" for DeepSeek Harness.
const STRUCTURED_CREDENTIALS: &str = "\
version: 1
records:
  client-connection/browser-session:
    kind: browser-session
    payload:
      version: 1
      secret: web-session-signing-secret
refs:
  DEEPSEEK_API_KEY: sk-structured-key
";

#[test]
fn credentials_reads_structured_store_refs() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(CREDENTIALS_FILE);
    std::fs::write(&path, STRUCTURED_CREDENTIALS).unwrap();
    assert_eq!(
        read_credential_value(&path, DEFAULT_API_KEY_ENV)
            .unwrap()
            .as_deref(),
        Some("sk-structured-key")
    );
    assert_eq!(read_credential_value(&path, "absent").unwrap(), None);
}

#[test]
fn credentials_write_updates_refs_and_keeps_records() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(CREDENTIALS_FILE);
    std::fs::write(&path, STRUCTURED_CREDENTIALS).unwrap();
    write_credential_value(&path, DEFAULT_API_KEY_ENV, "sk-updated").unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(
        text.contains("web-session-signing-secret"),
        "records must survive a credential write: {text}"
    );
    assert!(text.contains("sk-updated"));
    assert!(!text.contains("sk-structured-key"));
    let doc: serde_yml::Value = serde_yml::from_str(&text).expect("yaml parse");
    assert_eq!(doc.get("version").and_then(|v| v.as_i64()), Some(1));
    assert!(
        doc.get("records")
            .and_then(|records| records.get("client-connection/browser-session"))
            .and_then(|record| record.get("payload"))
            .and_then(|payload| payload.get("secret"))
            .is_some_and(|secret| secret.is_string()),
        "dsh web session secret must survive: {text}"
    );
    assert_eq!(
        read_credential_value(&path, DEFAULT_API_KEY_ENV)
            .unwrap()
            .as_deref(),
        Some("sk-updated")
    );
}

#[test]
fn read_auth_and_account_read_structured_credentials() {
    let dir = tempfile::tempdir().unwrap();
    with_dsh_home(dir.path(), || {
        // Isolate from a host DEEPSEEK_API_KEY (QA shells often export one).
        let prev_key = std::env::var_os(DEFAULT_API_KEY_ENV);
        std::env::remove_var(DEFAULT_API_KEY_ENV);
        std::fs::write(dir.path().join(CREDENTIALS_FILE), STRUCTURED_CREDENTIALS).unwrap();
        let auth = DshAdapter.read_auth().unwrap();
        let account = DshAdapter.read_account().unwrap();
        restore_env(DEFAULT_API_KEY_ENV, prev_key);

        assert!(
            auth.has_credentials,
            "a structured refs entry is a live login"
        );
        assert_eq!(auth.health, AuthHealth::Configured);
        assert_eq!(auth.source.as_deref(), Some("dsh:credentials"));
        assert_eq!(account.agent, AgentId::Dsh);
        assert_eq!(
            account.credentials["api_key"].as_str(),
            Some("sk-structured-key")
        );
    });
}

#[test]
fn write_credential_value_still_creates_flat_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(CREDENTIALS_FILE);
    write_credential_value(&path, DEFAULT_API_KEY_ENV, "sk-fresh").unwrap();
    let doc: serde_yml::Value =
        serde_yml::from_str(&std::fs::read_to_string(&path).unwrap()).expect("yaml parse");
    assert_eq!(
        doc.get(DEFAULT_API_KEY_ENV).and_then(|v| v.as_str()),
        Some("sk-fresh")
    );
    assert_eq!(
        read_credential_value(&path, DEFAULT_API_KEY_ENV)
            .unwrap()
            .as_deref(),
        Some("sk-fresh")
    );
}

#[test]
fn upsert_llm_row_quotes_at_plugin_id_as_yaml_safe() {
    let rendered = upsert_llm_row("", &DshLlmFields::default()).unwrap();
    assert!(
        rendered.contains("- id: \"@deepseek-ai/dsh-llm-deepseek\""),
        "plugin id must be YAML-quoted: {rendered}"
    );
    let parsed: serde_yml::Value = serde_yml::from_str(&rendered).expect("yaml parse");
    let seq = parsed.as_sequence().expect("top-level sequence");
    let row = seq[0].as_mapping().expect("row mapping");
    let id = row
        .get(serde_yml::Value::from("id"))
        .and_then(|v| v.as_str())
        .expect("id string");
    assert_eq!(id, LLM_PLUGIN_ID);
    let fields = read_llm_fields_from_text(&rendered);
    assert_eq!(fields.model, DEFAULT_MODEL);
}

fn read_llm_fields_from_text(text: &str) -> DshLlmFields {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(HOME_PATCH_FILE);
    std::fs::write(&path, text).unwrap();
    read_llm_fields(&path).unwrap()
}

#[test]
fn upsert_replaces_unquoted_legacy_id_row_with_quoted() {
    let legacy = "- id: @deepseek-ai/dsh-llm-deepseek\n  config:\n    apiKeyEnv: DEEPSEEK_API_KEY\n    model: deepseek-v4-flash\n";
    let rendered = upsert_llm_row(legacy, &DshLlmFields::default()).unwrap();
    assert!(
        rendered.contains("- id: \"@deepseek-ai/dsh-llm-deepseek\""),
        "{rendered}"
    );
    assert_eq!(rendered.matches(LLM_PLUGIN_ID).count(), 1);
    let fields = read_llm_fields_from_text(&rendered);
    assert_eq!(fields.api_key_env, DEFAULT_API_KEY_ENV);
    assert_eq!(fields.model, DEFAULT_MODEL);
}

#[test]
fn yaml_quote_covers_at_and_flow_indicators() {
    assert_eq!(yaml_quote("@deepseek-ai/x"), "\"@deepseek-ai/x\"");
    assert_eq!(yaml_quote("{not}"), "\"{not}\"");
    assert_eq!(yaml_quote("plain"), "plain");
    assert_eq!(yaml_quote("has:colon"), "\"has:colon\"");
}
