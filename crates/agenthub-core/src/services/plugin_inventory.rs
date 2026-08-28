//! Read-only vendor plugin / extension pack inventory.
//!
//! Lists Claude and Grok **plugin packages**, not MCP servers. Prefer official
//! CLI `--json`; if the CLI is missing, read verified live files. Never treat
//! `mcpServers` as plugin rows. This does not install, enable, or write.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Serialize;
use serde_json::{Map as JsonMap, Value as JsonValue};
use toml_edit::DocumentMut;

use crate::models::AgentId;
use crate::utils::paths::{agent_home, home_dir};

const CLI_TIMEOUT: Duration = Duration::from_secs(15);
const CLI_ARGS: &[&str] = &["plugin", "list", "--json"];

/// One component inside a plugin pack (skill / command / agent / hook / MCP / LSP).
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PluginComponent {
    /// skills | commands | agents | hooks | mcp | lsp
    pub kind: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// One installed (or live-discovered) plugin / extension pack.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PluginEntry {
    pub id: String,
    pub agent: AgentId,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub marketplace: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trusted: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// cli | live
    pub source: String,
    pub components: Vec<PluginComponent>,
}

/// Per-agent list attempt (Claude/Grok) or a closed/planned cell.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PluginAgentStatus {
    pub agent: AgentId,
    /// listed | planned | unsupported
    pub support: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub plugin_count: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PluginInventory {
    pub agents: Vec<PluginAgentStatus>,
    pub plugins: Vec<PluginEntry>,
}

/// Injectable scan roots + CLI runner (tests override binaries and homes).
pub struct PluginScanContext<'a> {
    pub user_home: PathBuf,
    pub claude_home: PathBuf,
    pub grok_home: PathBuf,
    pub claude_bin: Option<PathBuf>,
    pub grok_bin: Option<PathBuf>,
    pub runner: &'a dyn PluginCliRunner,
}

/// Runs official `plugin` subcommands without AgentHub writing vendor cache.
pub trait PluginCliRunner: Send + Sync {
    fn run_list_json(&self, program: &Path) -> CliRun;
    fn run_plugin(&self, program: &Path, args: &[&str]) -> CliRun {
        run_cli(program, args, CLI_TIMEOUT)
    }
}

#[derive(Debug, Clone)]
pub struct CliRun {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    pub spawn_error: Option<String>,
}

impl CliRun {
    pub fn success(&self) -> bool {
        self.spawn_error.is_none() && !self.timed_out && self.exit_code == Some(0)
    }

    pub fn unavailable(&self) -> bool {
        self.spawn_error.is_some()
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct SystemPluginCliRunner;

impl PluginCliRunner for SystemPluginCliRunner {
    fn run_list_json(&self, program: &Path) -> CliRun {
        run_cli(program, CLI_ARGS, CLI_TIMEOUT)
    }
}

/// Scan Claude + Grok plugin packs using official CLI when present.
pub fn list_plugin_inventory() -> PluginInventory {
    let user_home = home_dir().unwrap_or_else(|_| PathBuf::from("/"));
    let claude_home = agent_home(AgentId::Claude).unwrap_or_else(|_| user_home.join(".claude"));
    let grok_home = agent_home(AgentId::Grok).unwrap_or_else(|_| user_home.join(".grok"));
    let ctx = PluginScanContext {
        user_home,
        claude_home,
        grok_home,
        claude_bin: which::which("claude").ok(),
        grok_bin: which::which("grok").ok(),
        runner: &SystemPluginCliRunner,
    };
    list_plugin_inventory_with(&ctx)
}

pub fn list_plugin_inventory_with(ctx: &PluginScanContext<'_>) -> PluginInventory {
    let mut agents = Vec::new();
    let mut plugins = Vec::new();

    for agent in AgentId::ALL {
        match agent {
            AgentId::Claude => {
                let (status, rows) = scan_wired_agent(
                    AgentId::Claude,
                    ctx.claude_bin.as_deref(),
                    ctx.runner,
                    &ctx.user_home,
                    || scan_claude_live(&ctx.claude_home, &ctx.user_home),
                );
                plugins.extend(rows);
                agents.push(status);
            }
            AgentId::Grok => {
                let (status, rows) = scan_wired_agent(
                    AgentId::Grok,
                    ctx.grok_bin.as_deref(),
                    ctx.runner,
                    &ctx.user_home,
                    || scan_grok_live(&ctx.grok_home, &ctx.user_home),
                );
                plugins.extend(rows);
                agents.push(status);
            }
            AgentId::Codex | AgentId::Pi => agents.push(PluginAgentStatus {
                agent,
                support: "planned".into(),
                source: None,
                error_code: Some("planned".into()),
                error: None,
                plugin_count: 0,
            }),
            AgentId::Cursor => agents.push(closed_status(agent, "unsupported-cursor")),
            AgentId::Dsh => agents.push(closed_status(agent, "unsupported-dsh")),
            AgentId::Kimi | AgentId::WorkBuddy => {
                agents.push(closed_status(agent, "unsupported-no-cli"));
            }
        }
    }

    plugins.sort_by(|a, b| {
        a.agent
            .as_str()
            .cmp(b.agent.as_str())
            .then(a.name.cmp(&b.name))
            .then(a.id.cmp(&b.id))
    });
    agents.sort_by(|a, b| a.agent.as_str().cmp(b.agent.as_str()));
    PluginInventory { agents, plugins }
}

fn closed_status(agent: AgentId, code: &str) -> PluginAgentStatus {
    PluginAgentStatus {
        agent,
        support: "unsupported".into(),
        source: None,
        error_code: Some(code.into()),
        error: None,
        plugin_count: 0,
    }
}

fn scan_wired_agent(
    agent: AgentId,
    bin: Option<&Path>,
    runner: &dyn PluginCliRunner,
    user_home: &Path,
    live: impl FnOnce() -> Result<Vec<PluginEntry>, String>,
) -> (PluginAgentStatus, Vec<PluginEntry>) {
    if let Some(program) = bin {
        let run = runner.run_list_json(program);
        if run.unavailable() {
            // CLI path present but not spawnable → same as missing: live fallback.
        } else if run.timed_out {
            return (
                fail_status(agent, "cli-failed", "plugin list timed out"),
                Vec::new(),
            );
        } else if !run.success() {
            let detail = first_line(&run.stderr)
                .unwrap_or_else(|| format!("exit {}", run.exit_code.unwrap_or(-1)));
            return (fail_status(agent, "cli-failed", &detail), Vec::new());
        } else {
            match parse_cli_plugin_list(agent, &run.stdout, user_home) {
                Ok(rows) => {
                    let count = rows.len();
                    return (
                        PluginAgentStatus {
                            agent,
                            support: "listed".into(),
                            source: Some("cli".into()),
                            error_code: None,
                            error: None,
                            plugin_count: count,
                        },
                        rows,
                    );
                }
                Err(e) => return (fail_status(agent, "cli-failed", &e), Vec::new()),
            }
        }
    }

    match live() {
        Ok(rows) => {
            let count = rows.len();
            let source = if count > 0 || live_dir_present(agent, user_home) {
                Some("live".into())
            } else {
                None
            };
            let (error_code, error) = if bin.is_none() && count == 0 {
                (Some("cli-unavailable".into()), None)
            } else {
                (None, None)
            };
            (
                PluginAgentStatus {
                    agent,
                    support: "listed".into(),
                    source,
                    error_code,
                    error,
                    plugin_count: count,
                },
                rows,
            )
        }
        Err(e) => {
            let code = if bin.is_none() {
                "cli-unavailable"
            } else {
                "live-unreadable"
            };
            (fail_status(agent, code, &e), Vec::new())
        }
    }
}

fn live_dir_present(agent: AgentId, _user_home: &Path) -> bool {
    // Presence is encoded by the live scanner returning rows or an empty
    // successful read; this helper stays for status source when homes exist.
    let _ = agent;
    true
}

fn fail_status(agent: AgentId, code: &str, error: &str) -> PluginAgentStatus {
    PluginAgentStatus {
        agent,
        support: "listed".into(),
        source: None,
        error_code: Some(code.into()),
        error: Some(error.to_string()),
        plugin_count: 0,
    }
}

fn run_cli(program: &Path, args: &[&str], timeout: Duration) -> CliRun {
    match crate::utils::process::run_capture_timeout(program, args, timeout) {
        Ok(output) => CliRun {
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            exit_code: output.status.code(),
            timed_out: false,
            spawn_error: None,
        },
        Err(e) if e.kind() == io::ErrorKind::TimedOut => CliRun {
            stdout: String::new(),
            stderr: String::new(),
            exit_code: None,
            timed_out: true,
            spawn_error: None,
        },
        Err(e) => CliRun {
            stdout: String::new(),
            stderr: String::new(),
            exit_code: None,
            timed_out: false,
            spawn_error: Some(e.to_string()),
        },
    }
}

fn first_line(s: &str) -> Option<String> {
    s.lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .map(ToString::to_string)
}

fn extract_json_value(raw: &str) -> Result<JsonValue, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(JsonValue::Array(vec![]));
    }
    let bytes = trimmed.as_bytes();
    let start = bytes
        .iter()
        .position(|b| *b == b'{' || *b == b'[')
        .ok_or_else(|| "plugin list output is not JSON".to_string())?;
    serde_json::from_str(&trimmed[start..]).map_err(|e| format!("plugin list JSON: {e}"))
}

/// Parse official `plugin list --json`. Skips marketplace-only (`available`) rows
/// and never promotes `mcpServers` maps into plugin entries.
pub fn parse_cli_plugin_list(
    agent: AgentId,
    stdout: &str,
    user_home: &Path,
) -> Result<Vec<PluginEntry>, String> {
    let value = extract_json_value(stdout)?;
    let items = plugin_json_items(&value)?;
    let mut out = Vec::new();
    for item in items {
        if item.get("mcpServers").is_some() && item.get("name").is_none() {
            continue;
        }
        if json_status(item) == Some("available") {
            continue;
        }
        if let Some(entry) = plugin_from_json(agent, item, "cli", user_home) {
            out.push(entry);
        }
    }
    Ok(out)
}

fn plugin_json_items(value: &JsonValue) -> Result<Vec<&JsonValue>, String> {
    match value {
        JsonValue::Array(arr) => {
            if arr
                .iter()
                .any(|v| v.get("mcpServers").is_some() && v.get("name").is_none())
            {
                return Err("refusing to treat mcpServers as plugin rows".into());
            }
            Ok(arr.iter().collect())
        }
        JsonValue::Object(map) => {
            if is_mcp_only_object(map) {
                return Err("refusing to treat mcpServers as plugin rows".into());
            }
            for key in ["installed", "plugins", "items"] {
                if let Some(JsonValue::Array(arr)) = map.get(key) {
                    return Ok(arr.iter().collect());
                }
            }
            if looks_like_plugin_object(map) {
                return Ok(vec![value]);
            }
            Ok(Vec::new())
        }
        JsonValue::Null => Ok(Vec::new()),
        _ => Err("plugin list JSON must be an array or object".into()),
    }
}

fn is_mcp_only_object(map: &JsonMap<String, JsonValue>) -> bool {
    map.contains_key("mcpServers")
        && !map.contains_key("name")
        && !map.contains_key("installed")
        && !map.contains_key("plugins")
}

fn looks_like_plugin_object(map: &JsonMap<String, JsonValue>) -> bool {
    map.contains_key("name") || map.contains_key("plugin") || map.contains_key("id")
}

fn json_status(item: &JsonValue) -> Option<&str> {
    item.get("status").and_then(JsonValue::as_str)
}

fn plugin_from_json(
    agent: AgentId,
    item: &JsonValue,
    source: &str,
    user_home: &Path,
) -> Option<PluginEntry> {
    let name = string_field(item, &["name", "plugin", "pluginName", "id"])?;
    if name.is_empty() || name == "mcpServers" {
        return None;
    }
    let (name, marketplace_from_id) = split_name_marketplace(&name);
    let marketplace = string_field(
        item,
        &["marketplace", "market", "sourceMarketplace", "source"],
    )
    .or(marketplace_from_id);
    let version = string_field(item, &["version"]);
    let scope = string_field(item, &["scope"]);
    let description = string_field(item, &["description"]);
    let path = string_field(item, &["path", "installPath", "directory", "location"])
        .map(|p| redact_home_path(&p, user_home));
    let enabled = bool_field(item, &["enabled", "isEnabled"]).or_else(|| match json_status(item) {
        Some("disabled") => Some(false),
        Some("enabled") | Some("installed") => Some(true),
        _ => None,
    });
    let trusted = bool_field(item, &["trusted", "isTrusted", "trust"]);
    let components = components_from_json(item);
    let id = plugin_id(agent, &name, marketplace.as_deref(), path.as_deref());
    Some(PluginEntry {
        id,
        agent,
        name,
        marketplace,
        version,
        scope,
        enabled,
        trusted,
        path,
        description,
        source: source.into(),
        components,
    })
}

fn string_field(item: &JsonValue, keys: &[&str]) -> Option<String> {
    for key in keys {
        match item.get(*key) {
            Some(JsonValue::String(s)) if !s.trim().is_empty() => {
                return Some(s.trim().to_string());
            }
            Some(JsonValue::Number(n)) => return Some(n.to_string()),
            _ => {}
        }
    }
    None
}

fn bool_field(item: &JsonValue, keys: &[&str]) -> Option<bool> {
    for key in keys {
        match item.get(*key) {
            Some(JsonValue::Bool(v)) => return Some(*v),
            Some(JsonValue::String(s)) if s.eq_ignore_ascii_case("true") => return Some(true),
            Some(JsonValue::String(s)) if s.eq_ignore_ascii_case("false") => return Some(false),
            _ => {}
        }
    }
    None
}

fn split_name_marketplace(raw: &str) -> (String, Option<String>) {
    if let Some((name, market)) = raw.split_once('@') {
        if !name.is_empty() && !market.is_empty() {
            return (name.to_string(), Some(market.to_string()));
        }
    }
    (raw.to_string(), None)
}

fn plugin_id(agent: AgentId, name: &str, marketplace: Option<&str>, path: Option<&str>) -> String {
    let mut id = match marketplace {
        Some(m) => format!("{}:{}@{}", agent.as_str(), name, m),
        None => format!("{}:{}", agent.as_str(), name),
    };
    if let Some(p) = path {
        id.push('#');
        id.push_str(p);
    }
    id
}

fn components_from_json(item: &JsonValue) -> Vec<PluginComponent> {
    let mut out = Vec::new();
    if let Some(components) = item.get("components") {
        match components {
            JsonValue::Object(map) => {
                for (kind, value) in map {
                    push_component_value(&mut out, normalize_component_kind(kind), value);
                }
            }
            JsonValue::Array(arr) => {
                for value in arr {
                    let kind = string_field(value, &["kind", "type", "category"])
                        .unwrap_or_else(|| "skills".into());
                    push_component_value(&mut out, normalize_component_kind(&kind), value);
                }
            }
            _ => {}
        }
    }
    for (kind, key) in [
        ("skills", "skills"),
        ("commands", "commands"),
        ("agents", "agents"),
        ("hooks", "hooks"),
        ("mcp", "mcp"),
        ("mcp", "mcpServers"),
        ("lsp", "lsp"),
        ("lsp", "lspServers"),
    ] {
        if item.get("components").is_some() && key != "mcpServers" {
            continue;
        }
        if let Some(value) = item.get(key) {
            if key == "mcpServers" && value.is_object() {
                push_mcp_map(&mut out, value);
            } else {
                push_component_value(&mut out, kind, value);
            }
        }
    }
    out.sort_by(|a, b| a.kind.cmp(&b.kind).then(a.name.cmp(&b.name)));
    out.dedup();
    out
}

fn normalize_component_kind(kind: &str) -> &'static str {
    match kind.trim().to_ascii_lowercase().as_str() {
        "skill" | "skills" => "skills",
        "command" | "commands" | "slash" | "slashcommands" => "commands",
        "agent" | "agents" | "subagent" | "subagents" => "agents",
        "hook" | "hooks" => "hooks",
        "mcp" | "mcpservers" | "mcp_servers" | "mcp-servers" => "mcp",
        "lsp" | "lspservers" | "lsp_servers" => "lsp",
        _ => "skills",
    }
}

fn push_component_value(out: &mut Vec<PluginComponent>, kind: &str, value: &JsonValue) {
    match value {
        JsonValue::Array(arr) => {
            for item in arr {
                push_one_component(out, kind, item);
            }
        }
        JsonValue::Object(map) => {
            if map.contains_key("name") || map.contains_key("id") {
                push_one_component(out, kind, value);
            } else {
                for (name, inner) in map {
                    let description = inner
                        .get("description")
                        .and_then(JsonValue::as_str)
                        .map(str::to_string);
                    out.push(PluginComponent {
                        kind: kind.into(),
                        name: name.clone(),
                        description,
                    });
                }
            }
        }
        JsonValue::String(name) if !name.is_empty() => out.push(PluginComponent {
            kind: kind.into(),
            name: name.clone(),
            description: None,
        }),
        JsonValue::Bool(true) => out.push(PluginComponent {
            kind: kind.into(),
            name: kind.into(),
            description: None,
        }),
        _ => {}
    }
}

fn push_one_component(out: &mut Vec<PluginComponent>, kind: &str, item: &JsonValue) {
    let name = match item {
        JsonValue::String(s) => s.trim().to_string(),
        JsonValue::Object(_) => string_field(item, &["name", "id", "command"]).unwrap_or_default(),
        _ => String::new(),
    };
    if name.is_empty() {
        return;
    }
    out.push(PluginComponent {
        kind: kind.into(),
        name,
        description: string_field(item, &["description"]),
    });
}

fn push_mcp_map(out: &mut Vec<PluginComponent>, value: &JsonValue) {
    if let JsonValue::Object(map) = value {
        for name in map.keys() {
            out.push(PluginComponent {
                kind: "mcp".into(),
                name: name.clone(),
                description: None,
            });
        }
    }
}

fn redact_home_path(path: &str, user_home: &Path) -> String {
    let home = user_home.to_string_lossy();
    let normalized = path.replace('\\', "/");
    let home_norm = home.replace('\\', "/");
    if let Some(rest) = normalized.strip_prefix(&home_norm) {
        if rest.is_empty() {
            return "~".into();
        }
        if rest.starts_with('/') {
            return format!("~{rest}");
        }
    }
    path.to_string()
}

fn scan_claude_live(claude_home: &Path, user_home: &Path) -> Result<Vec<PluginEntry>, String> {
    let mut by_key: JsonMap<String, JsonValue> = JsonMap::new();

    let installed_path = claude_home.join("plugins").join("installed_plugins.json");
    if installed_path.is_file() {
        let text = fs::read_to_string(&installed_path).map_err(|e| e.to_string())?;
        merge_installed_plugins(&mut by_key, &text)?;
    }

    let settings_path = claude_home.join("settings.json");
    if settings_path.is_file() {
        let text = fs::read_to_string(&settings_path).map_err(|e| e.to_string())?;
        merge_enabled_plugins(&mut by_key, &text)?;
    }

    let mut rows = Vec::new();
    for (key, value) in &by_key {
        if key == "mcpServers" {
            continue;
        }
        let mut item = value.clone();
        if item.get("name").is_none() {
            if let JsonValue::Object(map) = &mut item {
                let (name, marketplace) = split_name_marketplace(key);
                map.insert("name".into(), JsonValue::String(name));
                if let Some(m) = marketplace {
                    map.entry("marketplace".to_string())
                        .or_insert(JsonValue::String(m));
                }
            }
        }
        if let Some(mut entry) = plugin_from_json(AgentId::Claude, &item, "live", user_home) {
            if entry.path.is_none() {
                if let Some(p) = item.get("installPath").and_then(JsonValue::as_str) {
                    entry.path = Some(redact_home_path(p, user_home));
                }
            }
            if let Some(path) = item
                .get("installPath")
                .and_then(JsonValue::as_str)
                .map(PathBuf::from)
            {
                if entry.components.is_empty() && path.is_dir() {
                    entry.components = discover_components(&path);
                }
            }
            rows.push(entry);
        }
    }

    if rows.is_empty() {
        rows.extend(scan_plugin_tree(
            AgentId::Claude,
            &claude_home.join("plugins"),
            user_home,
            &["cache", "data", "marketplaces"],
        ));
    }
    Ok(rows)
}

fn merge_installed_plugins(
    dest: &mut JsonMap<String, JsonValue>,
    text: &str,
) -> Result<(), String> {
    let value: JsonValue = serde_json::from_str(text).map_err(|e| e.to_string())?;
    if let JsonValue::Object(map) = &value {
        if is_mcp_only_object(map) {
            return Ok(());
        }
        if let Some(plugins) = map.get("plugins").or_else(|| map.get("installedPlugins")) {
            merge_plugin_collection(dest, plugins);
            return Ok(());
        }
        if looks_like_plugin_object(map) {
            let key = string_field(&value, &["name", "id"]).unwrap_or_else(|| "plugin".into());
            dest.insert(key, value);
            return Ok(());
        }
        // Map keyed by plugin@marketplace
        if map.values().any(|v| v.is_object()) && !map.contains_key("mcpServers") {
            for (k, v) in map {
                if k == "mcpServers" {
                    continue;
                }
                dest.insert(k.clone(), v.clone());
            }
        }
        return Ok(());
    }
    if let JsonValue::Array(arr) = &value {
        merge_plugin_collection(dest, &JsonValue::Array(arr.clone()));
    }
    Ok(())
}

fn merge_plugin_collection(dest: &mut JsonMap<String, JsonValue>, plugins: &JsonValue) {
    match plugins {
        JsonValue::Array(arr) => {
            for item in arr {
                if item.get("mcpServers").is_some() && item.get("name").is_none() {
                    continue;
                }
                let key = string_field(item, &["id", "name"]).unwrap_or_else(|| item.to_string());
                if key == "mcpServers" {
                    continue;
                }
                dest.insert(key, item.clone());
            }
        }
        JsonValue::Object(map) => {
            for (k, v) in map {
                if k == "mcpServers" {
                    continue;
                }
                dest.insert(k.clone(), v.clone());
            }
        }
        _ => {}
    }
}

fn merge_enabled_plugins(dest: &mut JsonMap<String, JsonValue>, text: &str) -> Result<(), String> {
    let value: JsonValue = serde_json::from_str(text).map_err(|e| e.to_string())?;
    let Some(enabled) = value.get("enabledPlugins") else {
        return Ok(());
    };
    match enabled {
        JsonValue::Object(map) => {
            for (key, flag) in map {
                if key == "mcpServers" {
                    continue;
                }
                let enabled = match flag {
                    JsonValue::Bool(v) => *v,
                    JsonValue::String(s) => s.eq_ignore_ascii_case("true"),
                    _ => true,
                };
                let slot = dest.entry(key.clone()).or_insert_with(|| {
                    let (name, marketplace) = split_name_marketplace(key);
                    let mut obj = JsonMap::new();
                    obj.insert("name".into(), JsonValue::String(name));
                    if let Some(m) = marketplace {
                        obj.insert("marketplace".into(), JsonValue::String(m));
                    }
                    JsonValue::Object(obj)
                });
                if let JsonValue::Object(obj) = slot {
                    obj.insert("enabled".into(), JsonValue::Bool(enabled));
                    obj.entry("scope".to_string())
                        .or_insert(JsonValue::String("user".into()));
                }
            }
        }
        JsonValue::Array(arr) => {
            for item in arr {
                let key = match item {
                    JsonValue::String(s) => s.clone(),
                    JsonValue::Object(_) => string_field(item, &["name", "id"]).unwrap_or_default(),
                    _ => String::new(),
                };
                if key.is_empty() || key == "mcpServers" {
                    continue;
                }
                let slot = dest.entry(key.clone()).or_insert_with(|| {
                    let (name, marketplace) = split_name_marketplace(&key);
                    let mut obj = JsonMap::new();
                    obj.insert("name".into(), JsonValue::String(name));
                    if let Some(m) = marketplace {
                        obj.insert("marketplace".into(), JsonValue::String(m));
                    }
                    JsonValue::Object(obj)
                });
                if let JsonValue::Object(obj) = slot {
                    obj.insert("enabled".into(), JsonValue::Bool(true));
                }
            }
        }
        _ => {}
    }
    Ok(())
}

fn scan_grok_live(grok_home: &Path, user_home: &Path) -> Result<Vec<PluginEntry>, String> {
    let mut rows = scan_plugin_tree(
        AgentId::Grok,
        &grok_home.join("plugins"),
        user_home,
        &["data", "cache", "marketplaces"],
    );
    let enabled = grok_enabled_names(&grok_home.join("config.toml"));
    let disabled = grok_disabled_names(&grok_home.join("config.toml"));
    for row in &mut rows {
        if disabled
            .iter()
            .any(|n| names_match(n, &row.name, row.marketplace.as_deref()))
        {
            row.enabled = Some(false);
        } else if enabled
            .iter()
            .any(|n| names_match(n, &row.name, row.marketplace.as_deref()))
        {
            row.enabled = Some(true);
        } else if row.enabled.is_none() {
            row.enabled = Some(false);
        }
        if row.trusted.is_none() {
            row.trusted = Some(true);
        }
        if row.scope.is_none() {
            row.scope = Some("user".into());
        }
    }
    Ok(rows)
}

fn names_match(listed: &str, name: &str, marketplace: Option<&str>) -> bool {
    if listed == name {
        return true;
    }
    let (listed_name, _) = split_name_marketplace(listed);
    if listed_name == name {
        return true;
    }
    if let Some(market) = marketplace {
        if listed == format!("{name}@{market}") {
            return true;
        }
    }
    listed.rsplit('/').next() == Some(name)
}

fn grok_enabled_names(config: &Path) -> Vec<String> {
    grok_plugin_list_field(config, "enabled")
}

fn grok_disabled_names(config: &Path) -> Vec<String> {
    grok_plugin_list_field(config, "disabled")
}

fn grok_plugin_list_field(config: &Path, field: &str) -> Vec<String> {
    let Ok(text) = fs::read_to_string(config) else {
        return Vec::new();
    };
    let Ok(doc) = text.parse::<DocumentMut>() else {
        return Vec::new();
    };
    let Some(arr) = doc
        .get("plugins")
        .and_then(|v| v.get(field))
        .and_then(|v| v.as_array())
    else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|v| v.as_str().map(str::to_string))
        .collect()
}

fn scan_plugin_tree(
    agent: AgentId,
    root: &Path,
    user_home: &Path,
    skip_names: &[&str],
) -> Vec<PluginEntry> {
    let mut rows = Vec::new();
    let Ok(entries) = fs::read_dir(root) else {
        return rows;
    };
    for ent in entries.flatten() {
        let path = ent.path();
        let name = ent.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') || skip_names.iter().any(|s| name.eq_ignore_ascii_case(s)) {
            continue;
        }
        if !path.is_dir() {
            continue;
        }
        if path.join("plugin.json").is_file() || looks_like_plugin_dir(&path) {
            if let Some(row) = plugin_from_dir(agent, &path, user_home) {
                rows.push(row);
            }
            continue;
        }
        // e.g. user/<hash>/<name>
        rows.extend(scan_plugin_tree(agent, &path, user_home, skip_names));
    }
    rows
}

fn looks_like_plugin_dir(path: &Path) -> bool {
    path.join("skills").is_dir()
        || path.join("commands").is_dir()
        || path.join("agents").is_dir()
        || path.join("hooks").join("hooks.json").is_file()
        || path.join(".mcp.json").is_file()
        || path.join(".lsp.json").is_file()
}

fn plugin_from_dir(agent: AgentId, path: &Path, user_home: &Path) -> Option<PluginEntry> {
    let manifest = read_plugin_manifest(path);
    let fallback_name = path.file_name()?.to_string_lossy().to_string();
    let name = manifest
        .as_ref()
        .and_then(|v| string_field(v, &["name", "id"]))
        .unwrap_or(fallback_name);
    if name.is_empty() || name == "mcpServers" {
        return None;
    }
    let marketplace = manifest
        .as_ref()
        .and_then(|v| string_field(v, &["marketplace", "source"]));
    let version = manifest
        .as_ref()
        .and_then(|v| string_field(v, &["version"]));
    let description = manifest
        .as_ref()
        .and_then(|v| string_field(v, &["description"]));
    let mut components = discover_components(path);
    if let Some(man) = &manifest {
        if components.is_empty() {
            components = components_from_json(man);
        } else {
            for extra in components_from_json(man) {
                if !components
                    .iter()
                    .any(|c| c.kind == extra.kind && c.name == extra.name)
                {
                    components.push(extra);
                }
            }
        }
    }
    Some(PluginEntry {
        id: plugin_id(
            agent,
            &name,
            marketplace.as_deref(),
            Some(&path.to_string_lossy()),
        ),
        agent,
        name,
        marketplace,
        version,
        scope: Some("user".into()),
        enabled: None,
        trusted: None,
        path: Some(redact_home_path(&path.to_string_lossy(), user_home)),
        description,
        source: "live".into(),
        components,
    })
}

fn read_plugin_manifest(dir: &Path) -> Option<JsonValue> {
    let path = dir.join("plugin.json");
    let text = fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

fn discover_components(dir: &Path) -> Vec<PluginComponent> {
    let mut out = Vec::new();
    push_named_children(&mut out, "skills", &dir.join("skills"));
    push_named_children(&mut out, "commands", &dir.join("commands"));
    push_named_children(&mut out, "agents", &dir.join("agents"));
    if dir.join("hooks").join("hooks.json").is_file() || dir.join("hooks.json").is_file() {
        out.push(PluginComponent {
            kind: "hooks".into(),
            name: "hooks".into(),
            description: None,
        });
    }
    if let Ok(text) = fs::read_to_string(dir.join(".mcp.json")) {
        if let Ok(value) = serde_json::from_str::<JsonValue>(&text) {
            if let Some(servers) = value.get("mcpServers") {
                push_mcp_map(&mut out, servers);
            }
        }
    }
    if dir.join(".lsp.json").is_file() {
        out.push(PluginComponent {
            kind: "lsp".into(),
            name: "lsp".into(),
            description: None,
        });
    }
    out.sort_by(|a, b| a.kind.cmp(&b.kind).then(a.name.cmp(&b.name)));
    out
}

fn push_named_children(out: &mut Vec<PluginComponent>, kind: &str, dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for ent in entries.flatten() {
        let path = ent.path();
        let name = ent.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') {
            continue;
        }
        if path.is_dir()
            || name.ends_with(".md")
            || name.ends_with(".json")
            || name.ends_with(".toml")
        {
            let label = name
                .trim_end_matches(".md")
                .trim_end_matches(".json")
                .trim_end_matches(".toml")
                .to_string();
            out.push(PluginComponent {
                kind: kind.into(),
                name: label,
                description: None,
            });
        }
    }
}

#[cfg(test)]
#[path = "plugin_inventory/tests.rs"]
mod tests;
