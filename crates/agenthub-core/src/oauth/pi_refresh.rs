//! Refresh tokens for Pi multi-provider OAuth entries in auth.json.

use serde_json::{json, Value};

use crate::error::{AppError, Result};
use crate::logging::targets;
use crate::oauth::identity::{
    apply_identity_to_credentials, extract_oauth_identity, identity_from_credentials,
};

/// Refresh a Pi OAuth provider and return updated credentials JSON (auth_json shape).
pub fn refresh_pi_provider(credentials: &Value) -> Result<Value> {
    let provider = credentials
        .get("provider")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .or_else(|| {
            credentials
                .get("body")
                .and_then(|b| b.as_object())
                .and_then(|o| {
                    if o.len() == 1 {
                        o.keys().next().map(|s| s.as_str())
                    } else {
                        None
                    }
                })
        })
        .ok_or_else(|| {
            AppError::message(
                "oauth.refresh",
                "Pi account missing provider key; re-import or re-login",
            )
        })?;

    let refresh = credentials
        .get("refresh_token")
        .or_else(|| credentials.get("refresh"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .or_else(|| {
            credentials
                .pointer(&format!("/body/{provider}/refresh"))
                .or_else(|| credentials.pointer(&format!("/body/{provider}/refresh_token")))
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
        })
        .ok_or_else(|| {
            AppError::message(
                "oauth.refresh",
                "Pi account has no refresh token; re-run OAuth login",
            )
        })?
        .to_string();

    // Normalize aliases (e.g. openai → openai-codex) before dispatch.
    let canonical = super::pi_auth_json_key(provider).unwrap_or(provider);
    if !super::pi_provider_refreshable(canonical) {
        return Err(AppError::Unsupported(format!(
            "Pi token refresh not implemented for provider '{provider}'"
        )));
    }
    let token_json = match canonical {
        "anthropic" => refresh_anthropic(&refresh)?,
        "openai-codex" => refresh_openai_codex(&refresh)?,
        "xai" => refresh_xai(&refresh)?,
        other => {
            return Err(AppError::Unsupported(format!(
                "Pi token refresh not implemented for provider '{other}'"
            )));
        }
    };

    let access = token_json
        .get("access_token")
        .or_else(|| token_json.get("access"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::message("oauth.refresh", "refresh response missing access"))?
        .to_string();
    let new_refresh = token_json
        .get("refresh_token")
        .or_else(|| token_json.get("refresh"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or(&refresh)
        .to_string();
    let expires_in = token_json
        .get("expires_in")
        .and_then(|v| v.as_i64())
        .unwrap_or(3600);
    let expires_ms = chrono::Utc::now().timestamp_millis()
        + expires_in * 1000
        - crate::catalog::limits::OAUTH_REFRESH_SKEW_MS;
    let expires_at = chrono::DateTime::from_timestamp(expires_ms / 1000, 0)
        .map(|dt| dt.to_rfc3339())
        .unwrap_or_default();

    let entry = json!({
        "type": "oauth",
        "access": access,
        "refresh": new_refresh,
        "expires": expires_ms,
    });

    let mut identity = extract_oauth_identity(
        canonical,
        &token_json,
        Some(&access),
        token_json.get("id_token").and_then(|v| v.as_str()),
    );
    identity.merge_missing(&identity_from_credentials(credentials));

    let mut body = serde_json::Map::new();
    body.insert(canonical.to_string(), entry);

    let mut cred = serde_json::Map::new();
    cred.insert("format".into(), json!("auth_json"));
    cred.insert("provider".into(), json!(canonical));
    cred.insert("body".into(), Value::Object(body));
    cred.insert("access_token".into(), json!(access));
    cred.insert("refresh_token".into(), json!(new_refresh));
    cred.insert("expires_at".into(), json!(expires_at));
    apply_identity_to_credentials(&mut cred, &identity);

    tracing::info!(
        module = targets::OAUTH,
        op = "pi_refresh",
        provider = canonical,
        has_email = identity.email.is_some(),
        "pi oauth token refreshed"
    );

    Ok(Value::Object(cred))
}

fn refresh_anthropic(refresh_token: &str) -> Result<Value> {
    // Matches pi-ai anthropic refresh endpoint.
    post_json(
        "https://platform.claude.com/v1/oauth/token",
        &json!({
            "grant_type": "refresh_token",
            "client_id": "9d1c250a-e61b-44d9-88ed-5944d1962f5e",
            "refresh_token": refresh_token,
        }),
    )
}

fn refresh_openai_codex(refresh_token: &str) -> Result<Value> {
    post_form(
        "https://auth.openai.com/oauth/token",
        &[
            ("grant_type", "refresh_token"),
            ("client_id", "app_EMoamEEZ73f0CkXaXp7hrann"),
            ("refresh_token", refresh_token),
            ("scope", "openid profile email"),
        ],
    )
}

fn refresh_xai(refresh_token: &str) -> Result<Value> {
    // Pi uses auth.x.ai device-code client (not accounts.x.ai grok-cli).
    post_form(
        "https://auth.x.ai/oauth2/token",
        &[
            ("grant_type", "refresh_token"),
            ("client_id", "b1a00492-073a-47ea-816f-4c329264a828"),
            ("refresh_token", refresh_token),
        ],
    )
}

fn post_json(url: &str, body: &Value) -> Result<Value> {
    let mut req = ureq::post(url)
        .set("Content-Type", "application/json")
        .set("Accept", "application/json");
    req = req.timeout(crate::catalog::limits::OAUTH_TOKEN_HTTP_TIMEOUT);
    let resp = req
        .send_json(body.clone())
        .map_err(|e| AppError::message("oauth.refresh", format!("refresh failed: {e}")))?;
    parse_token_response(resp)
}

fn post_form(url: &str, fields: &[(&str, &str)]) -> Result<Value> {
    let mut req = ureq::post(url)
        .set("Content-Type", "application/x-www-form-urlencoded")
        .set("Accept", "application/json");
    req = req.timeout(crate::catalog::limits::OAUTH_TOKEN_HTTP_TIMEOUT);
    let resp = req
        .send_form(fields)
        .map_err(|e| AppError::message("oauth.refresh", format!("refresh failed: {e}")))?;
    parse_token_response(resp)
}

fn parse_token_response(resp: ureq::Response) -> Result<Value> {
    let status = resp.status();
    let body: Value = resp
        .into_json()
        .map_err(|e| AppError::message("oauth.refresh", format!("invalid token JSON: {e}")))?;
    if !(200..300).contains(&status) {
        let msg = body
            .get("error_description")
            .or_else(|| body.get("error"))
            .and_then(|v| v.as_str())
            .unwrap_or("refresh rejected");
        return Err(AppError::message(
            "oauth.refresh",
            format!("{msg} (HTTP {status})"),
        ));
    }
    Ok(body)
}
