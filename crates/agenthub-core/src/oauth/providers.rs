//! Per-agent OAuth public-client configuration (PKCE + loopback).
//!
//! Values live only in this module — do not copy client IDs / endpoints into
//! public docs or issues. Do not invent client secrets.

use serde_json::{json, Map, Value};

use crate::error::{AppError, Result};
use crate::logging::targets;
use crate::models::AgentId;

use super::identity::{apply_identity_to_credentials, extract_oauth_identity, identity_extra};
use super::TokenBundle;

#[cfg(test)]
thread_local! {
    static TOKEN_URL_OVERRIDE: std::cell::RefCell<Option<String>> = std::cell::RefCell::new(None);
}

#[cfg(test)]
pub fn with_token_url_override<T>(url: impl Into<String>, f: impl FnOnce() -> T) -> T {
    TOKEN_URL_OVERRIDE.with(|slot| *slot.borrow_mut() = Some(url.into()));
    let result = f();
    TOKEN_URL_OVERRIDE.with(|slot| *slot.borrow_mut() = None);
    result
}

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
    /// Host in the registered loopback redirect URI (`localhost` vs `127.0.0.1`).
    pub redirect_host: &'static str,
    /// Path on the loopback redirect URI (e.g. `/callback`, `/auth/callback`).
    pub redirect_path: &'static str,
    pub scopes: &'static str,
    pub body_style: TokenBodyStyle,
    /// Optional extra authorize query params (e.g. Claude `code=true`).
    pub authorize_extra: &'static str,
    /// When true, token exchange JSON body includes `state` (Anthropic/Pi).
    pub token_includes_state: bool,
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
pub static CLAUDE: OAuthProvider = OAuthProvider {
    id: "claude",
    agent: AgentId::Claude,
    authorize_url: "https://claude.ai/oauth/authorize",
    // Primary token endpoint used by Claude Code tooling.
    token_url: "https://console.anthropic.com/v1/oauth/token",
    client_id: "9d1c250a-e61b-44d9-88ed-5944d1962f5e",
    redirect_port: None,
    redirect_host: "127.0.0.1",
    redirect_path: "/callback",
    scopes: "org:create_api_key user:profile user:inference",
    body_style: TokenBodyStyle::Json,
    authorize_extra: "&code=true",
    token_includes_state: false,
};

/// OpenAI Codex CLI public client. Hydra matches redirect_uri exactly
/// (`localhost` ≠ `127.0.0.1`; path must be `/auth/callback`).
const CODEX_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
const CODEX_AUTHORIZE_URL: &str = "https://auth.openai.com/oauth/authorize";
const CODEX_TOKEN_URL: &str = "https://auth.openai.com/oauth/token";
const CODEX_REDIRECT_HOST: &str = "localhost";
const CODEX_REDIRECT_PATH: &str = "/auth/callback";
const CODEX_SCOPES: &str =
    "openid profile email offline_access api.connectors.read api.connectors.invoke";
const CODEX_AUTHORIZE_EXTRA: &str =
    "&id_token_add_organizations=true&codex_cli_simplified_flow=true&originator=codex_cli_rs";

/// OpenAI Codex / ChatGPT OAuth (fixed loopback port required by provider).
pub static CODEX: OAuthProvider = OAuthProvider {
    id: "codex",
    agent: AgentId::Codex,
    authorize_url: CODEX_AUTHORIZE_URL,
    token_url: CODEX_TOKEN_URL,
    client_id: CODEX_CLIENT_ID,
    redirect_port: Some(1455),
    redirect_host: CODEX_REDIRECT_HOST,
    redirect_path: CODEX_REDIRECT_PATH,
    scopes: CODEX_SCOPES,
    body_style: TokenBodyStyle::Form,
    authorize_extra: CODEX_AUTHORIZE_EXTRA,
    token_includes_state: false,
};

/// xAI / Grok OAuth (fixed loopback port required by provider).
pub static XAI: OAuthProvider = OAuthProvider {
    id: "xai",
    agent: AgentId::Grok,
    authorize_url: "https://accounts.x.ai/oauth/authorize",
    token_url: "https://accounts.x.ai/oauth/token",
    client_id: "grok-cli",
    redirect_port: Some(56121),
    redirect_host: "127.0.0.1",
    redirect_path: "/callback",
    scopes: "openid offline_access",
    body_style: TokenBodyStyle::Form,
    authorize_extra: "",
    token_includes_state: false,
};

/// Pi → Anthropic subscription OAuth (matches `@earendil-works/pi-ai` anthropic flow).
pub static PI_ANTHROPIC: OAuthProvider = OAuthProvider {
    id: "pi-anthropic",
    agent: AgentId::Pi,
    authorize_url: "https://claude.ai/oauth/authorize",
    // Pi uses platform.claude.com (not console.anthropic.com).
    token_url: "https://platform.claude.com/v1/oauth/token",
    client_id: "9d1c250a-e61b-44d9-88ed-5944d1962f5e",
    redirect_port: Some(53692),
    redirect_host: "localhost",
    redirect_path: "/callback",
    scopes: "org:create_api_key user:profile user:inference user:sessions:claude_code user:mcp_servers user:file_upload",
    body_style: TokenBodyStyle::Json,
    authorize_extra: "&code=true",
    token_includes_state: true,
};

/// Pi → OpenAI Codex OAuth (matches pi-ai openai-codex redirect path).
pub static PI_OPENAI_CODEX: OAuthProvider = OAuthProvider {
    id: "pi-openai-codex",
    agent: AgentId::Pi,
    authorize_url: CODEX_AUTHORIZE_URL,
    token_url: CODEX_TOKEN_URL,
    client_id: CODEX_CLIENT_ID,
    redirect_port: Some(1455),
    redirect_host: CODEX_REDIRECT_HOST,
    redirect_path: CODEX_REDIRECT_PATH,
    scopes: CODEX_SCOPES,
    body_style: TokenBodyStyle::Form,
    authorize_extra: CODEX_AUTHORIZE_EXTRA,
    token_includes_state: false,
};

impl OAuthProvider {
    /// Loopback redirect URI registered with the provider (host + path, not bind address).
    pub fn loopback_redirect_uri(&self, port: u16) -> String {
        let host = if self.redirect_host.is_empty() {
            "127.0.0.1"
        } else {
            self.redirect_host
        };
        let path = if self.redirect_path.is_empty() {
            "/callback"
        } else {
            self.redirect_path
        };
        format!("http://{host}:{port}{path}")
    }

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
        self.exchange_code_with_state(code, verifier, redirect_uri, None)
    }

    pub fn exchange_code_with_state(
        &self,
        code: &str,
        verifier: &str,
        redirect_uri: &str,
        state: Option<&str>,
    ) -> Result<TokenBundle> {
        // Some providers embed "#state" suffix in the returned code; strip it.
        let code = code.split('#').next().unwrap_or(code).trim();
        let mut fields: Vec<(&str, &str)> = vec![
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", redirect_uri),
            ("client_id", self.client_id),
            ("code_verifier", verifier),
        ];
        if self.token_includes_state {
            if let Some(st) = state.filter(|s| !s.is_empty()) {
                fields.push(("state", st));
            }
        }
        let body = self.token_request(&fields)?;
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

    fn token_endpoint(&self) -> String {
        #[cfg(test)]
        {
            if let Some(url) = TOKEN_URL_OVERRIDE.with(|slot| slot.borrow().clone()) {
                return url;
            }
        }
        self.token_url.to_string()
    }

    fn token_request(&self, fields: &[(&str, &str)]) -> Result<Value> {
        let token_url = self.token_endpoint();
        let result = match self.body_style {
            TokenBodyStyle::Form => {
                let mut req = ureq::post(&token_url)
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
                let mut req = ureq::post(&token_url)
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
        let body: Value = resp
            .into_json()
            .map_err(|e| AppError::message("oauth.token", format!("invalid token JSON: {e}")))?;

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
            Some((chrono::Utc::now() + chrono::Duration::seconds(expires_in.max(0))).to_rfc3339())
        } else {
            body.get("expires_at")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        };

        let id_token = body
            .get("id_token")
            .and_then(|v| v.as_str())
            .map(str::to_string);

        // Best-effort identity for Connections UI (email / plan / subject).
        let identity =
            extract_oauth_identity(self.id, &body, access.as_deref(), id_token.as_deref());

        let mut cred_map = Map::new();
        cred_map.insert("type".into(), json!("oauth"));
        cred_map.insert("provider".into(), json!(self.id));
        if let Some(ref at) = access {
            cred_map.insert("access_token".into(), json!(at));
        }
        if let Some(ref rt) = refresh {
            cred_map.insert("refresh_token".into(), json!(rt));
        }
        if let Some(ei) = body.get("expires_in") {
            cred_map.insert("expires_in".into(), ei.clone());
        }
        if let Some(ref exp) = expires_at {
            cred_map.insert("expires_at".into(), json!(exp));
        }
        if let Some(tt) = body.get("token_type") {
            cred_map.insert("token_type".into(), tt.clone());
        }
        if let Some(sc) = body.get("scope") {
            cred_map.insert("scope".into(), sc.clone());
        }
        if let Some(ref idt) = id_token {
            cred_map.insert("id_token".into(), json!(idt));
        }
        // Keep raw for debugging / future parsers; redaction masks secret keys.
        cred_map.insert("raw".into(), body.clone());
        apply_identity_to_credentials(&mut cred_map, &identity);

        let label_hint = identity
            .display_label()
            .unwrap_or_else(|| format!("{} · OAuth", self.agent.display_name()));

        let mut extra = identity_extra(self.id, &identity, expires_at.as_deref(), "oauth_pkce");
        // Ensure identityLabel is set even when only fallback label exists.
        if let Some(obj) = extra.as_object_mut() {
            obj.entry("identityLabel".to_string())
                .or_insert_with(|| json!(&label_hint));
        }

        if !identity.is_empty() {
            tracing::info!(
                module = targets::OAUTH,
                op = "token_identity",
                provider = self.id,
                has_email = identity.email.is_some(),
                has_subscription = identity.subscription.is_some(),
                "oauth identity extracted"
            );
        }

        Ok(TokenBundle {
            credentials: Value::Object(cred_map),
            label_hint: Some(label_hint),
            extra,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_authorize_url_contains_pkce_and_client() {
        let url = CLAUDE.build_authorize_url("http://127.0.0.1:12345/callback", "st", "ch");
        assert!(url.contains(&format!("client_id={}", CLAUDE.client_id)));
        assert!(url.contains("code_challenge=ch"));
        assert!(url.contains("code_challenge_method=S256"));
        assert!(url.contains("code=true"));
    }

    #[test]
    fn claude_loopback_redirect_uses_ipv4_callback() {
        assert_eq!(
            CLAUDE.loopback_redirect_uri(12345),
            "http://127.0.0.1:12345/callback"
        );
    }

    #[test]
    fn codex_authorize_url_matches_registered_cli_loopback() {
        let redirect = CODEX.loopback_redirect_uri(1455);
        assert_eq!(redirect, "http://localhost:1455/auth/callback");
        let url = CODEX.build_authorize_url(&redirect, "st", "ch");
        assert!(url.contains(&format!("client_id={}", CODEX_CLIENT_ID)));
        assert!(url.contains("redirect_uri=http%3A%2F%2Flocalhost%3A1455%2Fauth%2Fcallback"));
        assert!(url.contains("id_token_add_organizations=true"));
        assert!(url.contains("codex_cli_simplified_flow=true"));
        assert!(url.contains("originator=codex_cli_rs"));
        assert!(url.contains("api.connectors.read"));
        assert!(!url.contains("127.0.0.1"));
        assert!(!url.contains("/callback&"));
    }

    #[test]
    fn pi_openai_codex_shares_codex_authorize_registration() {
        let redirect = PI_OPENAI_CODEX.loopback_redirect_uri(1455);
        assert_eq!(redirect, CODEX.loopback_redirect_uri(1455));
        let url = PI_OPENAI_CODEX.build_authorize_url(&redirect, "st", "ch");
        assert!(url.contains("id_token_add_organizations=true"));
        assert!(url.contains("codex_cli_simplified_flow=true"));
        assert!(url.contains("originator=codex_cli_rs"));
    }

    #[test]
    fn oauth_provider_for_known_agents() {
        assert!(oauth_provider_for(AgentId::Claude).is_some());
        assert!(oauth_provider_for(AgentId::Codex).is_some());
        assert!(oauth_provider_for(AgentId::Grok).is_some());
        assert!(oauth_provider_for(AgentId::Kimi).is_none());
    }

    #[test]
    fn bundle_from_token_json_sets_email_label_from_claude_account() {
        let body = json!({
            "access_token": "at-1",
            "refresh_token": "rt-1",
            "expires_in": 3600,
            "account": {
                "uuid": "acct-1",
                "email_address": "me@anthropic.test"
            },
            "organization": { "uuid": "org-1" }
        });
        let bundle = CLAUDE.bundle_from_token_json(body).expect("bundle");
        assert_eq!(bundle.label_hint.as_deref(), Some("me@anthropic.test"));
        assert_eq!(
            bundle.credentials.get("email").and_then(|v| v.as_str()),
            Some("me@anthropic.test")
        );
        assert_eq!(
            bundle.extra.get("email").and_then(|v| v.as_str()),
            Some("me@anthropic.test")
        );
        assert_eq!(
            bundle.extra.get("identityLabel").and_then(|v| v.as_str()),
            Some("me@anthropic.test")
        );
        assert_eq!(
            bundle
                .credentials
                .get("organization_id")
                .and_then(|v| v.as_str()),
            Some("org-1")
        );
    }

    #[test]
    fn bundle_from_token_json_fallback_label_when_no_identity() {
        let body = json!({
            "access_token": "opaque-token",
            "refresh_token": "rt",
            "expires_in": 60
        });
        let bundle = XAI.bundle_from_token_json(body).expect("bundle");
        assert_eq!(bundle.label_hint.as_deref(), Some("Grok · OAuth"));
        assert!(bundle.credentials.get("email").is_none());
    }
}
