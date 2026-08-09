use std::path::{Path, PathBuf};

use super::pi_auth::{
    combined_live_account, expand_auth_to_live_accounts, merge_auth_json, pi_config_dir,
    read_auth_json,
};
use super::{
    api_key_live_account, auth_file_revision, detect_binary, inspect_auth_credentials,
    oauth_auth_health, require_api_key, AgentAdapter,
};
use crate::error::{AppError, Result};
use crate::models::{
    AccountKind, AgentConfig, AgentId, AuthState, Capability, CapabilityState, DetectResult,
    InstallChannel, LiveAccount, RunOptions, RunSpec, RuntimeId,
};
use crate::runtime;
use crate::utils::atomic::atomic_write;

pub struct PiAdapter;

impl AgentAdapter for PiAdapter {
    fn id(&self) -> AgentId {
        AgentId::Pi
    }

    fn detect(&self) -> DetectResult {
        let requires = self
            .install_channels()
            .first()
            .map(|c| c.requires.clone())
            .unwrap_or_default();
        let env_ready = runtime::is_ready(&requires);
        // Prefer PATH `pi` (npm global shim); channel inferred from path / well-known.
        detect_binary(AgentId::Pi, &["pi"], &["--version"], Some("npm"), env_ready)
    }

    fn install_channels(&self) -> Vec<InstallChannel> {
        // npm only: upstream also documents `curl https://pi.dev/install.sh | sh`
        // and Homebrew, but AgentHub's native channel is Windows ps1 / Unix sh
        // allowlists. install.sh is Unix shell (not a Windows-native installer),
        // and Homebrew/winget are outside the current Runtime/InstallChannel
        // abstraction — keep channels honest rather than advertising dead paths.
        vec![InstallChannel {
            id: "npm".into(),
            label: "npm (@earendil-works/pi-coding-agent)".into(),
            requires: vec![RuntimeId::NodeJs, RuntimeId::Npm],
            min_runtime_notes: Some("Node.js >= 18; install uses --ignore-scripts".into()),
        }]
    }

    fn read_config(&self) -> Result<AgentConfig> {
        let dir = pi_config_dir()?;
        let settings_path = dir.join("settings.json");
        let models_path = dir.join("models.json");

        let settings = read_json_object_or_empty(&settings_path)?;
        let models = if models_path.exists() {
            Some(read_json_object_or_empty(&models_path)?)
        } else {
            None
        };

        // Raw envelope for display; write_config is fail-closed until merge rules
        // for settings/models are proven safe.
        let mut raw = serde_json::Map::new();
        raw.insert("settings".into(), settings);
        if let Some(m) = models {
            raw.insert("models".into(), m);
        }
        raw.insert(
            "paths".into(),
            serde_json::json!({
                "configDir": dir,
                "settings": settings_path,
                "models": models_path,
            }),
        );

        Ok(AgentConfig {
            agent: AgentId::Pi,
            raw: serde_json::Value::Object(raw),
        })
    }

    fn write_config(&self, _config: &AgentConfig) -> Result<()> {
        write_pi_config(_config)
    }

    fn read_auth(&self) -> Result<AuthState> {
        let auth = pi_config_dir()?.join("auth.json");
        let has = auth.is_file();
        let (kind, summary, has_credentials, health) = if !has {
            (
                None,
                "no auth.json".into(),
                false,
                crate::models::AuthHealth::Missing,
            )
        } else {
            match read_auth_json().and_then(|body| {
                let n = body.as_object().map(|o| o.len()).unwrap_or(0);
                if n == 0 {
                    return Ok((
                        None,
                        "auth.json exists but contains no provider credentials".into(),
                        false,
                        crate::models::AuthHealth::Unknown,
                    ));
                }
                let entries = expand_auth_to_live_accounts(&body)?;
                let has_oauth = entries.iter().any(|entry| entry.kind == AccountKind::Oauth);
                let has_api_key = entries
                    .iter()
                    .any(|entry| entry.kind == AccountKind::ApiKey);
                let kind = match (has_oauth, has_api_key) {
                    (true, true) => Some("mixed"),
                    (true, false) => Some("oauth"),
                    (false, true) => Some("api_key"),
                    (false, false) => Some("file-auth.json"),
                };
                let summary = format!(
                    "auth.json present ({n} provider credentials; {})",
                    kind.unwrap_or("file-auth.json")
                );
                let provider_healths: Vec<_> = body
                    .as_object()
                    .expect("Pi auth.json was validated as an object")
                    .values()
                    .map(pi_provider_auth_health)
                    .collect();
                let health = aggregate_pi_provider_auth_health(provider_healths);
                if health == crate::models::AuthHealth::Unknown {
                    return Ok((
                        None,
                        "auth.json present but credentials could not be classified".into(),
                        false,
                        crate::models::AuthHealth::Unknown,
                    ));
                }
                Ok((
                    kind.map(str::to_owned),
                    summary,
                    !entries.is_empty(),
                    health,
                ))
            }) {
                Ok(result) => result,
                Err(_) => (
                    None,
                    "auth.json present but credentials could not be classified".into(),
                    false,
                    crate::models::AuthHealth::Unknown,
                ),
            }
        };
        Ok(AuthState {
            agent: AgentId::Pi,
            kind,
            summary,
            has_credentials,
            health,
            source: Some("pi:auth.json".into()),
            revision: auth_file_revision(&auth),
        })
    }

    fn read_account(&self) -> Result<LiveAccount> {
        let body = read_auth_json()?;
        if body.as_object().map(|o| o.is_empty()).unwrap_or(true) {
            return Err(AppError::NotFound(
                "no live Pi auth.json found to import".into(),
            ));
        }
        // Combined snapshot for "import whole file" / live status.
        // Multi-provider expansion happens in AccountService::import_live.
        combined_live_account(&body)
    }

    fn apply_account(&self, account: &LiveAccount) -> Result<()> {
        if account.agent != AgentId::Pi {
            return Err(AppError::InvalidArg("account agent mismatch for pi".into()));
        }
        let format = account
            .credentials
            .get("format")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        match format {
            "auth_json" => {
                let body = account.credentials.get("body").cloned().ok_or_else(|| {
                    AppError::InvalidArg("Pi account credentials.body is required".into())
                })?;
                if !body.is_object() {
                    return Err(AppError::InvalidArg(
                        "Pi account credentials.body must be a JSON object".into(),
                    ));
                }
                // Merge provider keys so switching one OAuth account does not
                // wipe other providers already stored in auth.json.
                let merged = merge_auth_json(&body)?;
                let path = pi_config_dir()?.join("auth.json");
                let mut bytes = serde_json::to_vec_pretty(&merged)?;
                bytes.push(b'\n');
                atomic_write(&path, &bytes)?;
                let written = std::fs::read_to_string(&path)?;
                let parsed: serde_json::Value = serde_json::from_str(&written)?;
                if parsed != merged {
                    return Err(AppError::message(
                        "account.verify",
                        "Pi auth.json verification failed after write",
                    ));
                }
                Ok(())
            }
            "api_key" => Err(AppError::Unsupported(
                "Pi live apply for standalone API key accounts is not supported; \
                 import auth.json (provider-keyed) or use `pi --api-key` / models.json"
                    .into(),
            )),
            other => Err(AppError::InvalidArg(format!(
                "unsupported Pi account credential format: {other}"
            ))),
        }
    }

    fn build_api_key_account(&self, api_key: &str) -> Result<LiveAccount> {
        // Allow storing a pool entry for future use / manual apply; live apply
        // of this format remains Unsupported (auth.json provider schema varies).
        let key = require_api_key(api_key)?;
        Ok(api_key_live_account(
            AgentId::Pi,
            key,
            serde_json::json!({
                "format": "api_key",
                "api_key": key,
            }),
            "API Key",
            serde_json::json!({
                "source": "manual",
                "note": "pool-only; apply to live auth.json is unsupported"
            }),
        ))
    }

    fn skills_dir(&self) -> Option<PathBuf> {
        // Global skills true source per pi docs: ~/.pi/agent/skills
        // (not project-local .pi/skills).
        pi_config_dir().ok().map(|d| d.join("skills"))
    }

    fn capability(&self, cap: Capability) -> CapabilityState {
        use Capability::*;
        match cap {
            AccountSwitch | Skills | LiveBackup | StructuredStream | ProjectHistory
            | ProjectDelete => CapabilityState::full(),
            ConfigWrite => CapabilityState::full(),
            ApiKeyAccount => {
                CapabilityState::partial("可入池；auth.json provider schema 不稳定，不写回")
            }
            DangerousMode => CapabilityState::partial("--approve 仅信任项目文件，非完全跳过确认"),
            ProviderPresets => CapabilityState::unsupported("暂无内置 Pi provider 预设"),
            Usage => CapabilityState::full(),
            Mcp | ModelSelect | SessionResume => CapabilityState::planned("待验证接入"),
        }
    }

    fn live_backup_paths(&self) -> Vec<PathBuf> {
        let mut paths = Vec::new();
        if let Ok(dir) = pi_config_dir() {
            // Only stable, documented / measured files — no models-store cache.
            paths.push(dir.join("settings.json"));
            paths.push(dir.join("models.json"));
            paths.push(dir.join("auth.json"));
        }
        paths
    }

    fn build_run_spec(&self, binary: &Path, prompt: &str, opts: &RunOptions) -> Result<RunSpec> {
        // text: `pi -p "…"` `--mode text --no-session`
        // structured (Chat): `--mode json` event stream (NDJSON session events)
        // No public stable "always approve tools" flag — do not invent one.
        // `--approve` only trusts project-local files for the run (not tool YOLO).
        let mode = if super::wants_structured_for(opts.process_mode, AgentId::Pi) {
            "json"
        } else {
            "text"
        };
        let mut args = vec![
            "-p".into(),
            prompt.to_string(),
            "--mode".into(),
            mode.into(),
            "--no-session".into(),
        ];
        if opts.allow_dangerous {
            // Closest documented non-interactive trust flag for project files.
            args.push("--approve".into());
        }
        Ok(RunSpec {
            agent: AgentId::Pi,
            program: binary.to_path_buf(),
            args,
            cwd: opts.cwd.clone(),
            env: vec![],
        })
    }
}

/// Classify one Pi provider entry without mixing expiry/token facts from its
/// siblings.  A provider that contains an API key remains configured even if
/// stale OAuth fields are also present.
pub(crate) fn pi_provider_auth_health(entry: &serde_json::Value) -> crate::models::AuthHealth {
    let metadata = inspect_auth_credentials(entry);
    if metadata.has_api_key {
        crate::models::AuthHealth::Configured
    } else if metadata.has_access_token || metadata.has_refresh_token {
        oauth_auth_health(metadata)
    } else {
        crate::models::AuthHealth::Unknown
    }
}

/// Aggregate provider health by usable capability.  `NeedsLogin` is only the
/// overall state when every present provider has been classified as unusable;
/// an unknown entry intentionally keeps the result conservative rather than
/// claiming the whole agent is signed out.
pub(crate) fn aggregate_pi_provider_auth_health<I>(provider_healths: I) -> crate::models::AuthHealth
where
    I: IntoIterator<Item = crate::models::AuthHealth>,
{
    use crate::models::AuthHealth;

    fn rank(health: &AuthHealth) -> u8 {
        match health {
            AuthHealth::Verified => 5,
            AuthHealth::Renewable => 4,
            AuthHealth::Configured => 3,
            AuthHealth::Unknown => 2,
            AuthHealth::NeedsLogin => 1,
            AuthHealth::Missing => 0,
        }
    }

    provider_healths
        .into_iter()
        .max_by_key(rank)
        .unwrap_or(AuthHealth::Missing)
}

fn read_json_object_or_empty(path: &Path) -> Result<serde_json::Value> {
    if !path.exists() {
        return Ok(serde_json::json!({}));
    }
    let text = std::fs::read_to_string(path)?;
    let value: serde_json::Value = serde_json::from_str(&text)?;
    if !value.is_object() {
        return Err(AppError::InvalidArg(format!(
            "{} must be a JSON object",
            path.display()
        )));
    }
    Ok(value)
}

const REDACTED_MARKER: &str = "***";

fn write_pi_config(config: &AgentConfig) -> Result<()> {
    if config.agent != AgentId::Pi {
        return Err(AppError::InvalidArg(format!(
            "config agent mismatch: expected pi, got {}",
            config.agent.as_str()
        )));
    }
    let raw = config
        .raw
        .as_object()
        .ok_or_else(|| AppError::InvalidArg("Pi settings_config must be a JSON object".into()))?;
    let dir = pi_config_dir()?;

    let desired_models = raw
        .get("models")
        .or_else(|| raw.get("providers").map(|_| &config.raw))
        .ok_or_else(|| {
            AppError::InvalidArg(
                "Pi settings_config must contain models.providers or providers".into(),
            )
        })?;
    let models_path = dir.join("models.json");
    let live_models = read_json_object_or_empty(&models_path)?;
    let merged = merge_pi_models(&live_models, desired_models)?;
    let merged_settings = raw
        .get("settings")
        .map(|settings| {
            if !settings.is_object() {
                return Err(AppError::InvalidArg(
                    "Pi settings_config.settings must be a JSON object".into(),
                ));
            }
            let live_settings = read_json_object_or_empty(&dir.join("settings.json"))?;
            Ok(merge_redacted_json(&live_settings, settings))
        })
        .transpose()?;

    // A live snapshot contains settings + models + paths.  Only settings is
    // written back from that envelope; paths is adapter metadata and is never
    // persisted to Pi's files.
    if let Some(settings) = merged_settings {
        write_json_value(&dir.join("settings.json"), &settings)?;
    }
    write_json_value(&models_path, &merged)?;
    Ok(())
}

fn write_json_value(path: &Path, value: &serde_json::Value) -> Result<()> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    atomic_write(path, &bytes)
}

/// Merge Pi's documented `{ "providers": { ... } }` model store by provider
/// key.  Unrelated providers and unknown fields remain intact.  A redacted
/// secret (`***`) from the UI never replaces an existing credential.
fn merge_pi_models(
    live: &serde_json::Value,
    desired: &serde_json::Value,
) -> Result<serde_json::Value> {
    let live_obj = live.as_object().ok_or_else(|| {
        AppError::InvalidArg("existing Pi models.json must be a JSON object".into())
    })?;
    let desired_obj = desired.as_object().ok_or_else(|| {
        AppError::InvalidArg("target Pi models.json must be a JSON object".into())
    })?;
    let desired_providers = desired_obj
        .get("providers")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| {
            AppError::InvalidArg("target Pi models.json.providers must be a JSON object".into())
        })?;

    let mut merged = live_obj.clone();
    let mut providers = merged
        .remove("providers")
        .unwrap_or_else(|| serde_json::json!({}));
    let providers_obj = providers.as_object_mut().ok_or_else(|| {
        AppError::InvalidArg("existing Pi models.json.providers must be a JSON object".into())
    })?;
    for (provider, desired_config) in desired_providers {
        if !desired_config.is_object() {
            return Err(AppError::InvalidArg(format!(
                "Pi provider {provider} must be a JSON object"
            )));
        }
        let next = providers_obj
            .get(provider)
            .map(|existing| merge_redacted_json(existing, desired_config))
            .unwrap_or_else(|| desired_config.clone());
        providers_obj.insert(provider.clone(), next);
    }
    merged.insert("providers".into(), providers);

    // Apply desired top-level options (for example `baseUrl` overrides) while
    // retaining unknown keys already present in the live file.
    for (key, value) in desired_obj {
        if key != "providers" {
            merged.insert(key.clone(), value.clone());
        }
    }
    Ok(serde_json::Value::Object(merged))
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn build_run_spec_print_mode() {
        let adapter = PiAdapter;
        let bin = PathBuf::from("pi");
        let opts = RunOptions::default();
        let spec = adapter.build_run_spec(&bin, "hello", &opts).unwrap();
        assert_eq!(spec.agent, AgentId::Pi);
        assert_eq!(spec.program, bin);
        assert_eq!(spec.args[0], "-p");
        assert_eq!(spec.args[1], "hello");
        assert!(spec.args.iter().any(|a| a == "--mode"));
        assert!(spec.args.iter().any(|a| a == "text"));
        assert!(spec.args.iter().any(|a| a == "--no-session"));
        assert!(!spec.args.iter().any(|a| a == "--approve"));
    }

    #[test]
    fn build_run_spec_allow_dangerous_adds_approve() {
        let adapter = PiAdapter;
        let mut opts = RunOptions::default();
        opts.allow_dangerous = true;
        let spec = adapter.build_run_spec(Path::new("pi"), "x", &opts).unwrap();
        assert!(spec.args.iter().any(|a| a == "--approve"));
    }

    #[test]
    fn install_channels_npm_only() {
        let channels = PiAdapter.install_channels();
        assert_eq!(channels.len(), 1);
        assert_eq!(channels[0].id, "npm");
        assert!(channels[0].requires.contains(&RuntimeId::NodeJs));
        assert!(channels[0].requires.contains(&RuntimeId::Npm));
    }

    #[test]
    fn skills_dir_is_under_agent_config() {
        let dir = PiAdapter.skills_dir().expect("skills_dir");
        let s = dir.to_string_lossy().replace('\\', "/");
        assert!(
            s.ends_with("/.pi/agent/skills") || s.contains("/agent/skills"),
            "unexpected skills_dir: {s}"
        );
    }

    #[test]
    fn live_backup_paths_include_settings_and_auth() {
        let paths = PiAdapter.live_backup_paths();
        assert!(!paths.is_empty());
        let joined: Vec<String> = paths
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert!(joined.iter().any(|n| n == "settings.json"));
        assert!(joined.iter().any(|n| n == "auth.json"));
        assert!(joined.iter().any(|n| n == "models.json"));
    }

    #[test]
    fn merge_models_preserves_unrelated_providers_and_redacted_keys() {
        let live = json!({
            "providers": {
                "keep": { "baseUrl": "https://keep", "apiKey": "live-secret", "unknown": 1 },
                "custom": { "baseUrl": "https://old", "apiKey": "old-secret" }
            },
            "unknownTopLevel": true
        });
        let desired = json!({
            "providers": {
                "custom": { "baseUrl": "https://new", "apiKey": "***" }
            }
        });
        let merged = merge_pi_models(&live, &desired).unwrap();
        assert_eq!(merged["providers"]["keep"]["apiKey"], "live-secret");
        assert_eq!(merged["providers"]["keep"]["unknown"], 1);
        assert_eq!(merged["providers"]["custom"]["baseUrl"], "https://new");
        assert_eq!(merged["providers"]["custom"]["apiKey"], "old-secret");
        assert_eq!(merged["unknownTopLevel"], true);
    }

    #[test]
    fn merge_models_requires_provider_object() {
        let err = merge_pi_models(&json!({}), &json!({"models": []})).unwrap_err();
        assert_eq!(err.code(), "invalid_arg");
    }
}
