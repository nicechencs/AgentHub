use super::*;
use crate::models::{Capability, CapabilityLevel};
use serde_json::json;
use std::sync::Mutex;

static ENV_LOCK: Mutex<()> = Mutex::new(());

fn restore_env(key: &str, prev: Option<std::ffi::OsString>) {
    match prev {
        Some(v) => std::env::set_var(key, v),
        None => std::env::remove_var(key),
    }
}

fn with_zcode_home<T>(dir: &std::path::Path, f: impl FnOnce() -> T) -> T {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let prev = std::env::var_os("ZCODE_HOME");
    std::env::set_var("ZCODE_HOME", dir);
    let out = f();
    restore_env("ZCODE_HOME", prev);
    out
}

#[test]
fn capability_is_exhaustive_and_honest() {
    let adapter = ZcodeAdapter;
    assert_eq!(adapter.id(), AgentId::Zcode);
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
        adapter.capability(Capability::ProjectHistory).level,
        CapabilityLevel::Planned
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
fn skills_dir_is_under_zcode_home() {
    let dir = tempfile::tempdir().unwrap();
    with_zcode_home(dir.path(), || {
        assert_eq!(
            ZcodeAdapter.skills_dir().as_deref(),
            Some(dir.path().join("skills").as_path())
        );
    });
}

#[test]
fn apply_and_read_api_key_round_trip_preserves_other_providers() {
    let dir = tempfile::tempdir().unwrap();
    with_zcode_home(dir.path(), || {
        let path = v2_config_path(dir.path());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let initial = json!({
            "provider": {
                "builtin:zai": {
                    "name": "Z.ai - API Key",
                    "kind": "anthropic",
                    "options": { "apiKey": "", "baseURL": "https://api.z.ai/api/anthropic" },
                    "enabled": true,
                    "source": "custom",
                    "models": {}
                },
                "custom-uuid": {
                    "name": "grok",
                    "kind": "openai",
                    "options": {
                        "apiKey": "sk-keep-me",
                        "baseURL": "https://example.test/v1"
                    },
                    "source": "custom",
                    "models": {}
                }
            }
        });
        std::fs::write(&path, serde_json::to_vec_pretty(&initial).unwrap()).unwrap();

        let account = ZcodeAdapter
            .build_api_key_account("sk-agenthub-test-key")
            .unwrap();
        ZcodeAdapter.apply_account(&account).unwrap();

        let auth = ZcodeAdapter.read_auth().unwrap();
        assert!(auth.has_credentials);
        assert_eq!(auth.health, AuthHealth::Configured);

        let live = ZcodeAdapter.read_account().unwrap();
        assert_eq!(
            live.credentials.get("api_key").and_then(Value::as_str),
            Some("sk-agenthub-test-key")
        );
        assert_eq!(
            live.credentials
                .get("provider_id")
                .and_then(Value::as_str),
            Some(MANAGED_PROVIDER_ID)
        );

        let disk: Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(
            disk.pointer("/provider/custom-uuid/options/apiKey")
                .and_then(Value::as_str),
            Some("sk-keep-me"),
            "must not wipe unrelated custom providers"
        );
        assert_eq!(
            disk.pointer(&format!(
                "/provider/{MANAGED_PROVIDER_ID}/options/apiKey"
            ))
            .and_then(Value::as_str),
            Some("sk-agenthub-test-key")
        );
        assert_eq!(
            disk.pointer(&format!("/provider/{MANAGED_PROVIDER_ID}/enabled"))
                .and_then(Value::as_bool),
            Some(true)
        );
    });
}

#[test]
fn write_config_projected_api_key_shape() {
    let dir = tempfile::tempdir().unwrap();
    with_zcode_home(dir.path(), || {
        ZcodeAdapter
            .write_config(&AgentConfig {
                agent: AgentId::Zcode,
                raw: json!({
                    "apiKey": "sk-projected",
                    "baseURL": "https://open.bigmodel.cn/api/anthropic",
                    "kind": "anthropic",
                    "name": "BigModel"
                }),
            })
            .unwrap();
        let path = v2_config_path(dir.path());
        let disk: Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(
            disk.pointer(&format!(
                "/provider/{MANAGED_PROVIDER_ID}/options/apiKey"
            ))
            .and_then(Value::as_str),
            Some("sk-projected")
        );
        assert_eq!(
            disk.pointer(&format!(
                "/provider/{MANAGED_PROVIDER_ID}/options/baseURL"
            ))
            .and_then(Value::as_str),
            Some("https://open.bigmodel.cn/api/anthropic")
        );
    });
}

#[test]
fn pick_prefers_enabled_custom_over_empty_builtin() {
    let providers = json!({
        "builtin:zai": {
            "name": "Z.ai",
            "options": { "apiKey": "" },
            "enabled": true,
            "source": "custom"
        },
        "aabbcc": {
            "name": "custom",
            "options": { "apiKey": "sk-custom-live" },
            "enabled": true,
            "source": "custom"
        }
    });
    let hit = pick_provider_for_import(&providers).unwrap();
    assert_eq!(hit.id, "aabbcc");
    assert_eq!(hit.api_key, "sk-custom-live");
}

#[test]
fn detect_reports_not_found_without_desktop_or_cli() {
    // Ambient machines may have zcode installed; still assert shape of NotFound notes
    // when neither desktop nor PATH hit — we only check notes contain setup URL when NotFound.
    let result = detect_installation();
    if result.status == DetectStatus::NotFound {
        assert!(result
            .notes
            .iter()
            .any(|n| n.contains("zcode.z.ai") || n.contains(SETUP_URL)));
    }
}
