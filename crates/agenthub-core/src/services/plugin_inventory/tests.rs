use super::*;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use tempfile::tempdir;

struct ScriptedCli {
    by_bin: HashMap<String, CliRun>,
}

impl PluginCliRunner for ScriptedCli {
    fn run_list_json(&self, program: &Path) -> CliRun {
        let key = program
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        self.by_bin.get(&key).cloned().unwrap_or(CliRun {
            stdout: String::new(),
            stderr: String::new(),
            exit_code: None,
            timed_out: false,
            spawn_error: Some("not found".into()),
        })
    }
}

fn ok_json(stdout: &str) -> CliRun {
    CliRun {
        stdout: stdout.to_string(),
        stderr: String::new(),
        exit_code: Some(0),
        timed_out: false,
        spawn_error: None,
    }
}

fn missing() -> CliRun {
    CliRun {
        stdout: String::new(),
        stderr: String::new(),
        exit_code: None,
        timed_out: false,
        spawn_error: Some("No such file or directory".into()),
    }
}

fn failed(stderr: &str) -> CliRun {
    CliRun {
        stdout: String::new(),
        stderr: stderr.to_string(),
        exit_code: Some(1),
        timed_out: false,
        spawn_error: None,
    }
}

fn ctx<'a>(
    claude_home: PathBuf,
    grok_home: PathBuf,
    user_home: PathBuf,
    runner: &'a ScriptedCli,
    claude_bin: Option<&'a str>,
    grok_bin: Option<&'a str>,
) -> PluginScanContext<'a> {
    PluginScanContext {
        user_home: user_home.clone(),
        claude_home,
        grok_home,
        pi_config: user_home.join(".pi").join("agent"),
        other_homes: Vec::new(),
        claude_bin: claude_bin.map(PathBuf::from),
        grok_bin: grok_bin.map(PathBuf::from),
        runner,
    }
}

#[test]
fn cli_json_lists_claude_and_grok_plugins() {
    let dir = tempdir().unwrap();
    let runner = ScriptedCli {
        by_bin: HashMap::from([
            (
                "claude".into(),
                ok_json(
                    r#"[{
                        "name": "demo@official",
                        "version": "1.2.0",
                        "enabled": true,
                        "scope": "user",
                        "path": "/home/me/.claude/plugins/cache/demo/1.2.0",
                        "components": { "skills": [{"name": "ship"}] }
                    }]"#,
                ),
            ),
            (
                "grok".into(),
                ok_json(
                    r#"[{
                        "status": "enabled",
                        "name": "gdrive",
                        "version": "0.4.0",
                        "marketplace": "xAI Official",
                        "path": "/home/me/.grok/plugins/gdrive",
                        "has_mcp": true,
                        "components": {
                            "mcp": [{"name": "gdrive"}]
                        }
                    }]"#,
                ),
            ),
        ]),
    };
    let inv = list_plugin_inventory_with(&ctx(
        dir.path().join("claude"),
        dir.path().join("grok"),
        PathBuf::from("/home/me"),
        &runner,
        Some("/usr/bin/claude"),
        Some("/usr/bin/grok"),
    ));
    assert_eq!(inv.plugins.len(), 2);
    assert!(inv
        .sources
        .iter()
        .any(|s| s.agent == AgentId::Cursor && s.source_kind == "mcp"));
    assert!(inv
        .sources
        .iter()
        .any(|s| s.agent == AgentId::Dsh && s.source_kind == "cordis"));
    let claude = inv
        .plugins
        .iter()
        .find(|p| p.agent == AgentId::Claude)
        .unwrap();
    assert_eq!(claude.name, "demo");
    assert_eq!(claude.marketplace.as_deref(), Some("official"));
    assert_eq!(claude.version.as_deref(), Some("1.2.0"));
    assert_eq!(claude.enabled, Some(true));
    assert_eq!(claude.source, "cli");
    assert_eq!(
        claude.path.as_deref(),
        Some("~/.claude/plugins/cache/demo/1.2.0")
    );
    assert!(claude
        .components
        .iter()
        .any(|c| c.kind == "skills" && c.name == "ship"));

    let grok = inv
        .plugins
        .iter()
        .find(|p| p.agent == AgentId::Grok)
        .unwrap();
    assert_eq!(grok.name, "gdrive");
    assert_eq!(grok.source, "cli");
    assert!(grok
        .components
        .iter()
        .any(|c| c.kind == "mcp" && c.name == "gdrive"));
    assert!(!inv.plugins.iter().any(|p| p.name == "mcpServers"));
}

#[test]
fn cli_available_rows_are_not_installed_plugins() {
    let items = parse_cli_plugin_list(
        AgentId::Grok,
        r#"[{"status":"available","name":"vercel","marketplace":"xAI Official"}]"#,
        Path::new("/home/me"),
    )
    .unwrap();
    assert!(items.is_empty());
}

#[test]
fn cli_mcp_servers_object_is_rejected() {
    let err = parse_cli_plugin_list(
        AgentId::Claude,
        r#"{"mcpServers":{"fs":{"command":"npx"}}}"#,
        Path::new("/home/me"),
    )
    .unwrap_err();
    assert!(err.contains("mcpServers"), "{err}");
}

#[test]
fn missing_cli_reads_live_files_and_skips_mcp_servers() {
    let dir = tempdir().unwrap();
    let claude = dir.path().join("claude");
    let grok = dir.path().join("grok");
    fs::create_dir_all(claude.join("plugins")).unwrap();
    fs::write(
        claude.join("settings.json"),
        r#"{
            "theme": "dark",
            "mcpServers": { "filesystem": { "command": "npx" } },
            "enabledPlugins": { "pack@official": true }
        }"#,
    )
    .unwrap();
    let pack_dir = claude.join("plugins").join("cache").join("pack").join("3.0.0");
    fs::create_dir_all(pack_dir.join("skills").join("ship")).unwrap();
    fs::write(
        pack_dir.join("plugin.json"),
        r#"{"name":"pack","version":"3.0.0"}"#,
    )
    .unwrap();
    fs::write(
        claude.join("plugins").join("installed_plugins.json"),
        r#"{
            "plugins": {
                "pack@official": {
                    "version": "3.0.0",
                    "installPath": "/home/me/.claude/plugins/cache/pack/3.0.0",
                    "scope": "user"
                }
            },
            "mcpServers": { "should-not-become-a-plugin": { "command": "npx" } }
        }"#,
    )
    .unwrap();

    fs::create_dir_all(
        grok.join("plugins")
            .join("gdrive")
            .join("skills")
            .join("search"),
    )
    .unwrap();
    fs::write(
        grok.join("plugins").join("gdrive").join("plugin.json"),
        r#"{"name":"gdrive","version":"0.1.0","description":"Drive"}"#,
    )
    .unwrap();
    fs::write(
        grok.join("plugins").join("gdrive").join(".mcp.json"),
        r#"{"mcpServers":{"gdrive":{"command":"npx"}}}"#,
    )
    .unwrap();
    fs::write(
        grok.join("config.toml"),
        "[plugins]\nenabled = [\"gdrive\"]\n\n[mcp_servers.docs]\ncommand = \"uvx\"\n",
    )
    .unwrap();

    let runner = ScriptedCli {
        by_bin: HashMap::new(),
    };
    let inv = list_plugin_inventory_with(&ctx(
        claude,
        grok,
        PathBuf::from("/home/me"),
        &runner,
        None,
        None,
    ));

    assert!(!inv
        .plugins
        .iter()
        .any(|p| p.name == "filesystem" || p.name == "docs" || p.name == "mcpServers"));
    let claude_plug = inv
        .plugins
        .iter()
        .find(|p| p.agent == AgentId::Claude)
        .unwrap();
    assert_eq!(claude_plug.name, "pack");
    assert_eq!(claude_plug.marketplace.as_deref(), Some("official"));
    assert_eq!(claude_plug.version.as_deref(), Some("3.0.0"));
    assert_eq!(claude_plug.enabled, Some(true));
    assert_eq!(claude_plug.source, "live");
    let claude_path = claude_plug.path.as_deref().unwrap().replace('\\', "/");
    assert!(
        claude_path.contains("plugins/cache/pack/3.0.0"),
        "{claude_path}"
    );

    let grok_plug = inv
        .plugins
        .iter()
        .find(|p| p.agent == AgentId::Grok)
        .unwrap();
    assert_eq!(grok_plug.name, "gdrive");
    assert_eq!(grok_plug.enabled, Some(true));
    assert!(grok_plug
        .components
        .iter()
        .any(|c| c.kind == "skills" && c.name == "search"));
    assert!(grok_plug
        .components
        .iter()
        .any(|c| c.kind == "mcp" && c.name == "gdrive"));

    let claude_st = inv
        .agents
        .iter()
        .find(|a| a.agent == AgentId::Claude)
        .unwrap();
    assert_eq!(claude_st.source.as_deref(), Some("live"));
}

#[test]
fn claude_enabled_without_cache_has_no_install_path() {
    let dir = tempdir().unwrap();
    let claude = dir.path().join("claude");
    fs::create_dir_all(claude.join("plugins")).unwrap();
    fs::write(
        claude.join("settings.json"),
        r#"{"enabledPlugins":{"ghost@official":true}}"#,
    )
    .unwrap();
    let runner = ScriptedCli {
        by_bin: HashMap::new(),
    };
    let inv = list_plugin_inventory_with(&ctx(
        claude,
        dir.path().join("grok"),
        PathBuf::from("/home/me"),
        &runner,
        None,
        None,
    ));
    let ghost = inv
        .plugins
        .iter()
        .find(|p| p.agent == AgentId::Claude && p.name == "ghost")
        .unwrap();
    assert!(ghost.path.is_none());
    assert_eq!(ghost.enabled, Some(true));
}

#[test]
fn grok_enabled_name_without_dir_is_listed() {
    let dir = tempdir().unwrap();
    let grok = dir.path().join("grok");
    fs::create_dir_all(grok.join("plugins")).unwrap();
    fs::write(
        grok.join("config.toml"),
        "[plugins]\nenabled = [\"missing-pack\"]\n",
    )
    .unwrap();
    let runner = ScriptedCli {
        by_bin: HashMap::new(),
    };
    let inv = list_plugin_inventory_with(&ctx(
        dir.path().join("claude"),
        grok,
        PathBuf::from("/home/me"),
        &runner,
        None,
        None,
    ));
    let missing = inv
        .plugins
        .iter()
        .find(|p| p.agent == AgentId::Grok && p.name == "missing-pack")
        .unwrap();
    assert_eq!(missing.enabled, Some(true));
    assert!(missing.path.is_none());
}

#[test]
fn missing_cli_and_empty_live_is_fail_closed_not_a_fake_list() {
    let dir = tempdir().unwrap();
    let runner = ScriptedCli {
        by_bin: HashMap::new(),
    };
    let inv = list_plugin_inventory_with(&ctx(
        dir.path().join("claude"),
        dir.path().join("grok"),
        dir.path().to_path_buf(),
        &runner,
        None,
        None,
    ));
    assert!(inv.plugins.is_empty());
    let claude = inv
        .agents
        .iter()
        .find(|a| a.agent == AgentId::Claude)
        .unwrap();
    assert_eq!(claude.plugin_count, 0);
    assert_eq!(claude.error_code.as_deref(), Some("cli-unavailable"));
    assert_eq!(claude.support, "listed");
}

#[test]
fn cli_nonzero_does_not_invent_live_rows() {
    let dir = tempdir().unwrap();
    let claude = dir.path().join("claude");
    fs::create_dir_all(claude.join("plugins")).unwrap();
    fs::write(
        claude.join("settings.json"),
        r#"{"enabledPlugins":{"ghost@official":true}}"#,
    )
    .unwrap();
    let runner = ScriptedCli {
        by_bin: HashMap::from([
            ("claude".into(), failed("boom")),
            ("grok".into(), missing()),
        ]),
    };
    let inv = list_plugin_inventory_with(&ctx(
        claude,
        dir.path().join("grok"),
        dir.path().to_path_buf(),
        &runner,
        Some("/bin/claude"),
        Some("/bin/grok"),
    ));
    assert!(!inv.plugins.iter().any(|p| p.agent == AgentId::Claude));
    let st = inv
        .agents
        .iter()
        .find(|a| a.agent == AgentId::Claude)
        .unwrap();
    assert_eq!(st.error_code.as_deref(), Some("cli-failed"));
    assert!(st.error.as_deref().unwrap_or("").contains("boom"));
}

#[test]
fn other_agents_are_not_listed_from_mcp() {
    let dir = tempdir().unwrap();
    let runner = ScriptedCli {
        by_bin: HashMap::new(),
    };
    let inv = list_plugin_inventory_with(&ctx(
        dir.path().join("claude"),
        dir.path().join("grok"),
        dir.path().to_path_buf(),
        &runner,
        None,
        None,
    ));
    let codex = inv
        .agents
        .iter()
        .find(|a| a.agent == AgentId::Codex)
        .unwrap();
    assert_eq!(codex.support, "planned");
    assert_eq!(codex.plugin_count, 0);
    let pi = inv.agents.iter().find(|a| a.agent == AgentId::Pi).unwrap();
    assert_eq!(pi.support, "listed");
    assert_eq!(pi.plugin_count, 0);
    for agent in [
        AgentId::Cursor,
        AgentId::Kimi,
        AgentId::WorkBuddy,
        AgentId::Dsh,
    ] {
        let st = inv.agents.iter().find(|a| a.agent == agent).unwrap();
        assert_eq!(st.support, "unsupported");
        assert_eq!(st.plugin_count, 0);
    }
    assert!(!inv
        .plugins
        .iter()
        .any(|p| !matches!(p.agent, AgentId::Claude | AgentId::Grok | AgentId::Pi)));
}

#[test]
fn pi_settings_packages_are_listed_from_live_files() {
    let dir = tempdir().unwrap();
    let user_home = dir.path().to_path_buf();
    let pi = user_home.join(".pi").join("agent");
    let pack = pi.join("npm").join("node_modules").join("pi-subagents");
    fs::create_dir_all(pack.join("skills").join("search")).unwrap();
    fs::create_dir_all(pack.join("agents")).unwrap();
    fs::write(pack.join("agents").join("delegate.md"), "# delegate\n").unwrap();
    fs::write(
        pack.join("package.json"),
        r#"{
            "name": "pi-subagents",
            "version": "0.64.0",
            "description": "Subagent workflows",
            "pi": { "extensions": ["./index.ts"], "skills": ["./skills"] }
        }"#,
    )
    .unwrap();
    fs::write(
        pi.join("settings.json"),
        r#"{
            "theme": "light",
            "mcpServers": { "should-not-become-a-plugin": { "command": "npx" } },
            "packages": [
                "npm:pi-subagents",
                { "source": "npm:@scope/other@1.2.3" },
                "git:github.com/user/repo@v1",
                "./local-ext",
                "../escape"
            ]
        }"#,
    )
    .unwrap();
    fs::create_dir_all(pi.join("local-ext").join("skills").join("ship")).unwrap();
    let git_pack = pi.join("git").join("github.com").join("user").join("repo");
    fs::create_dir_all(git_pack.join("skills").join("review")).unwrap();
    fs::write(
        git_pack.join("package.json"),
        r#"{"name":"repo","version":"9.0.0","description":"Git pack"}"#,
    )
    .unwrap();

    let runner = ScriptedCli {
        by_bin: HashMap::from([("pi".into(), failed("should not run plugin list"))]),
    };
    let inv = list_plugin_inventory_with(&ctx(
        dir.path().join("claude"),
        dir.path().join("grok"),
        user_home.clone(),
        &runner,
        None,
        None,
    ));

    let pi_st = inv.agents.iter().find(|a| a.agent == AgentId::Pi).unwrap();
    assert_eq!(pi_st.support, "listed");
    assert_eq!(pi_st.source.as_deref(), Some("live"));
    assert_eq!(pi_st.plugin_count, 4);
    assert!(!inv
        .plugins
        .iter()
        .any(|p| p.name == "should-not-become-a-plugin"
            || p.name == "mcpServers"
            || p.name.contains("escape")));

    let npm = inv
        .plugins
        .iter()
        .find(|p| p.agent == AgentId::Pi && p.name == "pi-subagents")
        .unwrap();
    assert_eq!(npm.marketplace.as_deref(), Some("npm"));
    assert_eq!(npm.version.as_deref(), Some("0.64.0"));
    assert_eq!(npm.requested_version, None);
    assert_eq!(npm.source, "live");
    assert_eq!(npm.scope.as_deref(), Some("user"));
    assert!(npm
        .path
        .as_deref()
        .unwrap()
        .contains(".pi/agent/npm/node_modules/pi-subagents"));
    assert!(npm
        .components
        .iter()
        .any(|c| c.kind == "skills" && c.name == "search"));
    assert!(npm
        .components
        .iter()
        .any(|c| c.kind == "agents" && c.name == "delegate"));

    let scoped = inv
        .plugins
        .iter()
        .find(|p| p.agent == AgentId::Pi && p.name == "@scope/other")
        .unwrap();
    assert_eq!(scoped.marketplace.as_deref(), Some("npm"));
    assert_eq!(scoped.version, None);
    assert_eq!(scoped.requested_version.as_deref(), Some("1.2.3"));
    assert!(scoped.path.is_none());

    let git = inv
        .plugins
        .iter()
        .find(|p| p.agent == AgentId::Pi && p.marketplace.as_deref() == Some("git"))
        .unwrap();
    assert_eq!(git.name, "repo");
    assert_eq!(git.version.as_deref(), Some("9.0.0"));
    assert_eq!(git.requested_version.as_deref(), Some("v1"));
    assert!(git
        .components
        .iter()
        .any(|c| c.kind == "skills" && c.name == "review"));

    let local = inv
        .plugins
        .iter()
        .find(|p| p.agent == AgentId::Pi && p.marketplace.as_deref() == Some("local"))
        .unwrap();
    assert_eq!(local.name, "local-ext");
    assert_eq!(local.requested_version, None);
    assert!(local
        .components
        .iter()
        .any(|c| c.kind == "skills" && c.name == "ship"));

    let settings_src = inv
        .sources
        .iter()
        .find(|s| s.agent == AgentId::Pi && s.source_kind == "config")
        .unwrap();
    assert!(settings_src.exists);
    assert_eq!(settings_src.item_count, 5);
}

#[test]
fn pi_pinned_npm_keeps_requested_version_when_installed_differs() {
    let dir = tempdir().unwrap();
    let user_home = dir.path().to_path_buf();
    let pi = user_home.join(".pi").join("agent");
    let pack = pi.join("npm").join("node_modules").join("pi-subagents");
    fs::create_dir_all(&pack).unwrap();
    fs::write(
        pack.join("package.json"),
        r#"{"name":"pi-subagents","version":"0.64.0"}"#,
    )
    .unwrap();
    fs::write(
        pi.join("settings.json"),
        r#"{"packages":["npm:pi-subagents@0.70.0"]}"#,
    )
    .unwrap();

    let runner = ScriptedCli {
        by_bin: HashMap::from([("pi".into(), failed("should not run plugin list"))]),
    };
    let inv = list_plugin_inventory_with(&ctx(
        dir.path().join("claude"),
        dir.path().join("grok"),
        user_home,
        &runner,
        None,
        None,
    ));

    let npm = inv
        .plugins
        .iter()
        .find(|p| p.agent == AgentId::Pi && p.name == "pi-subagents")
        .unwrap();
    assert_eq!(npm.version.as_deref(), Some("0.64.0"));
    assert_eq!(npm.requested_version.as_deref(), Some("0.70.0"));
    assert!(npm.path.is_some());
}

#[test]
fn claude_installed_object_and_empty_cli_array() {
    let rows = parse_cli_plugin_list(AgentId::Claude, "[]", Path::new("/tmp")).unwrap();
    assert!(rows.is_empty());
    let rows = parse_cli_plugin_list(
        AgentId::Claude,
        r#"{"installed":[{"name":"alpha","version":"2","enabled":false}]}"#,
        Path::new("/tmp"),
    )
    .unwrap();
    assert_eq!(rows[0].name, "alpha");
    assert_eq!(rows[0].enabled, Some(false));
}
