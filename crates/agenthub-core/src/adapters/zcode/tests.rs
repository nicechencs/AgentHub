use super::*;
use crate::error::AppError;
use crate::models::{
    AgentConfig, AgentId, AuthHealth, Capability, CapabilityLevel, DetectStatus, RunOptions,
};
use serde_json::{json, Value};
use std::path::Path;
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
    let result = detect_installation();
    match result.status {
        DetectStatus::Installed => {
            assert_eq!(result.agent, AgentId::Zcode);
            assert_eq!(result.channel.as_deref(), Some("native"));
            let path = result.binary_path.expect("installed ZCode needs a path");
            assert!(
                path.is_file(),
                "detected binary must exist: {}",
                path.display()
            );
            let lower = path.to_string_lossy().replace('\\', "/").to_ascii_lowercase();
            assert!(
                lower.ends_with("/programs/zcode/zcode.exe")
                    || lower.ends_with("/zcode/zcode.exe")
                    || lower.ends_with("/macos/zcode")
                    || path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n.eq_ignore_ascii_case("zcode")
                            || n.eq_ignore_ascii_case("zcode.exe")
                            || n.eq_ignore_ascii_case("zcode.cmd")),
                "unexpected desktop/CLI path: {}",
                path.display()
            );
        }
        DetectStatus::NotFound => {
            assert!(result
                .notes
                .iter()
                .any(|n| n.contains("zcode.z.ai") || n.contains(SETUP_URL)));
        }
    }
}

#[test]
fn windows_well_known_paths_include_programs_zcode() {
    let paths = well_known_exe_paths();
    #[cfg(windows)]
    {
        assert!(
            paths.iter().any(|p| {
                p.to_string_lossy()
                    .replace('\\', "/")
                    .to_ascii_lowercase()
                    .ends_with("/programs/zcode/zcode.exe")
            }),
            "missing LOCALAPPDATA/Programs/ZCode/ZCode.exe in {paths:?}"
        );
    }
    #[cfg(not(windows))]
    {
        assert!(!paths.is_empty());
    }
}

#[test]
fn read_version_hint_none_without_unpacked_package_json() {
    let dir = tempfile::tempdir().unwrap();
    let exe = dir.path().join("ZCode.exe");
    std::fs::write(&exe, b"").unwrap();
    std::fs::create_dir_all(
        dir.path()
            .join("resources")
            .join("app.asar.unpacked"),
    )
    .unwrap();
    assert_eq!(read_version_hint(&exe), None);
}

#[test]
fn read_version_hint_reads_unpacked_package_json() {
    let dir = tempfile::tempdir().unwrap();
    let exe = dir.path().join("ZCode.exe");
    std::fs::write(&exe, b"").unwrap();
    let unpacked = dir
        .path()
        .join("resources")
        .join("app.asar.unpacked");
    std::fs::create_dir_all(&unpacked).unwrap();
    std::fs::write(unpacked.join("package.json"), r#"{"version":"3.10.1"}"#).unwrap();
    assert_eq!(read_version_hint(&exe).as_deref(), Some("3.10.1"));
}

#[test]
fn live_backup_paths_include_v2_config() {
    let dir = tempfile::tempdir().unwrap();
    with_zcode_home(dir.path(), || {
        let paths = ZcodeAdapter.live_backup_paths();
        assert!(
            paths.iter().any(|p| {
                p.file_name().and_then(|n| n.to_str()) == Some("config.json")
                    && p.parent()
                        .and_then(|parent| parent.file_name())
                        .and_then(|n| n.to_str())
                        == Some("v2")
            }),
            "expected v2/config.json in {paths:?}"
        );
    });
}

#[test]
fn install_channels_native_only_no_runtime() {
    let channels = ZcodeAdapter.install_channels();
    assert_eq!(channels.len(), 1);
    assert_eq!(channels[0].id, "native");
    assert!(channels[0].requires.is_empty());
}

/// Live ZCode 3.10 layout: empty builtin API Key slot + Start Plan JWT.
const START_PLAN_JWT: &str = "eyJhbGciOiJub25lIn0.eyJzdWIiOiJ6Y29kZS1zdGFydC1wbGFuLXRlc3QifQ.sig";

fn live_v2_provider_map() -> Value {
    json!({
        "builtin:zai": {
            "name": "Z.ai - API Key",
            "kind": "anthropic",
            "options": {
                "apiKey": "",
                "baseURL": "https://api.z.ai/api/anthropic",
                "apiKeyRequired": true
            },
            "enabled": true,
            "source": "custom",
            "models": { "GLM-5.3": {} }
        },
        "builtin:zai-coding-plan": {
            "name": "Z.ai - Coding Plan",
            "kind": "anthropic",
            "options": { "apiKey": "abcd1234deadbeef.coding-plan-placeholder" },
            "enabled": false,
            "source": "custom",
            "models": { "GLM-5.3": {} }
        },
        "builtin:zai-start-plan": {
            "name": "Z.ai - Coding Plan",
            "kind": "anthropic",
            "options": { "apiKey": START_PLAN_JWT },
            "enabled": true,
            "source": "custom",
            "models": { "GLM-5.3": {}, "GLM-5.3-Flash": {} }
        }
    })
}

#[test]
fn live_start_plan_jwt_is_not_a_portable_api_key() {
    let providers = live_v2_provider_map();
    assert!(
        providers_with_api_key(&providers).is_empty(),
        "plan JWTs and dotted coding-plan tokens must not count as API keys"
    );
    assert!(pick_provider_for_import(&providers).is_none());
}

#[test]
fn read_auth_ignores_start_plan_jwt_fixture() {
    let dir = tempfile::tempdir().unwrap();
    with_zcode_home(dir.path(), || {
        let path = v2_config_path(dir.path());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            serde_json::to_vec_pretty(&json!({ "provider": live_v2_provider_map() })).unwrap(),
        )
        .unwrap();
        let auth = ZcodeAdapter.read_auth().unwrap();
        assert!(!auth.has_credentials);
        assert_eq!(auth.health, AuthHealth::Missing);
        assert!(ZcodeAdapter.read_account().is_err());
    });
}

#[test]
fn custom_api_key_still_imported_beside_start_plan_jwt() {
    let mut providers = live_v2_provider_map();
    providers["aabbcc"] = json!({
        "name": "custom",
        "kind": "anthropic",
        "options": { "apiKey": "sk-custom-live", "baseURL": "https://example.test/v1" },
        "enabled": true,
        "source": "custom"
    });
    let hit = pick_provider_for_import(&providers).unwrap();
    assert_eq!(hit.id, "aabbcc");
    assert_eq!(hit.api_key, "sk-custom-live");
}

#[test]
fn build_run_spec_rejects_desktop_zcode_exe() {
    let desktop = Path::new(r"C:\Users\example\AppData\Local\Programs\ZCode\ZCode.exe");
    let err = ZcodeAdapter
        .build_run_spec(desktop, "hello", &RunOptions::default())
        .unwrap_err();
    assert!(
        matches!(err, AppError::Unsupported(_)),
        "desktop exe must not be spawned as CLI: {err:?}"
    );
}

#[test]
fn build_run_spec_accepts_path_cli_zcode_exe() {
    let cli = Path::new(r"C:\npm\zcode.exe");
    let spec = ZcodeAdapter
        .build_run_spec(cli, "hello", &RunOptions::default())
        .unwrap();
    assert_eq!(spec.program, cli);
    assert_eq!(spec.args, vec!["hello".to_string()]);
}

#[test]
fn is_desktop_app_binary_matches_windows_and_macos_layouts() {
    assert!(is_desktop_app_binary(Path::new(
        r"C:\Users\example\AppData\Local\Programs\ZCode\ZCode.exe"
    )));
    assert!(!is_desktop_app_binary(Path::new(r"C:\npm\zcode.exe")));
    assert!(is_desktop_app_binary(Path::new(
        "/Applications/ZCode.app/Contents/MacOS/ZCode"
    )));
}

#[test]
fn looks_like_jwt_requires_three_dot_separated_parts() {
    assert!(looks_like_jwt(START_PLAN_JWT));
    assert!(!looks_like_jwt("sk-not-a-jwt"));
    assert!(!looks_like_jwt("c23be09d709f4c5aa2f4b8ac17d7ae1a.placeholder"));
}
