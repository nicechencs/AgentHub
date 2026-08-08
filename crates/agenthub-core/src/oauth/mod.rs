//! OAuth PKCE loopback login (browser authorize → localhost callback → token exchange).
//!
//! Uses a short-lived in-process session store + std TCP listener (no Tokio runtime).

mod pkce;
mod providers;
mod server;
mod session;

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
    STORE
        .get_or_init(|| Arc::new(SessionStore::new()))
        .clone()
}

/// Start PKCE authorize: bind loopback, build URL, optionally open browser.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartOAuthResult {
    pub state: String,
    pub authorize_url: String,
    pub redirect_uri: String,
    pub agent_id: AgentId,
    /// True when system browser open was attempted.
    pub browser_opened: bool,
}

/// Begin OAuth for an agent. Spawns a callback listener thread.
pub fn start_oauth(agent: AgentId, open_browser: bool) -> Result<StartOAuthResult> {
    let provider = oauth_provider_for(agent).ok_or_else(|| {
        AppError::Unsupported(format!(
            "OAuth PKCE is not configured for {}",
            agent.as_str()
        ))
    })?;

    let pkce = PkcePair::generate()?;
    let state = pkce::random_state()?;

    // Prefer fixed redirect ports when the provider requires them; else ephemeral.
    let listener = server::bind_listener(provider.redirect_port)?;
    let port = listener.local_addr()?.port();
    let redirect_uri = format!("http://127.0.0.1:{port}/callback");

    let authorize_url = provider.build_authorize_url(&redirect_uri, &state, &pkce.challenge);

    let st = store();
    st.insert(session::OAuthSession {
        state: state.clone(),
        agent,
        verifier: pkce.verifier.clone(),
        redirect_uri: redirect_uri.clone(),
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

/// Exchange code for tokens and persist as pool account (does not switch live).
pub fn complete_oauth(accounts: &AccountService, state: &str) -> Result<Account> {
    let st = store();
    let session = st.take_ready(state)?;
    let code = session
        .code
        .clone()
        .ok_or_else(|| AppError::message("oauth.no_code", "OAuth 回调未包含 code"))?;

    let provider = oauth_provider_for(session.agent).ok_or_else(|| {
        AppError::Unsupported(format!(
            "OAuth provider missing for {}",
            session.agent.as_str()
        ))
    })?;

    let tokens = provider.exchange_code(&code, &session.verifier, &session.redirect_uri)?;
    let live = LiveAccount {
        agent: session.agent,
        kind: AccountKind::Oauth,
        credentials: tokens.credentials,
        label_hint: tokens.label_hint.clone(),
        extra: tokens.extra.clone(),
    };

    let label = tokens
        .label_hint
        .unwrap_or_else(|| format!("{} oauth", session.agent.as_str()));

    let account = accounts.create(AccountInput {
        agent_id: session.agent,
        kind: AccountKind::Oauth,
        label,
        credentials: live.credentials,
        extra: live.extra,
        is_current: false,
    })?;

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

/// Convenience: start + wait + complete (blocking). Used by CLI.
pub fn run_oauth_blocking(
    accounts: &AccountService,
    agent: AgentId,
    timeout_secs: u64,
) -> Result<Account> {
    let start = start_oauth(agent, true)?;
    let _ = wait_oauth(&start.state, timeout_secs)?;
    complete_oauth(accounts, &start.state)
}

/// Whether OAuth PKCE is wired for this agent.
pub fn oauth_supported(agent: AgentId) -> bool {
    oauth_provider_for(agent).is_some()
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
