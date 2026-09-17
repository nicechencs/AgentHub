use super::*;
use std::fs;
use tempfile::tempdir;

fn stdio_spec(name: &str, command: &str, args: &[&str]) -> McpServerSpec {
    McpServerSpec {
        name: name.into(),
        transport: "stdio".into(),
        command: Some(command.into()),
        args: args.iter().map(|s| (*s).to_string()).collect(),
        url: None,
        enabled: Some(true),
    }
}

fn grok_table<'a>(doc: &'a toml_edit::DocumentMut, name: &str) -> &'a toml_edit::Table {
    doc.get("mcp_servers")
        .and_then(|item| item.as_table())
        .and_then(|servers| servers.get(name))
        .and_then(|item| item.as_table())
        .unwrap_or_else(|| panic!("missing mcp_servers.{name}"))
}

#[test]
fn catalog_lists_stdio_templates_for_writable_agents() {
    let rows = list_mcp_catalog();
    assert!(rows.iter().any(|r| r.id == "filesystem"));
    assert!(rows.iter().all(|r| r.transport == "stdio"));
    assert!(writable_mcp_agents().contains(&AgentId::Grok));
    for row in &rows {
        assert!(
            row.agents.iter().any(|id| id == "grok"),
            "{} missing grok",
            row.id
        );
    }
}

#[test]
fn grok_is_writable_kimi_is_not() {
    assert!(agent_supports_mcp_write(AgentId::Grok));
    assert!(ensure_writable(AgentId::Grok).is_ok());
    assert!(ensure_writable(AgentId::Kimi).is_err());
}

#[test]
fn json_upsert_and_disable_roundtrip() {
    let dir = tempdir().unwrap();
    let path = dir.path().join(".claude.json");
    let spec = stdio_spec(
        "memory",
        "npx",
        &["-y", "@modelcontextprotocol/server-memory"],
    );
    upsert_json_server(path.clone(), AgentId::Claude, &spec, "stdio", true).unwrap();
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.contains("mcpServers"));
    assert!(text.contains("memory"));
    set_json_enabled(path.clone(), AgentId::Claude, "memory", false).unwrap();
    let value: JsonValue = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(
        value["mcpServers"]["memory"]["enabled"],
        JsonValue::Bool(false)
    );
}

#[test]
fn rejects_bad_names() {
    assert!(validate_name("").is_err());
    assert!(validate_name("bad name").is_err());
    assert!(validate_name("ok_name-1").is_ok());
}

#[test]
fn parses_http_authority() {
    let (s, h, p) = parse_http_host_port("https://example.com/mcp").unwrap();
    assert_eq!((s.as_str(), h.as_str(), p), ("https", "example.com", 443));
    let (s, h, p) = parse_http_host_port("http://127.0.0.1:8080/x").unwrap();
    assert_eq!((s.as_str(), h.as_str(), p), ("http", "127.0.0.1", 8080));
}

#[test]
fn grok_stdio_upsert_preserves_env_and_other_tables() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("config.toml");
    fs::write(
        &path,
        r#"[models]
default = "grok"

[model."grok"]
model = "grok-4.6"

[mcp_servers.nx_mcp]
command = "old-python"
args = ["-m", "nx_mcp.server"]
enabled = true
startup_timeout_sec = 60

[mcp_servers.nx_mcp.env]
NX_MCP_WORKSPACE = "C:\\nx-workspace"
"#,
    )
    .unwrap();

    let spec = stdio_spec("nx_mcp", "python", &["-m", "nx_mcp.server"]);
    let result = write_grok_toml_at(&path, &spec, "stdio", true).unwrap();
    assert_eq!(result.agent, AgentId::Grok);
    assert!(result.enabled);

    let text = fs::read_to_string(&path).unwrap();
    assert!(text.contains("[models]"), "{text}");
    assert!(text.contains("grok-4.6"), "{text}");
    assert!(text.contains("NX_MCP_WORKSPACE"), "{text}");
    assert!(text.contains("startup_timeout_sec"), "{text}");
    assert!(text.contains("command = \"python\""), "{text}");
    assert!(!text.contains("old-python"), "{text}");

    let doc: DocumentMut = text.parse().unwrap();
    let table = grok_table(&doc, "nx_mcp");
    assert_eq!(table.get("enabled").and_then(|v| v.as_bool()), Some(true));
    let env = table
        .get("env")
        .and_then(|item| item.as_table())
        .expect("nested env table");
    assert_eq!(
        env.get("NX_MCP_WORKSPACE").and_then(|v| v.as_str()),
        Some(r"C:\nx-workspace")
    );
}

#[test]
fn grok_disable_sets_enabled_false_and_keeps_entry() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let spec = stdio_spec("nx_mcp", "python", &["-m", "nx_mcp.server"]);
    write_grok_toml_at(&path, &spec, "stdio", true).unwrap();
    let result = set_grok_enabled_at(&path, "nx_mcp", false).unwrap();
    assert!(!result.enabled);

    let text = fs::read_to_string(&path).unwrap();
    assert!(text.contains("[mcp_servers.nx_mcp]"), "{text}");
    assert!(text.contains("python"), "{text}");
    let doc: DocumentMut = text.parse().unwrap();
    assert_eq!(
        grok_table(&doc, "nx_mcp")
            .get("enabled")
            .and_then(|v| v.as_bool()),
        Some(false)
    );

    set_grok_enabled_at(&path, "nx_mcp", true).unwrap();
    let doc: DocumentMut = fs::read_to_string(&path).unwrap().parse().unwrap();
    assert_eq!(
        grok_table(&doc, "nx_mcp")
            .get("enabled")
            .and_then(|v| v.as_bool()),
        Some(true)
    );
}

#[test]
fn grok_http_upsert_and_disabled_create() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let spec = McpServerSpec {
        name: "remote".into(),
        transport: "http".into(),
        command: None,
        args: vec![],
        url: Some("https://mcp.example.invalid/api".into()),
        enabled: Some(false),
    };
    write_grok_toml_at(&path, &spec, "http", false).unwrap();
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.contains("https://mcp.example.invalid/api"), "{text}");
    assert!(!text.contains("command"), "{text}");
    let doc: DocumentMut = text.parse().unwrap();
    let table = grok_table(&doc, "remote");
    assert_eq!(table.get("enabled").and_then(|v| v.as_bool()), Some(false));
    assert_eq!(
        table.get("transport").and_then(|v| v.as_str()),
        Some("http")
    );
}

#[test]
fn grok_stdio_upsert_clears_url_when_switching_transport() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let http = McpServerSpec {
        name: "mixed".into(),
        transport: "http".into(),
        command: None,
        args: vec![],
        url: Some("https://mcp.example.invalid/api".into()),
        enabled: Some(true),
    };
    write_grok_toml_at(&path, &http, "http", true).unwrap();
    write_grok_toml_at(&path, &stdio_spec("mixed", "echo", &["ok"]), "stdio", true).unwrap();
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.contains("echo"), "{text}");
    assert!(!text.contains("https://mcp.example.invalid/api"), "{text}");
    assert!(!text.contains("transport"), "{text}");
}

#[test]
fn grok_enable_missing_server_errors() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("config.toml");
    fs::write(&path, "[models]\ndefault = \"grok\"\n").unwrap();
    let err = set_grok_enabled_at(&path, "missing", false).unwrap_err();
    assert!(err.to_string().contains("missing"), "{err}");
}

#[test]
fn grok_rejects_unknown_transport() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let spec = stdio_spec("demo", "echo", &[]);
    let err = write_grok_toml_at(&path, &spec, "unknown", true).unwrap_err();
    assert!(err.to_string().contains("unknown"), "{err}");
}

#[test]
fn codex_disable_removes_entry_and_keeps_neighbors() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("config.toml");
    write_codex_toml_at(&path, &stdio_spec("keep", "echo", &[]), "stdio", true).unwrap();
    write_codex_toml_at(&path, &stdio_spec("drop", "echo", &[]), "stdio", true).unwrap();
    set_codex_enabled_at(&path, "drop", false).unwrap();
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.contains("keep"), "{text}");
    assert!(!text.contains("drop"), "{text}");
    let err = write_codex_toml_at(
        &path,
        &McpServerSpec {
            name: "remote".into(),
            transport: "http".into(),
            command: None,
            args: vec![],
            url: Some("https://mcp.example.invalid/api".into()),
            enabled: Some(true),
        },
        "http",
        true,
    )
    .unwrap_err();
    assert!(err.to_string().contains("stdio"), "{err}");
}
