//! AgentHub-owned Kiro upstream client (community protocol; not official REST).
//!
//! Verified Builder ID path (2026-09):
//! - host: `https://q.{region}.amazonaws.com/`
//! - list: `AmazonCodeWhispererService.ListAvailableModels`
//! - usage: `AmazonCodeWhispererService.GetUsageLimits` (credits)
//! - chat: `AmazonCodeWhispererStreamingService.GenerateAssistantResponse`
//! - `runtime.{region}.kiro.dev` requires `profileArn` (enterprise / deferred)

use std::io::Read;
use std::time::Instant;

use serde_json::{json, Value};
use uuid::Uuid;

use crate::error::{AppError, Result};
use crate::models::{AgentId, AgentRunResult, RunOptions, RunStatus};
use crate::utils::redact::redact_text;

use super::creds::{
    load_kiro_http_creds, persist_refreshed_token, KiroAuthKind, KiroHttpCreds, KiroHttpRouteParams,
};
use super::eventstream::collect_assistant_text;

const LIST_TARGET: &str = "AmazonCodeWhispererService.ListAvailableModels";
const USAGE_TARGET: &str = "AmazonCodeWhispererService.GetUsageLimits";
const CHAT_TARGET: &str = "AmazonCodeWhispererStreamingService.GenerateAssistantResponse";
const USER_AGENT: &str =
    "aws-sdk-js/1.0.27 ua/2.1 os/linux lang/js md/nodejs#22.0.0 api/codewhispererstreaming#1.0.27 m/E AgentHub-KiroHTTP";

/// Persisted Chat `native_session_id` prefix for HTTP conversation ids.
/// CLI `--resume-id` values are a different namespace and must not use this prefix.
pub(crate) const HTTP_NATIVE_SESSION_PREFIX: &str = "kiro-http:";

/// Encode an HTTP conversationId for ChatService persistence.
pub(crate) fn http_native_session_id(conversation_id: &str) -> String {
    format!("{HTTP_NATIVE_SESSION_PREFIX}{conversation_id}")
}

/// Strip `kiro-http:` prefix. Returns None for blank, unprefixed (CLI), or empty id.
pub(crate) fn parse_http_native_session_id(native: &str) -> Option<&str> {
    let native = native.trim();
    let rest = native.strip_prefix(HTTP_NATIVE_SESSION_PREFIX)?;
    let rest = rest.trim();
    if rest.is_empty() {
        None
    } else {
        Some(rest)
    }
}

/// How HTTP chat should treat `RunOptions.native_session_id`.
///
/// CLI `--resume-id` values are **not** prefixed → `http_resume_from_opts` is `None`
/// (skip HTTP). Unset/blank → `Some(New)`. `kiro-http:<cid>` → `Some(Conversation)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HttpResume<'a> {
    New,
    Conversation(&'a str),
}

pub(crate) fn http_resume_from_opts(opts: &RunOptions) -> Option<HttpResume<'_>> {
    match opts
        .native_session_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        None => Some(HttpResume::New),
        Some(id) => parse_http_native_session_id(id).map(HttpResume::Conversation),
    }
}

#[derive(Debug, Clone)]
pub(crate) struct KiroListModels {
    pub default_model: Option<String>,
    pub models: Vec<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct KiroChatTurn {
    pub text: String,
    pub conversation_id: Option<String>,
    pub model_id: String,
}

/// Ensure access token is usable (refresh OIDC/Desktop when near expiry).
pub(crate) fn ensure_access_token(creds: &mut KiroHttpCreds) -> Result<()> {
    if !creds.needs_refresh() {
        return Ok(());
    }
    match creds.auth_kind {
        KiroAuthKind::ApiKey => Ok(()),
        KiroAuthKind::Oidc => refresh_oidc(creds),
        KiroAuthKind::Desktop => refresh_desktop(creds),
    }
}

fn refresh_oidc(creds: &mut KiroHttpCreds) -> Result<()> {
    let refresh = creds
        .refresh_token
        .as_deref()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| AppError::message("kiro.http.refresh", "missing refresh token"))?;
    let client_id = creds
        .client_id
        .as_deref()
        .ok_or_else(|| AppError::message("kiro.http.refresh", "missing OIDC client id"))?;
    let client_secret = creds
        .client_secret
        .as_deref()
        .ok_or_else(|| AppError::message("kiro.http.refresh", "missing OIDC client secret"))?;
    let url = format!("https://oidc.{}.amazonaws.com/token", creds.region);
    let body = json!({
        "grantType": "refresh_token",
        "clientId": client_id,
        "clientSecret": client_secret,
        "refreshToken": refresh,
    });
    let resp = ureq::post(&url)
        .set("Content-Type", "application/json")
        .timeout(super::creds::http_timeout())
        .send_json(body)
        .map_err(map_ureq("kiro.http.oidc_refresh"))?;
    let value: Value = resp
        .into_json()
        .map_err(|e| AppError::message("kiro.http.oidc_refresh", redact_text(&e.to_string())))?;
    let access = value
        .get("accessToken")
        .or_else(|| value.get("access_token"))
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            AppError::message("kiro.http.oidc_refresh", "response missing accessToken")
        })?;
    creds.access_token = access.to_string();
    if let Some(r) = value
        .get("refreshToken")
        .or_else(|| value.get("refresh_token"))
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
    {
        creds.refresh_token = Some(r.to_string());
    }
    let expires_in = value
        .get("expiresIn")
        .or_else(|| value.get("expires_in"))
        .and_then(Value::as_u64)
        .unwrap_or(3600);
    creds.expires_at =
        Some(chrono::Utc::now() + chrono::Duration::seconds(expires_in.saturating_sub(60) as i64));
    let _ = persist_refreshed_token(creds);
    Ok(())
}

fn refresh_desktop(creds: &mut KiroHttpCreds) -> Result<()> {
    let refresh = creds
        .refresh_token
        .as_deref()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| AppError::message("kiro.http.refresh", "missing refresh token"))?;
    let url = format!(
        "https://prod.{}.auth.desktop.kiro.dev/refreshToken",
        creds.region
    );
    let body = json!({ "refreshToken": refresh });
    let resp = ureq::post(&url)
        .set("Content-Type", "application/json")
        .set("User-Agent", USER_AGENT)
        .timeout(super::creds::http_timeout())
        .send_json(body)
        .map_err(map_ureq("kiro.http.desktop_refresh"))?;
    let value: Value = resp
        .into_json()
        .map_err(|e| AppError::message("kiro.http.desktop_refresh", redact_text(&e.to_string())))?;
    let access = value
        .get("accessToken")
        .or_else(|| value.get("access_token"))
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            AppError::message("kiro.http.desktop_refresh", "response missing accessToken")
        })?;
    creds.access_token = access.to_string();
    if let Some(r) = value
        .get("refreshToken")
        .or_else(|| value.get("refresh_token"))
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
    {
        creds.refresh_token = Some(r.to_string());
    }
    if let Some(arn) = value
        .get("profileArn")
        .or_else(|| value.get("profile_arn"))
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
    {
        creds.profile_arn = Some(arn.to_string());
    }
    let expires_in = value
        .get("expiresIn")
        .or_else(|| value.get("expires_in"))
        .and_then(Value::as_u64)
        .unwrap_or(3600);
    creds.expires_at =
        Some(chrono::Utc::now() + chrono::Duration::seconds(expires_in.saturating_sub(60) as i64));
    let _ = persist_refreshed_token(creds);
    Ok(())
}

fn q_host(region: &str) -> String {
    format!("https://q.{region}.amazonaws.com/")
}

fn amz_headers(creds: &KiroHttpCreds, target: &str) -> Vec<(String, String)> {
    let mut headers = vec![
        (
            "Authorization".into(),
            format!("Bearer {}", creds.access_token),
        ),
        ("Content-Type".into(), "application/x-amz-json-1.0".into()),
        ("Accept".into(), "*/*".into()),
        ("x-amz-target".into(), target.into()),
        ("User-Agent".into(), USER_AGENT.into()),
        ("x-amz-user-agent".into(), USER_AGENT.into()),
        ("x-amzn-codewhisperer-optout".into(), "true".into()),
        ("amz-sdk-invocation-id".into(), Uuid::new_v4().to_string()),
        ("amz-sdk-request".into(), "attempt=1; max=1".into()),
    ];
    if let Some(tt) = creds.token_type_header() {
        headers.push(("tokentype".into(), tt.into()));
    }
    headers
}

fn post_amz(creds: &KiroHttpCreds, target: &str, body: &Value) -> Result<Vec<u8>> {
    let url = q_host(&creds.region);
    let mut req = ureq::post(&url).timeout(super::creds::http_timeout());
    for (k, v) in amz_headers(creds, target) {
        req = req.set(&k, &v);
    }
    let bytes = serde_json::to_vec(body)
        .map_err(|e| AppError::InvalidArg(format!("kiro http body: {e}")))?;
    match req.send_bytes(&bytes) {
        Ok(resp) => read_body(resp),
        Err(ureq::Error::Status(status, resp)) => {
            let body = read_body(resp).unwrap_or_default();
            let preview = String::from_utf8_lossy(&body);
            Err(AppError::message(
                "kiro.http.upstream",
                redact_text(&format!(
                    "HTTP {status}: {}",
                    preview.chars().take(300).collect::<String>()
                )),
            ))
        }
        Err(e) => Err(map_ureq("kiro.http.upstream")(e)),
    }
}

fn read_body(resp: ureq::Response) -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    resp.into_reader()
        .take(8 * 1024 * 1024)
        .read_to_end(&mut buf)
        .map_err(|e| AppError::message("kiro.http.read", redact_text(&e.to_string())))?;
    Ok(buf)
}

fn map_ureq(code: &'static str) -> impl Fn(ureq::Error) -> AppError {
    move |e| AppError::message(code, redact_text(&e.to_string()))
}

/// Official credit window. Empty JSON body is the verified Builder ID request.
pub(crate) fn get_usage_limits(access_token: &str, region: &str) -> Result<Value> {
    let region = region.trim();
    let region = if region.is_empty() {
        "us-east-1"
    } else {
        region
    };
    let creds = KiroHttpCreds {
        auth_kind: KiroAuthKind::Oidc,
        access_token: access_token.trim().to_string(),
        refresh_token: None,
        expires_at: None,
        region: region.to_string(),
        profile_arn: None,
        client_id: None,
        client_secret: None,
        origin: "KIRO_CLI".into(),
        sqlite_token_key: None,
        source: "account".into(),
    };
    if creds.access_token.is_empty() {
        return Err(AppError::message("kiro.http.usage", "missing access token"));
    }
    let raw = post_amz(&creds, USAGE_TARGET, &json!({}))?;
    serde_json::from_slice(&raw).map_err(|e| {
        AppError::message(
            "kiro.http.usage",
            redact_text(&format!("invalid JSON: {e}")),
        )
    })
}

/// List models via HTTP. Fail-closed (no static catalog).
pub(crate) fn list_models_http() -> Result<KiroListModels> {
    let mut creds = load_kiro_http_creds()?;
    ensure_access_token(&mut creds)?;
    let body = json!({ "origin": creds.origin });
    let raw = post_amz(&creds, LIST_TARGET, &body)?;
    let value: Value = serde_json::from_slice(&raw).map_err(|e| {
        AppError::message(
            "kiro.http.list_models",
            redact_text(&format!("invalid JSON: {e}")),
        )
    })?;
    Ok(parse_list_models_response(&value))
}

pub(crate) fn parse_list_models_response(value: &Value) -> KiroListModels {
    let default_model = value
        .get("defaultModel")
        .or_else(|| value.get("default_model"))
        .and_then(|v| {
            v.as_str().map(str::to_owned).or_else(|| {
                v.get("modelId")
                    .or_else(|| v.get("model_id"))
                    .and_then(|x| x.as_str())
                    .map(str::to_owned)
            })
        })
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let mut models = Vec::new();
    let mut seen = std::collections::HashSet::new();
    if let Some(arr) = value.get("models").and_then(Value::as_array) {
        for entry in arr {
            let id = entry
                .get("modelId")
                .or_else(|| entry.get("model_id"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty());
            let Some(id) = id else { continue };
            if seen.insert(id.to_string()) {
                models.push(id.to_string());
            }
        }
    }
    if let Some(id) = default_model.as_ref() {
        if seen.insert(id.clone()) {
            models.insert(0, id.clone());
        }
    }
    KiroListModels {
        default_model,
        models,
    }
}

pub(crate) fn build_chat_body(
    prompt: &str,
    model_id: &str,
    origin: &str,
    conversation_id: Option<&str>,
    profile_arn: Option<&str>,
) -> Value {
    let cid = conversation_id
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let mut body = json!({
        "conversationState": {
            "chatTriggerType": "MANUAL",
            "conversationId": cid,
            "currentMessage": {
                "userInputMessage": {
                    "content": prompt,
                    "modelId": model_id,
                    "origin": origin,
                }
            }
        }
    });
    // profileArn only when present (Builder ID / API Key omit it).
    if let Some(arn) = profile_arn.map(str::trim).filter(|s| !s.is_empty()) {
        body.as_object_mut()
            .unwrap()
            .insert("profileArn".into(), json!(arn));
    }
    body
}

pub(crate) fn creds_from_access_token(
    token: &str,
    params: Option<&KiroHttpRouteParams>,
) -> KiroHttpCreds {
    let token = token.trim().to_string();
    let inferred = KiroHttpRouteParams::from_access_token(&token);
    let params = params.unwrap_or(&inferred);
    let api_key = params.api_key || token.starts_with("ksk_");
    let auth_kind = if api_key {
        KiroAuthKind::ApiKey
    } else if params.origin == "KIRO_CLI" {
        KiroAuthKind::Oidc
    } else {
        KiroAuthKind::Desktop
    };
    let region = params.region.trim();
    KiroHttpCreds {
        auth_kind,
        access_token: token,
        refresh_token: None,
        expires_at: None,
        region: if region.is_empty() {
            "us-east-1".into()
        } else {
            region.to_owned()
        },
        profile_arn: params
            .profile_arn
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned),
        client_id: None,
        client_secret: None,
        origin: if params.origin.trim().is_empty() {
            "AI_EDITOR".into()
        } else {
            params.origin.clone()
        },
        sqlite_token_key: None,
        source: "bridge".into(),
    }
}

/// One non-streaming chat turn against Kiro upstream.
#[cfg(test)]
pub(crate) fn chat_turn_http(
    prompt: &str,
    model: Option<&str>,
    conversation_id: Option<&str>,
) -> Result<KiroChatTurn> {
    let mut creds = load_kiro_http_creds()?;
    chat_turn_with_creds(&mut creds, prompt, model, conversation_id)
}

pub(crate) fn chat_turn_with_access_token(
    token: &str,
    prompt: &str,
    model: Option<&str>,
    params: Option<&KiroHttpRouteParams>,
) -> Result<KiroChatTurn> {
    let mut creds = creds_from_access_token(token, params);
    // Pool logins have a current access token only. Do not invent a Desktop
    // refresh, and do not write another machine's kiro-cli sqlite.
    post_chat_turn(&mut creds, prompt, model, None)
}

fn chat_turn_with_creds(
    creds: &mut KiroHttpCreds,
    prompt: &str,
    model: Option<&str>,
    conversation_id: Option<&str>,
) -> Result<KiroChatTurn> {
    ensure_access_token(creds)?;
    post_chat_turn(creds, prompt, model, conversation_id)
}

fn post_chat_turn(
    creds: &mut KiroHttpCreds,
    prompt: &str,
    model: Option<&str>,
    conversation_id: Option<&str>,
) -> Result<KiroChatTurn> {
    let model_id = model
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("auto")
        .to_string();
    let body = build_chat_body(
        prompt,
        &model_id,
        &creds.origin,
        conversation_id,
        creds.profile_arn.as_deref(),
    );
    let raw = post_amz(&creds, CHAT_TARGET, &body)?;
    let (text, cid) = collect_assistant_text(&raw);
    if text.trim().is_empty() {
        let preview = String::from_utf8_lossy(&raw);
        if preview.contains("ValidationException")
            || preview.contains("AccessDenied")
            || preview.contains("__type")
        {
            return Err(AppError::message(
                "kiro.http.chat",
                redact_text(&preview.chars().take(300).collect::<String>()),
            ));
        }
        return Err(AppError::message(
            "kiro.http.chat",
            "empty assistant response from Kiro HTTP",
        ));
    }
    Ok(KiroChatTurn {
        text,
        conversation_id: cid.or_else(|| {
            body.pointer("/conversationState/conversationId")
                .and_then(Value::as_str)
                .map(str::to_owned)
        }),
        model_id,
    })
}

/// Build the explicit failed result used when an existing HTTP conversation
/// cannot be continued. Keeping the namespaced session id on the result lets
/// the caller retain the conversation for a retry instead of silently
/// switching it to the unrelated CLI session namespace.
fn conversation_id_preview(id: &str) -> String {
    id.chars().take(8).collect()
}

pub(crate) fn http_failed_run_result(
    duration_ms: u64,
    model: Option<&str>,
    conversation_id: Option<&str>,
    error: impl Into<String>,
) -> AgentRunResult {
    let model_id = model
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("auto");
    let command = format!(
        "kiro-http GenerateAssistantResponse model={}{}",
        model_id,
        conversation_id
            .map(|id| format!(" conversationId={}", conversation_id_preview(id)))
            .unwrap_or_default()
    );
    let error = redact_text(&error.into());
    AgentRunResult {
        agent: AgentId::Kiro,
        status: RunStatus::Failed,
        exit_code: None,
        duration_ms,
        stdout: String::new(),
        stderr: String::new(),
        command,
        error: Some(error),
        truncated: false,
        native_session_id: conversation_id.map(http_native_session_id),
    }
}

/// Try AgentHub-owned HTTP chat. `None` means skip HTTP (use CLI / ACP).
///
/// Session namespaces:
/// - unset → new HTTP conversation
/// - `kiro-http:<conversationId>` → continue that HTTP conversation
/// - any other id (CLI `--resume-id`) → skip HTTP
///
/// On success, persists a namespaced id so ChatService can resume later turns.
pub(crate) fn try_http_run_result(prompt: &str, opts: &RunOptions) -> Option<AgentRunResult> {
    let resume = http_resume_from_opts(opts)?;
    let conversation_id = match resume {
        HttpResume::New => None,
        HttpResume::Conversation(cid) => Some(cid),
    };
    let started = Instant::now();
    let model = opts.model.as_deref();
    let mut creds = match load_kiro_http_creds() {
        Ok(creds) => creds,
        Err(e) => {
            if conversation_id.is_some() {
                return Some(http_failed_run_result(
                    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
                    model,
                    conversation_id,
                    e.to_string(),
                ));
            }
            return None;
        }
    };
    match chat_turn_with_creds(&mut creds, prompt, model, conversation_id) {
        Ok(turn) => {
            let duration_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
            let command = format!(
                "kiro-http GenerateAssistantResponse model={}{}",
                turn.model_id,
                turn.conversation_id
                    .as_deref()
                    .map(|id| format!(" conversationId={}", conversation_id_preview(id)))
                    .unwrap_or_default()
            );
            Some(AgentRunResult {
                agent: AgentId::Kiro,
                status: RunStatus::Ok,
                exit_code: Some(0),
                duration_ms,
                stdout: turn.text,
                stderr: String::new(),
                command,
                error: None,
                truncated: false,
                native_session_id: turn.conversation_id.as_deref().map(http_native_session_id),
            })
        }
        Err(e) => {
            tracing::debug!(
                module = "adapters.kiro.http",
                error = %e,
                "Kiro HTTP chat failed"
            );
            conversation_id.map(|cid| {
                http_failed_run_result(
                    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
                    model,
                    Some(cid),
                    e.to_string(),
                )
            })
        }
    }
}

#[cfg(test)]
#[path = "client_tests.rs"]
mod tests;
