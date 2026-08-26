//! In-memory OAuth session store (TTL 30 minutes).

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::error::{AppError, Result};
use crate::models::AgentId;

use crate::catalog::limits::OAUTH_SESSION_TTL as TTL;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OAuthStatus {
    Waiting,
    CallbackReceived,
    Succeeded,
    Failed,
}

#[derive(Debug, Clone)]
pub struct OAuthSession {
    state: String,
    agent: AgentId,
    verifier: String,
    redirect_uri: String,
    /// Optional multi-provider key (Pi: anthropic / openai-codex / …).
    provider_key: Option<String>,
    status: OAuthStatus,
    code: Option<String>,
    error: Option<String>,
    created_at: Instant,
    /// Set while the one allowed token exchange is in flight.
    completing: bool,
}

impl OAuthSession {
    pub fn new(
        state: impl Into<String>,
        agent: AgentId,
        verifier: impl Into<String>,
        redirect_uri: impl Into<String>,
        provider_key: Option<String>,
    ) -> Self {
        Self {
            state: state.into(),
            agent,
            verifier: verifier.into(),
            redirect_uri: redirect_uri.into(),
            provider_key,
            status: OAuthStatus::Waiting,
            code: None,
            error: None,
            created_at: Instant::now(),
            completing: false,
        }
    }

    pub fn state(&self) -> &str {
        &self.state
    }

    pub fn agent(&self) -> AgentId {
        self.agent
    }

    pub fn verifier(&self) -> &str {
        &self.verifier
    }

    pub fn redirect_uri(&self) -> &str {
        &self.redirect_uri
    }

    pub fn provider_key(&self) -> Option<&str> {
        self.provider_key.as_deref()
    }

    pub fn status(&self) -> OAuthStatus {
        self.status
    }

    pub fn code(&self) -> Option<&str> {
        self.code.as_deref()
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn created_at(&self) -> Instant {
        self.created_at
    }

    fn scrub_secrets(&mut self) {
        self.verifier.clear();
        self.redirect_uri.clear();
        self.provider_key = None;
        self.code = None;
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuthSessionInfo {
    pub state: String,
    pub agent_id: AgentId,
    pub status: OAuthStatus,
    pub error: Option<String>,
}

/// Public start DTO alias used by docs.
pub type OAuthStart = super::StartOAuthResult;

pub struct SessionStore {
    inner: Mutex<HashMap<String, OAuthSession>>,
}

impl SessionStore {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
        }
    }

    pub fn insert(&self, session: OAuthSession) -> Result<()> {
        let mut g = self
            .inner
            .lock()
            .map_err(|_| AppError::message("oauth.store", "session store poisoned"))?;
        self.purge_locked(&mut g);
        if g.contains_key(session.state()) {
            return Err(AppError::message(
                "oauth.state",
                "OAuth state is already active",
            ));
        }
        g.insert(session.state().to_string(), session);
        Ok(())
    }

    pub fn get_info(&self, state: &str) -> Result<OAuthSessionInfo> {
        let mut g = self
            .inner
            .lock()
            .map_err(|_| AppError::message("oauth.store", "session store poisoned"))?;
        self.purge_locked(&mut g);
        let s = g
            .get(state)
            .ok_or_else(|| AppError::NotFound("oauth session not found".into()))?;
        Ok(OAuthSessionInfo {
            state: s.state().to_string(),
            agent_id: s.agent(),
            status: s.status(),
            error: s.error().map(str::to_string),
        })
    }

    pub fn set_code(&self, state: &str, code: String) -> Result<()> {
        let mut g = self
            .inner
            .lock()
            .map_err(|_| AppError::message("oauth.store", "session store poisoned"))?;
        self.purge_locked(&mut g);
        let s = g
            .get_mut(state)
            .ok_or_else(|| AppError::NotFound("oauth session not found".into()))?;
        match s.status() {
            OAuthStatus::Waiting => {
                let code = code.trim().to_string();
                if code.is_empty() {
                    return Err(AppError::message(
                        "oauth.code",
                        "OAuth callback code is empty",
                    ));
                }
                s.code = Some(code);
                s.status = OAuthStatus::CallbackReceived;
                Ok(())
            }
            OAuthStatus::CallbackReceived | OAuthStatus::Succeeded | OAuthStatus::Failed => {
                Err(AppError::message(
                    "oauth.replay",
                    "OAuth session is no longer accepting callbacks",
                ))
            }
        }
    }

    pub fn mark_error(&self, state: &str, err: impl Into<String>) -> Result<()> {
        let mut g = self
            .inner
            .lock()
            .map_err(|_| AppError::message("oauth.store", "session store poisoned"))?;
        self.purge_locked(&mut g);
        let _ = err.into();
        if let Some(s) = g.get_mut(state) {
            if !matches!(s.status, OAuthStatus::Succeeded) && !s.completing {
                s.status = OAuthStatus::Failed;
                s.error = Some("OAuth authorization failed".into());
                s.scrub_secrets();
            }
        }
        Ok(())
    }

    pub fn mark_succeeded(&self, state: &str) -> Result<()> {
        let mut g = self
            .inner
            .lock()
            .map_err(|_| AppError::message("oauth.store", "session store poisoned"))?;
        self.purge_locked(&mut g);
        if let Some(s) = g.get_mut(state) {
            if s.completing {
                s.status = OAuthStatus::Succeeded;
                s.completing = false;
                s.scrub_secrets();
            }
        }
        Ok(())
    }

    pub fn mark_completion_failed(&self, state: &str) -> Result<()> {
        let mut g = self
            .inner
            .lock()
            .map_err(|_| AppError::message("oauth.store", "session store poisoned"))?;
        self.purge_locked(&mut g);
        if let Some(s) = g.get_mut(state) {
            if s.completing {
                s.status = OAuthStatus::Failed;
                s.completing = false;
                s.error = Some("OAuth completion failed".into());
                s.scrub_secrets();
            }
        }
        Ok(())
    }

    pub fn is_waiting(&self, state: &str) -> bool {
        matches!(
            self.get_info(state),
            Ok(info) if info.status == OAuthStatus::Waiting
        )
    }

    /// Fail every waiting session whose redirect URI uses `port` so its listener
    /// can drop the socket. Used before rebinding a provider-fixed loopback port.
    pub fn cancel_waiting_on_port(&self, port: u16) -> Result<usize> {
        let mut g = self
            .inner
            .lock()
            .map_err(|_| AppError::message("oauth.store", "session store poisoned"))?;
        self.purge_locked(&mut g);
        let mut n = 0;
        for session in g.values_mut() {
            if session.status() != OAuthStatus::Waiting {
                continue;
            }
            if redirect_loopback_port(session.redirect_uri()) != Some(port) {
                continue;
            }
            session.status = OAuthStatus::Failed;
            session.error = Some("OAuth authorization failed".into());
            session.scrub_secrets();
            n += 1;
        }
        Ok(n)
    }

    /// Take session for token exchange (must have code).
    pub fn take_ready(&self, state: &str) -> Result<OAuthSession> {
        let mut g = self
            .inner
            .lock()
            .map_err(|_| AppError::message("oauth.store", "session store poisoned"))?;
        self.purge_locked(&mut g);
        let s = g
            .get_mut(state)
            .ok_or_else(|| AppError::NotFound("oauth session not found".into()))?;
        if s.completing || matches!(s.status(), OAuthStatus::Succeeded | OAuthStatus::Failed) {
            return Err(AppError::message(
                "oauth.replay",
                "OAuth session has already been consumed",
            ));
        }
        if s.status() != OAuthStatus::CallbackReceived {
            return Err(AppError::message(
                "oauth.not_ready",
                "OAuth callback has not completed",
            ));
        }
        if s.code().is_none() {
            return Err(AppError::message(
                "oauth.not_ready",
                "OAuth 回调尚未到达，请完成浏览器授权",
            ));
        }
        s.completing = true;
        Ok(s.clone())
    }

    fn purge_locked(&self, g: &mut HashMap<String, OAuthSession>) {
        purge_at(g, Instant::now());
    }
}

fn purge_at(g: &mut HashMap<String, OAuthSession>, now: Instant) {
    g.retain(|_, s| {
        now.checked_duration_since(s.created_at())
            .is_some_and(|age| age < TTL)
    });
}

pub(crate) fn redirect_loopback_port(redirect_uri: &str) -> Option<u16> {
    let rest = redirect_uri.strip_prefix("http://")?;
    let hostport = rest.split('/').next()?;
    hostport.rsplit_once(':')?.1.parse().ok()
}

#[cfg(test)]
mod tests;
