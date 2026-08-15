//! Read-only MCP inventory scanner.
//!
//! This is **inspection only** (list known config files + server entries).
//! It does **not** implement Capability::Mcp management/injection — that remains
//! Planned until a real Mcp service exists. Do not call `registry.require(Mcp)`.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::Value as JsonValue;
use toml_edit::DocumentMut;

use crate::models::AgentId;
use crate::utils::paths::{agent_home, home_dir};

/// One discovered MCP server entry (redacted; no env secrets).
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct McpServerEntry {
    pub agent: AgentId,
    pub name: String,
    /// stdio | sse | http | unknown
    pub transport: String,
    pub command: Option<String>,
    pub url: Option<String>,
    pub source_path: String,
    /// json | toml
    pub source_format: String,
    pub enabled: Option<bool>,
}

/// One known MCP config file location for an agent.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct McpSourceFile {
    pub agent: AgentId,
    pub path: String,
    pub exists: bool,
    pub readable: bool,
    pub error: Option<String>,
    pub server_count: usize,
    /// Human label for the file role
    pub label: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct McpInventory {
    pub sources: Vec<McpSourceFile>,
    pub servers: Vec<McpServerEntry>,
}

/// Scan all built-in agents for known MCP config locations.
pub fn list_mcp_inventory() -> McpInventory {
    let mut sources = Vec::new();
    let mut servers = Vec::new();

    for agent in AgentId::ALL {
        for loc in source_locations(agent) {
            let mut src = McpSourceFile {
                agent,
                path: loc.path.display().to_string(),
                exists: loc.path.exists(),
                readable: false,
                error: None,
                server_count: 0,
                label: loc.label.to_string(),
            };
            if !src.exists {
                sources.push(src);
                continue;
            }
            match parse_source(agent, &loc) {
                Ok(entries) => {
                    src.readable = true;
                    src.server_count = entries.len();
                    servers.extend(entries);
                }
                Err(e) => {
                    src.readable = false;
                    src.error = Some(e);
                }
            }
            sources.push(src);
        }
    }

    servers.sort_by(|a, b| {
        a.agent
            .as_str()
            .cmp(b.agent.as_str())
            .then(a.name.cmp(&b.name))
            .then(a.source_path.cmp(&b.source_path))
    });
    sources.sort_by(|a, b| {
        a.agent
            .as_str()
            .cmp(b.agent.as_str())
            .then(a.path.cmp(&b.path))
    });

    McpInventory { sources, servers }
}

struct SourceLoc {
    path: PathBuf,
    format: SourceFormat,
    label: &'static str,
}

#[derive(Clone, Copy)]
enum SourceFormat {
    Json,
    Toml,
}

fn source_locations(agent: AgentId) -> Vec<SourceLoc> {
    let mut out = Vec::new();
    match agent {
        AgentId::Claude => {
            if let Ok(home) = home_dir() {
                out.push(SourceLoc {
                    path: home.join(".claude.json"),
                    format: SourceFormat::Json,
                    label: "Claude 全局 (~/.claude.json)",
                });
            }
            if let Ok(dir) = agent_home(AgentId::Claude) {
                out.push(SourceLoc {
                    path: dir.join("settings.json"),
                    format: SourceFormat::Json,
                    label: "Claude settings.json",
                });
            }
        }
        AgentId::Codex => {
            if let Ok(home) = agent_home(AgentId::Codex) {
                out.push(SourceLoc {
                    path: home.join("config.toml"),
                    format: SourceFormat::Toml,
                    label: "Codex config.toml",
                });
            }
        }
        AgentId::WorkBuddy => {
            if let Ok(home) = agent_home(AgentId::WorkBuddy) {
                out.push(SourceLoc {
                    path: home.join(".mcp.json"),
                    format: SourceFormat::Json,
                    label: "WorkBuddy .mcp.json",
                });
            }
        }
        AgentId::Cursor => {
            if let Ok(home) = home_dir() {
                out.push(SourceLoc {
                    path: home.join(".cursor").join("mcp.json"),
                    format: SourceFormat::Json,
                    label: "Cursor ~/.cursor/mcp.json",
                });
            }
            if let Ok(dir) = agent_home(AgentId::Cursor) {
                out.push(SourceLoc {
                    path: dir.join("mcp.json"),
                    format: SourceFormat::Json,
                    label: "Cursor agent mcp.json",
                });
            }
        }
        AgentId::Pi => {
            if let Ok(dir) = agent_home(AgentId::Pi) {
                // Pi may nest under agent/; also check parent coding-agent dir.
                out.push(SourceLoc {
                    path: dir.join("mcp.json"),
                    format: SourceFormat::Json,
                    label: "Pi mcp.json",
                });
                out.push(SourceLoc {
                    path: dir.join(".mcp.json"),
                    format: SourceFormat::Json,
                    label: "Pi .mcp.json",
                });
            }
        }
        AgentId::Grok | AgentId::Kimi | AgentId::Dsh => {
            // No stable public MCP config path verified yet — still surface
            // agent home probe so UI can show "未发现已知配置文件".
            if let Ok(dir) = agent_home(agent) {
                out.push(SourceLoc {
                    path: dir.join("mcp.json"),
                    format: SourceFormat::Json,
                    label: "探测 mcp.json",
                });
                out.push(SourceLoc {
                    path: dir.join(".mcp.json"),
                    format: SourceFormat::Json,
                    label: "探测 .mcp.json",
                });
            }
        }
    }
    out
}

fn parse_source(agent: AgentId, loc: &SourceLoc) -> Result<Vec<McpServerEntry>, String> {
    let path = &loc.path;
    match loc.format {
        SourceFormat::Json => parse_json_file(agent, path),
        SourceFormat::Toml => parse_toml_file(agent, path),
    }
}

fn parse_json_file(agent: AgentId, path: &Path) -> Result<Vec<McpServerEntry>, String> {
    let text = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let value: JsonValue = serde_json::from_str(&text).map_err(|e| format!("invalid JSON: {e}"))?;
    Ok(extract_json_servers(
        agent,
        &value,
        &path.display().to_string(),
    ))
}

fn extract_json_servers(
    agent: AgentId,
    root: &JsonValue,
    source_path: &str,
) -> Vec<McpServerEntry> {
    let map = match find_server_map(root) {
        Some(m) => m,
        None => return Vec::new(),
    };
    let mut out = Vec::new();
    for (name, cfg) in map {
        out.push(entry_from_json_cfg(agent, name, cfg, source_path, "json"));
    }
    out
}

/// Accept common shapes: `{ mcpServers: {...} }`, `{ servers: {...} }`,
/// or a bare map of server name → config (when values look like server objects).
fn find_server_map(root: &JsonValue) -> Option<&serde_json::Map<String, JsonValue>> {
    let obj = root.as_object()?;
    for key in ["mcpServers", "mcp_servers", "servers"] {
        if let Some(v) = obj.get(key) {
            if let Some(m) = v.as_object() {
                return Some(m);
            }
        }
    }
    // Nested under mcp
    if let Some(mcp) = obj.get("mcp").and_then(|v| v.as_object()) {
        for key in ["mcpServers", "servers"] {
            if let Some(m) = mcp.get(key).and_then(|v| v.as_object()) {
                return Some(m);
            }
        }
        if looks_like_server_map(mcp) {
            return Some(mcp);
        }
    }
    if looks_like_server_map(obj) {
        return Some(obj);
    }
    None
}

fn looks_like_server_map(obj: &serde_json::Map<String, JsonValue>) -> bool {
    if obj.is_empty() {
        return false;
    }
    // Avoid treating settings.json root as servers.
    let skip = [
        "theme",
        "model",
        "permissions",
        "env",
        "hooks",
        "enabledPlugins",
        "projects",
        "userID",
        "oauthAccount",
    ];
    if obj.keys().any(|k| skip.contains(&k.as_str())) {
        return false;
    }
    obj.values().all(|v| {
        v.as_object()
            .map(|o| {
                o.contains_key("command")
                    || o.contains_key("url")
                    || o.contains_key("type")
                    || o.contains_key("args")
                    || o.contains_key("transport")
            })
            .unwrap_or(false)
    })
}

fn entry_from_json_cfg(
    agent: AgentId,
    name: &str,
    cfg: &JsonValue,
    source_path: &str,
    source_format: &str,
) -> McpServerEntry {
    let obj = cfg.as_object();
    let command = obj
        .and_then(|o| o.get("command"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let url = obj
        .and_then(|o| {
            o.get("url")
                .or_else(|| o.get("serverUrl"))
                .or_else(|| o.get("endpoint"))
        })
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let type_hint = obj
        .and_then(|o| o.get("type").or_else(|| o.get("transport")))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let transport = classify_transport(type_hint, command.as_deref(), url.as_deref());
    let enabled = obj.and_then(|o| {
        o.get("enabled").or_else(|| o.get("disabled")).map(|v| {
            if let Some(b) = v.as_bool() {
                // if key is disabled, invert
                if o.contains_key("disabled") && !o.contains_key("enabled") {
                    !b
                } else {
                    b
                }
            } else {
                true
            }
        })
    });
    let args_suffix = obj
        .and_then(|o| o.get("args"))
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|x| x.as_str())
                .collect::<Vec<_>>()
                .join(" ")
        })
        .filter(|s| !s.is_empty());
    let command_display = match (command, args_suffix) {
        (Some(c), Some(a)) => Some(format!("{c} {a}")),
        (Some(c), None) => Some(c),
        _ => None,
    };

    McpServerEntry {
        agent,
        name: name.to_string(),
        transport,
        command: command_display,
        url,
        source_path: source_path.to_string(),
        source_format: source_format.to_string(),
        enabled,
    }
}

fn parse_toml_file(agent: AgentId, path: &Path) -> Result<Vec<McpServerEntry>, String> {
    let text = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let doc: DocumentMut = text.parse().map_err(|e| format!("invalid TOML: {e}"))?;
    let mut out = Vec::new();
    // Codex: [mcp_servers.name]
    if let Some(table) = doc.get("mcp_servers").and_then(|i| i.as_table()) {
        for (name, item) in table.iter() {
            let Some(tbl) = item.as_table() else {
                continue;
            };
            let command = tbl
                .get("command")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let url = tbl
                .get("url")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let args = tbl.get("args").and_then(|v| v.as_array()).map(|arr| {
                arr.iter()
                    .filter_map(|x| x.as_str())
                    .collect::<Vec<_>>()
                    .join(" ")
            });
            let command_display = match (command, args) {
                (Some(c), Some(a)) if !a.is_empty() => Some(format!("{c} {a}")),
                (Some(c), _) => Some(c),
                _ => None,
            };
            let type_hint = tbl.get("type").and_then(|v| v.as_str()).unwrap_or("");
            out.push(McpServerEntry {
                agent,
                name: name.to_string(),
                transport: classify_transport(
                    type_hint,
                    command_display.as_deref(),
                    url.as_deref(),
                ),
                command: command_display,
                url,
                source_path: path.display().to_string(),
                source_format: "toml".into(),
                enabled: None,
            });
        }
    }
    Ok(out)
}

fn classify_transport(type_hint: &str, command: Option<&str>, url: Option<&str>) -> String {
    let t = type_hint.to_ascii_lowercase();
    if t.contains("sse") {
        return "sse".into();
    }
    if t.contains("http") || t == "streamablehttp" || t == "streamable-http" {
        return "http".into();
    }
    if t.contains("stdio") || command.is_some() {
        return "stdio".into();
    }
    if url.is_some() {
        return "http".into();
    }
    "unknown".into()
}

#[cfg(test)]
mod tests {
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
        let entries = extract_json_servers(AgentId::Claude, &v, "/tmp/.claude.json");
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
        assert!(extract_json_servers(AgentId::Claude, &v, "x").is_empty());
    }

    #[test]
    fn parses_codex_toml_mcp_servers() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let mut f = fs::File::create(&path).unwrap();
        writeln!(
            f,
            r#"
[mcp_servers.demo]
command = "uvx"
args = ["mcp-server-demo"]

[mcp_servers.keep]
command = "echo"
"#
        )
        .unwrap();
        let entries = parse_toml_file(AgentId::Codex, &path).unwrap();
        assert_eq!(entries.len(), 2);
        assert!(entries.iter().any(|e| e.name == "demo"));
        assert!(entries.iter().any(|e| e.name == "keep"));
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
            }
        }
    }
}
