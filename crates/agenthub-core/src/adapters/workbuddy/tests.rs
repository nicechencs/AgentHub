use super::*;
use crate::models::{AgentConfig, AgentId};
use serde_json::json;
use std::fs;
use std::sync::Mutex;

static ENV_LOCK: Mutex<()> = Mutex::new(());

fn restore_env(key: &str, prev: Option<std::ffi::OsString>) {
    match prev {
        Some(v) => std::env::set_var(key, v),
        None => std::env::remove_var(key),
    }
}

struct EnvGuard {
    key: &'static str,
    prev: Option<std::ffi::OsString>,
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        restore_env(self.key, self.prev.take());
    }
}

fn with_workbuddy_config<T>(dir: &std::path::Path, f: impl FnOnce() -> T) -> T {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _guard = EnvGuard {
        key: "WORKBUDDY_CONFIG_DIR",
        prev: std::env::var_os("WORKBUDDY_CONFIG_DIR"),
    };
    std::env::set_var("WORKBUDDY_CONFIG_DIR", dir);
    f()
}

#[test]
fn install_channels_native_only_no_runtime() {
    let channels = WorkBuddyAdapter.install_channels();
    assert_eq!(channels.len(), 1);
    assert_eq!(channels[0].id, "native");
    assert!(channels[0].requires.is_empty());
}

#[test]
fn skills_dir_under_workbuddy_home() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = WorkBuddyAdapter.skills_dir().expect("skills_dir");
    let s = dir.to_string_lossy().replace('\\', "/");
    assert!(
        s.ends_with("/.workbuddy/skills") || s.contains("workbuddy") && s.ends_with("/skills"),
        "unexpected skills_dir: {s}"
    );
}

#[test]
fn live_backup_paths_include_core_files() {
    let paths = WorkBuddyAdapter.live_backup_paths();
    assert!(!paths.is_empty());
    let names: Vec<String> = paths
        .iter()
        .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .collect();
    assert!(names.iter().any(|n| n == "settings.json"));
    assert!(names.iter().any(|n| n == "models.json"));
    assert!(names.iter().any(|n| n == ".mcp.json"));
}

#[test]
fn build_run_spec_headless_flags() {
    let tmp = tempfile_dir();
    // layout: install dir + bundled CLI under resources (production tree)
    let bin_dir = tmp
        .join("resources")
        .join("app.asar.unpacked")
        .join("cli")
        .join("bin");
    fs::create_dir_all(&bin_dir).unwrap();
    let codebuddy = bin_dir.join("codebuddy");
    fs::write(&codebuddy, b"#!/bin/sh\n").unwrap();
    let exe = tmp.join("WorkBuddy.exe");
    fs::write(&exe, b"mz").unwrap();

    let opts = RunOptions::default();
    let spec = WorkBuddyAdapter
        .build_run_spec(&exe, "hello", &opts)
        .unwrap();
    assert_eq!(spec.agent, AgentId::WorkBuddy);
    assert_eq!(spec.program, exe);
    assert_eq!(spec.args[0], codebuddy.to_string_lossy());
    assert_eq!(spec.args[1], "-p");
    assert_eq!(spec.args[2], "hello");
    assert!(spec.args.iter().any(|a| a == "--output-format"));
    assert!(spec.args.iter().any(|a| a == "text"));
    assert!(!spec
        .args
        .iter()
        .any(|a| a == "--dangerously-skip-permissions"));
    assert!(spec
        .env
        .iter()
        .any(|(k, v)| k == "ELECTRON_RUN_AS_NODE" && v == "1"));
    let display = spec.display_command();
    assert!(display.contains("ELECTRON_RUN_AS_NODE=1"));
    assert!(display.contains("-p"));
}

#[test]
fn build_run_spec_allow_dangerous() {
    let tmp = tempfile_dir();
    let bin_dir = tmp
        .join("resources")
        .join("app.asar.unpacked")
        .join("cli")
        .join("bin");
    fs::create_dir_all(&bin_dir).unwrap();
    fs::write(bin_dir.join("codebuddy"), b"x").unwrap();
    let exe = tmp.join("WorkBuddy.exe");
    fs::write(&exe, b"mz").unwrap();

    let mut opts = RunOptions::default();
    opts.allow_dangerous = true;
    let spec = WorkBuddyAdapter.build_run_spec(&exe, "x", &opts).unwrap();
    assert!(spec
        .args
        .iter()
        .any(|a| a == "--dangerously-skip-permissions"));
}

#[test]
fn merge_models_array_preserves_unknown_fields_and_redacted_key() {
    let live = json!({
        "models": [
            { "id": "keep", "name": "Keep", "apiKey": "keep-secret", "unknown": 7 },
            { "id": "custom", "name": "Old", "apiKey": "old-secret" }
        ],
        "availableModels": ["keep", "custom"],
        "other": true
    });
    let desired = json!({
        "models": [
            { "id": "custom", "name": "New", "apiKey": "***" }
        ]
    });
    let merged = merge_workbuddy_models(&live, &desired).unwrap();
    assert_eq!(merged["models"][0]["unknown"], 7);
    assert_eq!(merged["models"][1]["name"], "New");
    assert_eq!(merged["models"][1]["apiKey"], "old-secret");
    assert_eq!(merged["availableModels"], json!(["keep", "custom"]));
    assert_eq!(merged["other"], true);
}

#[test]
fn merge_models_supports_top_level_array_shape() {
    let live = json!([
        { "id": "keep", "apiKey": "secret" }
    ]);
    let desired = json!([
        { "id": "custom", "name": "Custom" }
    ]);
    let merged = merge_workbuddy_models(&live, &desired).unwrap();
    assert_eq!(merged[0]["id"], "keep");
    assert_eq!(merged[1]["id"], "custom");
}

#[test]
fn merge_models_keeps_object_shape_and_unknown_top_level_fields() {
    let live = json!({
        "models": [{ "id": "keep", "apiKey": "secret" }],
        "availableModels": ["keep"],
        "unknown": { "preserve": true }
    });
    let desired = json!([{ "id": "custom", "name": "Custom" }]);
    let merged = merge_workbuddy_models(&live, &desired).unwrap();
    assert_eq!(merged["models"][0]["id"], "keep");
    assert_eq!(merged["models"][1]["id"], "custom");
    assert_eq!(merged["availableModels"], json!(["keep"]));
    assert_eq!(merged["unknown"]["preserve"], true);
}

#[test]
fn merge_generic_id_with_a_different_key_appends_instead_of_replacing() {
    let live = json!([
        { "id": "custom-model", "name": "Custom Model", "apiKey": "sk-first" }
    ]);
    let desired = json!([
        { "id": "custom-model", "name": "Custom Model", "apiKey": "sk-second" }
    ]);
    let merged = merge_workbuddy_models(&live, &desired).unwrap();
    let rows = merged.as_array().unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["id"], "custom-model");
    assert_eq!(rows[0]["apiKey"], "sk-first");
    assert_eq!(rows[1]["id"], "custom-model-2");
    assert_eq!(rows[1]["apiKey"], "sk-second");
}

#[test]
fn merge_same_api_key_updates_the_existing_row_even_if_ids_differ() {
    let live = json!([
        { "id": "custom-model-2", "name": "Custom Model", "apiKey": "sk-keep" }
    ]);
    let desired = json!([
        { "id": "custom-model", "name": "Renamed", "apiKey": "sk-keep" }
    ]);
    let merged = merge_workbuddy_models(&live, &desired).unwrap();
    let rows = merged.as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["id"], "custom-model-2");
    assert_eq!(rows[0]["name"], "Renamed");
    assert_eq!(rows[0]["apiKey"], "sk-keep");
}

#[test]
fn catalog_capabilities_are_partial_not_blocked() {
    assert!(WorkBuddyAdapter
        .capability(crate::models::Capability::AccountSwitch)
        .is_usable());
    assert!(WorkBuddyAdapter
        .capability(crate::models::Capability::ApiKeyAccount)
        .is_usable());
    assert!(WorkBuddyAdapter
        .capability(crate::models::Capability::ConfigWrite)
        .is_usable());
    assert!(WorkBuddyAdapter
        .capability(crate::models::Capability::Skills)
        .is_usable());
    assert_eq!(
        WorkBuddyAdapter
            .capability(crate::models::Capability::ConfigWrite)
            .level,
        crate::models::CapabilityLevel::Partial
    );
}

#[test]
fn normalize_chat_url_accepts_full_path_and_v1_root() {
    assert_eq!(
        normalize_workbuddy_chat_url("https://api.example.com/v1/chat/completions").unwrap(),
        "https://api.example.com/v1/chat/completions"
    );
    assert_eq!(
        normalize_workbuddy_chat_url("https://api.example.com/v1/").unwrap(),
        "https://api.example.com/v1/chat/completions"
    );
    assert_eq!(
        normalize_workbuddy_chat_url("https://api.deepseek.com/chat/completions").unwrap(),
        "https://api.deepseek.com/v1/chat/completions"
    );
    assert!(normalize_workbuddy_chat_url("https://api.anthropic.com/v1/messages").is_err());
}

#[test]
fn expand_skips_empty_key_and_jwt_and_splits_portable_rows() {
    let models = json!([
        {
            "id": "grok-4.6",
            "name": "grok-4.6",
            "url": "https://api.qooo.io/v1/chat/completions",
            "apiKey": "sk-live-one"
        },
        {
            "id": "missing-key",
            "url": "https://api.example.com/v1/chat/completions",
            "apiKey": ""
        },
        {
            "id": "plan-jwt",
            "url": "https://api.example.com/v1/chat/completions",
            "apiKey": "eyJhbGciOiJub25lIn0.eyJzdWIiOiJwbGFuIn0.sig"
        },
        {
            "id": "deepseek-v4-flash",
            "name": "DeepSeek",
            "url": "https://api.deepseek.com/chat/completions",
            "apiKey": "sk-live-two"
        }
    ]);
    let lives = expand_workbuddy_catalog(&models);
    let ids: Vec<String> = lives
        .iter()
        .map(|live| workbuddy_model_slot(&live.credentials))
        .collect();
    assert_eq!(ids, ["grok-4.6", "deepseek-v4-flash"]);
    let grok = &lives[0];
    assert_eq!(
        grok.credentials.get("name").and_then(|v| v.as_str()),
        Some("grok-4.6")
    );
    assert_eq!(
        grok.credentials.get("url").and_then(|v| v.as_str()),
        Some("https://api.qooo.io/v1/chat/completions")
    );
    assert_eq!(
        grok.extra.get("endpoint").and_then(|v| v.as_str()),
        Some("https://api.qooo.io/v1/chat/completions")
    );
    assert_eq!(grok.credentials["catalog_row"]["id"], "grok-4.6");
    assert!(
        grok.label_hint
            .as_deref()
            .is_some_and(|label| label.contains("grok-4.6")),
        "label should name the catalog model, got {:?}",
        grok.label_hint
    );
    assert_eq!(
        WorkBuddyAdapter
            .identity_label(crate::models::AccountKind::ApiKey, &grok.credentials, None)
            .as_deref(),
        Some("grok-4.6")
    );
}

#[test]
fn restore_replaces_models_json_instead_of_merging() {
    let dir = tempfile_dir();
    with_workbuddy_config(&dir, || {
        let path = dir.join("models.json");
        fs::write(
            &path,
            serde_json::to_vec_pretty(&json!([
                { "id": "keep-me", "apiKey": "secret" },
                { "id": "drop-me", "apiKey": "other" }
            ]))
            .unwrap(),
        )
        .unwrap();
        restore_workbuddy_catalog(&AgentConfig {
            agent: AgentId::WorkBuddy,
            raw: json!({
                "models": [{ "id": "keep-me", "apiKey": "secret" }]
            }),
        })
        .unwrap();
        let written: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(written.as_array().map(|a| a.len()), Some(1));
        assert_eq!(written[0]["id"], "keep-me");
    });
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn apply_account_rewrites_chat_completions_to_v1_path() {
    let dir = tempfile_dir();
    with_workbuddy_config(&dir, || {
        let mut account = WorkBuddyAdapter.build_api_key_account("sk-apply").unwrap();
        attach_api_key_catalog_fields(
            &mut account.credentials,
            Some("https://api.anthropic.com/v1/messages"),
            Some("claude"),
        )
        .unwrap_err();
        attach_api_key_catalog_fields(
            &mut account.credentials,
            Some("https://api.deepseek.com/chat/completions"),
            Some("deepseek-v4-flash"),
        )
        .unwrap();
        WorkBuddyAdapter.apply_account(&account).unwrap();
        let written: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(dir.join("models.json")).unwrap()).unwrap();
        assert_eq!(written[0]["id"], "deepseek-v4-flash");
        assert_eq!(
            written[0]["url"],
            "https://api.deepseek.com/v1/chat/completions"
        );
    });
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn apply_account_appends_a_second_generic_api_key() {
    let dir = tempfile_dir();
    with_workbuddy_config(&dir, || {
        let mut first = WorkBuddyAdapter.build_api_key_account("sk-first").unwrap();
        attach_api_key_catalog_fields(
            &mut first.credentials,
            Some("https://api.deepseek.com/v1/chat/completions"),
            Some("custom-model"),
        )
        .unwrap();
        WorkBuddyAdapter.apply_account(&first).unwrap();

        let mut second = WorkBuddyAdapter.build_api_key_account("sk-second").unwrap();
        attach_api_key_catalog_fields(
            &mut second.credentials,
            Some("https://api.qooo.io/v1/chat/completions"),
            Some("custom-model"),
        )
        .unwrap();
        WorkBuddyAdapter.apply_account(&second).unwrap();

        let written: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(dir.join("models.json")).unwrap()).unwrap();
        let rows = written.as_array().unwrap();
        assert_eq!(
            rows.len(),
            2,
            "second generic id must not replace the first key"
        );
        assert_eq!(rows[0]["apiKey"], "sk-first");
        assert_eq!(rows[1]["apiKey"], "sk-second");
        assert_ne!(rows[0]["id"], rows[1]["id"]);
    });
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn resolve_bundled_codebuddy_ignores_extracted() {
    let tmp = tempfile_dir();
    // only extracted path — must NOT be used
    let extracted = tmp.join("extracted").join("cli").join("bin");
    fs::create_dir_all(&extracted).unwrap();
    fs::write(extracted.join("codebuddy"), b"bad").unwrap();
    assert!(resolve_bundled_codebuddy(&tmp).is_none());

    let good = tmp
        .join("resources")
        .join("app.asar.unpacked")
        .join("cli")
        .join("bin");
    fs::create_dir_all(&good).unwrap();
    let cb = good.join("codebuddy");
    fs::write(&cb, b"ok").unwrap();
    assert_eq!(resolve_bundled_codebuddy(&tmp), Some(cb));
    let _ = fs::remove_dir_all(&tmp);
}

#[test]
fn resolve_bundled_codebuddy_finds_macos_resources_layout() {
    let tmp = tempfile_dir();
    let macos = tmp.join("WorkBuddy.app").join("Contents").join("MacOS");
    let bin = tmp
        .join("WorkBuddy.app")
        .join("Contents")
        .join("Resources")
        .join("app.asar.unpacked")
        .join("cli")
        .join("bin");
    fs::create_dir_all(&macos).unwrap();
    fs::create_dir_all(&bin).unwrap();
    let cb = bin.join("codebuddy");
    fs::write(&cb, b"ok").unwrap();
    assert_eq!(resolve_bundled_codebuddy(&macos), Some(cb));
    let _ = fs::remove_dir_all(&tmp);
}

#[cfg(not(windows))]
#[test]
fn cf_bundle_executable_name_reads_xml_plist() {
    let tmp = tempfile_dir();
    let contents = tmp.join("Contents");
    fs::create_dir_all(&contents).unwrap();
    fs::write(
        contents.join("Info.plist"),
        r#"<?xml version="1.0"?>
<plist>
<dict>
	<key>CFBundleExecutable</key>
	<string>Electron</string>
</dict>
</plist>
"#,
    )
    .unwrap();
    assert_eq!(
        cf_bundle_executable_name(&tmp).as_deref(),
        Some("Electron")
    );
    let bins = macos_workbuddy_binaries(&tmp);
    assert_eq!(
        bins[0],
        tmp.join("Contents").join("MacOS").join("Electron")
    );
    assert!(bins
        .iter()
        .any(|p| p.file_name().and_then(|n| n.to_str()) == Some("WorkBuddy")));
    let _ = fs::remove_dir_all(&tmp);
}

#[test]
fn well_known_exe_paths_are_cheap_fixed_only() {
    let paths = well_known_exe_paths();
    // Must not be empty on Windows (LOCALAPPDATA or home) or Unix (Applications).
    // Registry is intentionally not in this list.
    assert!(!paths.is_empty());
    for p in &paths {
        let s = p.to_string_lossy().to_ascii_lowercase();
        assert!(
            s.contains("workbuddy"),
            "unexpected well-known path: {}",
            p.display()
        );
        assert!(
            !s.contains("uninstall"),
            "well-known must not be uninstaller path: {}",
            p.display()
        );
    }
    #[cfg(not(windows))]
    {
        assert!(
            paths.iter().any(|p| {
                p.to_string_lossy()
                    .replace('\\', "/")
                    .ends_with("/WorkBuddy.app/Contents/MacOS/Electron")
            }),
            "missing Electron binary candidate in {paths:?}"
        );
        assert!(
            paths.iter().any(|p| {
                p.to_string_lossy()
                    .replace('\\', "/")
                    .ends_with("/WorkBuddy.app/Contents/MacOS/WorkBuddy")
            }),
            "missing WorkBuddy binary candidate in {paths:?}"
        );
    }
}

#[test]
fn auth_info_path_is_platform_specific() {
    let path = auth_info_path();
    #[cfg(windows)]
    {
        let s = path
            .expect("windows auth path")
            .to_string_lossy()
            .replace('\\', "/");
        assert!(
            s.to_ascii_lowercase()
                .ends_with("/codebuddyextension/data/public/auth/workbuddy-desktop.info"),
            "unexpected windows auth path: {s}"
        );
    }
    #[cfg(target_os = "macos")]
    {
        let s = path
            .expect("macos auth path")
            .to_string_lossy()
            .replace('\\', "/");
        assert!(
            s.contains("/Library/Application Support/CodeBuddyExtension/Data/Public/auth/workbuddy-desktop.info"),
            "unexpected macos auth path: {s}"
        );
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        assert!(path.is_none());
    }
}

fn tempfile_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "agenthub-wb-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}
