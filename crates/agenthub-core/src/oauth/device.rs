//! OAuth 2.0 device-code flow (RFC 8628) for providers that do not use loopback PKCE.
//!
//! Currently used for Pi → xAI (matches `@earendil-works/pi-ai` xai OAuth).

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{AppError, Result};
use crate::logging::targets;
use crate::models::{Account, AccountInput, AccountKind, AgentId};
use crate::services::AccountService;

use super::catalog::pi_auth_json_key;

const XAI_CLIENT_ID: &str = "b1a00492-073a-47ea-816f-4c329264a828";
const XAI_SCOPE: &str = "openid profile email offline_access grok-cli:access api:access";
const XAI_DEVICE_CODE_URL: &str = "https://auth.x.ai/oauth2/device/code";
const XAI_TOKEN_URL: &str = "https://auth.x.ai/oauth2/token";
const DEFAULT_POLL_INTERVAL_SECS: u64 = 5;
const REFRESH_SKEW_MS: i64 = 5 * 60 * 1000;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceOAuthStart {
    pub state: String,
    pub agent_id: AgentId,
    pub provider_key: String,
    pub user_code: String,
    pub verification_uri: String,
    pub verification_uri_complete: Option<String>,
    pub interval_secs: u64,
    pub expires_in_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceOAuthPoll {
    pub state: String,
    pub status: DeviceOAuthStatus,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DeviceOAuthStatus {
    Pending,
    SlowDown,
    Complete,
    Failed,
    Expired,
}

struct DeviceSession {
    #[allow(dead_code)]
    agent: AgentId,
    provider_key: String,
    device_code: String,
    interval: Duration,
    expires_at: Instant,
    last_poll: Instant,
    access: Option<String>,
    refresh: Option<String>,
    expires_at_ms: Option<i64>,
    status: DeviceOAuthStatus,
    error: Option<String>,
}

static DEVICE_STORE: std::sync::OnceLock<Mutex<HashMap<String, DeviceSession>>> =
    std::sync::OnceLock::new();

fn store() -> &'static Mutex<HashMap<String, DeviceSession>> {
    DEVICE_STORE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Start xAI (or future) device-code login for Pi.
pub fn start_device_oauth(agent: AgentId, provider_key: &str) -> Result<DeviceOAuthStart> {
    if agent != AgentId::Pi {
        return Err(AppError::Unsupported(
            "device-code OAuth is currently only wired for Pi".into(),
        ));
    }
    let key = pi_auth_json_key(provider_key).ok_or_else(|| {
        AppError::InvalidArg(format!("unknown Pi OAuth provider: {provider_key}"))
    })?;
    if key != "xai" {
        return Err(AppError::Unsupported(format!(
            "device-code flow not implemented for provider '{key}' yet; use Pi CLI `/login {key}`"
        )));
    }

    let body = post_form(
        XAI_DEVICE_CODE_URL,
        &[
            ("client_id", XAI_CLIENT_ID),
            ("scope", XAI_SCOPE),
            ("referrer", "pi"),
        ],
    )?;

    let device_code = required_str(&body, "device_code")?;
    let user_code = required_str(&body, "user_code")?;
    let verification_uri = required_str(&body, "verification_uri")?;
    let verification_uri_complete = body
        .get("verification_uri_complete")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let interval = body
        .get("interval")
        .and_then(|v| v.as_u64())
        .filter(|n| *n > 0)
        .unwrap_or(DEFAULT_POLL_INTERVAL_SECS);
    let expires_in = body
        .get("expires_in")
        .and_then(|v| v.as_u64())
        .filter(|n| *n > 0)
        .unwrap_or(900);

    let state = format!("dev-{}", uuid::Uuid::new_v4());
    let session = DeviceSession {
        agent,
        provider_key: key.to_string(),
        device_code,
        interval: Duration::from_secs(interval),
        expires_at: Instant::now() + Duration::from_secs(expires_in),
        last_poll: Instant::now() - Duration::from_secs(interval),
        access: None,
        refresh: None,
        expires_at_ms: None,
        status: DeviceOAuthStatus::Pending,
        error: None,
    };
    store()
        .lock()
        .map_err(|_| AppError::message("oauth.device", "device store poisoned"))?
        .insert(state.clone(), session);

    tracing::info!(
        module = targets::OAUTH,
        op = "device_start",
        agent = agent.as_str(),
        provider = key,
        "device-code oauth started"
    );

    Ok(DeviceOAuthStart {
        state,
        agent_id: agent,
        provider_key: key.to_string(),
        user_code,
        verification_uri,
        verification_uri_complete,
        interval_secs: interval,
        expires_in_secs: expires_in,
    })
}

/// Poll device authorization once (caller should honor interval_secs).
pub fn poll_device_oauth(state: &str) -> Result<DeviceOAuthPoll> {
    let mut guard = store()
        .lock()
        .map_err(|_| AppError::message("oauth.device", "device store poisoned"))?;
    let session = guard
        .get_mut(state)
        .ok_or_else(|| AppError::NotFound(format!("device oauth session not found: {state}")))?;

    if session.status == DeviceOAuthStatus::Complete {
        return Ok(DeviceOAuthPoll {
            state: state.into(),
            status: DeviceOAuthStatus::Complete,
            error: None,
        });
    }
    if Instant::now() >= session.expires_at {
        session.status = DeviceOAuthStatus::Expired;
        session.error = Some("device code expired".into());
        return Ok(DeviceOAuthPoll {
            state: state.into(),
            status: DeviceOAuthStatus::Expired,
            error: session.error.clone(),
        });
    }
    if Instant::now() < session.last_poll + session.interval {
        return Ok(DeviceOAuthPoll {
            state: state.into(),
            status: DeviceOAuthStatus::Pending,
            error: None,
        });
    }
    session.last_poll = Instant::now();

    let device_code = session.device_code.clone();
    drop(guard);

    let body = post_form(
        XAI_TOKEN_URL,
        &[
            (
                "grant_type",
                "urn:ietf:params:oauth:grant-type:device_code",
            ),
            ("client_id", XAI_CLIENT_ID),
            ("device_code", &device_code),
        ],
    );

    let mut guard = store()
        .lock()
        .map_err(|_| AppError::message("oauth.device", "device store poisoned"))?;
    let session = guard
        .get_mut(state)
        .ok_or_else(|| AppError::NotFound(format!("device oauth session not found: {state}")))?;

    match body {
        Ok(json) => {
            if let Some(access) = json.get("access_token").and_then(|v| v.as_str()) {
                let refresh = json
                    .get("refresh_token")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let expires_in = json
                    .get("expires_in")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(3600);
                session.access = Some(access.to_string());
                session.refresh = refresh;
                session.expires_at_ms =
                    Some(chrono::Utc::now().timestamp_millis() + expires_in * 1000 - REFRESH_SKEW_MS);
                session.status = DeviceOAuthStatus::Complete;
                return Ok(DeviceOAuthPoll {
                    state: state.into(),
                    status: DeviceOAuthStatus::Complete,
                    error: None,
                });
            }
            let err = json
                .get("error")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            match err {
                "authorization_pending" => {
                    session.status = DeviceOAuthStatus::Pending;
                    Ok(DeviceOAuthPoll {
                        state: state.into(),
                        status: DeviceOAuthStatus::Pending,
                        error: None,
                    })
                }
                "slow_down" => {
                    if let Some(secs) = json.get("interval").and_then(|v| v.as_u64()) {
                        session.interval = Duration::from_secs(secs.max(1));
                    } else {
                        session.interval += Duration::from_secs(5);
                    }
                    session.status = DeviceOAuthStatus::SlowDown;
                    Ok(DeviceOAuthPoll {
                        state: state.into(),
                        status: DeviceOAuthStatus::SlowDown,
                        error: None,
                    })
                }
                "expired_token" | "expired" => {
                    session.status = DeviceOAuthStatus::Expired;
                    session.error = Some("device code expired".into());
                    Ok(DeviceOAuthPoll {
                        state: state.into(),
                        status: DeviceOAuthStatus::Expired,
                        error: session.error.clone(),
                    })
                }
                other => {
                    session.status = DeviceOAuthStatus::Failed;
                    let desc = json
                        .get("error_description")
                        .and_then(|v| v.as_str())
                        .unwrap_or(other);
                    session.error = Some(desc.to_string());
                    Ok(DeviceOAuthPoll {
                        state: state.into(),
                        status: DeviceOAuthStatus::Failed,
                        error: session.error.clone(),
                    })
                }
            }
        }
        Err(e) => {
            // Network blip: keep pending.
            tracing::warn!(
                module = targets::OAUTH,
                op = "device_poll",
                error = %e,
                "device poll request failed; will retry"
            );
            Ok(DeviceOAuthPoll {
                state: state.into(),
                status: DeviceOAuthStatus::Pending,
                error: None,
            })
        }
    }
}

/// Persist completed device-code tokens into the account pool (+ Pi auth.json merge via apply).
pub fn complete_device_oauth(accounts: &AccountService, state: &str) -> Result<Account> {
    let mut guard = store()
        .lock()
        .map_err(|_| AppError::message("oauth.device", "device store poisoned"))?;
    let session = guard
        .remove(state)
        .ok_or_else(|| AppError::NotFound(format!("device oauth session not found: {state}")))?;
    if session.status != DeviceOAuthStatus::Complete {
        return Err(AppError::message(
            "oauth.device",
            session
                .error
                .unwrap_or_else(|| "device oauth not complete".into()),
        ));
    }
    let access = session
        .access
        .ok_or_else(|| AppError::message("oauth.device", "missing access token"))?;
    let expires_at = session.expires_at_ms.and_then(|ms| {
        chrono::DateTime::from_timestamp(ms / 1000, 0).map(|dt| dt.to_rfc3339())
    });

    let live = crate::adapters::pi_auth::live_account_from_oauth_tokens(
        &session.provider_key,
        &access,
        session.refresh.as_deref(),
        expires_at.as_deref(),
        None,
        None,
    )?;

    let label = live
        .label_hint
        .clone()
        .unwrap_or_else(|| format!("pi:{}", session.provider_key));

    // Merge into live auth.json immediately so Pi CLI sees the credential.
    let patch = live
        .credentials
        .get("body")
        .cloned()
        .ok_or_else(|| AppError::message("oauth.device", "missing auth body"))?;
    let merged = crate::adapters::pi_auth::merge_auth_json(&patch)?;
    let path = crate::adapters::pi_auth::pi_auth_path()?;
    let mut bytes = serde_json::to_vec_pretty(&merged)?;
    bytes.push(b'\n');
    crate::utils::atomic::atomic_write(&path, &bytes)?;

    let account = accounts.create(AccountInput {
        agent_id: AgentId::Pi,
        kind: AccountKind::Oauth,
        label,
        credentials: live.credentials,
        extra: live.extra,
        is_current: false,
    })?;

    tracing::info!(
        module = targets::OAUTH,
        op = "device_complete",
        agent = "pi",
        provider = %session.provider_key,
        account_id = %account.id,
        "device-code oauth stored"
    );
    Ok(account)
}

fn post_form(url: &str, fields: &[(&str, &str)]) -> Result<Value> {
    let mut req = ureq::post(url)
        .set("Content-Type", "application/x-www-form-urlencoded")
        .set("Accept", "application/json");
    req = req.timeout(crate::catalog::limits::OAUTH_TOKEN_HTTP_TIMEOUT);
    let resp = req.send_form(fields).map_err(|e| {
        AppError::message("oauth.device", format!("device request failed: {e}"))
    })?;
    let status = resp.status();
    let body: Value = resp
        .into_json()
        .map_err(|e| AppError::message("oauth.device", format!("invalid JSON: {e}")))?;
    // Device token endpoint returns 400 with error=authorization_pending — still a body.
    if status >= 500 {
        return Err(AppError::message(
            "oauth.device",
            format!("device endpoint HTTP {status}"),
        ));
    }
    Ok(body)
}

fn required_str(body: &Value, field: &str) -> Result<String> {
    body.get(field)
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .ok_or_else(|| {
            AppError::message(
                "oauth.device",
                format!("device response missing field: {field}"),
            )
        })
}
