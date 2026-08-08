//! Per-agent OAuth public-client configuration (PKCE + loopback).
//!
//! Values live only in this module — do not copy client IDs / endpoints into
//! public docs or issues. Do not invent client secrets.

use serde_json::{json, Value};

use crate::error::{AppError, Result};
use crate::logging::targets;
use crate::models::AgentId;

use super::TokenBundle;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenBodyStyle {
    /// application/x-www-form-urlencoded (RFC 6749 default)
    Form,
    /// application/json body (Claude Code native style)
    Json,
}

#[derive(Debug, Clone, Copy)]
pub struct OAuthProvider {
    pub id: &'static str,
    pub agent: AgentId,
    pub authorize_url: &'static str,
    pub token_url: &'static str,
    pub client_id: &'static str,
    /// Fixed loopback port when required by the provider; None = ephemeral.
    pub redirect_port: Option<u16>,
    pub scopes: &'static str,
    pub body_style: TokenBodyStyle,
    /// Optional extra authorize query params (e.g. Claude `code=true`).
    pub authorize_extra: &'static str,
}

pub fn oauth_provider_for(agent: AgentId) -> Option<&'static OAuthProvider> {
    match agent {
        AgentId::Claude => Some(&CLAUDE),
        AgentId::Codex => Some(&CODEX),
        AgentId::Grok => Some(&XAI),
        _ => None,
    }
}

/// Claude Code native OAuth (public client).
static CLAUDE: OAuthProvider = OAuthProvider {
    id: "claude",
    agent: AgentId::Claude,
    authorize_url: "https://claude.ai/oauth/authorize",
    // Primary token endpoint used by Claude Code tooling.
    token_url: "https://console.anthropic.com/v1/oauth/token",
    client_id: "9d1c250a-e61b-44d9-88ed-5944d1962f5e",
    redirect_port: None,
    scopes: "org:create_api_key user:profile user:inference",
    body_style: TokenBodyStyle::Json,
    authorize_extra: "&code=true",
};

/// OpenAI Codex / ChatGPT OAuth (fixed loopback port required by provider).
static CODEX: OAuthProvider = OAuthProvider {
    id: "codex",
    agent: AgentId::Codex,
    authorize_url: "https://auth.openai.com/oauth/authorize",
    token_url: "https://auth.openai.com/oauth/token",
    client_id: "app_EMoamEEZ73f0CkXaXp7hrann",
    redirect_port: Some(1455),
    scopes: "openid profile email offline_access",
    body_style: TokenBodyStyle::Form,
    authorize_extra: "",
};

/// xAI / Grok OAuth (fixed loopback port required by provider).
static XAI: OAuthProvider = OAuthProvider {
    id: "xai",
    agent: AgentId::Grok,
    authorize_url: "https://accounts.x.ai/oauth/authorize",
    token_url: "https://accounts.x.ai/oauth/token",
    client_id: "grok-cli",
    redirect_port: Some(56121),
    scopes: "openid offline_access",
    body_style: TokenBodyStyle::Form,
    authorize_extra: "",
};

impl OAuthProvider {
    pub fn build_authorize_url(&self, redirect_uri: &str, state: &str, challenge: &str) -> String {
        format!(
            "{base}?response_type=code&client_id={client}&redirect_uri={redir}&scope={scope}&state={state}&code_challenge={ch}&code_challenge_method=S256{extra}",
            base = self.authorize_url,
            client = urlencoding::encode(self.client_id),
            redir = urlencoding::encode(redirect_uri),
            scope = urlencoding::encode(self.scopes),
            state = urlencoding::encode(state),
            ch = urlencoding::encode(challenge),
            extra = self.authorize_extra,
        )
    }

    pub fn exchange_code(
        &self,
        code: &str,
        verifier: &str,
        redirect_uri: &str,
    ) -> Result<TokenBundle> {
        // Some providers embed "#state" suffix in the returned code; strip it.
        let code = code.split('#').next().unwrap_or(code).trim();
        let body = self.token_request(
            &[
                ("grant_type", "authorization_code"),
                ("code", code),
                ("redirect_uri", redirect_uri),
                ("client_id", self.client_id),
                ("code_verifier", verifier),
            ],
        )?;
        self.bundle_from_token_json(body)
    }

    /// Refresh using refresh_token grant.
    pub fn refresh(&self, refresh_token: &str) -> Result<TokenBundle> {
        let body = self.token_request(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("client_id", self.client_id),
        ])?;
        self.bundle_from_token_json(body)
    }

    fn token_request(&self, fields: &[(&str, &str)]) -> Result<Value> {
        let result = match self.body_style {
            TokenBodyStyle::Form => {
                let mut req = ureq::post(self.token_url)
                    .set("Content-Type", "application/x-www-form-urlencoded")
                    .set("Accept", "application/json");
                req = req.timeout(crate::catalog::limits::OAUTH_TOKEN_HTTP_TIMEOUT);
                req.send_form(fields)
            }
            TokenBodyStyle::Json => {
                let mut map = serde_json::Map::new();
                for (k, v) in fields {
                    map.insert((*k).into(), Value::String((*v).into()));
                }
                let mut req = ureq::post(self.token_url)
                    .set("Content-Type", "application/json")
                    .set("Accept", "application/json");
                req = req.timeout(crate::catalog::limits::OAUTH_TOKEN_HTTP_TIMEOUT);
                req.send_json(Value::Object(map))
            }
        };

        let resp = result.map_err(|e| {
            AppError::message(
                "oauth.token",
                format!("token request failed for {}: {e}", self.id),
            )
        })?;

        let status = resp.status();
        let body: Value = resp.into_json().map_err(|e| {
            AppError::message("oauth.token", format!("invalid token JSON: {e}"))
        })?;

        if !(200..300).contains(&status) {
            let msg = body
                .get("error_description")
                .or_else(|| body.get("error"))
                .and_then(|v| v.as_str())
                .unwrap_or("token request rejected");
            tracing::warn!(
                module = targets::OAUTH,
                op = "token",
                provider = self.id,
                status,
                error = msg,
                "token endpoint rejected"
            );
            return Err(AppError::message(
                "oauth.token",
                format!("{msg} (HTTP {status})"),
            ));
        }
        Ok(body)
    }

    fn bundle_from_token_json(&self, body: Value) -> Result<TokenBundle> {
        let access = body
            .get("access_token")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        let refresh = body
            .get("refresh_token")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        if access.is_none() && refresh.is_none() {
            return Err(AppError::message(
                "oauth.token",
                "token response missing access_token/refresh_token",
            ));
        }

        let expires_in = body.get("expires_in").and_then(|v| v.as_i64()).unwrap_or(0);
        let expires_at = if expires_in > 0 {
            Some(
                (chrono::Utc::now() + chrono::Duration::seconds(expires_in.max(0))).to_rfc3339(),
            )
        } else {
            body.get("expires_at")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        };

        let credentials = json!({
            "type": "oauth",
            "provider": self.id,
            "access_token": access,
            "refresh_token": refresh,
            "expires_in": body.get("expires_in"),
            "expires_at": expires_at,
            "token_type": body.get("token_type"),
            "scope": body.get("scope"),
            "id_token": body.get("id_token"),
            "raw": body,
        });

        Ok(TokenBundle {
            credentials,
            label_hint: Some(format!("{} · OAuth", self.agent.display_name())),
            extra: json!({
                "source": "oauth_pkce",
                "provider": self.id,
                "subscription": null,
                "expiresAt": expires_at,
            }),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_authorize_url_contains_pkce_and_client() {
        let url = CLAUDE.build_authorize_url(
            "http://127.0.0.1:12345/callback",
            "st",
            "ch",
        );
        assert!(url.contains(&format!("client_id={}", CLAUDE.client_id)));
        assert!(url.contains("code_challenge=ch"));
        assert!(url.contains("code_challenge_method=S256"));
        assert!(url.contains("code=true"));
    }

    #[test]
    fn oauth_provider_for_known_agents() {
        assert!(oauth_provider_for(AgentId::Claude).is_some());
        assert!(oauth_provider_for(AgentId::Codex).is_some());
        assert!(oauth_provider_for(AgentId::Grok).is_some());
        assert!(oauth_provider_for(AgentId::Kimi).is_none());
    }
}
