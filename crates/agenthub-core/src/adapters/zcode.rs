//! ZCode (智谱 ADE) adapter — desktop install + catalog-append API Key rows in
//! `~/.zcode/v2/config.json` (`provider` map).
//!
//! ## Scope
//! - detect desktop app (and optional `zcode` CLI on PATH)
//! - native install channel: open official download page
//! - home `$ZCODE_HOME` or `~/.zcode`
//! - skills projection: `$ZCODE_HOME/skills`
//! - API Key writes upsert one catalog row; siblings stay in the model dropdown
//!
//! ## Honest limits
//! - No 国产 OAuth writer; Coding Plan / Start Plan slots are not API Key rows.
//! - Custom rows need a model list or ZCode will not show them.
//! - Project history lists the desktop task index and previews CLI session text.
//!   Delete stays closed: users remove tasks in ZCode.
//! - Usage harvests CLI `model_usage` rows; structured stream stays closed.
//! - Headless Chat run prefers `zcode` on PATH; desktop-only installs cannot
//!   invent a bundled CLI path.

use std::path::{Path, PathBuf};

use serde_json::{json, Map, Value};

use crate::error::{AppError, Result};
use crate::models::{
    AgentConfig, AgentId, AuthHealth, AuthState, Capability, CapabilityState, DetectResult,
    DetectStatus, LiveAccount, RunOptions, RunSpec,
};
use crate::utils::atomic::atomic_write;
use crate::utils::paths::{agent_home, home_dir};

use super::{
    api_key_live_account, auth_file_revision, detect_binary, require_api_key, AgentAdapter,
};

/// Official download / landing page (Setup-only).
pub const SETUP_URL: &str = "https://zcode.z.ai/";

/// Custom catalog row id when the write is not an official Z.ai / BigModel slot.
pub const MANAGED_PROVIDER_ID: &str = "agenthub-managed";
pub const BUILTIN_ZAI: &str = "builtin:zai";
pub const BUILTIN_BIGMODEL: &str = "builtin:bigmodel";

const DEFAULT_OFFICIAL_MODEL_IDS: &[&str] = &["GLM-5.3", "GLM-5.3-Flash", "GLM-5-Turbo"];
const ZAI_ANTHROPIC_URL: &str = "https://api.z.ai/api/anthropic";
const BIGMODEL_ANTHROPIC_URL: &str = "https://open.bigmodel.cn/api/anthropic";

pub const V2_CONFIG_FILE: &str = "config.json";
pub const V2_DIR: &str = "v2";

pub struct ZcodeAdapter;

/// Standalone install probe used by platform detectors (no full adapter required).
pub(crate) fn detect_installation() -> DetectResult {
    let env_ready = true; // native Setup has no Node/npm runtime dependency
    let mut notes = Vec::new();

    if let Some(exe) = resolve_zcode_desktop() {
        let version = read_version_hint(&exe);
        tracing::info!(
            target: crate::logging::targets::DETECT,
            module = crate::logging::targets::DETECT,
            op = "detect",
            agent = "zcode",
            via = "native",
            path = %exe.display(),
            version = version.as_deref().unwrap_or("?"),
            "ZCode desktop detected"
        );
        return DetectResult {
            agent: AgentId::Zcode,
            status: DetectStatus::Installed,
            version,
            binary_path: Some(exe),
            channel: Some("native".into()),
            env_ready,
            notes,
            extra_copies: Vec::new(),
        };
    }

    // Secondary: npm / PATH CLI (`zcode-app-cli` → `zcode`).
    let cli = detect_binary(
        AgentId::Zcode,
        &["zcode"],
        &["--version"],
        Some("npm"),
        env_ready,
    );
    if cli.status == DetectStatus::Installed {
        return cli;
    }

    notes.push(format!(
        "ZCode not found. Install via official download: {SETUP_URL}"
    ));
    DetectResult {
        agent: AgentId::Zcode,
        status: DetectStatus::NotFound,
        version: None,
        binary_path: None,
        channel: None,
        env_ready,
        notes,
        extra_copies: Vec::new(),
    }
}

impl AgentAdapter for ZcodeAdapter {
    fn id(&self) -> AgentId {
        AgentId::Zcode
    }

    fn detect(&self) -> DetectResult {
        detect_installation()
    }

    fn read_config(&self) -> Result<AgentConfig> {
        let home = agent_home(AgentId::Zcode)?;
        let path = v2_config_path(&home);
        let raw_file = read_json_object_or_empty(&path)?;
        let providers = raw_file
            .get("provider")
            .cloned()
            .unwrap_or_else(|| json!({}));
        let summary = summarize_providers(&providers);
        Ok(AgentConfig {
            agent: AgentId::Zcode,
            raw: json!({
                "provider": providers,
                "providerSummary": summary,
                "paths": {
                    "home": home,
                    "v2Config": path,
                }
            }),
        })
    }

    fn write_config(&self, config: &AgentConfig) -> Result<()> {
        write_zcode_config(config)
    }

    fn restore_config(&self, config: &AgentConfig) -> Result<()> {
        restore_zcode_catalog(config)
    }

    fn read_auth(&self) -> Result<AuthState> {
        let home = agent_home(AgentId::Zcode)?;
        let path = v2_config_path(&home);
        if !path.is_file() {
            return Ok(AuthState {
                agent: AgentId::Zcode,
                kind: None,
                summary: "no ZCode v2 config".into(),
                has_credentials: false,
                health: AuthHealth::Missing,
                source: Some("zcode:v2-config".into()),
                revision: None,
                also_present: Vec::new(),
                secret_hash: None,
            });
        }
        let root = match read_json_object_or_empty(&path) {
            Ok(v) => v,
            Err(_) => {
                return Ok(AuthState {
                    agent: AgentId::Zcode,
                    kind: None,
                    summary: "ZCode v2 config could not be parsed".into(),
                    has_credentials: false,
                    health: AuthHealth::Unknown,
                    source: Some("zcode:v2-config".into()),
                    revision: auth_file_revision(&path),
                    also_present: Vec::new(),
                    secret_hash: None,
                });
            }
        };
        let providers = root.get("provider").cloned().unwrap_or_else(|| json!({}));
        let keyed = providers_with_api_key(&providers);
        let plan_login = has_non_portable_provider_secret(&providers);
        if keyed.is_empty() {
            if plan_login {
                return Ok(AuthState {
                    agent: AgentId::Zcode,
                    kind: Some("desktop-login".into()),
                    summary: "ZCode 套餐登录在应用内，不会导入".into(),
                    has_credentials: true,
                    health: AuthHealth::Configured,
                    source: Some("zcode:v2-config".into()),
                    revision: auth_file_revision(&path),
                    also_present: Vec::new(),
                    secret_hash: None,
                });
            }
            return Ok(AuthState {
                agent: AgentId::Zcode,
                kind: None,
                summary: "no API key in ZCode v2 provider map".into(),
                has_credentials: false,
                health: AuthHealth::Missing,
                source: Some("zcode:v2-config".into()),
                revision: auth_file_revision(&path),
                also_present: Vec::new(),
                secret_hash: None,
            });
        }
        let enabled = keyed.iter().filter(|p| p.enabled).count();
        let mut also_present = Vec::new();
        if plan_login {
            also_present.push("desktop-login".into());
        }
        Ok(AuthState {
            agent: AgentId::Zcode,
            kind: Some("api_key".into()),
            summary: format!(
                "API key present in {} provider(s) ({} enabled)",
                keyed.len(),
                enabled
            ),
            has_credentials: true,
            health: AuthHealth::Configured,
            source: Some("zcode:v2-config".into()),
            revision: auth_file_revision(&path),
            also_present,
            secret_hash: None,
        })
    }

    fn read_account(&self) -> Result<LiveAccount> {
        let home = agent_home(AgentId::Zcode)?;
        let path = v2_config_path(&home);
        let root = read_json_object_or_empty(&path)?;
        let providers = root.get("provider").cloned().unwrap_or_else(|| json!({}));
        let picked = pick_provider_for_import(&providers)
            .ok_or_else(|| AppError::NotFound("no live ZCode API key to import".into()))?;
        Ok(live_account_from_hit(&picked))
    }

    fn expand_live_accounts(&self, snapshot: &LiveAccount) -> Result<Vec<LiveAccount>> {
        let home = agent_home(AgentId::Zcode)?;
        let path = v2_config_path(&home);
        let root = read_json_object_or_empty(&path)?;
        let providers = root.get("provider").cloned().unwrap_or_else(|| json!({}));
        let expanded = expand_zcode_catalog(&providers);
        if expanded.is_empty() {
            return Ok(vec![snapshot.clone()]);
        }
        Ok(expanded)
    }

    fn apply_account(&self, account: &LiveAccount) -> Result<()> {
        if account.agent != AgentId::Zcode {
            return Err(AppError::InvalidArg(
                "account agent mismatch for zcode".into(),
            ));
        }
        let key = account
            .credentials
            .get("api_key")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                AppError::InvalidArg("ZCode account requires credentials.api_key".into())
            })?;
        let provider_id = account
            .credentials
            .get("provider_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(BUILTIN_ZAI);
        let base_url = account
            .credentials
            .get("base_url")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty());
        let kind = account
            .credentials
            .get("kind")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("anthropic");
        let name = account
            .credentials
            .get("provider_name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("AgentHub");
        upsert_catalog_row(CatalogWrite {
            requested_id: provider_id,
            api_key: key,
            base_url,
            kind,
            name,
            models: account.credentials.get("models"),
        })
    }

    fn build_api_key_account(&self, api_key: &str) -> Result<LiveAccount> {
        let key = require_api_key(api_key)?;
        Ok(api_key_live_account(
            AgentId::Zcode,
            key,
            json!({
                "format": "api_key",
                "api_key": key,
                "provider": "zcode",
                "provider_id": BUILTIN_ZAI,
                "kind": "anthropic",
                "models": DEFAULT_OFFICIAL_MODEL_IDS,
            }),
            "API Key",
            json!({
                "source": "manual",
                "provider": "zcode",
                "provider_id": BUILTIN_ZAI,
            }),
        ))
    }

    fn authorization_key(
        &self,
        kind: crate::models::AccountKind,
        credentials: &Value,
    ) -> Option<String> {
        let base = super::default_authorization_key(kind, credentials)?;
        let slot = credentials
            .get("provider_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(BUILTIN_ZAI);
        Some(format!("{base}:{slot}"))
    }

    fn identity_label(
        &self,
        _kind: crate::models::AccountKind,
        credentials: &Value,
        label_hint: Option<&str>,
    ) -> Option<String> {
        if let Some(hint) = label_hint.map(str::trim).filter(|s| !s.is_empty()) {
            return Some(hint.to_string());
        }
        credentials
            .get("provider_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|id| format!("zcode:{id}"))
    }

    fn skills_dir(&self) -> Option<PathBuf> {
        agent_home(AgentId::Zcode)
            .ok()
            .map(|home| home.join("skills"))
    }

    fn live_backup_paths(&self) -> Vec<PathBuf> {
        let Ok(home) = agent_home(AgentId::Zcode) else {
            return Vec::new();
        };
        vec![v2_config_path(&home), home.join("cli").join("config.json")]
    }

    fn build_run_spec(&self, binary: &Path, prompt: &str, opts: &RunOptions) -> Result<RunSpec> {
        // Prefer a real `zcode` CLI. Desktop .exe / .app is not a verified
        // headless entry — fail closed rather than inventing Electron argv.
        // Windows is case-insensitive: `ZCode.exe` lowercases to `zcode.exe`,
        // so reject by install path, not by name.
        let program = resolve_zcode_cli()
            .filter(|p| p.is_file() && !is_desktop_app_binary(p))
            .or_else(|| {
                if is_desktop_app_binary(binary) {
                    return None;
                }
                let (name, _) = path_name_and_parent(binary);
                if name == "zcode" || name == "zcode.exe" || name == "zcode.cmd" {
                    Some(binary.to_path_buf())
                } else {
                    None
                }
            })
            .ok_or_else(|| {
                AppError::Unsupported(
                    "ZCode 桌面端暂无已验证的无界面启动方式；请安装 zcode CLI，或在 ZCode 应用内使用"
                        .into(),
                )
            })?;
        let _ = opts.allow_dangerous;
        let mut env = Vec::new();
        if let Ok(home) = agent_home(AgentId::Zcode) {
            env.push(("ZCODE_HOME".into(), home.to_string_lossy().into_owned()));
        }
        Ok(RunSpec {
            agent: AgentId::Zcode,
            program,
            // Official TUI accepts a prompt as trailing args; no verified -p flag yet.
            args: vec![prompt.to_string()],
            cwd: opts.cwd.clone(),
            env,
        })
    }

    fn capability(&self, cap: Capability) -> CapabilityState {
        use Capability::*;
        match cap {
            Skills | LiveBackup | ApiKeyAccount | Usage => CapabilityState::full(),
            ConfigWrite => CapabilityState::partial(
                "只追加或更新一条供应商，自定义必须带模型名单；不覆盖整份目录",
            ),
            AccountSwitch => CapabilityState::partial(
                "只启用对应供应商行，其它条目仍在模型列表里；无账号 OAuth 写入",
            ),
            DangerousMode => CapabilityState::unsupported("无已验证的非交互跳过确认 flag"),
            StructuredStream => CapabilityState::unsupported("无已验证的结构化事件流"),
            ProviderPresets => CapabilityState::unsupported("暂无内置 ZCode provider 预设"),
            ProjectHistory => {
                CapabilityState::partial("列出工作区与任务，可预览对话；请到 ZCode 里删除任务")
            }
            ProjectDelete => CapabilityState::unsupported("请到 ZCode 里删除任务"),
            Mcp | ModelSelect | SessionResume => CapabilityState::planned("待验证接入"),
        }
    }
}

/// Config / home root: `ZCODE_HOME` or `~/.zcode`.
#[allow(dead_code)] // public helper for integrations / future call sites
pub fn zcode_home() -> Result<PathBuf> {
    agent_home(AgentId::Zcode)
}

pub fn v2_config_path(home: &Path) -> PathBuf {
    home.join(V2_DIR).join(V2_CONFIG_FILE)
}

/// Resolve desktop binary from fixed install dirs.
pub fn resolve_zcode_desktop() -> Option<PathBuf> {
    for p in well_known_exe_paths() {
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

fn resolve_zcode_cli() -> Option<PathBuf> {
    which::which("zcode").ok()
}

/// Last file name and parent dir, treating both `/` and `\` as separators so
/// Windows path strings still classify on Unix test hosts.
fn path_name_and_parent(path: &Path) -> (String, String) {
    let raw = path.to_string_lossy().replace('\\', "/");
    let mut parts = raw.rsplit('/');
    let name = parts.next().unwrap_or("").to_ascii_lowercase();
    let parent = parts.next().unwrap_or("").to_ascii_lowercase();
    (name, parent)
}

/// Electron desktop binary, not a CLI. Windows cannot tell `ZCode.exe` from
/// `zcode.exe` by case, so parent dir (`ZCode` / `MacOS`) is the signal.
fn is_desktop_app_binary(path: &Path) -> bool {
    let (name, parent) = path_name_and_parent(path);
    if name.ends_with(".appimage") {
        return name.contains("zcode");
    }
    (name == "zcode.exe" || name == "zcode") && (parent == "zcode" || parent == "macos")
}

fn well_known_exe_paths() -> Vec<PathBuf> {
    let mut out = Vec::new();
    #[cfg(windows)]
    {
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            let local = PathBuf::from(local);
            out.push(local.join("Programs").join("ZCode").join("ZCode.exe"));
            out.push(local.join("ZCode").join("ZCode.exe"));
        }
        if let Ok(home) = home_dir() {
            out.push(
                home.join("AppData")
                    .join("Local")
                    .join("Programs")
                    .join("ZCode")
                    .join("ZCode.exe"),
            );
        }
    }
    #[cfg(target_os = "macos")]
    {
        out.push(PathBuf::from(
            "/Applications/ZCode.app/Contents/MacOS/ZCode",
        ));
        if let Ok(home) = home_dir() {
            out.push(
                home.join("Applications")
                    .join("ZCode.app")
                    .join("Contents")
                    .join("MacOS")
                    .join("ZCode"),
            );
        }
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if let Ok(home) = home_dir() {
            // Common AppImage / user install locations (best-effort).
            out.push(home.join("Applications").join("ZCode.AppImage"));
            out.push(home.join(".local").join("bin").join("ZCode"));
            out.push(home.join(".local").join("bin").join("zcode"));
        }
        out.push(PathBuf::from("/opt/ZCode/zcode"));
        out.push(PathBuf::from("/usr/local/bin/zcode"));
    }
    out
}

fn read_version_hint(exe: &Path) -> Option<String> {
    // Adjacent package.json (Electron) if present.
    let install_dir = exe.parent()?;
    for candidate in [
        install_dir
            .join("resources")
            .join("app.asar.unpacked")
            .join("package.json"),
        install_dir.join("package.json"),
        install_dir
            .join("..")
            .join("Resources")
            .join("app.asar.unpacked")
            .join("package.json"),
    ] {
        if !candidate.is_file() {
            continue;
        }
        let text = std::fs::read_to_string(&candidate).ok()?;
        let v: Value = serde_json::from_str(&text).ok()?;
        if let Some(ver) = v
            .get("version")
            .and_then(|x| x.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            return Some(ver.to_string());
        }
    }
    None
}

fn read_json_object_or_empty(path: &Path) -> Result<Value> {
    if !path.exists() {
        return Ok(json!({}));
    }
    let text = std::fs::read_to_string(path)?;
    let value: Value = serde_json::from_str(&text)?;
    if value.is_object() {
        Ok(value)
    } else {
        Err(AppError::InvalidArg(format!(
            "ZCode config must be a JSON object: {}",
            path.display()
        )))
    }
}

#[derive(Debug, Clone)]
struct ProviderKeyHit {
    id: String,
    name: Option<String>,
    kind: Option<String>,
    base_url: Option<String>,
    api_key: String,
    enabled: bool,
    source: Option<String>,
    models: Value,
}

fn is_non_portable_provider_secret(provider_id: &str, api_key: &str) -> bool {
    let id = provider_id.to_ascii_lowercase();
    if id.starts_with("builtin:") && (id.contains("coding-plan") || id.contains("start-plan")) {
        return true;
    }
    looks_like_jwt(api_key)
}

fn has_non_portable_provider_secret(providers: &Value) -> bool {
    let Some(map) = providers.as_object() else {
        return false;
    };
    map.iter().any(|(id, entry)| {
        let Some(api_key) = entry
            .get("options")
            .and_then(|o| o.get("apiKey"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
        else {
            return false;
        };
        is_non_portable_provider_secret(id, api_key)
    })
}

/// Strip Coding Plan / Start Plan login tokens from a `read_config` snapshot
/// so provider import cannot treat desktop plan login as a portable API Key.
pub(crate) fn scrub_non_portable_provider_secrets(raw: &mut Value) {
    let Some(map) = raw.get_mut("provider").and_then(Value::as_object_mut) else {
        return;
    };
    for (id, entry) in map.iter_mut() {
        let Some(api_key) = entry
            .get("options")
            .and_then(|o| o.get("apiKey"))
            .and_then(Value::as_str)
            .map(str::trim)
            .map(str::to_owned)
        else {
            continue;
        };
        if api_key.is_empty() || !is_non_portable_provider_secret(id, &api_key) {
            continue;
        }
        if let Some(options) = entry.get_mut("options").and_then(Value::as_object_mut) {
            options.insert("apiKey".into(), json!(""));
        }
    }
}

fn looks_like_jwt(value: &str) -> bool {
    let value = value.trim();
    value.starts_with("eyJ") && value.bytes().filter(|&b| b == b'.').count() == 2
}

fn providers_with_api_key(providers: &Value) -> Vec<ProviderKeyHit> {
    let Some(map) = providers.as_object() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (id, entry) in map {
        let Some(obj) = entry.as_object() else {
            continue;
        };
        let key = obj
            .get("options")
            .and_then(|o| o.get("apiKey"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty());
        let Some(api_key) = key else {
            continue;
        };
        // Coding Plan / Start Plan slots store a login JWT in apiKey.
        // Those are not portable API keys and must not be imported.
        if is_non_portable_provider_secret(id, api_key) {
            continue;
        }
        let enabled = obj.get("enabled").and_then(Value::as_bool).unwrap_or(true);
        let base_url = obj
            .get("options")
            .and_then(|o| o.get("baseURL"))
            .and_then(Value::as_str)
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        out.push(ProviderKeyHit {
            id: id.clone(),
            name: obj
                .get("name")
                .and_then(Value::as_str)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
            kind: obj
                .get("kind")
                .and_then(Value::as_str)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
            base_url,
            api_key: api_key.to_string(),
            enabled,
            source: obj
                .get("source")
                .and_then(Value::as_str)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
            models: obj.get("models").cloned().unwrap_or_else(|| json!({})),
        });
    }
    out
}

fn pick_provider_for_import(providers: &Value) -> Option<ProviderKeyHit> {
    let mut hits = providers_with_api_key(providers);
    if hits.is_empty() {
        return None;
    }
    // Prefer AgentHub-managed, then enabled custom, then any enabled, then first.
    if let Some(idx) = hits.iter().position(|h| h.id == MANAGED_PROVIDER_ID) {
        return Some(hits.swap_remove(idx));
    }
    if let Some(idx) = hits.iter().position(|h| {
        h.enabled && h.source.as_deref() == Some("custom") && !h.id.starts_with("builtin:")
    }) {
        return Some(hits.swap_remove(idx));
    }
    if let Some(idx) = hits.iter().position(|h| h.enabled) {
        return Some(hits.swap_remove(idx));
    }
    hits.into_iter().next()
}

fn live_account_from_hit(hit: &ProviderKeyHit) -> LiveAccount {
    let mut cred = json!({
        "format": "api_key",
        "api_key": hit.api_key,
        "provider": "zcode",
        "provider_id": hit.id,
    });
    if let Some(base) = hit.base_url.as_deref().filter(|s| !s.is_empty()) {
        cred["base_url"] = json!(base);
    }
    if let Some(kind) = hit.kind.as_deref().filter(|s| !s.is_empty()) {
        cred["kind"] = json!(kind);
    }
    if let Some(name) = hit.name.as_deref().filter(|s| !s.is_empty()) {
        cred["provider_name"] = json!(name);
    }
    if models_map_nonempty(&hit.models) {
        cred["models"] = hit.models.clone();
    }
    api_key_live_account(
        AgentId::Zcode,
        &hit.api_key,
        cred,
        "API Key",
        json!({
            "source": "live",
            "provider": "zcode",
            "provider_id": hit.id,
        }),
    )
}

fn expand_zcode_catalog(providers: &Value) -> Vec<LiveAccount> {
    providers_with_api_key(providers)
        .iter()
        .map(live_account_from_hit)
        .collect()
}

fn restore_zcode_catalog(config: &AgentConfig) -> Result<()> {
    if config.agent != AgentId::Zcode {
        return Err(AppError::InvalidArg(
            "config agent mismatch: expected zcode".into(),
        ));
    }
    let Some(providers) = config.raw.get("provider") else {
        return write_zcode_config(config);
    };
    if !providers.is_object() {
        return Err(AppError::InvalidArg(
            "ZCode catalog restore requires provider to be a JSON object".into(),
        ));
    }
    let home = agent_home(AgentId::Zcode)?;
    let path = v2_config_path(&home);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut root = read_json_object_or_empty(&path)?;
    let root_obj = root
        .as_object_mut()
        .ok_or_else(|| AppError::InvalidArg("ZCode v2 config root must be a JSON object".into()))?;
    root_obj.insert("provider".into(), providers.clone());
    let mut bytes = serde_json::to_vec_pretty(&root)?;
    bytes.push(b'\n');
    atomic_write(&path, &bytes)
}

fn summarize_providers(providers: &Value) -> Value {
    let hits = providers_with_api_key(providers);
    let total = providers.as_object().map(|m| m.len()).unwrap_or(0);
    json!({
        "providerCount": total,
        "withApiKey": hits.len(),
        "enabledWithApiKey": hits.iter().filter(|h| h.enabled).count(),
        "managedPresent": hits.iter().any(|h| h.id == MANAGED_PROVIDER_ID),
    })
}

struct CatalogWrite<'a> {
    requested_id: &'a str,
    api_key: &'a str,
    base_url: Option<&'a str>,
    kind: &'a str,
    name: &'a str,
    models: Option<&'a Value>,
}

fn is_plan_slot(provider_id: &str) -> bool {
    let id = provider_id.to_ascii_lowercase();
    id.starts_with("builtin:") && (id.contains("coding-plan") || id.contains("start-plan"))
}

fn official_slot_for_url(url: &str) -> Option<&'static str> {
    let normalized = url.trim().trim_end_matches('/').to_ascii_lowercase();
    if normalized.contains("zcode.z.ai") {
        return None;
    }
    if normalized.contains("open.bigmodel.cn") {
        return Some(BUILTIN_BIGMODEL);
    }
    if normalized.contains("api.z.ai") {
        return Some(BUILTIN_ZAI);
    }
    None
}

fn is_official_api_slot(provider_id: &str) -> bool {
    provider_id == BUILTIN_ZAI || provider_id == BUILTIN_BIGMODEL
}

fn resolve_catalog_row_id(requested: &str, base_url: Option<&str>) -> Result<String> {
    if is_plan_slot(requested) {
        return Err(AppError::InvalidArg(
            "ZCode 的套餐登录槽不能用 API Key 写入".into(),
        ));
    }
    if let Some(url) = base_url {
        if let Some(slot) = official_slot_for_url(url) {
            return Ok(slot.to_string());
        }
    }
    if requested.is_empty() || requested == MANAGED_PROVIDER_ID {
        if base_url.is_none() {
            return Ok(BUILTIN_ZAI.to_string());
        }
        return Ok(MANAGED_PROVIDER_ID.to_string());
    }
    Ok(requested.to_string())
}

fn models_map_nonempty(value: &Value) -> bool {
    match value {
        Value::Object(map) => map.keys().any(|key| !key.trim().is_empty()),
        Value::Array(items) => items.iter().any(|item| {
            item.as_str()
                .map(str::trim)
                .is_some_and(|id| !id.is_empty())
                || item
                    .get("id")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .is_some_and(|id| !id.is_empty())
        }),
        _ => false,
    }
}

fn default_official_models() -> Value {
    let mut map = Map::new();
    for id in DEFAULT_OFFICIAL_MODEL_IDS {
        map.insert((*id).to_string(), json!({}));
    }
    Value::Object(map)
}

fn normalize_models_map(value: &Value) -> Result<Map<String, Value>> {
    match value {
        Value::Object(map) => {
            let mut out = Map::new();
            for (id, meta) in map {
                let id = id.trim();
                if id.is_empty() {
                    continue;
                }
                out.insert(id.to_string(), meta.clone());
            }
            if out.is_empty() {
                return Err(AppError::InvalidArg(
                    "自定义供应商必须至少写一个模型，才会出现在 ZCode 的模型列表里".into(),
                ));
            }
            Ok(out)
        }
        Value::Array(items) => {
            let mut out = Map::new();
            for item in items {
                let id = item
                    .as_str()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .or_else(|| {
                        item.get("id")
                            .and_then(Value::as_str)
                            .map(str::trim)
                            .filter(|s| !s.is_empty())
                    });
                let Some(id) = id else {
                    continue;
                };
                let meta = item
                    .as_object()
                    .map(|o| json!(o))
                    .unwrap_or_else(|| json!({}));
                out.entry(id.to_string()).or_insert(meta);
            }
            if out.is_empty() {
                return Err(AppError::InvalidArg(
                    "自定义供应商必须至少写一个模型，才会出现在 ZCode 的模型列表里".into(),
                ));
            }
            Ok(out)
        }
        _ => Err(AppError::InvalidArg(
            "自定义供应商必须至少写一个模型，才会出现在 ZCode 的模型列表里".into(),
        )),
    }
}

fn merge_model_maps(existing: &Value, incoming: Map<String, Value>) -> Value {
    let mut out = existing.as_object().cloned().unwrap_or_default();
    for (id, meta) in incoming {
        out.entry(id).or_insert(meta);
    }
    Value::Object(out)
}

fn resolve_row_models(
    existing: Option<&Value>,
    incoming: Option<&Value>,
    official: bool,
) -> Result<Value> {
    let existing_nonempty = existing.is_some_and(models_map_nonempty);
    if let Some(incoming) = incoming.filter(|value| !value.is_null()) {
        if models_map_nonempty(incoming) {
            let incoming_map = normalize_models_map(incoming)?;
            return Ok(match existing {
                Some(prior) if models_map_nonempty(prior) => merge_model_maps(prior, incoming_map),
                _ => Value::Object(incoming_map),
            });
        }
        if !official {
            return Err(AppError::InvalidArg(
                "自定义供应商必须至少写一个模型，才会出现在 ZCode 的模型列表里".into(),
            ));
        }
    }
    if existing_nonempty {
        return Ok(existing.cloned().unwrap_or_else(|| json!({})));
    }
    if official {
        return Ok(default_official_models());
    }
    Err(AppError::InvalidArg(
        "自定义供应商必须至少写一个模型，才会出现在 ZCode 的模型列表里".into(),
    ))
}

fn default_official_name(provider_id: &str, fallback: &str) -> String {
    match provider_id {
        BUILTIN_ZAI => "Z.ai - API Key".into(),
        BUILTIN_BIGMODEL => "Bigmodel - API Key".into(),
        _ => fallback.to_string(),
    }
}

fn default_official_base_url(provider_id: &str) -> Option<&'static str> {
    match provider_id {
        BUILTIN_ZAI => Some(ZAI_ANTHROPIC_URL),
        BUILTIN_BIGMODEL => Some(BIGMODEL_ANTHROPIC_URL),
        _ => None,
    }
}

fn upsert_catalog_row(write: CatalogWrite<'_>) -> Result<()> {
    let provider_id = resolve_catalog_row_id(write.requested_id, write.base_url)?;
    let official = is_official_api_slot(&provider_id);
    let home = agent_home(AgentId::Zcode)?;
    let path = v2_config_path(&home);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut root = read_json_object_or_empty(&path)?;
    let root_obj = root
        .as_object_mut()
        .ok_or_else(|| AppError::InvalidArg("ZCode v2 config root must be a JSON object".into()))?;
    let provider_val = root_obj
        .entry("provider".to_string())
        .or_insert_with(|| json!({}));
    let provider_map = provider_val.as_object_mut().ok_or_else(|| {
        AppError::InvalidArg("ZCode v2 config.provider must be a JSON object".into())
    })?;

    let existing_models = provider_map
        .get(&provider_id)
        .and_then(|entry| entry.get("models"))
        .cloned();
    let models = resolve_row_models(existing_models.as_ref(), write.models, official)?;

    let name = default_official_name(&provider_id, write.name);
    let entry = provider_map.entry(provider_id.clone()).or_insert_with(|| {
        json!({
            "name": name.clone(),
            "kind": write.kind,
            "options": {
                "apiKey": "",
                "apiKeyRequired": true
            },
            "enabled": true,
            "source": "custom",
            "models": {}
        })
    });
    let entry_obj = entry.as_object_mut().ok_or_else(|| {
        AppError::InvalidArg(format!(
            "ZCode provider '{provider_id}' must be a JSON object"
        ))
    })?;
    entry_obj.insert("enabled".into(), json!(true));
    if !entry_obj.contains_key("source") {
        entry_obj.insert("source".into(), json!("custom"));
    }
    if !entry_obj.contains_key("name") {
        entry_obj.insert("name".into(), json!(name));
    }
    if !entry_obj.contains_key("kind") {
        entry_obj.insert("kind".into(), json!(write.kind));
    }
    let options = entry_obj
        .entry("options".to_string())
        .or_insert_with(|| json!({}));
    let options_obj = options.as_object_mut().ok_or_else(|| {
        AppError::InvalidArg(format!(
            "ZCode provider '{provider_id}'.options must be a JSON object"
        ))
    })?;
    options_obj.insert("apiKey".into(), json!(write.api_key));
    options_obj.insert("apiKeyRequired".into(), json!(true));
    if let Some(base) = write.base_url {
        options_obj.insert("baseURL".into(), json!(base));
    } else if !options_obj
        .get("baseURL")
        .and_then(Value::as_str)
        .map(str::trim)
        .is_some_and(|s| !s.is_empty())
    {
        if let Some(default_url) = default_official_base_url(&provider_id) {
            options_obj.insert("baseURL".into(), json!(default_url));
        }
    }
    entry_obj.insert("models".into(), models);

    let mut bytes = serde_json::to_vec_pretty(&root)?;
    bytes.push(b'\n');
    atomic_write(&path, &bytes)
}

fn write_zcode_config(config: &AgentConfig) -> Result<()> {
    if config.agent != AgentId::Zcode {
        return Err(AppError::InvalidArg(format!(
            "config agent mismatch: expected zcode, got {}",
            config.agent.as_str()
        )));
    }
    let raw = config
        .raw
        .as_object()
        .ok_or_else(|| AppError::InvalidArg("ZCode config must be a JSON object".into()))?;

    // Projected apply shape from account / UI: apiKey (+ optional providerId/baseURL/kind/models).
    if let Some(api_key) = raw
        .get("apiKey")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        let provider_id = raw
            .get("providerId")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(MANAGED_PROVIDER_ID);
        let base_url = raw
            .get("baseURL")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty());
        let kind = raw
            .get("kind")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("anthropic");
        let name = raw
            .get("name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("AgentHub");
        return upsert_catalog_row(CatalogWrite {
            requested_id: provider_id,
            api_key,
            base_url,
            kind,
            name,
            models: raw.get("models"),
        });
    }

    // Full read_config envelope: merge official / Hub catalog rows only.
    if let Some(provider) = raw.get("provider").and_then(Value::as_object) {
        let mut wrote = false;
        for id in [BUILTIN_ZAI, BUILTIN_BIGMODEL, MANAGED_PROVIDER_ID] {
            let Some(entry) = provider.get(id) else {
                continue;
            };
            let key = entry
                .get("options")
                .and_then(|o| o.get("apiKey"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty() && *s != "***");
            let Some(api_key) = key else {
                continue;
            };
            let base_url = entry
                .get("options")
                .and_then(|o| o.get("baseURL"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty());
            let kind = entry
                .get("kind")
                .and_then(Value::as_str)
                .unwrap_or("anthropic");
            let name = entry
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("AgentHub");
            upsert_catalog_row(CatalogWrite {
                requested_id: id,
                api_key,
                base_url,
                kind,
                name,
                models: entry.get("models"),
            })?;
            wrote = true;
        }
        if wrote {
            return Ok(());
        }
        return Err(AppError::InvalidArg(
            "ZCode write_config only merges builtin:zai / builtin:bigmodel / agenthub-managed API Key rows (or apiKey field)"
                .into(),
        ));
    }

    Err(AppError::InvalidArg(
        "ZCode write_config requires apiKey or a writable provider row".into(),
    ))
}

#[cfg(test)]
mod tests;
