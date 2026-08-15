use super::*;
use crate::models::{AccountKind, AgentId, Capability, CapabilityLevel};
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
    for cap in [
        Capability::ConfigWrite,
        Capability::AccountSwitch,
        Capability::ApiKeyAccount,
        Capability::Skills,
        Capability::LiveBackup,
        Capability::StructuredStream,
        Capability::DangerousMode,
        Capability::ProjectHistory,
        Capability::ProjectDelete,
        Capability::ProviderPresets,
        Capability::Usage,
        Capability::Mcp,
        Capability::ModelSelect,
        Capability::SessionResume,
    ] {
        let _ = adapter.capability(cap);
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
fn write_config_merges_llm_row_and_preserves_other_rows() {
    let dir = tempfile::tempdir().unwrap();
    with_dsh_home(dir.path(), || {
        let patch = dir.path().join(HOME_PATCH_FILE);
        std::fs::write(
            &patch,
            "- id: example.other\n  config:\n    keep: yes\n",
        )
        .unwrap();
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
        !spec.args.iter().any(|a| a.contains("yolo") || a.contains("danger")),
        "must not invent danger flags"
    );
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
