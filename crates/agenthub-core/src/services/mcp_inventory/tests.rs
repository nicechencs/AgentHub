use super::*;
use std::io::Write;
use tempfile::tempdir;

#[test]
fn extracts_claude_style_mcp_servers() {
    let v: JsonValue = serde_json::json!({
        "mcpServers": {
            "fs": { "command": "npx", "args": ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"] },
            "remote": { "type": "sse", "url": "https://example.com/sse" }
        }
    });
    let entries = parse_json_value(AgentId::Claude, &v, "/tmp/.claude.json").entries;
    assert_eq!(entries.len(), 2);
    let fs = entries.iter().find(|e| e.name == "fs").unwrap();
    assert_eq!(fs.transport, "stdio");
    assert!(fs.command.as_ref().unwrap().contains("npx"));
    let remote = entries.iter().find(|e| e.name == "remote").unwrap();
    assert_eq!(remote.transport, "sse");
    assert_eq!(remote.url.as_deref(), Some("https://example.com/sse"));
}

#[test]
fn ignores_settings_root_without_servers() {
    let v: JsonValue = serde_json::json!({
        "theme": "dark",
        "model": "opus",
        "permissions": {}
    });
    let parsed = parse_json_value(AgentId::Claude, &v, "x");
    assert!(parsed.entries.is_empty());
    assert!(parsed.snippet.is_none());
}

#[test]
fn json_source_snippet_is_only_mcp_section() {
    let v: JsonValue = serde_json::json!({
        "theme": "dark",
        "numStartups": 12,
        "mcpServers": {
            "fs": { "command": "npx", "args": ["-y", "demo"] }
        }
    });
    let parsed = parse_json_value(AgentId::Claude, &v, "/tmp/.claude.json");
    let snippet = parsed.snippet.expect("mcp section snippet");
    assert!(snippet.contains("mcpServers"), "{snippet}");
    assert!(snippet.contains("fs"), "{snippet}");
    assert!(!snippet.contains("theme"), "{snippet}");
    assert!(!snippet.contains("numStartups"), "{snippet}");
}

#[test]
fn json_server_snippet_is_only_that_server() {
    let v: JsonValue = serde_json::json!({
        "mcpServers": {
            "fs": { "command": "npx" },
            "docs": { "type": "sse", "url": "https://example.com/sse" }
        }
    });
    let parsed = parse_json_value(AgentId::Claude, &v, "x");
    let fs = parsed.entries.iter().find(|e| e.name == "fs").unwrap();
    let snippet = fs.snippet.as_deref().expect("server snippet");
    assert!(snippet.contains("fs"), "{snippet}");
    assert!(snippet.contains("npx"), "{snippet}");
    assert!(!snippet.contains("docs"), "{snippet}");
    assert!(!snippet.contains("example.com"), "{snippet}");
}

#[test]
fn json_snippet_keeps_env_and_secret_values() {
    let v: JsonValue = serde_json::json!({
        "mcpServers": {
            "gh": {
                "command": "npx",
                "env": { "GITHUB_TOKEN": "gho_secret", "DEBUG": "1" },
                "headers": { "Authorization": "Bearer secret-token" }
            }
        }
    });
    let parsed = parse_json_value(AgentId::Claude, &v, "x");
    let snippet = parsed.snippet.expect("snippet");
    assert!(snippet.contains("gho_secret"), "{snippet}");
    assert!(snippet.contains("secret-token"), "{snippet}");
    assert!(!snippet.contains("***"), "{snippet}");
    let server = parsed.entries[0].snippet.as_deref().unwrap();
    assert!(server.contains("gho_secret"), "{server}");
}

#[test]
fn nested_mcp_object_snippet_keeps_mcp_wrapper() {
    let v: JsonValue = serde_json::json!({
        "model": "opus",
        "mcp": {
            "servers": {
                "fs": { "command": "echo" }
            }
        }
    });
    let parsed = parse_json_value(AgentId::Claude, &v, "x");
    let snippet = parsed.snippet.expect("snippet");
    assert!(snippet.contains("\"mcp\""), "{snippet}");
    assert!(snippet.contains("servers"), "{snippet}");
    assert!(!snippet.contains("opus"), "{snippet}");
}

#[test]
fn parses_codex_toml_mcp_servers() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let mut f = fs::File::create(&path).unwrap();
    writeln!(
        f,
        r#"
model = "gpt"

[mcp_servers.demo]
command = "uvx"
args = ["mcp-server-demo"]

[mcp_servers.keep]
command = "echo"
"#
    )
    .unwrap();
    let parsed = parse_toml_file(AgentId::Codex, &path).unwrap();
    assert_eq!(parsed.entries.len(), 2);
    assert!(parsed.entries.iter().any(|e| e.name == "demo"));
    assert!(parsed.entries.iter().any(|e| e.name == "keep"));
    let snippet = parsed.snippet.expect("toml mcp snippet");
    assert!(snippet.contains("mcp_servers"), "{snippet}");
    assert!(snippet.contains("demo"), "{snippet}");
    assert!(!snippet.contains("gpt"), "{snippet}");
    let demo = parsed
        .entries
        .iter()
        .find(|e| e.name == "demo")
        .unwrap()
        .snippet
        .as_deref()
        .expect("demo snippet");
    assert!(demo.contains("demo"), "{demo}");
    assert!(demo.contains("uvx"), "{demo}");
    assert!(!demo.contains("keep"), "{demo}");
}

#[test]
fn toml_snippet_keeps_env_values() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let mut f = fs::File::create(&path).unwrap();
    writeln!(
        f,
        r#"
[mcp_servers.demo]
command = "uvx"
env = {{ API_KEY = "sk-secret", DEBUG = "1" }}
"#
    )
    .unwrap();
    let parsed = parse_toml_file(AgentId::Codex, &path).unwrap();
    let snippet = parsed.snippet.expect("snippet");
    assert!(snippet.contains("sk-secret"), "{snippet}");
    assert!(!snippet.contains("***"), "{snippet}");
}

#[test]
fn parses_grok_toml_mcp_servers_with_enabled_and_nested_env() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let mut f = fs::File::create(&path).unwrap();
    writeln!(
        f,
        r#"
[models]
default = "grok"

[mcp_servers.nx_mcp]
command = "C:\\tools\\python.exe"
args = ["-m", "nx_mcp.server"]
enabled = true

[mcp_servers.nx_mcp.env]
NX_MCP_WORKSPACE = "C:\\nx-workspace"

[mcp_servers.remote]
url = "https://mcp.example.invalid/api"
transport = "http"
enabled = false
"#
    )
    .unwrap();
    let parsed = parse_toml_file(AgentId::Grok, &path).unwrap();
    assert_eq!(parsed.entries.len(), 2);
    let nx = parsed.entries.iter().find(|e| e.name == "nx_mcp").unwrap();
    assert_eq!(nx.agent, AgentId::Grok);
    assert_eq!(nx.transport, "stdio");
    assert_eq!(nx.enabled, Some(true));
    assert_eq!(
        nx.command.as_deref(),
        Some(r"C:\tools\python.exe -m nx_mcp.server")
    );
    let snippet = parsed.snippet.expect("toml mcp snippet");
    assert!(snippet.contains("nx_mcp"), "{snippet}");
    assert!(snippet.contains("NX_MCP_WORKSPACE"), "{snippet}");
    assert!(!snippet.contains("grok"), "{snippet}");
    let remote = parsed.entries.iter().find(|e| e.name == "remote").unwrap();
    assert_eq!(remote.transport, "http");
    assert_eq!(remote.enabled, Some(false));
    assert_eq!(
        remote.url.as_deref(),
        Some("https://mcp.example.invalid/api")
    );
}

#[test]
fn toml_disabled_key_inverts_to_enabled_false() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let mut f = fs::File::create(&path).unwrap();
    writeln!(
        f,
        r#"
[mcp_servers.legacy]
command = "echo"
disabled = true
"#
    )
    .unwrap();
    let parsed = parse_toml_file(AgentId::Grok, &path).unwrap();
    assert_eq!(parsed.entries.len(), 1);
    assert_eq!(parsed.entries[0].enabled, Some(false));
}

#[test]
fn grok_source_locations_include_config_toml() {
    let locs = source_locations(AgentId::Grok);
    assert!(
        locs.iter()
            .any(|loc| { loc.path.ends_with("config.toml") && loc.label == "Grok config.toml" }),
        "expected Grok config.toml, got {:?}",
        locs.iter()
            .map(|loc| (loc.path.display().to_string(), loc.label))
            .collect::<Vec<_>>()
    );
    assert!(locs.iter().any(|loc| loc.label == "探测 mcp.json"));
    assert!(locs.iter().any(|loc| loc.label == "探测 .mcp.json"));
}

#[test]
fn cursor_source_locations_collapse_default_home_duplicate() {
    let locs = source_locations(AgentId::Cursor);
    assert_eq!(
        locs.len(),
        1,
        "~/.cursor/mcp.json and cursor-home/mcp.json are the same file"
    );
    let path = &locs[0].path;
    assert!(path.ends_with("mcp.json"), "{}", path.display());
    assert_eq!(locs[0].label, "Cursor ~/.cursor/mcp.json");
}

#[test]
fn source_locations_do_not_repeat_the_same_path() {
    for agent in AgentId::ALL {
        let locs = source_locations(agent);
        for (i, a) in locs.iter().enumerate() {
            for b in locs.iter().skip(i + 1) {
                assert!(
                    !same_source_path(&a.path, &b.path),
                    "{} listed {} twice",
                    agent.as_str(),
                    a.path.display()
                );
            }
        }
    }
}

#[test]
fn list_inventory_is_empty_in_isolated_tmpdir_homes() {
    // Smoke: function returns without panic; source list may be non-empty
    // (paths resolved) even when files are missing.
    let inv = list_mcp_inventory();
    assert!(!inv.sources.is_empty());
    for s in &inv.sources {
        if !s.exists {
            assert_eq!(s.server_count, 0);
            assert!(s.snippet.is_none());
        }
    }
}
