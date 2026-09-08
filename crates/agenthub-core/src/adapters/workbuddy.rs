//! WorkBuddy (腾讯 CodeBuddy 桌面) adapter.
//!
//! Install shape is an Electron desktop app that **bundles** CodeBuddy Agent CLI
//! (`codebuddy` / `cbc`). Headless runs via `ELECTRON_RUN_AS_NODE` against the
//! bundled CLI (argv built in `build_run_spec`).
//!
//! Production install trees only — never depend on unpack/extract scratch paths.

use std::path::{Path, PathBuf};

use crate::error::{AppError, Result};
use crate::models::{
    AgentConfig, AgentId, AuthHealth, AuthState, Capability, CapabilityState, DetectResult,
    DetectStatus, LiveAccount, RunOptions, RunSpec,
};
use crate::utils::expiry::{is_expired, normalize_credential_key};
use crate::utils::paths::home_dir;

use super::{api_key_live_account, auth_file_revision, require_api_key, AgentAdapter};

/// Official setup landing page (no npm / no allowlisted install.ps1).
pub const SETUP_URL: &str = "https://www.codebuddy.cn/work/";

pub struct WorkBuddyAdapter;

/// Standalone install probe used by platform detectors (no full adapter required).
pub(crate) fn detect_installation() -> DetectResult {
    let env_ready = true; // native Setup has no Node/npm runtime dependency
    let mut notes = Vec::new();

    let Some(exe) = resolve_workbuddy_exe() else {
        tracing::debug!(
            target: crate::logging::targets::DETECT,
            module = crate::logging::targets::DETECT,
            op = "detect",
            agent = "workbuddy",
            via = "not_found",
            "WorkBuddy desktop binary not found in default or registry paths"
        );
        notes.push(
            "WorkBuddy not found. Install via official Setup: https://www.codebuddy.cn/work/"
                .into(),
        );
        return DetectResult {
            agent: AgentId::WorkBuddy,
            status: DetectStatus::NotFound,
            version: None,
            binary_path: None,
            channel: None,
            env_ready,
            notes,
            extra_copies: Vec::new(),
        };
    };

    let install_dir = exe
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| exe.clone());
    let codebuddy = resolve_bundled_codebuddy(&install_dir);
    if codebuddy.is_none() {
        notes.push(
            "WorkBuddy.exe found but bundled codebuddy CLI missing under install resources".into(),
        );
    }

    let version =
        read_version_from_last_launch().or_else(|| read_version_from_package_json(&install_dir));

    if let Some(ref cb) = codebuddy {
        tracing::info!(
            target: crate::logging::targets::DETECT,
            module = crate::logging::targets::DETECT,
            op = "detect",
            agent = "workbuddy",
            via = "native",
            path = %exe.display(),
            codebuddy = %cb.display(),
            version = version.as_deref().unwrap_or("?"),
            "WorkBuddy desktop + bundled CLI detected"
        );
    } else {
        tracing::debug!(
            target: crate::logging::targets::DETECT,
            module = crate::logging::targets::DETECT,
            op = "detect",
            agent = "workbuddy",
            path = %exe.display(),
            "WorkBuddy.exe present without bundled codebuddy"
        );
    }

    DetectResult {
        agent: AgentId::WorkBuddy,
        status: DetectStatus::Installed,
        version,
        binary_path: Some(exe),
        channel: Some("native".into()),
        env_ready,
        notes,
        extra_copies: Vec::new(),
    }
}

impl AgentAdapter for WorkBuddyAdapter {
    fn id(&self) -> AgentId {
        AgentId::WorkBuddy
    }

    fn detect(&self) -> DetectResult {
        detect_installation()
    }

    fn read_config(&self) -> Result<AgentConfig> {
        let dir = workbuddy_config_dir()?;
        let settings_path = dir.join("settings.json");
        let models_path = dir.join("models.json");
        let mcp_path = dir.join(".mcp.json");

        let settings = read_json_value_or_empty(&settings_path)?;
        let models = if models_path.exists() {
            Some(read_json_value_or_empty(&models_path)?)
        } else {
            None
        };
        let mcp = if mcp_path.exists() {
            Some(read_json_value_or_empty(&mcp_path)?)
        } else {
            None
        };

        let mut raw = serde_json::Map::new();
        // `read_config` is the storage-side snapshot consumed by the provider
        // pool, so retain the real values here. Redaction belongs at output
        // boundaries (`Provider::redacted`/CLI serialization), otherwise an
        // imported `***` would overwrite a live secret on the next write.
        raw.insert("settings".into(), settings);
        if let Some(m) = models {
            raw.insert("models".into(), m);
        }
        if let Some(m) = mcp {
            raw.insert("mcp".into(), m);
        }
        raw.insert(
            "paths".into(),
            serde_json::json!({
                "configDir": dir,
                "settings": settings_path,
                "models": models_path,
                "mcp": mcp_path,
            }),
        );

        Ok(AgentConfig {
            agent: AgentId::WorkBuddy,
            raw: serde_json::Value::Object(raw),
        })
    }

    fn write_config(&self, _config: &AgentConfig) -> Result<()> {
        write_workbuddy_config(_config)
    }

    fn restore_config(&self, config: &AgentConfig) -> Result<()> {
        restore_workbuddy_catalog(config)
    }

    fn read_auth(&self) -> Result<AuthState> {
        workbuddy_auth_state()
    }

    fn read_account(&self) -> Result<LiveAccount> {
        let models = read_models_json()?;
        pick_model_for_import(&models)
            .map(|entry| live_account_from_model_entry(&entry))
            .ok_or_else(|| AppError::NotFound("no live WorkBuddy API key to import".into()))
    }

    fn expand_live_accounts(&self, snapshot: &LiveAccount) -> Result<Vec<LiveAccount>> {
        let models = match read_models_json() {
            Ok(value) => value,
            Err(_) => return Ok(vec![snapshot.clone()]),
        };
        let expanded = expand_workbuddy_catalog(&models);
        if expanded.is_empty() {
            return Ok(vec![snapshot.clone()]);
        }
        Ok(expanded)
    }

    fn apply_account(&self, account: &LiveAccount) -> Result<()> {
        upsert_workbuddy_model_from_account(account)
    }

    fn build_api_key_account(&self, api_key: &str) -> Result<LiveAccount> {
        let key = require_api_key(api_key)?;
        Ok(api_key_live_account(
            AgentId::WorkBuddy,
            key,
            serde_json::json!({
                "format": "api_key",
                "api_key": key,
                "provider": "workbuddy",
            }),
            "API Key",
            serde_json::json!({
                "source": "manual",
                "provider": "workbuddy",
            }),
        ))
    }

    fn authorization_key(
        &self,
        kind: crate::models::AccountKind,
        credentials: &serde_json::Value,
    ) -> Option<String> {
        let base = super::default_authorization_key(kind, credentials)?;
        let slot = workbuddy_model_slot(credentials);
        Some(format!("{base}:{slot}"))
    }

    fn identity_label(
        &self,
        _kind: crate::models::AccountKind,
        credentials: &serde_json::Value,
        label_hint: Option<&str>,
    ) -> Option<String> {
        if let Some(name) = credentials
            .get("name")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            return Some(name.to_string());
        }
        let slot = workbuddy_model_slot(credentials);
        if slot != "custom" {
            return Some(format!("workbuddy:{slot}"));
        }
        label_hint
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
    }

    fn skills_dir(&self) -> Option<PathBuf> {
        workbuddy_config_dir().ok().map(|d| d.join("skills"))
    }

    fn capability(&self, cap: Capability) -> CapabilityState {
        use Capability::*;
        match cap {
            Skills | LiveBackup | DangerousMode | ProjectHistory | ProjectDelete | Usage
            | ApiKeyAccount => CapabilityState::full(),
            ConfigWrite => CapabilityState::partial("只追加或更新一条自定义模型，不覆盖整份列表"),
            AccountSwitch => CapabilityState::partial(
                "只写入对应自定义模型，其它条目仍在列表里；不会改 WorkBuddy 当前选中的模型",
            ),
            StructuredStream => {
                CapabilityState::unsupported("CLI 仅提供 text 输出，无结构化事件流")
            }
            ProviderPresets => CapabilityState::unsupported("暂无内置 WorkBuddy provider 预设"),
            Mcp | ModelSelect | SessionResume => CapabilityState::planned("待验证接入"),
        }
    }

    fn live_backup_paths(&self) -> Vec<PathBuf> {
        let mut paths = Vec::new();
        if let Ok(dir) = workbuddy_config_dir() {
            paths.push(dir.join("settings.json"));
            paths.push(dir.join("models.json"));
            paths.push(dir.join(".mcp.json"));
        }
        if let Some(auth) = auth_info_path() {
            paths.push(auth);
        }
        paths
    }

    fn build_run_spec(&self, binary: &Path, prompt: &str, opts: &RunOptions) -> Result<RunSpec> {
        // `binary` is the Electron desktop executable from detect.
        // CLI is a separate bundled path under resources / Contents/Resources.
        let install_dir = binary
            .parent()
            .map(Path::to_path_buf)
            .ok_or_else(|| AppError::NotFound("WorkBuddy binary has no parent directory".into()))?;
        let codebuddy = resolve_bundled_codebuddy(&install_dir).ok_or_else(|| {
            AppError::NotFound(
                "bundled codebuddy CLI not found under WorkBuddy install resources".into(),
            )
        })?;

        let mut args = vec![
            codebuddy.to_string_lossy().into_owned(),
            "-p".into(),
            prompt.to_string(),
            "--output-format".into(),
            "text".into(),
        ];
        if opts.allow_dangerous {
            args.push("--dangerously-skip-permissions".into());
        }

        let mut env = vec![("ELECTRON_RUN_AS_NODE".into(), "1".into())];
        if let Ok(dir) = workbuddy_config_dir() {
            let s = dir.to_string_lossy().into_owned();
            env.push(("WORKBUDDY_CONFIG_DIR".into(), s.clone()));
            env.push(("CODEBUDDY_CONFIG_DIR".into(), s));
        }

        tracing::debug!(
            target: crate::logging::targets::RUN,
            module = crate::logging::targets::RUN,
            op = "build_run_spec",
            agent = "workbuddy",
            program = %binary.display(),
            codebuddy = %codebuddy.display(),
            electron_run_as_node = true,
            "WorkBuddy headless run_spec ready"
        );

        Ok(RunSpec {
            agent: AgentId::WorkBuddy,
            program: binary.to_path_buf(),
            args,
            cwd: opts.cwd.clone(),
            env,
        })
    }
}

/// Config root: `WORKBUDDY_CONFIG_DIR` / `CODEBUDDY_CONFIG_DIR` or `~/.workbuddy`.
pub fn workbuddy_config_dir() -> Result<PathBuf> {
    crate::utils::paths::agent_config_dir(AgentId::WorkBuddy)
}

/// Resolve WorkBuddy.exe: fixed install dirs first, registry only as fallback.
///
/// Hot path must not spawn PowerShell when the default install is present
/// (Agents page runs detect often; registry scan was a major latency source).
pub fn resolve_workbuddy_exe() -> Option<PathBuf> {
    for p in well_known_exe_paths() {
        if p.is_file() {
            tracing::debug!(
                target: crate::logging::targets::DETECT,
                module = crate::logging::targets::DETECT,
                op = "resolve_workbuddy_exe",
                via = "well_known",
                path = %p.display(),
                "WorkBuddy.exe found in fixed install path"
            );
            return Some(p);
        }
    }
    // Slow path: HKCU Uninstall → DisplayIcon (spawns powershell once).
    if let Some(from_reg) = resolve_exe_from_uninstall_registry() {
        if from_reg.is_file() {
            tracing::debug!(
                target: crate::logging::targets::DETECT,
                module = crate::logging::targets::DETECT,
                op = "resolve_workbuddy_exe",
                via = "registry",
                path = %from_reg.display(),
                "WorkBuddy.exe found via uninstall registry"
            );
            return Some(from_reg);
        }
    }
    None
}

/// Cheap fixed candidates only — no process spawn.
pub(crate) fn well_known_exe_paths() -> Vec<PathBuf> {
    let mut out = Vec::new();
    #[cfg(windows)]
    {
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            out.push(
                PathBuf::from(local)
                    .join("Programs")
                    .join("WorkBuddy")
                    .join("WorkBuddy.exe"),
            );
        }
        if let Ok(home) = home_dir() {
            out.push(
                home.join("AppData")
                    .join("Local")
                    .join("Programs")
                    .join("WorkBuddy")
                    .join("WorkBuddy.exe"),
            );
        }
    }
    #[cfg(not(windows))]
    {
        // macOS app bundle. Current WorkBuddy.mac keeps CFBundleExecutable as
        // `Electron` (not `WorkBuddy`); probe both plus Info.plist when present.
        for bundle in macos_workbuddy_bundles() {
            out.extend(macos_workbuddy_binaries(&bundle));
        }
    }
    out
}

#[cfg(not(windows))]
fn macos_workbuddy_bundles() -> Vec<PathBuf> {
    let mut bundles = vec![PathBuf::from("/Applications/WorkBuddy.app")];
    if let Ok(home) = home_dir() {
        bundles.push(home.join("Applications").join("WorkBuddy.app"));
    }
    bundles
}

#[cfg(not(windows))]
fn macos_workbuddy_binaries(bundle: &Path) -> Vec<PathBuf> {
    let macos = bundle.join("Contents").join("MacOS");
    let mut names: Vec<String> = Vec::new();
    if let Some(name) = cf_bundle_executable_name(bundle) {
        names.push(name);
    }
    for fallback in ["WorkBuddy", "Electron"] {
        if !names.iter().any(|existing| existing == fallback) {
            names.push(fallback.to_string());
        }
    }
    names.into_iter().map(|name| macos.join(name)).collect()
}

/// XML Info.plist only. Binary plists and missing bundles return None.
#[cfg(not(windows))]
fn cf_bundle_executable_name(bundle: &Path) -> Option<String> {
    let text = std::fs::read_to_string(bundle.join("Contents").join("Info.plist")).ok()?;
    let rest = text.split_once("<key>CFBundleExecutable</key>")?.1;
    let name = rest
        .trim_start()
        .strip_prefix("<string>")?
        .split_once("</string>")?
        .0
        .trim();
    if name.is_empty() || name.contains(['/', '\\', '\0']) {
        return None;
    }
    Some(name.to_string())
}

fn electron_resource_dirs(install_dir: &Path) -> Vec<PathBuf> {
    let mut dirs = vec![install_dir.join("resources")];
    // Windows: WorkBuddy.exe sits next to `resources/`.
    // macOS: Electron lives in Contents/MacOS, assets in Contents/Resources.
    if install_dir
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case("macos"))
    {
        if let Some(contents) = install_dir.parent() {
            dirs.push(contents.join("Resources"));
        }
    }
    dirs
}

/// Production bundled CLI only (never unpack/extract scratch paths).
pub fn resolve_bundled_codebuddy(install_dir: &Path) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    for root in electron_resource_dirs(install_dir) {
        let bin = root.join("app.asar.unpacked").join("cli").join("bin");
        candidates.push(bin.join("codebuddy"));
        if cfg!(windows) {
            candidates.push(bin.join("codebuddy.cmd"));
            // Some builds ship extension-less or .exe
            candidates.push(bin.join("codebuddy.exe"));
        }
    }
    for p in candidates {
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

/// Allowlisted silent uninstaller (Windows).
pub fn resolve_uninstaller() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            let p = PathBuf::from(local)
                .join("Programs")
                .join("WorkBuddy")
                .join("Uninstall WorkBuddy.exe");
            if p.is_file() {
                return Some(p);
            }
        }
        if let Some(exe) = resolve_workbuddy_exe() {
            if let Some(dir) = exe.parent() {
                let p = dir.join("Uninstall WorkBuddy.exe");
                if p.is_file() {
                    return Some(p);
                }
            }
        }
    }
    None
}

fn auth_info_path() -> Option<PathBuf> {
    let relative = PathBuf::from("CodeBuddyExtension")
        .join("Data")
        .join("Public")
        .join("auth")
        .join("workbuddy-desktop.info");
    #[cfg(windows)]
    {
        let local = std::env::var("LOCALAPPDATA").ok()?;
        Some(PathBuf::from(local).join(relative))
    }
    #[cfg(target_os = "macos")]
    {
        Some(
            home_dir()
                .ok()?
                .join("Library")
                .join("Application Support")
                .join(relative),
        )
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let _ = relative;
        None
    }
}

#[derive(Debug, Clone, Copy)]
struct WorkBuddyAuthMetadata {
    expires_expired: Option<bool>,
    refresh_expired: Option<bool>,
}

/// Extract only non-sensitive identity/expiry metadata from the desktop info
/// file. Token-looking fields are deliberately ignored.
fn workbuddy_auth_metadata(value: &serde_json::Value) -> Option<WorkBuddyAuthMetadata> {
    fn visit(
        value: &serde_json::Value,
        has_identity: &mut bool,
        expires: &mut Option<bool>,
        refresh_expires: &mut Option<bool>,
    ) {
        let Some(object) = value.as_object() else {
            return;
        };
        for (raw_key, child) in object {
            let key = normalize_credential_key(raw_key);
            let non_empty = child.as_str().map(str::trim).is_some_and(|s| !s.is_empty());
            match key.as_str() {
                "email" | "email_address" | "emailaddress" | "user_id" | "userid"
                | "account_id" | "accountid" | "username" | "name" => {
                    *has_identity |= non_empty;
                }
                "expires_at" | "expiresat" | "expires" => {
                    if let Some(value) = is_expired(child) {
                        *expires = Some(value);
                    }
                }
                "refresh_expires_at" | "refreshexpiresat" | "refresh_expires" => {
                    if let Some(value) = is_expired(child) {
                        *refresh_expires = Some(value);
                    }
                }
                _ => {}
            }
            visit(child, has_identity, expires, refresh_expires);
        }
    }

    let mut has_identity = false;
    let mut expires = None;
    let mut refresh_expires = None;
    visit(value, &mut has_identity, &mut expires, &mut refresh_expires);
    (has_identity || expires.is_some() || refresh_expires.is_some()).then_some(
        WorkBuddyAuthMetadata {
            expires_expired: expires,
            refresh_expired: refresh_expires,
        },
    )
}

fn read_version_from_last_launch() -> Option<String> {
    let dir = workbuddy_config_dir().ok()?;
    let path = dir.join("last-launch.json");
    if !path.is_file() {
        return None;
    }
    let text = std::fs::read_to_string(path).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    v.get("version")
        .and_then(|x| x.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn read_version_from_package_json(install_dir: &Path) -> Option<String> {
    let mut paths: Vec<PathBuf> = electron_resource_dirs(install_dir)
        .into_iter()
        .map(|root| root.join("app.asar.unpacked").join("package.json"))
        .collect();
    paths.push(install_dir.join("package.json"));
    for path in paths {
        if !path.is_file() {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        if let Some(version) = v
            .get("version")
            .and_then(|x| x.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            return Some(version.to_string());
        }
    }
    None
}

/// Slow fallback: parse DisplayIcon from HKCU Uninstall keys (Windows only).
/// Call only after well-known paths miss — spawns PowerShell.
#[cfg(windows)]
fn resolve_exe_from_uninstall_registry() -> Option<PathBuf> {
    use std::os::windows::process::CommandExt;
    use std::process::Command;
    use std::time::Instant;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let started = Instant::now();
    // Query display name matching WorkBuddy and read DisplayIcon.
    let script = r#"
$ErrorActionPreference = 'SilentlyContinue'
$keys = Get-ChildItem 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall' -ErrorAction SilentlyContinue
foreach ($k in $keys) {
  $p = Get-ItemProperty $k.PSPath -ErrorAction SilentlyContinue
  if ($p.DisplayName -like 'WorkBuddy*') {
    if ($p.DisplayIcon) { Write-Output $p.DisplayIcon; exit 0 }
    if ($p.InstallLocation) {
      $exe = Join-Path $p.InstallLocation 'WorkBuddy.exe'
      if (Test-Path $exe) { Write-Output $exe; exit 0 }
    }
  }
}
"#;
    let out = Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            script,
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    tracing::debug!(
        target: crate::logging::targets::DETECT,
        module = crate::logging::targets::DETECT,
        op = "resolve_workbuddy_registry",
        elapsed_ms = started.elapsed().as_millis() as u64,
        ok = out.status.success(),
        "uninstall registry PowerShell probe finished"
    );
    if !out.status.success() {
        return None;
    }
    let line = String::from_utf8_lossy(&out.stdout)
        .lines()
        .next()?
        .trim()
        .to_string();
    if line.is_empty() {
        return None;
    }
    // DisplayIcon may be "C:\...\WorkBuddy.exe,0"
    let path_part = line.split(',').next()?.trim().trim_matches('"');
    let p = PathBuf::from(path_part);
    if p.is_file() {
        Some(p)
    } else if p.extension().is_none() {
        let exe = p.join("WorkBuddy.exe");
        if exe.is_file() {
            Some(exe)
        } else {
            None
        }
    } else {
        None
    }
}

#[cfg(not(windows))]
fn resolve_exe_from_uninstall_registry() -> Option<PathBuf> {
    None
}

fn read_json_value_or_empty(path: &Path) -> Result<serde_json::Value> {
    if !path.exists() {
        return Ok(serde_json::json!({}));
    }
    let text = std::fs::read_to_string(path)?;
    let value: serde_json::Value = serde_json::from_str(&text)?;
    // models.json may be a top-level array — accept any JSON value.
    Ok(value)
}

const REDACTED_MARKER: &str = "***";

fn write_workbuddy_config(config: &AgentConfig) -> Result<()> {
    if config.agent != AgentId::WorkBuddy {
        return Err(AppError::InvalidArg(format!(
            "config agent mismatch: expected workbuddy, got {}",
            config.agent.as_str()
        )));
    }
    let raw = config.raw.as_object().ok_or_else(|| {
        AppError::InvalidArg("WorkBuddy settings_config must be a JSON object".into())
    })?;
    // ProviderService stores a complete read_config envelope, while the UI
    // sends the user-level models.json object directly. Ignore adapter metadata
    // (settings/mcp/paths) and only project the models payload.
    let desired = raw.get("models").unwrap_or(&config.raw);
    let models_path = workbuddy_config_dir()?.join("models.json");
    let live = read_json_value_or_empty(&models_path)?;
    let merged = merge_workbuddy_models(&live, desired)?;
    let mut bytes = serde_json::to_vec_pretty(&merged)?;
    bytes.push(b'\n');
    crate::utils::atomic::atomic_write(&models_path, &bytes)
}

/// Merge both WorkBuddy/CodeBuddy models.json shapes:
/// `[{...}]` and `{ "models": [{...}], "availableModels": [...] }`.
/// Entries are keyed by `id`; unknown fields and unrelated models survive.
fn merge_workbuddy_models(
    live: &serde_json::Value,
    desired: &serde_json::Value,
) -> Result<serde_json::Value> {
    let desired_entries = workbuddy_model_entries(desired, "target")?;
    let live_entries = workbuddy_model_entries(live, "existing")?;
    let merged_entries = merge_workbuddy_entries(&live_entries, &desired_entries)?;

    match (live, desired) {
        (serde_json::Value::Array(_), _) => Ok(serde_json::Value::Array(merged_entries)),
        (serde_json::Value::Object(live_obj), serde_json::Value::Array(_))
            if live_obj.is_empty() =>
        {
            Ok(serde_json::Value::Array(merged_entries))
        }
        (serde_json::Value::Object(_), serde_json::Value::Array(_)) => {
            let mut out = live.as_object().cloned().unwrap_or_default();
            out.insert("models".into(), serde_json::Value::Array(merged_entries));
            Ok(serde_json::Value::Object(out))
        }
        (_, serde_json::Value::Object(desired_obj)) => {
            let mut out = live.as_object().cloned().unwrap_or_default();
            for (key, value) in desired_obj {
                if key != "models" {
                    out.insert(key.clone(), value.clone());
                }
            }
            out.insert("models".into(), serde_json::Value::Array(merged_entries));
            Ok(serde_json::Value::Object(out))
        }
        _ => Err(AppError::InvalidArg(
            "target WorkBuddy models.json must be an array or object".into(),
        )),
    }
}

fn workbuddy_model_entries(
    value: &serde_json::Value,
    label: &str,
) -> Result<Vec<serde_json::Value>> {
    match value {
        serde_json::Value::Array(items) => Ok(items.clone()),
        serde_json::Value::Object(object) => {
            match object.get("models").and_then(serde_json::Value::as_array) {
                Some(items) => Ok(items.clone()),
                None if label == "existing" => Ok(Vec::new()),
                None => Err(AppError::InvalidArg(format!(
                    "{label} WorkBuddy models.json.models must be an array"
                ))),
            }
        }
        _ => Err(AppError::InvalidArg(format!(
            "{label} WorkBuddy models.json must be an array or object"
        ))),
    }
}

fn workbuddy_entry_id(entry: &serde_json::Value) -> Option<&str> {
    entry
        .get("id")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|id| !id.is_empty())
}

fn workbuddy_entry_api_key(entry: &serde_json::Value) -> Option<&str> {
    entry
        .get("apiKey")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|key| !key.is_empty())
}

fn is_generic_workbuddy_model_id(id: &str) -> bool {
    matches!(id, "custom" | "custom-model")
}

fn uniquify_workbuddy_model_id(base: &str, entries: &[serde_json::Value]) -> String {
    let taken = |candidate: &str| {
        entries
            .iter()
            .any(|entry| workbuddy_entry_id(entry) == Some(candidate))
    };
    if !taken(base) {
        return base.to_string();
    }
    let mut n = 2;
    loop {
        let candidate = format!("{base}-{n}");
        if !taken(&candidate) {
            return candidate;
        }
        n += 1;
    }
}

fn merge_workbuddy_entry_keep_id(
    existing: &serde_json::Value,
    desired: &serde_json::Value,
) -> serde_json::Value {
    let mut merged = merge_redacted_json(existing, desired);
    if let Some(existing_id) = workbuddy_entry_id(existing) {
        if let Some(object) = merged.as_object_mut() {
            object.insert("id".into(), serde_json::json!(existing_id));
        }
    }
    merged
}

fn merge_workbuddy_entries(
    live: &[serde_json::Value],
    desired: &[serde_json::Value],
) -> Result<Vec<serde_json::Value>> {
    let mut merged = live.to_vec();
    for desired_entry in desired {
        let desired_obj = desired_entry.as_object().ok_or_else(|| {
            AppError::InvalidArg("WorkBuddy models entries must be JSON objects".into())
        })?;
        let id = desired_obj
            .get("id")
            .and_then(serde_json::Value::as_str)
            .filter(|id| !id.trim().is_empty())
            .ok_or_else(|| {
                AppError::InvalidArg("WorkBuddy model entry requires a non-empty id".into())
            })?;
        let incoming_key =
            workbuddy_entry_api_key(desired_entry).filter(|key| *key != REDACTED_MARKER);

        // Same API key already in the list: update that row and keep its id.
        // Re-writing a login after a generic-id uniquify must not append again.
        if let Some(key) = incoming_key {
            if let Some(existing_index) = merged
                .iter()
                .position(|entry| workbuddy_entry_api_key(entry) == Some(key))
            {
                merged[existing_index] =
                    merge_workbuddy_entry_keep_id(&merged[existing_index], desired_entry);
                continue;
            }
        }

        if let Some(existing_index) = merged
            .iter()
            .position(|entry| workbuddy_entry_id(entry) == Some(id))
        {
            let existing_key = workbuddy_entry_api_key(&merged[existing_index]);
            let different_key = match (existing_key, incoming_key) {
                (Some(old), Some(new)) => old != new,
                _ => false,
            };
            // Default scaffold ids (`custom` / `custom-model`) are shared by
            // every "add API Key" form. A second key must append, not replace.
            if is_generic_workbuddy_model_id(id) && different_key {
                let unique_id = uniquify_workbuddy_model_id(id, &merged);
                let mut unique = desired_entry.clone();
                if let Some(object) = unique.as_object_mut() {
                    object.insert("id".into(), serde_json::json!(unique_id.clone()));
                    let name = object
                        .get("name")
                        .and_then(serde_json::Value::as_str)
                        .map(str::trim)
                        .unwrap_or("");
                    if name.is_empty() || is_generic_workbuddy_model_id(name) || name == id {
                        object.insert("name".into(), serde_json::json!(unique_id));
                    }
                }
                merged.push(unique);
            } else {
                merged[existing_index] =
                    merge_redacted_json(&merged[existing_index], desired_entry);
            }
        } else {
            merged.push(desired_entry.clone());
        }
    }
    Ok(merged)
}

fn merge_redacted_json(
    existing: &serde_json::Value,
    desired: &serde_json::Value,
) -> serde_json::Value {
    if desired.as_str() == Some(REDACTED_MARKER) {
        return existing.clone();
    }
    match (existing, desired) {
        (serde_json::Value::Object(old), serde_json::Value::Object(new)) => {
            let mut merged = old.clone();
            for (key, value) in new {
                let next = merged
                    .get(key)
                    .map(|prior| merge_redacted_json(prior, value))
                    .unwrap_or_else(|| value.clone());
                merged.insert(key.clone(), next);
            }
            serde_json::Value::Object(merged)
        }
        _ => desired.clone(),
    }
}

const WORKBUDDY_CHAT_COMPLETIONS_SUFFIX: &str = "/v1/chat/completions";

fn read_models_json() -> Result<serde_json::Value> {
    let path = workbuddy_config_dir()?.join("models.json");
    read_json_value_or_empty(&path)
}

fn looks_like_jwt(value: &str) -> bool {
    let value = value.trim();
    value.starts_with("eyJ") && value.bytes().filter(|&b| b == b'.').count() == 2
}

fn workbuddy_model_slot(credentials: &serde_json::Value) -> String {
    credentials
        .get("model_id")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .or_else(|| {
            credentials
                .get("id")
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
        })
        .unwrap_or("custom")
        .to_string()
}

fn portable_workbuddy_entries(models: &serde_json::Value) -> Vec<serde_json::Value> {
    workbuddy_model_entries(models, "existing")
        .unwrap_or_default()
        .into_iter()
        .filter(is_portable_workbuddy_entry)
        .collect()
}

fn is_portable_workbuddy_entry(entry: &serde_json::Value) -> bool {
    let Some(object) = entry.as_object() else {
        return false;
    };
    let id = object
        .get("id")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let key = object
        .get("apiKey")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty() && *s != REDACTED_MARKER);
    match (id, key) {
        (Some(_), Some(api_key)) => !looks_like_jwt(api_key),
        _ => false,
    }
}

fn pick_model_for_import(models: &serde_json::Value) -> Option<serde_json::Value> {
    portable_workbuddy_entries(models).into_iter().next()
}

fn expand_workbuddy_catalog(models: &serde_json::Value) -> Vec<LiveAccount> {
    portable_workbuddy_entries(models)
        .iter()
        .map(live_account_from_model_entry)
        .collect()
}

fn live_account_from_model_entry(entry: &serde_json::Value) -> LiveAccount {
    let object = entry.as_object();
    let id = object
        .and_then(|o| o.get("id"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("custom");
    let api_key = object
        .and_then(|o| o.get("apiKey"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .unwrap_or_default();
    let name = object
        .and_then(|o| o.get("name"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let url = object
        .and_then(|o| o.get("url"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let vendor = object
        .and_then(|o| o.get("vendor"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let mut cred = serde_json::json!({
        "format": "api_key",
        "api_key": api_key,
        "provider": "workbuddy",
        "model_id": id,
        "id": id,
        "catalog_row": entry.clone(),
    });
    if let Some(name) = name {
        cred["name"] = serde_json::json!(name);
    }
    if let Some(url) = url {
        cred["url"] = serde_json::json!(url);
        cred["base_url"] = serde_json::json!(url);
    }
    if let Some(vendor) = vendor {
        cred["vendor"] = serde_json::json!(vendor);
    }
    for flag in [
        "supportsToolCall",
        "supportsImages",
        "supportsReasoning",
        "useCustomProtocol",
        "reasoning",
    ] {
        if let Some(value) = object.and_then(|o| o.get(flag)).cloned() {
            cred[flag] = value;
        }
    }
    let label = name.unwrap_or("API Key");
    let mut extra = serde_json::json!({
        "source": "live",
        "provider": "workbuddy",
        "model_id": id,
    });
    if let Some(url) = url {
        extra["endpoint"] = serde_json::json!(url);
    }
    api_key_live_account(AgentId::WorkBuddy, api_key, cred, label, extra)
}

fn desktop_login_present() -> (bool, Option<PathBuf>, AuthHealth, String) {
    let Some(path) = auth_info_path() else {
        return (
            false,
            None,
            AuthHealth::Unknown,
            "WorkBuddy auth metadata path is unavailable".into(),
        );
    };
    if !path.is_file() {
        return (
            false,
            Some(path),
            AuthHealth::Missing,
            "no WorkBuddy desktop login metadata".into(),
        );
    }
    let body = match std::fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
    {
        Some(body) => body,
        None => {
            return (
                false,
                Some(path),
                AuthHealth::Unknown,
                "WorkBuddy login metadata could not be parsed".into(),
            );
        }
    };
    let Some(metadata) = workbuddy_auth_metadata(&body) else {
        return (
            false,
            Some(path),
            AuthHealth::Unknown,
            "WorkBuddy login metadata is incomplete".into(),
        );
    };
    let health = if metadata.expires_expired == Some(true) && metadata.refresh_expired == Some(true)
    {
        AuthHealth::NeedsLogin
    } else if metadata.refresh_expired == Some(false) {
        AuthHealth::Renewable
    } else {
        AuthHealth::Configured
    };
    let summary = if health == AuthHealth::NeedsLogin {
        "WorkBuddy 桌面登录已过期，请在 WorkBuddy 里重新登录".into()
    } else {
        "WorkBuddy 桌面登录在应用内，不会导入".into()
    };
    (true, Some(path), health, summary)
}

fn workbuddy_auth_state() -> Result<AuthState> {
    let models = read_models_json().unwrap_or_else(|_| serde_json::json!([]));
    let portable = portable_workbuddy_entries(&models);
    let (desktop, desktop_path, desktop_health, desktop_summary) = desktop_login_present();
    if !portable.is_empty() {
        let mut also_present = Vec::new();
        if desktop {
            also_present.push("desktop-login".into());
        }
        return Ok(AuthState {
            agent: AgentId::WorkBuddy,
            kind: Some("api_key".into()),
            summary: format!("API key present in {} custom model(s)", portable.len()),
            has_credentials: true,
            health: AuthHealth::Configured,
            source: Some("workbuddy:models.json".into()),
            revision: workbuddy_config_dir()
                .ok()
                .map(|dir| dir.join("models.json"))
                .and_then(|path| auth_file_revision(&path)),
            also_present,
            secret_hash: None,
        });
    }
    if desktop {
        return Ok(AuthState {
            agent: AgentId::WorkBuddy,
            kind: Some("desktop-login".into()),
            summary: desktop_summary,
            has_credentials: true,
            health: desktop_health,
            source: Some("workbuddy:desktop-login-metadata".into()),
            revision: desktop_path
                .as_ref()
                .and_then(|path| auth_file_revision(path)),
            also_present: Vec::new(),
            secret_hash: None,
        });
    }
    Ok(AuthState {
        agent: AgentId::WorkBuddy,
        kind: None,
        summary: desktop_summary,
        has_credentials: false,
        health: desktop_health,
        source: desktop_path
            .is_some()
            .then(|| "workbuddy:desktop-login-metadata".into()),
        revision: desktop_path
            .as_ref()
            .and_then(|path| auth_file_revision(path)),
        also_present: Vec::new(),
        secret_hash: None,
    })
}

/// WorkBuddy writes `/v1/chat/completions`. OpenAI-compatible hosts that
/// expose `/chat/completions` (DeepSeek official) are rewritten to the `/v1` path.
pub(crate) fn normalize_workbuddy_chat_url(raw: &str) -> Result<String> {
    let trimmed = raw.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return Err(AppError::InvalidArg(
            "WorkBuddy 只认 /v1/chat/completions 端点".into(),
        ));
    }
    let lower = trimmed.to_ascii_lowercase();
    if lower.ends_with(WORKBUDDY_CHAT_COMPLETIONS_SUFFIX) {
        return Ok(trimmed.to_string());
    }
    if lower.ends_with("/v1") {
        return Ok(format!("{trimmed}/chat/completions"));
    }
    if let Some(prefix) = trimmed
        .get(..trimmed.len() - "/chat/completions".len())
        .filter(|_| lower.ends_with("/chat/completions"))
    {
        return Ok(format!("{prefix}{WORKBUDDY_CHAT_COMPLETIONS_SUFFIX}"));
    }
    Err(AppError::InvalidArg(
        "WorkBuddy 只认 /v1/chat/completions 端点".into(),
    ))
}

pub(crate) fn attach_api_key_catalog_fields(
    credentials: &mut serde_json::Value,
    base_url: Option<&str>,
    model_id: Option<&str>,
) -> Result<()> {
    let url = base_url.map(str::trim).filter(|s| !s.is_empty());
    let id = model_id.map(str::trim).filter(|s| !s.is_empty());
    let (Some(url), Some(id)) = (url, id) else {
        return Err(AppError::InvalidArg(
            "WorkBuddy 添加登录需要模型名称和 /v1/chat/completions 接口地址".into(),
        ));
    };
    let normalized = normalize_workbuddy_chat_url(url)?;
    let object = credentials.as_object_mut().ok_or_else(|| {
        AppError::InvalidArg("WorkBuddy credentials must be a JSON object".into())
    })?;
    object.insert("url".into(), serde_json::json!(normalized.clone()));
    object.insert("base_url".into(), serde_json::json!(normalized));
    object.insert("model_id".into(), serde_json::json!(id));
    object.insert("id".into(), serde_json::json!(id));
    Ok(())
}

fn restore_workbuddy_catalog(config: &AgentConfig) -> Result<()> {
    if config.agent != AgentId::WorkBuddy {
        return Err(AppError::InvalidArg(
            "config agent mismatch: expected workbuddy".into(),
        ));
    }
    let Some(models) = config.raw.get("models") else {
        return Ok(());
    };
    let models_path = workbuddy_config_dir()?.join("models.json");
    if let Some(parent) = models_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut bytes = serde_json::to_vec_pretty(models)?;
    bytes.push(b'\n');
    crate::utils::atomic::atomic_write(&models_path, &bytes)
}

fn upsert_workbuddy_model_from_account(account: &LiveAccount) -> Result<()> {
    if account.agent != AgentId::WorkBuddy {
        return Err(AppError::InvalidArg(
            "account agent mismatch for workbuddy".into(),
        ));
    }
    let key = account
        .credentials
        .get("api_key")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            AppError::InvalidArg("WorkBuddy account requires credentials.api_key".into())
        })?;
    let model_id = workbuddy_model_slot(&account.credentials);
    if model_id == "custom"
        && account
            .credentials
            .get("model_id")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .is_none()
        && account
            .credentials
            .get("id")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .is_none()
    {
        return Err(AppError::InvalidArg(
            "WorkBuddy 添加登录需要模型名称和 /v1/chat/completions 接口地址".into(),
        ));
    }
    let url = account
        .credentials
        .get("url")
        .and_then(serde_json::Value::as_str)
        .or_else(|| {
            account
                .credentials
                .get("base_url")
                .and_then(serde_json::Value::as_str)
        })
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| AppError::InvalidArg("WorkBuddy 只认 /v1/chat/completions 端点".into()))?;
    let url = normalize_workbuddy_chat_url(url)?;
    let name = account
        .credentials
        .get("name")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(&model_id);
    let vendor = account
        .credentials
        .get("vendor")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("Custom");
    let mut entry = serde_json::json!({
        "id": model_id,
        "name": name,
        "vendor": vendor,
        "url": url,
        "apiKey": key,
        "useCustomProtocol": false,
    });
    for flag in ["supportsToolCall", "supportsImages", "supportsReasoning"] {
        if let Some(value) = account.credentials.get(flag).cloned() {
            entry[flag] = value;
        } else if flag == "supportsToolCall" {
            entry[flag] = serde_json::json!(true);
        }
    }
    let models_path = workbuddy_config_dir()?.join("models.json");
    let live = read_json_value_or_empty(&models_path)?;
    let merged = merge_workbuddy_models(&live, &serde_json::json!([entry]))?;
    if let Some(parent) = models_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut bytes = serde_json::to_vec_pretty(&merged)?;
    bytes.push(b'\n');
    crate::utils::atomic::atomic_write(&models_path, &bytes)
}

#[cfg(test)]
mod tests;
