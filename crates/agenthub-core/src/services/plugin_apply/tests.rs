use super::*;
use crate::services::plugin_inventory::CliRun;
use serde_json::{json, Value as JsonValue};
use std::path::Path;
use std::sync::Mutex;
use tempfile::tempdir;

struct FakeCli {
    fail: bool,
    /// When true, mutate live files before returning (including on failure).
    mutate: bool,
    claude_settings: PathBuf,
    grok_config: PathBuf,
    calls: Mutex<Vec<(String, Vec<String>)>>,
}

impl PluginCliRunner for FakeCli {
    fn run_list_json(&self, program: &Path) -> CliRun {
        self.run_plugin(program, &["plugin", "list", "--json"])
    }

    fn run_plugin(&self, program: &Path, args: &[&str]) -> CliRun {
        let bin = program
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        self.calls
            .lock()
            .unwrap()
            .push((bin.clone(), args.iter().map(|s| (*s).to_string()).collect()));
        if args.get(1) == Some(&"list") && args.iter().any(|a| *a == "--available") {
            return CliRun {
                stdout: r#"[{"status":"available","name":"superpowers","marketplace":"xAI Official","components":{"skills":[{"name":"tdd"}]}}]"#.into(),
                stderr: String::new(),
                exit_code: Some(0),
                timed_out: false,
                spawn_error: None,
            };
        }
        if self.mutate {
            let action = args.get(1).copied().unwrap_or("");
            let spec = args.get(2).copied().unwrap_or("");
            if action == "enable" || action == "disable" {
                let enabled = action == "enable";
                if bin.contains("claude") {
                    mutate_claude(&self.claude_settings, spec, enabled);
                } else if bin.contains("grok") {
                    mutate_grok(&self.grok_config, spec, enabled);
                }
            } else if action == "install" || action == "uninstall" {
                if bin.contains("claude") {
                    mutate_claude(&self.claude_settings, spec, action == "install");
                } else if bin.contains("grok") {
                    mutate_grok(&self.grok_config, spec, action == "install");
                }
            }
        }
        if self.fail {
            return CliRun {
                stdout: String::new(),
                stderr: "cli boom".into(),
                exit_code: Some(1),
                timed_out: false,
                spawn_error: None,
            };
        }
        CliRun {
            stdout: String::new(),
            stderr: String::new(),
            exit_code: Some(0),
            timed_out: false,
            spawn_error: None,
        }
    }
}

fn mutate_claude(path: &Path, spec: &str, enabled: bool) {
    let mut value: JsonValue = fs::read_to_string(path)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_else(|| json!({}));
    let obj = value.as_object_mut().unwrap();
    let map = obj
        .entry("enabledPlugins")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .unwrap();
    map.insert(spec.to_string(), JsonValue::Bool(enabled));
    fs::write(path, serde_json::to_string_pretty(&value).unwrap()).unwrap();
}

fn mutate_grok(path: &Path, name: &str, enabled: bool) {
    if enabled {
        fs::write(
            path,
            format!("[plugins]\nenabled = [\"{name}\"]\ndisabled = []\n"),
        )
        .unwrap();
    } else {
        fs::write(
            path,
            format!("[plugins]\nenabled = []\ndisabled = [\"{name}\"]\n"),
        )
        .unwrap();
    }
}

fn claude_enabled(path: &Path, spec: &str) -> Option<bool> {
    let value: JsonValue = serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
    value.get("enabledPlugins")?.get(spec)?.as_bool()
}

fn grok_lists(path: &Path) -> (Vec<String>, Vec<String>) {
    let text = fs::read_to_string(path).unwrap();
    let doc = text.parse::<toml_edit::DocumentMut>().unwrap();
    let field = |name: &str| -> Vec<String> {
        doc.get("plugins")
            .and_then(|v| v.get(name))
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    };
    (field("enabled"), field("disabled"))
}

fn fixture() -> (tempfile::TempDir, PathBuf, PathBuf, PathBuf, PathBuf, PathBuf) {
    let dir = tempdir().unwrap();
    let user_home = dir.path().to_path_buf();
    let claude_home = dir.path().join("claude");
    let grok_home = dir.path().join("grok");
    fs::create_dir_all(&claude_home).unwrap();
    fs::create_dir_all(&grok_home).unwrap();
    let settings = claude_home.join("settings.json");
    let config = grok_home.join("config.toml");
    fs::write(
        &settings,
        r#"{
  "theme": "dark",
  "enabledPlugins": {
    "pack@official": false
  }
}"#,
    )
    .unwrap();
    fs::write(
        &config,
        "[plugins]\nenabled = []\ndisabled = [\"gdrive\"]\n",
    )
    .unwrap();
    (dir, user_home, claude_home, grok_home, settings, config)
}

fn apply_ctx<'a>(
    user_home: PathBuf,
    claude_home: PathBuf,
    grok_home: PathBuf,
    runner: &'a FakeCli,
) -> PluginApplyContext<'a> {
    PluginApplyContext {
        user_home,
        claude_home,
        grok_home,
        claude_bin: Some(PathBuf::from("/usr/bin/claude")),
        grok_bin: Some(PathBuf::from("/usr/bin/grok")),
        runner,
    }
}

fn fake_cli(fail: bool, mutate: bool, settings: PathBuf, config: PathBuf) -> FakeCli {
    FakeCli {
        fail,
        mutate,
        claude_settings: settings,
        grok_config: config,
        calls: Mutex::new(Vec::new()),
    }
}

#[test]
fn enable_disable_round_trip_with_fake_executor() {
    let (_dir, user_home, claude_home, grok_home, settings, config) = fixture();
    let fake = fake_cli(false, true, settings.clone(), config.clone());
    let ctx = apply_ctx(user_home, claude_home, grok_home, &fake);

    enable_plugin_with(&ctx, AgentId::Claude, "pack", Some("official")).unwrap();
    assert_eq!(claude_enabled(&settings, "pack@official"), Some(true));
    disable_plugin_with(&ctx, AgentId::Claude, "pack", Some("official")).unwrap();
    assert_eq!(claude_enabled(&settings, "pack@official"), Some(false));

    enable_plugin_with(&ctx, AgentId::Grok, "gdrive", None).unwrap();
    let (enabled, disabled) = grok_lists(&config);
    assert_eq!(enabled, vec!["gdrive".to_string()]);
    assert!(disabled.is_empty());
    disable_plugin_with(&ctx, AgentId::Grok, "gdrive", None).unwrap();
    let (enabled, disabled) = grok_lists(&config);
    assert!(enabled.is_empty());
    assert_eq!(disabled, vec!["gdrive".to_string()]);

    let calls = fake.calls.lock().unwrap().clone();
    assert_eq!(
        calls,
        vec![
            (
                "claude".into(),
                vec!["plugin".into(), "enable".into(), "pack@official".into()]
            ),
            (
                "claude".into(),
                vec!["plugin".into(), "disable".into(), "pack@official".into()]
            ),
            (
                "grok".into(),
                vec!["plugin".into(), "enable".into(), "gdrive".into()]
            ),
            (
                "grok".into(),
                vec!["plugin".into(), "disable".into(), "gdrive".into()]
            ),
        ]
    );
}

#[test]
fn cli_failure_does_not_leave_half_written_files() {
    let (_dir, user_home, claude_home, grok_home, settings, config) = fixture();
    let original_settings = fs::read(&settings).unwrap();
    let original_config = fs::read(&config).unwrap();
    let fake = fake_cli(true, true, settings.clone(), config.clone());
    let ctx = apply_ctx(user_home, claude_home, grok_home, &fake);

    let err = enable_plugin_with(&ctx, AgentId::Claude, "pack", Some("official")).unwrap_err();
    assert!(err.contains("cli boom"), "{err}");
    assert_eq!(fs::read(&settings).unwrap(), original_settings);

    let err = disable_plugin_with(&ctx, AgentId::Grok, "gdrive", None).unwrap_err();
    assert!(err.contains("cli boom"), "{err}");
    assert_eq!(fs::read(&config).unwrap(), original_config);
}

#[test]
fn missing_cli_does_not_write() {
    let (_dir, user_home, claude_home, grok_home, settings, config) = fixture();
    let original_settings = fs::read(&settings).unwrap();
    let original_config = fs::read(&config).unwrap();
    let fake = fake_cli(false, true, settings.clone(), config.clone());
    let ctx = PluginApplyContext {
        user_home,
        claude_home,
        grok_home,
        claude_bin: None,
        grok_bin: None,
        runner: &fake,
    };
    assert!(enable_plugin_with(&ctx, AgentId::Claude, "pack", Some("official")).is_err());
    assert!(disable_plugin_with(&ctx, AgentId::Grok, "gdrive", None).is_err());
    assert!(fake.calls.lock().unwrap().is_empty());
    assert_eq!(fs::read(&settings).unwrap(), original_settings);
    assert_eq!(fs::read(&config).unwrap(), original_config);
}

#[test]
fn unsupported_agent_does_not_write() {
    let (_dir, user_home, claude_home, grok_home, settings, config) = fixture();
    let original_settings = fs::read(&settings).unwrap();
    let fake = fake_cli(false, true, settings.clone(), config);
    let ctx = apply_ctx(user_home, claude_home, grok_home, &fake);
    for agent in [
        AgentId::Codex,
        AgentId::Pi,
        AgentId::Cursor,
        AgentId::Dsh,
        AgentId::Kimi,
        AgentId::WorkBuddy,
    ] {
        let err = enable_plugin_with(&ctx, agent, "anything", None).unwrap_err();
        assert!(err.contains("Claude and Grok"), "{err}");
    }
    assert!(fake.calls.lock().unwrap().is_empty());
    assert_eq!(fs::read(&settings).unwrap(), original_settings);
}

#[test]
fn grok_available_list_is_not_installed_inventory() {
    let (_dir, user_home, claude_home, grok_home, settings, config) = fixture();
    let fake = fake_cli(false, false, settings, config);
    let ctx = apply_ctx(user_home, claude_home, grok_home, &fake);
    let rows = list_available_plugins_with(&ctx, AgentId::Grok).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].name, "superpowers");
    assert_eq!(rows[0].source, "available");
    assert!(rows[0].components.iter().any(|c| c.kind == "skills"));
}

#[test]
fn install_without_confirm_does_not_call_cli() {
    let (_dir, user_home, claude_home, grok_home, settings, config) = fixture();
    let original_config = fs::read(&config).unwrap();
    let fake = fake_cli(false, true, settings, config.clone());
    let ctx = apply_ctx(user_home, claude_home, grok_home, &fake);
    let err = install_plugin_with(
        &ctx,
        AgentId::Grok,
        "superpowers",
        PluginInstallOptions { confirmed: false },
    )
    .unwrap_err();
    assert!(err.contains("confirmation"), "{err}");
    assert!(fake.calls.lock().unwrap().is_empty());
    assert_eq!(fs::read(&config).unwrap(), original_config);
}

#[test]
fn grok_install_after_confirm_passes_trust() {
    let (_dir, user_home, claude_home, grok_home, settings, config) = fixture();
    let fake = fake_cli(false, false, settings, config);
    let ctx = apply_ctx(user_home, claude_home, grok_home, &fake);
    install_plugin_with(
        &ctx,
        AgentId::Grok,
        "superpowers",
        PluginInstallOptions { confirmed: true },
    )
    .unwrap();
    let calls = fake.calls.lock().unwrap().clone();
    assert_eq!(
        calls,
        vec![(
            "grok".into(),
            vec![
                "plugin".into(),
                "install".into(),
                "superpowers".into(),
                "--trust".into()
            ]
        )]
    );
}

#[test]
fn claude_install_after_confirm_passes_yes() {
    let (_dir, user_home, claude_home, grok_home, settings, config) = fixture();
    let fake = fake_cli(false, false, settings, config);
    let ctx = apply_ctx(user_home, claude_home, grok_home, &fake);
    install_plugin_with(
        &ctx,
        AgentId::Claude,
        "demo@official",
        PluginInstallOptions { confirmed: true },
    )
    .unwrap();
    let calls = fake.calls.lock().unwrap().clone();
    assert_eq!(
        calls[0].1,
        vec![
            "plugin".to_string(),
            "install".to_string(),
            "demo@official".to_string(),
            "-y".to_string(),
            "-s".to_string(),
            "user".to_string()
        ]
    );
}

#[test]
fn grok_install_git_and_local_sources_are_allowed() {
    let (_dir, user_home, claude_home, grok_home, settings, config) = fixture();
    let fake = fake_cli(false, false, settings, config);
    let ctx = apply_ctx(user_home, claude_home, grok_home, &fake);
    install_plugin_with(
        &ctx,
        AgentId::Grok,
        "xai-org/example",
        PluginInstallOptions { confirmed: true },
    )
    .unwrap();
    install_plugin_with(
        &ctx,
        AgentId::Grok,
        "/tmp/demo-plugin",
        PluginInstallOptions { confirmed: true },
    )
    .unwrap();
    let calls = fake.calls.lock().unwrap().clone();
    assert_eq!(calls[0].1[2], "xai-org/example");
    assert_eq!(calls[1].1[2], "/tmp/demo-plugin");
}

#[test]
fn claude_rejects_git_or_path_install() {
    let (_dir, user_home, claude_home, grok_home, settings, config) = fixture();
    let fake = fake_cli(false, false, settings, config);
    let ctx = apply_ctx(user_home, claude_home, grok_home, &fake);
    let err = install_plugin_with(
        &ctx,
        AgentId::Claude,
        "https://github.com/acme/plugin.git",
        PluginInstallOptions { confirmed: true },
    )
    .unwrap_err();
    assert!(err.contains("name@marketplace"), "{err}");
    assert!(fake.calls.lock().unwrap().is_empty());
}

#[test]
fn install_cli_failure_restores_config() {
    let (_dir, user_home, claude_home, grok_home, settings, config) = fixture();
    let original_config = fs::read(&config).unwrap();
    let fake = fake_cli(true, true, settings, config.clone());
    let ctx = apply_ctx(user_home, claude_home, grok_home, &fake);
    let err = install_plugin_with(
        &ctx,
        AgentId::Grok,
        "superpowers",
        PluginInstallOptions { confirmed: true },
    )
    .unwrap_err();
    assert!(err.contains("cli boom"), "{err}");
    assert_eq!(fs::read(&config).unwrap(), original_config);
}

#[test]
fn uninstall_keeps_data_by_default() {
    let (_dir, user_home, claude_home, grok_home, settings, config) = fixture();
    let fake = fake_cli(false, false, settings, config);
    let ctx = apply_ctx(user_home, claude_home, grok_home, &fake);
    uninstall_plugin_with(
        &ctx,
        AgentId::Grok,
        "gdrive",
        None,
        PluginUninstallOptions { keep_data: true },
    )
    .unwrap();
    uninstall_plugin_with(
        &ctx,
        AgentId::Claude,
        "pack",
        Some("official"),
        PluginUninstallOptions { keep_data: true },
    )
    .unwrap();
    let calls = fake.calls.lock().unwrap().clone();
    assert_eq!(
        calls[0].1,
        vec![
            "plugin".to_string(),
            "uninstall".to_string(),
            "gdrive".to_string(),
            "--confirm".to_string(),
            "--keep-data".to_string()
        ]
    );
    assert!(calls[1].1.contains(&"--keep-data".to_string()));
    assert!(calls[1].1.contains(&"-y".to_string()));
}

#[test]
fn uninstall_omits_keep_data_when_user_deletes_data() {
    let (_dir, user_home, claude_home, grok_home, settings, config) = fixture();
    let fake = fake_cli(false, false, settings, config);
    let ctx = apply_ctx(user_home, claude_home, grok_home, &fake);
    uninstall_plugin_with(
        &ctx,
        AgentId::Grok,
        "gdrive",
        None,
        PluginUninstallOptions { keep_data: false },
    )
    .unwrap();
    let args = fake.calls.lock().unwrap()[0].1.clone();
    assert!(!args.contains(&"--keep-data".to_string()), "{args:?}");
    assert!(args.contains(&"--confirm".to_string()));
}

#[test]
fn preview_marketplace_pack_uses_available_components() {
    let (_dir, user_home, claude_home, grok_home, settings, config) = fixture();
    let fake = fake_cli(false, false, settings, config);
    let ctx = apply_ctx(user_home, claude_home, grok_home, &fake);
    let preview = preview_plugin_install_with(&ctx, AgentId::Grok, "superpowers").unwrap();
    assert_eq!(preview.name, "superpowers");
    assert!(preview.components.iter().any(|c| c.name == "tdd"));
}

#[test]
fn preview_git_source_stays_at_empty_components() {
    let (_dir, user_home, claude_home, grok_home, settings, config) = fixture();
    let fake = fake_cli(false, false, settings, config);
    let ctx = apply_ctx(user_home, claude_home, grok_home, &fake);
    let preview =
        preview_plugin_install_with(&ctx, AgentId::Grok, "https://github.com/acme/plugin.git")
            .unwrap();
    assert!(preview.components.is_empty());
    assert!(fake.calls.lock().unwrap().is_empty());
}

#[test]
fn pi_and_codex_cannot_install() {
    let (_dir, user_home, claude_home, grok_home, settings, config) = fixture();
    let fake = fake_cli(false, false, settings, config);
    let ctx = apply_ctx(user_home, claude_home, grok_home, &fake);
    for agent in [AgentId::Pi, AgentId::Codex] {
        let err = install_plugin_with(
            &ctx,
            agent,
            "anything",
            PluginInstallOptions { confirmed: true },
        )
        .unwrap_err();
        assert!(err.contains("Claude and Grok"), "{err}");
    }
    assert!(fake.calls.lock().unwrap().is_empty());
}
