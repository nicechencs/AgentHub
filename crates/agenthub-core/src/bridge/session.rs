//! In-process session credentials for subscription-backed bridge upstreams.
//!
//! This module only resolves an already-loaded Codex credentials JSON value. It does not
//! persist, refresh, send, or expose credentials through IPC.

use serde_json::Value;

use crate::error::{AppError, Result};

use super::ResolvedAuth;

/// Resolve the Codex subscription bearer from an account credentials JSON value.
///
/// Codex `auth.json` is recognized by its explicit `format` marker or by a top-level `tokens`
/// object containing an access or refresh token. The access token is kept only in the returned
/// in-process [`ResolvedAuth`]; refresh tokens are intentionally ignored by this provider.
pub fn resolve_codex_subscription_auth(credentials: &Value) -> Result<ResolvedAuth> {
    if !is_codex_auth_json(credentials) {
        return Err(AppError::InvalidArg(
            "Codex subscription credentials are not auth_json".into(),
        ));
    }

    let access_token = credentials
        .pointer("/tokens/access_token")
        .and_then(Value::as_str)
        .filter(|token| !token.is_empty())
        .or_else(|| {
            credentials
                .pointer("/body/tokens/access_token")
                .and_then(Value::as_str)
                .filter(|token| !token.is_empty())
        })
        .ok_or_else(|| {
            AppError::InvalidArg(
                "Codex subscription credentials missing access_token; re-authenticate Codex".into(),
            )
        })?;

    Ok(ResolvedAuth::bearer(access_token))
}

fn is_codex_auth_json(credentials: &Value) -> bool {
    if credentials
        .get("format")
        .and_then(Value::as_str)
        .is_some_and(|format| format.eq_ignore_ascii_case("auth_json"))
    {
        return true;
    }

    credentials
        .get("tokens")
        .and_then(Value::as_object)
        .is_some_and(|tokens| {
            tokens.contains_key("access_token") || tokens.contains_key("refresh_token")
        })
}

#[cfg(test)]
mod tests;
