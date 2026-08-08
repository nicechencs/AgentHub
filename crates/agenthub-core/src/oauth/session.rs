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
    pub state: String,
    pub agent: AgentId,
    pub verifier: String,
    pub redirect_uri: String,
    pub status: OAuthStatus,
    pub code: Option<String>,
    pub error: Option<String>,
    pub created_at: Instant,
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
        g.insert(session.state.clone(), session);
        Ok(())
    }

    pub fn get_info(&self, state: &str) -> Result<OAuthSessionInfo> {
        let g = self
            .inner
            .lock()
            .map_err(|_| AppError::message("oauth.store", "session store poisoned"))?;
        let s = g
            .get(state)
            .ok_or_else(|| AppError::NotFound(format!("oauth session not found: {state}")))?;
        Ok(OAuthSessionInfo {
            state: s.state.clone(),
            agent_id: s.agent,
            status: s.status,
            error: s.error.clone(),
        })
    }

    pub fn set_code(&self, state: &str, code: String) -> Result<()> {
        let mut g = self
            .inner
            .lock()
            .map_err(|_| AppError::message("oauth.store", "session store poisoned"))?;
        let s = g
            .get_mut(state)
            .ok_or_else(|| AppError::NotFound(format!("oauth session not found: {state}")))?;
        s.code = Some(code);
        s.status = OAuthStatus::CallbackReceived;
        Ok(())
    }

    pub fn mark_error(&self, state: &str, err: impl Into<String>) -> Result<()> {
        let mut g = self
            .inner
            .lock()
            .map_err(|_| AppError::message("oauth.store", "session store poisoned"))?;
        if let Some(s) = g.get_mut(state) {
            s.status = OAuthStatus::Failed;
            s.error = Some(err.into());
        }
        Ok(())
    }

    pub fn mark_succeeded(&self, state: &str) -> Result<()> {
        let mut g = self
            .inner
            .lock()
            .map_err(|_| AppError::message("oauth.store", "session store poisoned"))?;
        if let Some(s) = g.get_mut(state) {
            s.status = OAuthStatus::Succeeded;
        }
        Ok(())
    }

    /// Take session for token exchange (must have code).
    pub fn take_ready(&self, state: &str) -> Result<OAuthSession> {
        let g = self
            .inner
            .lock()
            .map_err(|_| AppError::message("oauth.store", "session store poisoned"))?;
        let s = g
            .get(state)
            .ok_or_else(|| AppError::NotFound(format!("oauth session not found: {state}")))?
            .clone();
        if s.code.is_none() {
            return Err(AppError::message(
                "oauth.not_ready",
                "OAuth 回调尚未到达，请完成浏览器授权",
            ));
        }
        Ok(s)
    }

    fn purge_locked(&self, g: &mut HashMap<String, OAuthSession>) {
        let now = Instant::now();
        g.retain(|_, s| now.duration_since(s.created_at) < TTL);
    }
}
