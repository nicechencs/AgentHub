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
        if self.mutate {
            let action = args.get(1).copied().unwrap_or("");
            let spec = args.get(2).copied().unwrap_or("");
            let enabled = action == "enable";
            if bin.contains("claude") {
                mutate_claude(&self.claude_settings, spec, enabled);
            } else if bin.contains("grok") {
                mutate_grok(&self.grok_config, spec, enabled);
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

fn fixture() -> (tempfile::TempDir, PathBuf, PathBuf, PathBuf, PathBuf) {
    let dir = tempdir().unwrap();
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
    (dir, claude_home, grok_home, settings, config)
}

#[test]
fn enable_disable_round_trip_with_fake_executor() {
    let (_dir, claude_home, grok_home, settings, config) = fixture();
    let fake = FakeCli {
        fail: false,
        mutate: true,
        claude_settings: settings.clone(),
        grok_config: config.clone(),
        calls: Mutex::new(Vec::new()),
    };
    let ctx = PluginApplyContext {
        claude_home,
        grok_home,
        claude_bin: Some(PathBuf::from("/usr/bin/claude")),
        grok_bin: Some(PathBuf::from("/usr/bin/grok")),
        runner: &fake,
    };

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
    let (_dir, claude_home, grok_home, settings, config) = fixture();
    let original_settings = fs::read(&settings).unwrap();
    let original_config = fs::read(&config).unwrap();
    let fake = FakeCli {
        fail: true,
        mutate: true,
        claude_settings: settings.clone(),
        grok_config: config.clone(),
        calls: Mutex::new(Vec::new()),
    };
    let ctx = PluginApplyContext {
        claude_home,
        grok_home,
        claude_bin: Some(PathBuf::from("/bin/claude")),
        grok_bin: Some(PathBuf::from("/bin/grok")),
        runner: &fake,
    };

    let err = enable_plugin_with(&ctx, AgentId::Claude, "pack", Some("official")).unwrap_err();
    assert!(err.contains("cli boom"), "{err}");
    assert_eq!(fs::read(&settings).unwrap(), original_settings);

    let err = disable_plugin_with(&ctx, AgentId::Grok, "gdrive", None).unwrap_err();
    assert!(err.contains("cli boom"), "{err}");
    assert_eq!(fs::read(&config).unwrap(), original_config);
}

#[test]
fn missing_cli_does_not_write() {
    let (_dir, claude_home, grok_home, settings, config) = fixture();
    let original_settings = fs::read(&settings).unwrap();
    let original_config = fs::read(&config).unwrap();
    let fake = FakeCli {
        fail: false,
        mutate: true,
        claude_settings: settings.clone(),
        grok_config: config.clone(),
        calls: Mutex::new(Vec::new()),
    };
    let ctx = PluginApplyContext {
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
    let (_dir, claude_home, grok_home, settings, config) = fixture();
    let original_settings = fs::read(&settings).unwrap();
    let fake = FakeCli {
        fail: false,
        mutate: true,
        claude_settings: settings.clone(),
        grok_config: config,
        calls: Mutex::new(Vec::new()),
    };
    let ctx = PluginApplyContext {
        claude_home,
        grok_home,
        claude_bin: Some(PathBuf::from("/bin/claude")),
        grok_bin: Some(PathBuf::from("/bin/grok")),
        runner: &fake,
    };
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
