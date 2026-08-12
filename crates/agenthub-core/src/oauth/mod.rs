//! OAuth PKCE loopback login (browser authorize → localhost callback → token exchange).
//!
//! Uses a short-lived in-process session store + std TCP listener (no Tokio runtime).

mod catalog;
mod device;
mod identity;
mod pi_refresh;
mod pkce;
mod providers;
mod server;
mod session;

pub use catalog::{
    is_device_code_option, list_oauth_options, pi_auth_json_key, resolve_pkce_provider,
    OAuthFlowKind, OAuthLoginOption,
};
pub use device::{
    complete_device_oauth, device_oauth_agent, poll_device_oauth, start_device_oauth,
    DeviceOAuthPoll, DeviceOAuthStart, DeviceOAuthStatus,
};
pub use identity::{
    apply_identity_to_credentials, decode_jwt_payload, extract_oauth_identity, identity_extra,
    identity_from_credentials, OAuthIdentity,
};
pub use pi_refresh::refresh_pi_provider;
pub use providers::{oauth_provider_for, OAuthProvider};
pub use server::open_in_browser;
pub use session::{OAuthSessionInfo, OAuthStart, OAuthStatus};

use std::sync::Arc;
use std::thread;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::error::{AppError, Result};
use crate::logging::targets;
use crate::models::{Account, AccountInput, AccountKind, AgentId, LiveAccount};
use crate::services::AccountService;
use crate::utils::redact::redact_text;

use self::pkce::PkcePair;
use self::server::spawn_callback_listener;
// open_in_browser is re-exported at module root for GUI/shell callers.
use self::session::SessionStore;

/// Process-wide OAuth session store (GUI may start then wait).
static STORE: std::sync::OnceLock<Arc<SessionStore>> = std::sync::OnceLock::new();

fn store() -> Arc<SessionStore> {
    STORE.get_or_init(|| Arc::new(SessionStore::new())).clone()
}

/// Start PKCE authorize: bind loopback, build URL, optionally open browser.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartOAuthResult {
    pub state: String,
    pub authorize_url: String,
    pub redirect_uri: String,
    pub agent_id: AgentId,
    /// Selected multi-provider key when applicable (Pi).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_key: Option<String>,
    /// True when system browser open was attempted.
    pub browser_opened: bool,
}

/// Begin OAuth for an agent. Spawns a callback listener thread.
///
/// `provider_key` selects which upstream login to run when the agent supports
/// multiple (Pi: `anthropic` / `openai-codex`). Omit for single-provider agents.
pub fn start_oauth(
    agent: AgentId,
    open_browser: bool,
    provider_key: Option<&str>,
) -> Result<StartOAuthResult> {
    if is_device_code_option(agent, provider_key) {
        return Err(AppError::InvalidArg(
            "this provider uses device-code flow; call start_device_oauth instead".into(),
        ));
    }

    let provider = resolve_pkce_provider(agent, provider_key).ok_or_else(|| {
        if agent == AgentId::Pi {
            AppError::InvalidArg(
                "Pi OAuth requires providerKey: anthropic | openai-codex (xai 使用设备码)".into(),
            )
        } else {
            AppError::Unsupported(format!(
                "OAuth PKCE is not configured for {}",
                agent.as_str()
            ))
        }
    })?;

    let pkce = PkcePair::generate()?;
    let state = pkce::random_state()?;

    // Prefer fixed redirect ports when the provider requires them; else ephemeral.
    let listener = server::bind_listener(provider.redirect_port)?;
    let port = listener.local_addr()?.port();
    let path = if provider.redirect_path.is_empty() {
        "/callback"
    } else {
        provider.redirect_path
    };
    // Pi Anthropic registers `localhost` (not 127.0.0.1) as redirect host.
    let host = if agent == AgentId::Pi {
        "localhost"
    } else {
        "127.0.0.1"
    };
    let redirect_uri = format!("http://{host}:{port}{path}");

    let authorize_url = provider.build_authorize_url(&redirect_uri, &state, &pkce.challenge);

    let resolved_key = provider_key
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .or_else(|| {
            if agent == AgentId::Pi {
                pi_auth_json_key(provider.id.strip_prefix("pi-").unwrap_or(provider.id))
                    .map(|s| s.to_string())
            } else {
                None
            }
        });

    let st = store();
    st.insert(session::OAuthSession {
        state: state.clone(),
        agent,
        verifier: pkce.verifier.clone(),
        redirect_uri: redirect_uri.clone(),
        provider_key: resolved_key.clone(),
        status: OAuthStatus::Waiting,
        code: None,
        error: None,
        created_at: std::time::Instant::now(),
    })?;

    let st2 = Arc::clone(&st);
    let state2 = state.clone();
    thread::spawn(move || {
        if let Err(e) = spawn_callback_listener(listener, st2, &state2) {
            let err_msg = redact_text(&e.to_string());
            tracing::warn!(
                module = targets::OAUTH,
                code = e.code(),
                op = "callback",
                error = %err_msg,
                "oauth callback listener failed"
            );
        }
    });

    let browser_opened = if open_browser {
        match open_in_browser(&authorize_url) {
            Ok(()) => true,
            Err(e) => {
                let err_msg = redact_text(&e.to_string());
                tracing::warn!(
                    module = targets::OAUTH,
                    code = e.code(),
                    op = "open_browser",
                    error = %err_msg,
                    "failed to open system browser"
                );
                false
            }
        }
    } else {
        false
    };

    tracing::info!(
        module = targets::OAUTH,
        op = "start",
        agent = agent.as_str(),
        port,
        browser_opened,
        "oauth started"
    );

    Ok(StartOAuthResult {
        state,
        authorize_url,
        redirect_uri,
        agent_id: agent,
        provider_key: resolved_key,
        browser_opened,
    })
}

/// Poll session until success/error/timeout.
pub fn wait_oauth(state: &str, timeout_secs: u64) -> Result<OAuthSessionInfo> {
    let st = store();
    let deadline = std::time::Instant::now() + Duration::from_secs(timeout_secs.max(1));
    loop {
        let info = st.get_info(state)?;
        match info.status {
            OAuthStatus::Waiting => {
                if std::time::Instant::now() >= deadline {
                    st.mark_error(state, "OAuth 等待超时")?;
                    return Err(AppError::message("oauth.timeout", "OAuth 等待回调超时"));
                }
                thread::sleep(Duration::from_millis(200));
            }
            OAuthStatus::CallbackReceived | OAuthStatus::Succeeded | OAuthStatus::Failed => {
                return Ok(info);
            }
        }
    }
}

/// Resolve the target agent of an existing PKCE session without waiting or
/// changing its state. UI callers use this to take the matching lifecycle
/// lock before completing a flow that writes account/auth data.
pub fn oauth_session_info(state: &str) -> Result<OAuthSessionInfo> {
    store().get_info(state)
}

/// Exchange code for tokens and persist as pool account (does not switch live).
pub fn complete_oauth(accounts: &AccountService, state: &str) -> Result<Account> {
    let st = store();
    let session = st.take_ready(state)?;
    let code = session
        .code
        .clone()
        .ok_or_else(|| AppError::message("oauth.no_code", "OAuth 回调未包含 code"))?;

    let provider = resolve_pkce_provider(session.agent, session.provider_key.as_deref())
        .ok_or_else(|| {
            AppError::Unsupported(format!(
                "OAuth provider missing for {} ({})",
                session.agent.as_str(),
                session.provider_key.as_deref().unwrap_or("-")
            ))
        })?;

    let tokens = provider.exchange_code_with_state(
        &code,
        &session.verifier,
        &session.redirect_uri,
        Some(&session.state),
    )?;

    let account = if session.agent == AgentId::Pi {
        complete_pi_oauth(accounts, &session, tokens)?
    } else {
        // Codex live apply only accepts `format=auth_json` with a full auth.json
        // body. Convert the generic PKCE token bundle before pool insert so the
        // account is switchable immediately (not only after import-live).
        let credentials = if session.agent == AgentId::Codex {
            crate::adapters::normalize_codex_oauth_credentials(&tokens.credentials)?
        } else {
            tokens.credentials
        };
        let live = LiveAccount {
            agent: session.agent,
            kind: AccountKind::Oauth,
            credentials,
            label_hint: tokens.label_hint.clone(),
            extra: tokens.extra.clone(),
        };

        let label = tokens
            .label_hint
            .unwrap_or_else(|| format!("{} oauth", session.agent.as_str()));

        accounts.create(AccountInput {
            agent_id: session.agent,
            kind: AccountKind::Oauth,
            label,
            credentials: live.credentials,
            extra: live.extra,
            is_current: false,
        })?
    };

    st.mark_succeeded(state)?;
    tracing::info!(
        module = targets::OAUTH,
        op = "complete",
        agent = session.agent.as_str(),
        account_id = %account.id,
        "oauth account stored"
    );
    Ok(account)
}

fn complete_pi_oauth(
    accounts: &AccountService,
    session: &session::OAuthSession,
    tokens: TokenBundle,
) -> Result<Account> {
    let provider_key = session
        .provider_key
        .as_deref()
        .and_then(pi_auth_json_key)
        .ok_or_else(|| {
            AppError::message(
                "oauth.pi",
                "Pi OAuth session missing provider key (anthropic|openai-codex)",
            )
        })?;

    let access = tokens
        .credentials
        .get("access_token")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::message("oauth.pi", "token response missing access_token"))?;
    let refresh = tokens
        .credentials
        .get("refresh_token")
        .and_then(|v| v.as_str());
    let expires_at = tokens
        .credentials
        .get("expires_at")
        .and_then(|v| v.as_str());
    let expires_in = tokens
        .credentials
        .get("expires_in")
        .and_then(|v| v.as_i64());
    let id_token = tokens.credentials.get("id_token").and_then(|v| v.as_str());

    let live = crate::adapters::pi_auth::live_account_from_oauth_tokens(
        provider_key,
        access,
        refresh,
        expires_at,
        expires_in,
        id_token,
    )?;

    let label = live
        .label_hint
        .clone()
        .or(tokens.label_hint)
        .unwrap_or_else(|| format!("pi:{provider_key}"));

    // AccountService owns the shared process/file lock and compensation path
    // for all Pi auth.json mutations.
    accounts.persist_pi_oauth_live(live, label)
}

/// Convenience: start + wait + complete (blocking). Used by CLI.
pub fn run_oauth_blocking(
    accounts: &AccountService,
    agent: AgentId,
    timeout_secs: u64,
) -> Result<Account> {
    let start = start_oauth(agent, true, None)?;
    let _ = wait_oauth(&start.state, timeout_secs)?;
    complete_oauth(accounts, &start.state)
}

/// Whether any OAuth login option is available for this agent.
pub fn oauth_supported(agent: AgentId) -> bool {
    catalog::oauth_supported(agent)
}

/// Token exchange payload.
#[derive(Debug, Clone)]
pub struct TokenBundle {
    pub credentials: serde_json::Value,
    pub label_hint: Option<String>,
    pub extra: serde_json::Value,
}

impl TokenBundle {
    pub fn from_json(v: serde_json::Value) -> Self {
        let label = v
            .get("email")
            .or_else(|| v.get("account"))
            .and_then(|x| x.as_str())
            .map(|s| s.to_string());
        let extra = json!({
            "subscription": v.get("subscription").cloned().unwrap_or(json!(null)),
            "email": label.clone(),
            "source": "oauth_pkce",
        });
        Self {
            credentials: v,
            label_hint: label,
            extra,
        }
    }
}
