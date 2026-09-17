//! MCP catalog → probe → write / enable for selected Agents.
//!
//! Product path mirrors Octop Connector custom MCP (list → probe → enable into
//! an Agent config), without OAuth, secret encryption, or a public gateway.

use std::fs;
use std::net::{TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::{Map as JsonMap, Value as JsonValue};
use toml_edit::{value, Array, DocumentMut, Item, Table};

use crate::error::{AppError, Result};
use crate::models::AgentId;
use crate::utils::atomic::atomic_write;
use crate::utils::paths::{agent_home, home_dir};

/// Built-in catalog row (local templates only — not a remote marketplace).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct McpCatalogEntry {
    pub id: String,
    pub name: String,
    pub title: String,
    pub description: String,
    pub transport: String,
    pub command: Option<String>,
    pub args: Vec<String>,
    pub url: Option<String>,
    pub agents: Vec<String>,
}

/// Probe / upsert request from the UI.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct McpServerSpec {
    pub name: String,
    #[serde(default = "default_transport")]
    pub transport: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

fn default_transport() -> String {
    "stdio".into()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct McpProbeResult {
    pub ok: bool,
    pub message: String,
    pub transport: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct McpWriteResult {
    pub agent: AgentId,
    pub name: String,
    pub path: String,
    pub enabled: bool,
}

/// Agents that accept MCP writes in this slice.
pub fn writable_mcp_agents() -> &'static [AgentId] {
    &[
        AgentId::Claude,
        AgentId::Codex,
        AgentId::Grok,
        AgentId::Cursor,
        AgentId::WorkBuddy,
    ]
}

pub fn agent_supports_mcp_write(agent: AgentId) -> bool {
    writable_mcp_agents().contains(&agent)
}

/// Local catalog — stdio templates only; no OAuth connectors.
pub fn list_mcp_catalog() -> Vec<McpCatalogEntry> {
    let agents: Vec<String> = writable_mcp_agents()
        .iter()
        .map(|a| a.as_str().to_string())
        .collect();
    vec![
        McpCatalogEntry {
            id: "filesystem".into(),
            name: "filesystem".into(),
            title: "Filesystem".into(),
            description: "读写本地目录（stdio；请把最后一个参数换成工作目录）".into(),
            transport: "stdio".into(),
            command: Some("npx".into()),
            args: vec![
                "-y".into(),
                "@modelcontextprotocol/server-filesystem".into(),
                ".".into(),
            ],
            url: None,
            agents: agents.clone(),
        },
        McpCatalogEntry {
            id: "memory".into(),
            name: "memory".into(),
            title: "Memory".into(),
            description: "进程内记忆（stdio）".into(),
            transport: "stdio".into(),
            command: Some("npx".into()),
            args: vec!["-y".into(), "@modelcontextprotocol/server-memory".into()],
            url: None,
            agents: agents.clone(),
        },
        McpCatalogEntry {
            id: "fetch".into(),
            name: "fetch".into(),
            title: "Fetch".into(),
            description: "拉取公开网页（stdio；不做登录）".into(),
            transport: "stdio".into(),
            command: Some("npx".into()),
            args: vec!["-y".into(), "@modelcontextprotocol/server-fetch".into()],
            url: None,
            agents,
        },
    ]
}

pub fn probe_mcp_server(spec: &McpServerSpec) -> Result<McpProbeResult> {
    validate_name(&spec.name)?;
    let transport =
        normalize_transport(&spec.transport, spec.command.as_deref(), spec.url.as_deref());
    match transport.as_str() {
        "stdio" => Ok(probe_stdio(spec)),
        "http" | "sse" => probe_http(spec, &transport),
        other => Ok(McpProbeResult {
            ok: false,
            message: format!("暂不支持探测传输「{other}」"),
            transport: other.to_string(),
            detail: None,
        }),
    }
}

pub fn upsert_mcp_server(agent: AgentId, spec: &McpServerSpec) -> Result<McpWriteResult> {
    ensure_writable(agent)?;
    validate_name(&spec.name)?;
    let enabled = spec.enabled.unwrap_or(true);
    let transport =
        normalize_transport(&spec.transport, spec.command.as_deref(), spec.url.as_deref());
    match agent {
        AgentId::Claude => {
            upsert_json_server(claude_primary_path()?, agent, spec, &transport, enabled)
        }
        AgentId::Cursor => {
            upsert_json_server(cursor_primary_path()?, agent, spec, &transport, enabled)
        }
        AgentId::WorkBuddy => {
            upsert_json_server(workbuddy_primary_path()?, agent, spec, &transport, enabled)
        }
        AgentId::Codex => write_codex_toml(spec, &transport, enabled),
        AgentId::Grok => write_grok_toml(spec, &transport, enabled),
        _ => unreachable!("ensure_writable"),
    }
}

pub fn set_mcp_server_enabled(
    agent: AgentId,
    name: &str,
    enabled: bool,
) -> Result<McpWriteResult> {
    ensure_writable(agent)?;
    validate_name(name)?;
    match agent {
        AgentId::Claude => set_json_enabled(claude_primary_path()?, agent, name, enabled),
        AgentId::Cursor => set_json_enabled(cursor_primary_path()?, agent, name, enabled),
        AgentId::WorkBuddy => set_json_enabled(workbuddy_primary_path()?, agent, name, enabled),
        AgentId::Codex => set_codex_enabled(name, enabled),
        AgentId::Grok => set_grok_enabled(name, enabled),
        _ => unreachable!("ensure_writable"),
    }
}

fn ensure_writable(agent: AgentId) -> Result<()> {
    if agent_supports_mcp_write(agent) {
        Ok(())
    } else {
        Err(AppError::message(
            "mcp.write.unsupported",
            format!("{} 本波还不支持写入 MCP", agent.as_str()),
        ))
    }
}

fn validate_name(name: &str) -> Result<()> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(AppError::message("mcp.name.empty", "请填写 MCP 名称"));
    }
    if !trimmed
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
    {
        return Err(AppError::message(
            "mcp.name.invalid",
            "名称只能用字母、数字、. _ -",
        ));
    }
    Ok(())
}

fn normalize_transport(hint: &str, command: Option<&str>, url: Option<&str>) -> String {
    let t = hint.trim().to_ascii_lowercase();
    if t.contains("sse") {
        return "sse".into();
    }
    if t.contains("http") || t == "streamablehttp" || t == "streamable-http" {
        return "http".into();
    }
    if t == "stdio" || command.is_some() {
        return "stdio".into();
    }
    if url.is_some() {
        return "http".into();
    }
    if t.is_empty() {
        "stdio".into()
    } else {
        t
    }
}

fn probe_stdio(spec: &McpServerSpec) -> McpProbeResult {
    let Some(command) = spec
        .command
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    else {
        return McpProbeResult {
            ok: false,
            message: "stdio 需要填写 command".into(),
            transport: "stdio".into(),
            detail: None,
        };
    };

    let path = Path::new(command);
    if path.is_absolute() || command.contains('/') || command.contains('\\') {
        if path.exists() {
            return McpProbeResult {
                ok: true,
                message: "已找到本地命令文件".into(),
                transport: "stdio".into(),
                detail: Some(path.display().to_string()),
            };
        }
        return McpProbeResult {
            ok: false,
            message: format!("找不到命令文件：{command}"),
            transport: "stdio".into(),
            detail: None,
        };
    }

    if command_on_path(command) {
        McpProbeResult {
            ok: true,
            message: format!("PATH 上找到「{command}」"),
            transport: "stdio".into(),
            detail: None,
        }
    } else {
        McpProbeResult {
            ok: false,
            message: format!("PATH 上找不到「{command}」（仍可写入，运行时由对方拉起）"),
            transport: "stdio".into(),
            detail: Some("not_on_path".into()),
        }
    }
}

fn command_on_path(command: &str) -> bool {
    #[cfg(windows)]
    {
        Command::new("where")
            .arg(command)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
    #[cfg(not(windows))]
    {
        Command::new("sh")
            .args([
                "-c",
                &format!("command -v {} >/dev/null 2>&1", shell_escape(command)),
            ])
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
}

fn shell_escape(s: &str) -> String {
    if s.chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/'))
    {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
}

fn probe_http(spec: &McpServerSpec, transport: &str) -> Result<McpProbeResult> {
    let url = spec
        .url
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| AppError::message("mcp.probe.url", "HTTP/SSE 需要填写 url"))?;

    let (scheme, host, port) = parse_http_host_port(url)?;
    if scheme != "http" && scheme != "https" {
        return Ok(McpProbeResult {
            ok: false,
            message: format!("只探测 http/https，当前是 {scheme}"),
            transport: transport.to_string(),
            detail: None,
        });
    }
    let addr = format!("{host}:{port}");
    let start = Instant::now();
    let mut addrs = addr.to_socket_addrs().map_err(|e| {
        AppError::message("mcp.probe.resolve", format!("无法解析主机：{e}"))
    })?;
    let sock = addrs
        .next()
        .ok_or_else(|| AppError::message("mcp.probe.resolve", "无法解析主机"))?;
    let ok = TcpStream::connect_timeout(&sock, Duration::from_secs(3)).is_ok();
    let ms = start.elapsed().as_millis();
    if ok {
        Ok(McpProbeResult {
            ok: true,
            message: format!("已连通 {host}:{port}（{ms}ms；未做登录）"),
            transport: transport.to_string(),
            detail: Some(url.to_string()),
        })
    } else {
        Ok(McpProbeResult {
            ok: false,
            message: format!("连不上 {host}:{port}"),
            transport: transport.to_string(),
            detail: Some(url.to_string()),
        })
    }
}

fn parse_http_host_port(url: &str) -> Result<(String, String, u16)> {
    let (scheme, after_scheme) = url
        .split_once("://")
        .map(|(scheme, rest)| (scheme.to_ascii_lowercase(), rest))
        .ok_or_else(|| {
            AppError::message("mcp.probe.url.invalid", "地址需要带 http:// 或 https://")
        })?;
    let authority = after_scheme
        .split(['/', '?', '#'])
        .next()
        .unwrap_or("")
        .trim();
    if authority.is_empty() {
        return Err(AppError::message("mcp.probe.host", "地址缺少主机名"));
    }
    let (host, port) = if let Some(host) = authority.strip_prefix('[') {
        let (h, rest) = host
            .split_once(']')
            .ok_or_else(|| AppError::message("mcp.probe.host", "IPv6 地址不完整"))?;
        let port = rest
            .strip_prefix(':')
            .map(|p| p.parse::<u16>())
            .transpose()
            .map_err(|_| AppError::message("mcp.probe.port", "端口无效"))?
            .unwrap_or(if scheme == "https" { 443 } else { 80 });
        (h.to_string(), port)
    } else if let Some((h, p)) = authority.rsplit_once(':') {
        if h.contains(':') {
            (
                authority.to_string(),
                if scheme == "https" { 443 } else { 80 },
            )
        } else {
            let port = p
                .parse::<u16>()
                .map_err(|_| AppError::message("mcp.probe.port", "端口无效"))?;
            (h.to_string(), port)
        }
    } else {
        (
            authority.to_string(),
            if scheme == "https" { 443 } else { 80 },
        )
    };
    Ok((scheme, host, port))
}

fn claude_primary_path() -> Result<PathBuf> {
    Ok(home_dir()?.join(".claude.json"))
}

fn cursor_primary_path() -> Result<PathBuf> {
    Ok(home_dir()?.join(".cursor").join("mcp.json"))
}

fn workbuddy_primary_path() -> Result<PathBuf> {
    Ok(agent_home(AgentId::WorkBuddy)?.join(".mcp.json"))
}

fn codex_primary_path() -> Result<PathBuf> {
    Ok(agent_home(AgentId::Codex)?.join("config.toml"))
}

fn grok_primary_path() -> Result<PathBuf> {
    Ok(agent_home(AgentId::Grok)?.join("config.toml"))
}

fn upsert_json_server(
    path: PathBuf,
    agent: AgentId,
    spec: &McpServerSpec,
    transport: &str,
    enabled: bool,
) -> Result<McpWriteResult> {
    let mut root = read_json_object(&path)?;
    let servers = json_servers_map_mut(&mut root)?;
    let mut body = JsonMap::new();
    match transport {
        "stdio" => {
            let command = spec
                .command
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .ok_or_else(|| AppError::message("mcp.write.command", "stdio 需要 command"))?;
            body.insert("command".into(), JsonValue::String(command.to_string()));
            if !spec.args.is_empty() {
                body.insert(
                    "args".into(),
                    JsonValue::Array(
                        spec.args
                            .iter()
                            .map(|a| JsonValue::String(a.clone()))
                            .collect(),
                    ),
                );
            }
        }
        "http" | "sse" => {
            let url = spec
                .url
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .ok_or_else(|| AppError::message("mcp.write.url", "HTTP/SSE 需要 url"))?;
            body.insert("url".into(), JsonValue::String(url.to_string()));
            body.insert(
                "type".into(),
                JsonValue::String(if transport == "sse" {
                    "sse".into()
                } else {
                    "http".into()
                }),
            );
        }
        other => {
            return Err(AppError::message(
                "mcp.write.transport",
                format!("暂不支持写入传输「{other}」"),
            ));
        }
    }
    body.insert("enabled".into(), JsonValue::Bool(enabled));
    servers.insert(spec.name.trim().to_string(), JsonValue::Object(body));
    write_json_pretty(&path, &JsonValue::Object(root))?;
    Ok(McpWriteResult {
        agent,
        name: spec.name.trim().to_string(),
        path: path.display().to_string(),
        enabled,
    })
}

fn set_json_enabled(
    path: PathBuf,
    agent: AgentId,
    name: &str,
    enabled: bool,
) -> Result<McpWriteResult> {
    let mut root = read_json_object(&path)?;
    let servers = json_servers_map_mut(&mut root)?;
    let entry = servers.get_mut(name).ok_or_else(|| {
        AppError::message("mcp.enable.missing", format!("配置里没有「{name}」"))
    })?;
    let obj = entry.as_object_mut().ok_or_else(|| {
        AppError::message(
            "mcp.enable.shape",
            format!("「{name}」不是对象，无法改启用状态"),
        )
    })?;
    obj.insert("enabled".into(), JsonValue::Bool(enabled));
    obj.remove("disabled");
    write_json_pretty(&path, &JsonValue::Object(root))?;
    Ok(McpWriteResult {
        agent,
        name: name.to_string(),
        path: path.display().to_string(),
        enabled,
    })
}

fn read_json_object(path: &Path) -> Result<JsonMap<String, JsonValue>> {
    if !path.exists() {
        return Ok(JsonMap::new());
    }
    let text = fs::read_to_string(path).map_err(|e| {
        AppError::message("mcp.read", format!("读不到 {}: {e}", path.display()))
    })?;
    if text.trim().is_empty() {
        return Ok(JsonMap::new());
    }
    let value: JsonValue = serde_json::from_str(&text).map_err(|e| {
        AppError::message(
            "mcp.read.json",
            format!("{} 不是合法 JSON：{e}", path.display()),
        )
    })?;
    match value {
        JsonValue::Object(map) => Ok(map),
        _ => Err(AppError::message(
            "mcp.read.shape",
            format!("{} 根节点必须是对象", path.display()),
        )),
    }
}

fn json_servers_map_mut(
    root: &mut JsonMap<String, JsonValue>,
) -> Result<&mut JsonMap<String, JsonValue>> {
    for key in ["mcpServers", "mcp_servers", "servers"] {
        if root.get(key).map(|v| v.is_object()).unwrap_or(false) {
            return Ok(root
                .get_mut(key)
                .and_then(|v| v.as_object_mut())
                .expect("checked object"));
        }
    }
    root.insert("mcpServers".into(), JsonValue::Object(JsonMap::new()));
    Ok(root
        .get_mut("mcpServers")
        .and_then(|v| v.as_object_mut())
        .expect("just inserted"))
}

fn write_json_pretty(path: &Path, value: &JsonValue) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let body = serde_json::to_string_pretty(value)?;
    let mut bytes = body.into_bytes();
    if !bytes.ends_with(b"\n") {
        bytes.push(b'\n');
    }
    atomic_write(path, &bytes)?;
    Ok(())
}

fn write_codex_toml(spec: &McpServerSpec, transport: &str, enabled: bool) -> Result<McpWriteResult> {
    write_codex_toml_at(&codex_primary_path()?, spec, transport, enabled)
}

fn write_codex_toml_at(
    path: &Path,
    spec: &McpServerSpec,
    transport: &str,
    enabled: bool,
) -> Result<McpWriteResult> {
    if !enabled {
        return set_codex_enabled_at(path, spec.name.trim(), false);
    }
    if transport != "stdio" {
        return Err(AppError::message(
            "mcp.write.codex.transport",
            "Codex 本波只写入 stdio MCP",
        ));
    }
    let command = spec
        .command
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| AppError::message("mcp.write.command", "stdio 需要 command"))?;

    let mut doc = read_toml_doc(path)?;
    let servers = toml_mcp_servers_mut(&mut doc, "mcp.write.codex.shape")?;
    let mut table = Table::new();
    table.insert("command", value(command));
    insert_toml_args(&mut table, &spec.args);
    servers.insert(spec.name.trim(), Item::Table(table));
    write_toml_doc(path, &doc)?;
    Ok(McpWriteResult {
        agent: AgentId::Codex,
        name: spec.name.trim().to_string(),
        path: path.display().to_string(),
        enabled: true,
    })
}

fn set_codex_enabled(name: &str, enabled: bool) -> Result<McpWriteResult> {
    set_codex_enabled_at(&codex_primary_path()?, name, enabled)
}

fn set_codex_enabled_at(path: &Path, name: &str, enabled: bool) -> Result<McpWriteResult> {
    if enabled {
        return Err(AppError::message(
            "mcp.enable.codex",
            "Codex 没有单独的启用开关；请重新写入该 MCP",
        ));
    }
    if !path.exists() {
        return Err(AppError::message(
            "mcp.enable.missing",
            format!("还没有 Codex 配置文件：{}", path.display()),
        ));
    }
    let mut doc = read_toml_doc(path)?;
    let Some(servers) = doc.get_mut("mcp_servers").and_then(|i| i.as_table_mut()) else {
        return Err(AppError::message(
            "mcp.enable.missing",
            format!("配置里没有「{name}」"),
        ));
    };
    if servers.remove(name).is_none() {
        return Err(AppError::message(
            "mcp.enable.missing",
            format!("配置里没有「{name}」"),
        ));
    }
    write_toml_doc(path, &doc)?;
    Ok(McpWriteResult {
        agent: AgentId::Codex,
        name: name.to_string(),
        path: path.display().to_string(),
        enabled: false,
    })
}

fn write_grok_toml(spec: &McpServerSpec, transport: &str, enabled: bool) -> Result<McpWriteResult> {
    write_grok_toml_at(&grok_primary_path()?, spec, transport, enabled)
}

fn write_grok_toml_at(
    path: &Path,
    spec: &McpServerSpec,
    transport: &str,
    enabled: bool,
) -> Result<McpWriteResult> {
    let name = spec.name.trim();
    let mut doc = read_toml_doc(path)?;
    let servers = toml_mcp_servers_mut(&mut doc, "mcp.write.grok.shape")?;
    if !servers.contains_key(name) {
        servers.insert(name, Item::Table(Table::new()));
    }
    let table = servers.get_mut(name).and_then(Item::as_table_mut).ok_or_else(|| {
        AppError::message("mcp.write.grok.shape", format!("「{name}」必须是表"))
    })?;
    apply_grok_server_fields(table, spec, transport, enabled)?;
    write_toml_doc(path, &doc)?;
    Ok(McpWriteResult {
        agent: AgentId::Grok,
        name: name.to_string(),
        path: path.display().to_string(),
        enabled,
    })
}

fn set_grok_enabled(name: &str, enabled: bool) -> Result<McpWriteResult> {
    set_grok_enabled_at(&grok_primary_path()?, name, enabled)
}

fn set_grok_enabled_at(path: &Path, name: &str, enabled: bool) -> Result<McpWriteResult> {
    if !path.exists() {
        return Err(AppError::message(
            "mcp.enable.missing",
            format!("还没有 Grok 配置文件：{}", path.display()),
        ));
    }
    let mut doc = read_toml_doc(path)?;
    let Some(servers) = doc.get_mut("mcp_servers").and_then(|i| i.as_table_mut()) else {
        return Err(AppError::message(
            "mcp.enable.missing",
            format!("配置里没有「{name}」"),
        ));
    };
    let Some(table) = servers.get_mut(name).and_then(Item::as_table_mut) else {
        return Err(AppError::message(
            "mcp.enable.missing",
            format!("配置里没有「{name}」"),
        ));
    };
    table.insert("enabled", value(enabled));
    table.remove("disabled");
    write_toml_doc(path, &doc)?;
    Ok(McpWriteResult {
        agent: AgentId::Grok,
        name: name.to_string(),
        path: path.display().to_string(),
        enabled,
    })
}

fn toml_mcp_servers_mut<'a>(
    doc: &'a mut DocumentMut,
    err_code: &'static str,
) -> Result<&'a mut Table> {
    let servers = doc
        .entry("mcp_servers")
        .or_insert(Item::Table(Table::new()))
        .as_table_mut()
        .ok_or_else(|| AppError::message(err_code, "mcp_servers 必须是表"))?;
    servers.set_implicit(true);
    Ok(servers)
}

fn insert_toml_args(table: &mut Table, args: &[String]) {
    if args.is_empty() {
        table.remove("args");
        return;
    }
    let mut arr = Array::new();
    for arg in args {
        arr.push(arg.as_str());
    }
    table.insert("args", Item::Value(toml_edit::Value::Array(arr)));
}

fn apply_grok_server_fields(
    table: &mut Table,
    spec: &McpServerSpec,
    transport: &str,
    enabled: bool,
) -> Result<()> {
    match transport {
        "stdio" => {
            let command = spec
                .command
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .ok_or_else(|| AppError::message("mcp.write.command", "stdio 需要 command"))?;
            table.insert("command", value(command));
            insert_toml_args(table, &spec.args);
            table.remove("url");
            table.remove("type");
            table.remove("transport");
        }
        "http" | "sse" => {
            let url = spec
                .url
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .ok_or_else(|| AppError::message("mcp.write.url", "HTTP/SSE 需要 url"))?;
            table.insert("url", value(url));
            table.insert("transport", value(transport));
            table.remove("command");
            table.remove("args");
            table.remove("type");
        }
        other => {
            return Err(AppError::message(
                "mcp.write.grok.transport",
                format!("暂不支持写入传输「{other}」"),
            ));
        }
    }
    table.insert("enabled", value(enabled));
    table.remove("disabled");
    Ok(())
}

fn read_toml_doc(path: &Path) -> Result<DocumentMut> {
    if !path.exists() {
        return Ok(DocumentMut::new());
    }
    let text = fs::read_to_string(path).map_err(|e| {
        AppError::message("mcp.read", format!("读不到 {}: {e}", path.display()))
    })?;
    text.parse().map_err(|e| {
        AppError::message(
            "mcp.read.toml",
            format!("{} 不是合法 TOML：{e}", path.display()),
        )
    })
}

fn write_toml_doc(path: &Path, doc: &DocumentMut) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut body = doc.to_string();
    if !body.ends_with('\n') {
        body.push('\n');
    }
    atomic_write(path, body.as_bytes())?;
    Ok(())
}

#[cfg(test)]
mod tests;
