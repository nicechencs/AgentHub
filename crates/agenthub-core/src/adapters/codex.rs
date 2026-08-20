use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::error::{AppError, Result};
use crate::models::{
    AccountKind, AgentConfig, AgentId, AuthState, Capability, CapabilityState, DetectResult,
    LiveAccount, RunOptions, RunSpec,
};
use crate::runtime;
use crate::utils::atomic::atomic_write;
use crate::utils::paths::{agent_home, home_dir};

use super::{
    api_key_live_account, auth_file_revision, detect_binary, inspect_auth_credentials,
    oauth_auth_health, require_api_key, write_toml_config, AgentAdapter,
};

pub struct CodexAdapter;

/// Standalone install probe used by platform detectors (no full adapter required).
pub(crate) fn detect_installation() -> DetectResult {
    let requires = crate::catalog::install::adapter_install_channels(AgentId::Codex)
        .first()
        .map(|c| c.requires.clone())
        .unwrap_or_default();
    let env_ready = runtime::is_ready(&requires);
    detect_binary(
        AgentId::Codex,
        &["codex"],
        &["--version"],
        Some("npm"),
        env_ready,
    )
}

impl AgentAdapter for CodexAdapter {
    fn id(&self) -> AgentId {
        AgentId::Codex
    }

    fn detect(&self) -> DetectResult {
        detect_installation()
    }

    fn read_config(&self) -> Result<AgentConfig> {
        // Dual shape settingsConfig:
        //   { "format": "toml", "content": "<config.toml>", "auth": { "OPENAI_API_KEY": "..." } }
        // `auth` is only attached when live auth holds a non-empty API key
        // (OAuth token blobs stay out of the provider pool).
        let home = agent_home(AgentId::Codex)?;
        let path = home.join("config.toml");
        let mut raw = if path.exists() {
            let text = std::fs::read_to_string(&path)?;
            json!({ "format": "toml", "content": text })
        } else {
            json!({})
        };
        if let Some(api_key) = read_live_openai_api_key(&home.join("auth.json"))? {
            if let Some(obj) = raw.as_object_mut() {
                obj.insert("auth".into(), json!({ "OPENAI_API_KEY": api_key }));
            }
        }
        Ok(AgentConfig {
            agent: AgentId::Codex,
            raw,
        })
    }

    fn write_config(&self, config: &AgentConfig) -> Result<()> {
        let home = agent_home(AgentId::Codex)?;
        let path = home.join("config.toml");
        write_toml_config(AgentId::Codex, &path, config)?;
        // API providers may also write live auth with OPENAI_API_KEY.
        if let Some(api_key) = extract_settings_openai_api_key(&config.raw) {
            write_codex_api_key_auth(&home.join("auth.json"), &api_key)?;
        }
        Ok(())
    }

    fn read_auth(&self) -> Result<AuthState> {
        let home = agent_home(AgentId::Codex)?;
        Ok(codex_auth_state(&home.join("auth.json")))
    }

    fn read_account(&self) -> Result<LiveAccount> {
        let path = agent_home(AgentId::Codex)?.join("auth.json");
        if !path.exists() {
            return Err(AppError::NotFound(
                "no live Codex auth.json found to import".into(),
            ));
        }
        let text = std::fs::read_to_string(&path)?;
        let body: serde_json::Value = serde_json::from_str(&text)?;
        let label_hint = extract_codex_label(&body);
        Ok(LiveAccount {
            agent: AgentId::Codex,
            kind: AccountKind::Oauth,
            credentials: serde_json::json!({
                "format": "auth_json",
                "body": body,
            }),
            label_hint,
            extra: serde_json::json!({ "source": "auth.json" }),
        })
    }

    fn apply_account(&self, account: &LiveAccount) -> Result<()> {
        if account.agent != AgentId::Codex {
            return Err(AppError::InvalidArg(
                "account agent mismatch for codex".into(),
            ));
        }
        let format = account
            .credentials
            .get("format")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        // OAuth PKCE historically stored a flat token bundle (`type=oauth`) without
        // `format=auth_json`. Normalize that shape so switch can still write live.
        let credentials = match format {
            "auth_json" => account.credentials.clone(),
            "api_key" => {
                return Err(AppError::Unsupported(
                    "Codex live apply for API key accounts is not supported; import OAuth auth.json or use provider config".into(),
                ));
            }
            "" | "oauth" => normalize_oauth_credentials(&account.credentials)?,
            other => {
                // Unknown label, but still try token-bundle recovery before failing.
                match normalize_oauth_credentials(&account.credentials) {
                    Ok(normalized) => normalized,
                    Err(_) => {
                        return Err(AppError::InvalidArg(format!(
                            "unsupported Codex account credential format: {other} \
                             (expected auth_json with body, or OAuth token fields that can be converted)"
                        )));
                    }
                }
            }
        };
        let body = credentials.get("body").cloned().ok_or_else(|| {
            AppError::InvalidArg("Codex account credentials.body is required".into())
        })?;
        if !body.is_object() {
            return Err(AppError::InvalidArg(
                "Codex account credentials.body must be a JSON object".into(),
            ));
        }
        let home = agent_home(AgentId::Codex)?;
        let path = home.join("auth.json");
        let mut bytes = serde_json::to_vec_pretty(&body)?;
        bytes.push(b'\n');
        atomic_write(&path, &bytes)?;
        let written = std::fs::read_to_string(&path)?;
        let parsed: serde_json::Value = serde_json::from_str(&written)?;
        if parsed != body {
            return Err(AppError::message(
                "account.verify",
                "Codex auth.json verification failed after write",
            ));
        }
        // Official ChatGPT OAuth: drop leftover AgentHub 本机路由 keys so
        // Codex does not send this token at 127.0.0.1.
        crate::integrations::agents::codex::leftover::strip_bridge_leftovers_in_path(
            &home.join("config.toml"),
        )?;
        Ok(())
    }

    fn build_api_key_account(&self, api_key: &str) -> Result<LiveAccount> {
        // Codex primary auth is OAuth via auth.json; still allow storing an
        // OpenAI API key shaped credential for pool management / future apply.
        let key = require_api_key(api_key)?;
        Ok(api_key_live_account(
            AgentId::Codex,
            key,
            serde_json::json!({
                "format": "api_key",
                "api_key": key,
            }),
            "API Key",
            serde_json::json!({ "source": "manual" }),
        ))
    }

    fn skills_dir(&self) -> Option<PathBuf> {
        home_dir().ok().map(|h| h.join(".codex").join("skills"))
    }

    fn capability(&self, cap: Capability) -> CapabilityState {
        use Capability::*;
        match cap {
            ConfigWrite | AccountSwitch | Skills | LiveBackup | StructuredStream
            | DangerousMode | ProjectHistory | ProjectDelete | ProviderPresets => {
                CapabilityState::full()
            }
            ApiKeyAccount => CapabilityState::partial("可入池；live 应用仅支持 OAuth auth.json"),
            Usage => CapabilityState::full(),
            SessionResume => CapabilityState::partial(
                "Chat 后续轮次走 print+resume；终端可复制官方续接命令",
            ),
            Mcp | ModelSelect => CapabilityState::planned("待验证接入"),
        }
    }

    fn live_backup_paths(&self) -> Vec<PathBuf> {
        let mut paths = Vec::new();
        if let Ok(home) = agent_home(AgentId::Codex) {
            paths.push(home.join("config.toml"));
            paths.push(home.join("auth.json"));
        }
        paths
    }

    fn build_run_spec(&self, binary: &Path, prompt: &str, opts: &RunOptions) -> Result<RunSpec> {
        // text: codex exec <prompt>
        // structured (Chat): codex exec --json <prompt>  → JSONL process events
        // AgentHub Chat chooses the workdir (often not a trusted git repo).
        // Pass Codex's trust skip only on this AgentHub-managed spawn so a
        // user-picked folder works. Does not change `codex` invoked outside
        // AgentHub.
        let mut args = vec!["exec".into(), "--skip-git-repo-check".into()];
        if let Some(sid) = opts
            .native_session_id
            .as_deref()
            .and_then(super::session_resume::valid_session_id)
        {
            args.push("resume".into());
            args.push(sid.to_string());
        }
        if super::wants_structured_for(opts.process_mode, AgentId::Codex) {
            args.push("--json".into());
        }
        if opts.allow_dangerous {
            args.push("--dangerously-bypass-approvals-and-sandbox".into());
        }
        args.push(prompt.to_string());
        Ok(RunSpec {
            agent: AgentId::Codex,
            program: binary.to_path_buf(),
            args,
            cwd: opts.cwd.clone(),
            env: vec![],
        })
    }
}

pub(crate) fn codex_auth_state(auth: &Path) -> AuthState {
    if !auth.is_file() {
        return AuthState {
            agent: AgentId::Codex,
            kind: None,
            summary: "no auth.json".into(),
            has_credentials: false,
            health: crate::models::AuthHealth::Missing,
            source: Some("codex:auth.json".into()),
            revision: None,
            also_present: Vec::new(),
        };
    }
    let body = match std::fs::read_to_string(auth)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
    {
        Some(body) => body,
        None => {
            return AuthState {
                agent: AgentId::Codex,
                kind: None,
                summary: "auth.json could not be parsed".into(),
                has_credentials: false,
                health: crate::models::AuthHealth::Unknown,
                source: Some("codex:auth.json".into()),
                revision: auth_file_revision(auth),
                also_present: Vec::new(),
            };
        }
    };
    let has_api_key = body
        .get("OPENAI_API_KEY")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .is_some_and(|s| !s.is_empty());
    let metadata = inspect_auth_credentials(&body);
    let has_oauth = metadata.has_access_token || metadata.has_refresh_token;
    if has_api_key {
        let state = AuthState {
            agent: AgentId::Codex,
            kind: Some("api_key".into()),
            summary: "OPENAI_API_KEY present in auth.json".into(),
            has_credentials: true,
            health: crate::models::AuthHealth::Configured,
            source: Some("codex:auth.json".into()),
            revision: auth_file_revision(auth),
            also_present: Vec::new(),
        };
        return if has_oauth {
            state.with_also_present(["oauth"])
        } else {
            state
        };
    }
    if !has_oauth {
        return AuthState {
            agent: AgentId::Codex,
            kind: None,
            summary: "auth.json present but credentials could not be classified".into(),
            has_credentials: false,
            health: crate::models::AuthHealth::Unknown,
            source: Some("codex:auth.json".into()),
            revision: auth_file_revision(auth),
            also_present: Vec::new(),
        };
    }
    let health = oauth_auth_health(metadata);
    AuthState {
        agent: AgentId::Codex,
        kind: Some("oauth".into()),
        summary: if health == crate::models::AuthHealth::NeedsLogin {
            "Codex OAuth credentials are expired; run `codex login`".into()
        } else {
            "Codex OAuth credentials present".into()
        },
        has_credentials: true,
        health,
        source: Some("codex:auth.json".into()),
        revision: auth_file_revision(auth),
        also_present: Vec::new(),
    }
}

fn extract_codex_label(body: &serde_json::Value) -> Option<String> {
    body.get("account")
        .and_then(|a| a.get("email"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .or_else(|| {
            body.get("email")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
        .or_else(|| Some("codex-oauth".into()))
}

/// Read OPENAI_API_KEY from live auth.json when it is a non-empty string.
/// OAuth-shaped files often set `"OPENAI_API_KEY": null` — those are ignored.
fn read_live_openai_api_key(path: &Path) -> Result<Option<String>> {
    if !path.exists() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(path)?;
    let body: Value = serde_json::from_str(&text)?;
    Ok(body
        .get("OPENAI_API_KEY")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string()))
}

/// Pull API key from provider settings_config (AgentHub + dual-shape aliases).
fn extract_settings_openai_api_key(raw: &Value) -> Option<String> {
    let auth = raw.get("auth")?;
    auth.get("OPENAI_API_KEY")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

/// Write API-key mode auth.json. Replaces OAuth blob.
fn write_codex_api_key_auth(path: &Path, api_key: &str) -> Result<()> {
    crate::integrations::agents::codex::write_api_key_auth(path, api_key)
}



/// Convert generic OAuth token fields into the Codex pool/live shape:
/// `{ format: "auth_json", body: { auth_mode, OPENAI_API_KEY, tokens, last_refresh }, ... }`.
///
/// Accepts:
/// - already-normalized `auth_json` credentials (returned as-is when body is valid)
/// - flat token bundles from PKCE (`type=oauth`, top-level access/refresh/id tokens)
/// - nested `body.tokens` / `tokens` maps
///
/// Identity fields already present on `credentials` are preserved at the top level
/// so refresh/heal/UI keep working without re-decoding JWTs.
pub(crate) fn normalize_oauth_credentials(credentials: &Value) -> Result<Value> {
    if credentials
        .get("format")
        .and_then(|v| v.as_str())
        .is_some_and(|f| f == "auth_json")
    {
        if credentials
            .get("body")
            .and_then(|b| b.as_object())
            .is_some_and(|body| {
                body.get("tokens")
                    .and_then(|t| t.as_object())
                    .is_some_and(|tokens| {
                        tokens
                            .get("access_token")
                            .and_then(|v| v.as_str())
                            .is_some_and(|s| !s.is_empty())
                            || tokens
                                .get("refresh_token")
                                .and_then(|v| v.as_str())
                                .is_some_and(|s| !s.is_empty())
                    })
            })
        {
            return Ok(credentials.clone());
        }
    }

    let access = first_nonempty_str(
        credentials,
        &[
            "/access_token",
            "/tokens/access_token",
            "/body/tokens/access_token",
            "/raw/access_token",
            "/body/access_token",
        ],
    );
    let refresh = first_nonempty_str(
        credentials,
        &[
            "/refresh_token",
            "/tokens/refresh_token",
            "/body/tokens/refresh_token",
            "/raw/refresh_token",
            "/body/refresh_token",
        ],
    );
    let id_token = first_nonempty_str(
        credentials,
        &[
            "/id_token",
            "/tokens/id_token",
            "/body/tokens/id_token",
            "/raw/id_token",
            "/body/id_token",
        ],
    );
    let account_id = first_nonempty_str(
        credentials,
        &[
            "/account_id",
            "/tokens/account_id",
            "/body/tokens/account_id",
            "/chatgpt_account_id",
            "/body/account_id",
        ],
    );
    let last_refresh = first_nonempty_str(
        credentials,
        &["/last_refresh", "/body/last_refresh", "/raw/last_refresh"],
    )
    .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());

    let access = access.ok_or_else(|| {
        AppError::InvalidArg(
            "Codex OAuth credentials missing access_token; re-run OAuth login or import live auth.json"
                .into(),
        )
    })?;

    let mut tokens = serde_json::Map::new();
    tokens.insert("access_token".into(), json!(access));
    if let Some(ref rt) = refresh {
        tokens.insert("refresh_token".into(), json!(rt));
    }
    if let Some(ref idt) = id_token {
        tokens.insert("id_token".into(), json!(idt));
    }
    if let Some(ref aid) = account_id {
        tokens.insert("account_id".into(), json!(aid));
    }

    let body = json!({
        "auth_mode": "chatgpt",
        "OPENAI_API_KEY": null,
        "tokens": tokens,
        "last_refresh": last_refresh,
    });

    let mut cred = serde_json::Map::new();
    cred.insert("format".into(), json!("auth_json"));
    cred.insert("body".into(), body);
    // Flatten common token/identity fields for refresh + heal helpers.
    cred.insert("access_token".into(), json!(access));
    if let Some(rt) = refresh {
        cred.insert("refresh_token".into(), json!(rt));
    }
    if let Some(idt) = id_token {
        cred.insert("id_token".into(), json!(idt));
    }
    if let Some(aid) = account_id {
        cred.insert("account_id".into(), json!(aid));
    }
    for key in [
        "email",
        "sub",
        "organization_id",
        "org_uuid",
        "plan_type",
        "expires_at",
        "expires_in",
        "provider",
    ] {
        if let Some(v) = credentials.get(key).cloned() {
            if !v.is_null() {
                cred.entry(key.to_string()).or_insert(v);
            }
        }
    }
    Ok(Value::Object(cred))
}

fn first_nonempty_str(value: &Value, pointers: &[&str]) -> Option<String> {
    for pointer in pointers {
        if let Some(s) = value
            .pointer(pointer)
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            return Some(s.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn extract_settings_openai_api_key_reads_auth_block() {
        let raw = json!({
            "format": "toml",
            "content": "model = \"gpt-5\"\n",
            "auth": { "OPENAI_API_KEY": "sk-test-key" }
        });
        assert_eq!(
            extract_settings_openai_api_key(&raw).as_deref(),
            Some("sk-test-key")
        );
        assert!(extract_settings_openai_api_key(&json!({"format":"toml","content":"x"})).is_none());
        assert!(extract_settings_openai_api_key(&json!({"auth":{"OPENAI_API_KEY":""}})).is_none());
    }

    #[test]
    fn write_codex_api_key_auth_roundtrips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("auth.json");
        write_codex_api_key_auth(&path, "sk-from-pool").unwrap();
        let auth: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(auth["OPENAI_API_KEY"], "sk-from-pool");
        assert_eq!(
            read_live_openai_api_key(&path).unwrap().as_deref(),
            Some("sk-from-pool")
        );
    }

    #[test]
    fn read_live_openai_api_key_ignores_oauth_null() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("auth.json");
        std::fs::write(
            &path,
            r#"{ "OPENAI_API_KEY": null, "auth_mode": "chatgpt", "tokens": {} }"#,
        )
        .unwrap();
        assert!(read_live_openai_api_key(&path).unwrap().is_none());
    }

    #[test]
    fn normalize_oauth_credentials_from_pkce_bundle() {
        let bundle = json!({
            "type": "oauth",
            "provider": "codex",
            "access_token": "at-1",
            "refresh_token": "rt-1",
            "id_token": "idt-1",
            "account_id": "acc-1",
            "email": "user@example.com",
            "expires_at": "2026-08-20T00:00:00+00:00",
            "raw": {
                "access_token": "at-1",
                "refresh_token": "rt-1",
                "id_token": "idt-1"
            }
        });
        let normalized = normalize_oauth_credentials(&bundle).unwrap();
        assert_eq!(
            normalized.get("format").and_then(|v| v.as_str()),
            Some("auth_json")
        );
        assert_eq!(
            normalized
                .pointer("/body/auth_mode")
                .and_then(|v| v.as_str()),
            Some("chatgpt")
        );
        assert!(normalized
            .pointer("/body/OPENAI_API_KEY")
            .unwrap()
            .is_null());
        assert_eq!(
            normalized
                .pointer("/body/tokens/access_token")
                .and_then(|v| v.as_str()),
            Some("at-1")
        );
        assert_eq!(
            normalized
                .pointer("/body/tokens/refresh_token")
                .and_then(|v| v.as_str()),
            Some("rt-1")
        );
        assert_eq!(
            normalized
                .pointer("/body/tokens/account_id")
                .and_then(|v| v.as_str()),
            Some("acc-1")
        );
        assert_eq!(
            normalized.get("email").and_then(|v| v.as_str()),
            Some("user@example.com")
        );
        assert_eq!(
            normalized.get("refresh_token").and_then(|v| v.as_str()),
            Some("rt-1")
        );
    }

    #[test]
    fn normalize_oauth_credentials_keeps_valid_auth_json() {
        let already = json!({
            "format": "auth_json",
            "body": {
                "auth_mode": "chatgpt",
                "OPENAI_API_KEY": null,
                "tokens": {
                    "access_token": "at-keep",
                    "refresh_token": "rt-keep"
                },
                "last_refresh": "2026-08-01T00:00:00Z"
            },
            "email": "keep@example.com"
        });
        let normalized = normalize_oauth_credentials(&already).unwrap();
        assert_eq!(normalized, already);
    }

    #[test]
    fn normalize_oauth_credentials_requires_access_token() {
        let err = normalize_oauth_credentials(&json!({
            "type": "oauth",
            "provider": "codex",
            "refresh_token": "rt-only"
        }))
        .unwrap_err();
        assert_eq!(err.code(), "invalid_arg");
        assert!(err.to_string().contains("access_token"));
    }

    #[test]
    fn apply_account_strips_leftover_bridge_keys() {
        static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let codex = home.join(".codex");
        std::fs::create_dir_all(&codex).unwrap();
        std::fs::write(
            codex.join("config.toml"),
            r#"model_provider = "agenthub_grok_bridge"
preferred_auth_method = "apikey"

[model_providers.agenthub_grok_bridge]
base_url = "http://127.0.0.1:43121/v1"
wire_api = "responses"
"#,
        )
        .unwrap();
        let prev = std::env::var_os("HOME");
        std::env::set_var("HOME", &home);
        let account = LiveAccount {
            agent: AgentId::Codex,
            kind: AccountKind::Oauth,
            credentials: json!({
                "format": "auth_json",
                "body": {
                    "auth_mode": "chatgpt",
                    "OPENAI_API_KEY": null,
                    "tokens": {
                        "access_token": "at-official",
                        "refresh_token": "rt-official"
                    },
                    "last_refresh": "2026-08-20T00:00:00Z"
                },
                "email": "41375197@qq.com"
            }),
            label_hint: Some("41375197@qq.com".into()),
            extra: json!({}),
        };
        let result = CodexAdapter.apply_account(&account);
        match prev {
            Some(value) => std::env::set_var("HOME", value),
            None => std::env::remove_var("HOME"),
        }
        result.unwrap();
        let stored = std::fs::read_to_string(codex.join("config.toml")).unwrap();
        assert!(!stored.contains("agenthub_grok_bridge"));
        assert!(!stored.contains("preferred_auth_method"));
        assert!(!stored.contains("127.0.0.1"));
        let auth: Value =
            serde_json::from_str(&std::fs::read_to_string(codex.join("auth.json")).unwrap()).unwrap();
        assert_eq!(auth["tokens"]["access_token"], "at-official");
    }
}
