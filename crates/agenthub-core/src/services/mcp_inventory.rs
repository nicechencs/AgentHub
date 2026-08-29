//! Read-only MCP inventory scanner.
//!
//! This is **inspection only** (list known config files + server entries).
//! It does **not** implement Capability::Mcp management/injection — that remains
//! Planned until a real Mcp service exists. Do not call `registry.require(Mcp)`.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::{json, Map as JsonMap, Value as JsonValue};
use toml_edit::{value as toml_value, DocumentMut, Item, Table, Value as TomlValue};

use crate::models::AgentId;
use crate::utils::paths::{agent_home, home_dir};
use crate::utils::redact::is_secret_key;

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
    /// Redacted raw fragment for this server only (pretty JSON / TOML).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snippet: Option<String>,
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
    /// Redacted MCP-related section of the file (not the whole config).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snippet: Option<String>,
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
                snippet: None,
            };
            if !src.exists {
                sources.push(src);
                continue;
            }
            match parse_source(agent, &loc) {
                Ok(parsed) => {
                    src.readable = true;
                    src.server_count = parsed.entries.len();
                    src.snippet = parsed.snippet;
                    servers.extend(parsed.entries);
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
        AgentId::Grok | AgentId::Kimi | AgentId::Dsh | AgentId::Zcode => {
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

struct ParsedSource {
    snippet: Option<String>,
    entries: Vec<McpServerEntry>,
}

enum FoundServers<'a> {
    AtRootKey {
        key: &'static str,
        map: &'a JsonMap<String, JsonValue>,
    },
    UnderMcp {
        mcp: &'a JsonValue,
        map: &'a JsonMap<String, JsonValue>,
    },
    Bare {
        map: &'a JsonMap<String, JsonValue>,
    },
}

impl<'a> FoundServers<'a> {
    fn map(&self) -> &'a JsonMap<String, JsonValue> {
        match self {
            Self::AtRootKey { map, .. } | Self::UnderMcp { map, .. } | Self::Bare { map } => map,
        }
    }

    fn source_value(&self) -> JsonValue {
        match self {
            Self::AtRootKey { key, map } => json!({ *key: map }),
            Self::UnderMcp { mcp, .. } => json!({ "mcp": mcp }),
            Self::Bare { map } => JsonValue::Object((*map).clone()),
        }
    }
}

fn parse_source(agent: AgentId, loc: &SourceLoc) -> Result<ParsedSource, String> {
    let path = &loc.path;
    match loc.format {
        SourceFormat::Json => parse_json_file(agent, path),
        SourceFormat::Toml => parse_toml_file(agent, path),
    }
}

fn parse_json_file(agent: AgentId, path: &Path) -> Result<ParsedSource, String> {
    let text = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let value: JsonValue = serde_json::from_str(&text).map_err(|e| format!("invalid JSON: {e}"))?;
    Ok(parse_json_value(agent, &value, &path.display().to_string()))
}

fn parse_json_value(agent: AgentId, root: &JsonValue, source_path: &str) -> ParsedSource {
    let Some(found) = find_servers(root) else {
        return ParsedSource {
            snippet: None,
            entries: Vec::new(),
        };
    };
    let snippet = pretty_json(&redact_mcp_json(&found.source_value()));
    let entries = found
        .map()
        .iter()
        .map(|(name, cfg)| entry_from_json_cfg(agent, name, cfg, source_path, "json"))
        .collect();
    ParsedSource { snippet, entries }
}

/// Accept common shapes: `{ mcpServers: {...} }`, `{ servers: {...} }`,
/// or a bare map of server name → config (when values look like server objects).
fn find_servers(root: &JsonValue) -> Option<FoundServers<'_>> {
    let obj = root.as_object()?;
    for key in ["mcpServers", "mcp_servers", "servers"] {
        if let Some(m) = obj.get(key).and_then(|v| v.as_object()) {
            return Some(FoundServers::AtRootKey { key, map: m });
        }
    }
    if let Some(mcp_val) = obj.get("mcp") {
        if let Some(mcp) = mcp_val.as_object() {
            for key in ["mcpServers", "servers"] {
                if let Some(m) = mcp.get(key).and_then(|v| v.as_object()) {
                    return Some(FoundServers::UnderMcp {
                        mcp: mcp_val,
                        map: m,
                    });
                }
            }
            if looks_like_server_map(mcp) {
                return Some(FoundServers::UnderMcp {
                    mcp: mcp_val,
                    map: mcp,
                });
            }
        }
    }
    if looks_like_server_map(obj) {
        return Some(FoundServers::Bare { map: obj });
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

    let snippet = pretty_json(&redact_mcp_json(&json!({ name: cfg })));
    McpServerEntry {
        agent,
        name: name.to_string(),
        transport,
        command: command_display,
        url,
        source_path: source_path.to_string(),
        source_format: source_format.to_string(),
        enabled,
        snippet,
    }
}

fn parse_toml_file(agent: AgentId, path: &Path) -> Result<ParsedSource, String> {
    let text = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let doc: DocumentMut = text.parse().map_err(|e| format!("invalid TOML: {e}"))?;
    let Some(servers_item) = doc.get("mcp_servers") else {
        return Ok(ParsedSource {
            snippet: None,
            entries: Vec::new(),
        });
    };
    let snippet = toml_source_snippet(servers_item);
    let mut out = Vec::new();
    // Codex: [mcp_servers.name]
    if let Some(table) = servers_item.as_table() {
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
            let server_snippet = toml_server_snippet(name, item);
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
                snippet: server_snippet,
            });
        }
    }
    Ok(ParsedSource {
        snippet,
        entries: out,
    })
}

fn toml_source_snippet(servers_item: &Item) -> Option<String> {
    let mut snippet_doc = DocumentMut::new();
    snippet_doc.insert("mcp_servers", servers_item.clone());
    redact_toml_document(&mut snippet_doc);
    clip_snippet(snippet_doc.to_string())
}

fn toml_server_snippet(name: &str, item: &Item) -> Option<String> {
    let mut servers = Table::new();
    servers.set_implicit(true);
    servers.insert(name, item.clone());
    let mut snippet_doc = DocumentMut::new();
    snippet_doc.insert("mcp_servers", Item::Table(servers));
    redact_toml_document(&mut snippet_doc);
    clip_snippet(snippet_doc.to_string())
}

fn is_mcp_secret_bag(key: &str) -> bool {
    matches!(
        key.to_ascii_lowercase().as_str(),
        "env" | "headers" | "header" | "secrets"
    )
}

fn redact_mcp_json(value: &JsonValue) -> JsonValue {
    match value {
        JsonValue::Object(map) => {
            let mut out = JsonMap::new();
            for (k, child) in map {
                if is_secret_key(k) {
                    out.insert(k.clone(), JsonValue::String("***".into()));
                } else if is_mcp_secret_bag(k) {
                    out.insert(k.clone(), mask_json_string_leaves(child));
                } else {
                    out.insert(k.clone(), redact_mcp_json(child));
                }
            }
            JsonValue::Object(out)
        }
        JsonValue::Array(items) => JsonValue::Array(items.iter().map(redact_mcp_json).collect()),
        other => other.clone(),
    }
}

fn mask_json_string_leaves(value: &JsonValue) -> JsonValue {
    match value {
        JsonValue::Object(map) => {
            let mut out = JsonMap::new();
            for (k, child) in map {
                out.insert(k.clone(), mask_json_string_leaves(child));
            }
            JsonValue::Object(out)
        }
        JsonValue::Array(items) => {
            JsonValue::Array(items.iter().map(mask_json_string_leaves).collect())
        }
        JsonValue::String(_) => JsonValue::String("***".into()),
        other => other.clone(),
    }
}

fn pretty_json(value: &JsonValue) -> Option<String> {
    clip_snippet(serde_json::to_string_pretty(value).ok()?)
}

fn clip_snippet(s: String) -> Option<String> {
    const MAX: usize = 16 * 1024;
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return None;
    }
    if s.len() <= MAX {
        return Some(s);
    }
    let mut end = MAX;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    Some(format!("{}…\n(truncated)", &s[..end]))
}

fn redact_toml_document(doc: &mut DocumentMut) {
    let keys: Vec<String> = doc.iter().map(|(k, _)| k.to_string()).collect();
    for k in keys {
        if is_secret_key(&k) {
            doc[&k] = toml_value("***");
            continue;
        }
        if let Some(item) = doc.get_mut(&k) {
            if is_mcp_secret_bag(&k) {
                mask_toml_string_leaves(item);
            } else {
                redact_toml_item(item);
            }
        }
    }
}

fn redact_toml_item(item: &mut Item) {
    match item {
        Item::None => {}
        Item::Value(v) => redact_toml_value(v, false),
        Item::Table(t) => redact_toml_table(t, false),
        Item::ArrayOfTables(a) => {
            for t in a.iter_mut() {
                redact_toml_table(t, false);
            }
        }
    }
}

fn redact_toml_table(table: &mut Table, mask_all_strings: bool) {
    let keys: Vec<String> = table.iter().map(|(k, _)| k.to_string()).collect();
    for k in keys {
        if is_secret_key(&k) {
            table[&k] = toml_value("***");
            continue;
        }
        let bag = is_mcp_secret_bag(&k);
        match table.get_mut(&k) {
            Some(Item::Value(v)) => redact_toml_value(v, mask_all_strings || bag),
            Some(Item::Table(t)) => redact_toml_table(t, mask_all_strings || bag),
            Some(Item::ArrayOfTables(a)) => {
                for t in a.iter_mut() {
                    redact_toml_table(t, mask_all_strings || bag);
                }
            }
            _ => {}
        }
    }
}

fn redact_toml_value(val: &mut TomlValue, mask_all_strings: bool) {
    match val {
        TomlValue::String(_) if mask_all_strings => {
            *val = TomlValue::from("***");
        }
        TomlValue::InlineTable(t) => {
            let keys: Vec<String> = t
                .get_values()
                .into_iter()
                .filter_map(|(path, _)| path.first().map(|k| k.get().to_string()))
                .collect();
            for k in keys {
                let secret = is_secret_key(&k);
                let bag = is_mcp_secret_bag(&k);
                let is_string = t.get(&k).and_then(|v| v.as_str()).is_some();
                if secret || (mask_all_strings && is_string) {
                    t.insert(&k, TomlValue::from("***"));
                    continue;
                }
                if let Some(inner) = t.get_mut(&k) {
                    redact_toml_value(inner, mask_all_strings || bag);
                }
            }
        }
        TomlValue::Array(arr) => {
            for v in arr.iter_mut() {
                redact_toml_value(v, mask_all_strings);
            }
        }
        _ => {}
    }
}

fn mask_toml_string_leaves(item: &mut Item) {
    match item {
        Item::Value(v) => redact_toml_value(v, true),
        Item::Table(t) => redact_toml_table(t, true),
        Item::ArrayOfTables(a) => {
            for t in a.iter_mut() {
                redact_toml_table(t, true);
            }
        }
        Item::None => {}
    }
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
mod tests;
