use std::path::{Path, PathBuf};

use super::pi_auth::{
    apply_pi_api_key_to_dir, combined_live_account, expand_auth_to_live_accounts, merge_auth_json,
    pi_config_dir, read_auth_json, write_verified_auth_json,
};
use super::{
    api_key_live_account, auth_file_revision, detect_binary, inspect_auth_credentials,
    oauth_auth_health, require_api_key, AgentAdapter,
};
use crate::error::{AppError, Result};
use crate::models::{
    AccountKind, AgentConfig, AgentId, AuthState, Capability, CapabilityState, DetectResult,
    LiveAccount, RunOptions, RunSpec,
};
use crate::runtime;
use crate::utils::atomic::atomic_write;

pub struct PiAdapter;

/// Standalone install probe used by platform detectors (no full adapter required).
pub(crate) fn detect_installation() -> DetectResult {
    let requires = crate::catalog::install::adapter_install_channels(AgentId::Pi)
        .first()
        .map(|c| c.requires.clone())
        .unwrap_or_default();
    let env_ready = runtime::is_ready(&requires);
    // Prefer PATH `pi` (npm global shim); channel inferred from path / well-known.
    detect_binary(AgentId::Pi, &["pi"], &["--version"], Some("npm"), env_ready)
}

impl AgentAdapter for PiAdapter {
    fn id(&self) -> AgentId {
        AgentId::Pi
    }

    fn detect(&self) -> DetectResult {
        detect_installation()
    }

    fn read_config(&self) -> Result<AgentConfig> {
        let dir = pi_config_dir()?;
        let settings_path = dir.join("settings.json");
        let models_path = dir.join("models.json");
        let auth_path = dir.join("auth.json");

        let settings = read_json_object_or_empty(&settings_path)?;
        let models = if models_path.exists() {
            Some(read_json_object_or_empty(&models_path)?)
        } else {
            None
        };
        let auth = read_json_object_or_empty(&auth_path)?;

        // Raw envelope for display; write_config is fail-closed until merge rules
        // for settings/models are proven safe.
        let mut raw = serde_json::Map::new();
        raw.insert("settings".into(), settings);
        if let Some(m) = models {
            raw.insert("models".into(), m);
        }
        raw.insert("auth".into(), auth);
        raw.insert(
            "paths".into(),
            serde_json::json!({
                "configDir": dir,
                "settings": settings_path,
                "models": models_path,
                "auth": auth_path,
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
        Ok(pi_auth_state(&pi_config_dir()?.join("auth.json")))
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
                write_verified_auth_json(&pi_config_dir()?.join("auth.json"), &merged)
            }
            "api_key" => {
                let key = account
                    .credentials
                    .get("api_key")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| AppError::InvalidArg("Pi api_key is required".into()))?;
                let key = require_api_key(key)?;
                let provider = account
                    .credentials
                    .get("provider")
                    .and_then(|v| v.as_str())
                    .or_else(|| account.extra.get("provider").and_then(|v| v.as_str()))
                    .unwrap_or("");
                apply_pi_api_key_to_dir(&pi_config_dir()?, provider, key)
            }
            other => Err(AppError::InvalidArg(format!(
                "unsupported Pi account credential format: {other}"
            ))),
        }
    }

    fn build_api_key_account(&self, api_key: &str) -> Result<LiveAccount> {
        // Pool-only: no official slot is known at add time. Live apply writes
        // auth.json only when credentials.provider / extra.provider is an
        // official slot (anthropic/openai/…).
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
                "note": "pool-only unless credentials.provider is an official auth.json slot"
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
            ApiKeyAccount => CapabilityState::partial(
                "可入池；写回 auth.json 需带官方厂商槽（anthropic/openai/…）；自定义 URL 走 models.json / 供应商切换",
            ),
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

/// Classify the given Pi `auth.json` path. Does not call `read_auth_json()`
/// (that helper re-enters `pi_config_dir()`).
pub(crate) fn pi_auth_state(auth: &Path) -> AuthState {
    let has = auth.is_file();
    let (kind, summary, has_credentials, health) = if !has {
        (
            None,
            "no auth.json".into(),
            false,
            crate::models::AuthHealth::Missing,
        )
    } else {
        match read_pi_auth_json_object(auth).and_then(|body| {
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
    let state = AuthState {
        agent: AgentId::Pi,
        kind,
        summary,
        has_credentials,
        health,
        source: Some("pi:auth.json".into()),
        revision: auth_file_revision(auth),
        also_present: Vec::new(),
    };
    if state.kind.as_deref() == Some("mixed") {
        state.with_also_present(["oauth", "api_key"])
    } else {
        state
    }
}

fn read_pi_auth_json_object(auth: &Path) -> Result<serde_json::Value> {
    let text = std::fs::read_to_string(auth)?;
    let body: serde_json::Value = serde_json::from_str(&text)?;
    if !body.is_object() {
        return Err(AppError::InvalidArg(
            "Pi auth.json must be a JSON object".into(),
        ));
    }
    Ok(body)
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

    let models_path = dir.join("models.json");
    let merged_models = raw
        .get("models")
        .or_else(|| raw.get("providers").map(|_| &config.raw))
        .map(|desired_models| {
            let live_models = read_json_object_or_empty(&models_path)?;
            merge_pi_models(&live_models, desired_models)
        })
        .transpose()?;
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

    if merged_models.is_none() && raw.get("auth").is_none() {
        return Err(AppError::InvalidArg(
            "Pi settings_config must contain models.providers, providers, or auth".into(),
        ));
    }

    let merged_auth = raw
        .get("auth")
        .map(|auth| {
            if !auth.is_object() {
                return Err(AppError::InvalidArg(
                    "Pi settings_config.auth must be a JSON object".into(),
                ));
            }
            if raw.contains_key("paths") {
                Ok(auth.clone())
            } else {
                let live_auth = read_json_object_or_empty(&dir.join("auth.json"))?;
                Ok(merge_redacted_json(&live_auth, auth))
            }
        })
        .transpose()?;

    // A live snapshot contains settings + models + auth + paths.  `paths`
    // makes auth a complete-file restore; generated apply envelopes merge only
    // the provider keys they own.
    if let Some(settings) = merged_settings {
        write_json_value(&dir.join("settings.json"), &settings)?;
    }
    if let Some(models) = merged_models {
        write_json_value(&models_path, &models)?;
    }
    if let Some(auth) = merged_auth {
        write_json_value(&dir.join("auth.json"), &auth)?;
    }
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
    // Root apply envelopes may include auth/settings/paths next to
    // `providers`. Those belong in their own files — never models.json.
    for (key, value) in desired_obj {
        if matches!(key.as_str(), "providers" | "auth" | "settings" | "paths") {
            continue;
        }
        merged.insert(key.clone(), value.clone());
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
    use crate::models::RuntimeId;
    use serde_json::json;
    use std::sync::Mutex;

    static PI_CONFIG_ENV_LOCK: Mutex<()> = Mutex::new(());

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

    #[test]
    fn write_config_auth_only_merges_and_snapshot_auth_replaces() {
        let _guard = PI_CONFIG_ENV_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let previous = std::env::var_os("PI_CODING_AGENT_DIR");
        std::env::set_var("PI_CODING_AGENT_DIR", dir.path());
        std::fs::write(
            dir.path().join("auth.json"),
            serde_json::to_vec_pretty(&json!({
                "keep": { "type": "oauth", "access": "keep-access" },
                "anthropic": { "type": "oauth", "access": "old-access", "refresh": "old-refresh" }
            }))
            .unwrap(),
        )
        .unwrap();

        write_pi_config(&AgentConfig {
            agent: AgentId::Pi,
            raw: json!({
                "auth": {
                    "anthropic": {
                        "type": "oauth",
                        "access": "new-access",
                        "refresh": "new-refresh"
                    },
                    "keep": { "type": "oauth", "access": "***" }
                }
            }),
        })
        .unwrap();
        let merged = read_json_object_or_empty(&dir.path().join("auth.json")).unwrap();
        assert_eq!(merged["keep"]["access"], "keep-access");
        assert_eq!(merged["anthropic"]["access"], "new-access");

        write_pi_config(&AgentConfig {
            agent: AgentId::Pi,
            raw: json!({
                "auth": {
                    "only": { "type": "oauth", "access": "snapshot-access" }
                },
                "paths": { "auth": "snapshot.json" }
            }),
        })
        .unwrap();
        let replaced = read_json_object_or_empty(&dir.path().join("auth.json")).unwrap();
        assert_eq!(
            replaced,
            json!({
                "only": { "type": "oauth", "access": "snapshot-access" }
            })
        );

        match previous {
            Some(value) => std::env::set_var("PI_CODING_AGENT_DIR", value),
            None => std::env::remove_var("PI_CODING_AGENT_DIR"),
        }
    }

    #[test]
    fn write_config_models_and_auth_do_not_cross_files() {
        let _guard = PI_CONFIG_ENV_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let previous = std::env::var_os("PI_CODING_AGENT_DIR");
        std::env::set_var("PI_CODING_AGENT_DIR", dir.path());
        std::fs::write(
            dir.path().join("auth.json"),
            serde_json::to_vec_pretty(&json!({
                "keep": { "type": "oauth", "access": "keep-access" }
            }))
            .unwrap(),
        )
        .unwrap();
        std::fs::write(
            dir.path().join("models.json"),
            serde_json::to_vec_pretty(&json!({
                "providers": {
                    "keep": { "baseUrl": "https://keep.example", "apiKey": "keep-secret" }
                }
            }))
            .unwrap(),
        )
        .unwrap();

        write_pi_config(&AgentConfig {
            agent: AgentId::Pi,
            raw: json!({
                "models": {
                    "providers": {
                        "custom": {
                            "baseUrl": "https://relay.example/v1",
                            "api": "openai-completions",
                            "apiKey": "sk-relay",
                            "models": [{ "id": "custom-model" }]
                        }
                    }
                },
                "auth": {
                    "openai": { "type": "api_key", "key": "sk-openai" }
                }
            }),
        })
        .unwrap();

        let models = read_json_object_or_empty(&dir.path().join("models.json")).unwrap();
        assert_eq!(
            models["providers"]["custom"]["baseUrl"],
            "https://relay.example/v1"
        );
        assert_eq!(models["providers"]["keep"]["apiKey"], "keep-secret");
        assert!(models.get("auth").is_none(), "auth must not leak into models.json");

        let auth = read_json_object_or_empty(&dir.path().join("auth.json")).unwrap();
        assert_eq!(auth["openai"]["type"], "api_key");
        assert_eq!(auth["openai"]["key"], "sk-openai");
        assert_eq!(auth["keep"]["access"], "keep-access");

        // Legacy root `{ providers, auth }` must also keep auth out of models.json.
        write_pi_config(&AgentConfig {
            agent: AgentId::Pi,
            raw: json!({
                "providers": {
                    "extra": { "baseUrl": "https://extra.example", "apiKey": "sk-extra" }
                },
                "auth": {
                    "deepseek": { "type": "api_key", "key": "sk-ds" }
                }
            }),
        })
        .unwrap();
        let models = read_json_object_or_empty(&dir.path().join("models.json")).unwrap();
        assert_eq!(models["providers"]["extra"]["baseUrl"], "https://extra.example");
        assert!(models.get("auth").is_none());
        let auth = read_json_object_or_empty(&dir.path().join("auth.json")).unwrap();
        assert_eq!(auth["deepseek"]["key"], "sk-ds");
        assert_eq!(auth["openai"]["key"], "sk-openai");

        match previous {
            Some(value) => std::env::set_var("PI_CODING_AGENT_DIR", value),
            None => std::env::remove_var("PI_CODING_AGENT_DIR"),
        }
    }
}
