//! In-process session credentials for subscription-backed bridge upstreams.
//!
//! This module only resolves an already-loaded Codex credentials JSON value. It does not
//! persist, refresh, send, or expose credentials through IPC.

use serde_json::Value;

use crate::error::{AppError, Result};

use super::ResolvedAuth;

/// Pointers aligned with `adapters/codex.rs::normalize_oauth_credentials`.
const CODEX_ACCESS_TOKEN_POINTERS: &[&str] = &[
    "/access_token",
    "/tokens/access_token",
    "/body/tokens/access_token",
    "/raw/access_token",
    "/body/access_token",
];

/// Resolve the Codex subscription bearer from an account credentials JSON value.
///
/// Codex `auth.json` is recognized by its explicit `format` marker, a top-level `tokens`
/// object containing an access or refresh token, or a non-empty access token at any
/// OauthOther pointer shared with `normalize_oauth_credentials`. The access token is
/// kept only in the returned in-process [`ResolvedAuth`]; refresh tokens are
/// intentionally ignored by this provider.
pub fn resolve_codex_subscription_auth(credentials: &Value) -> Result<ResolvedAuth> {
    if !is_codex_auth_json(credentials) {
        return Err(AppError::InvalidArg(
            "Codex subscription credentials are not auth_json".into(),
        ));
    }

    let access_token = first_access_token(credentials).ok_or_else(|| {
        AppError::InvalidArg(
            "Codex subscription credentials missing access_token; re-authenticate Codex".into(),
        )
    })?;

    Ok(ResolvedAuth::bearer(access_token))
}

fn first_access_token(credentials: &Value) -> Option<&str> {
    CODEX_ACCESS_TOKEN_POINTERS.iter().find_map(|pointer| {
        credentials
            .pointer(pointer)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|token| !token.is_empty())
    })
}

fn is_codex_auth_json(credentials: &Value) -> bool {
    if credentials
        .get("format")
        .and_then(Value::as_str)
        .is_some_and(|format| format.eq_ignore_ascii_case("auth_json"))
    {
        return true;
    }

    if credentials
        .get("tokens")
        .and_then(Value::as_object)
        .is_some_and(|tokens| {
            tokens.contains_key("access_token") || tokens.contains_key("refresh_token")
        })
    {
        return true;
    }

    first_access_token(credentials).is_some()
}

#[cfg(test)]
mod tests;
