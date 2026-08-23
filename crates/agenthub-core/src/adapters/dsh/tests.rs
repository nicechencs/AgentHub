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
