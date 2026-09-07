//! Upstream subscription quota windows (5h / 7d) for OAuth accounts.
//!
//! Codex (aligned with sub2api). These URLs stay hardcoded; live catalogs are
//! not stored in the DB or a config file:
//! - conversation: `POST https://chatgpt.com/backend-api/codex/responses`
//! - quota: that same `POST` (`x-codex-*` headers), then
//!   `GET https://chatgpt.com/backend-api/wham/usage`
//! - models: `GET https://chatgpt.com/backend-api/codex/models`
//!   (see [`crate::utils::chatgpt_codex_models`]). Never `api.openai.com/v1/models`
//!   for ChatGPT OAuth.
//! Quota details: 5h/7d come from `x-codex-*` headers. `/wham/usage` top-level
//! `rate_limit` is the shared ChatGPT/Codex pool; `additional_rate_limits`
//! (e.g. Spark `codex_bengalfox`) is used only when that pool has no windows.
//!
//! Claude OAuth: `GET https://api.anthropic.com/api/oauth/usage`.
//!
//! Results are written into `account.extra` for the existing UI fields
//! (`quota5hPct`, `quota7dPct`, `quotaResetIn`). List probes are best-effort;
//! an explicit Connections refresh surfaces probe failures.
//!
//! The Connections 5h QuotaBar is official-only: if Codex `/responses` headers
//! or `/wham/usage` omit the 5h window, `quota5hPct` is cleared and the UI
//! hides that bar. Do not invent a 5h percent from the 7d window.

use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use serde_json::{json, Map, Value};

use crate::bridge::grok_cli::{grok_cli_identity_header_pairs, GROK_CLI_PROXY_BASE_URL};
use crate::catalog::limits::{ACCOUNT_QUOTA_CACHE_TTL, ACCOUNT_QUOTA_HTTP_TIMEOUT};
use crate::error::{AppError, Result};
use crate::logging::targets;
use crate::models::{Account, AccountKind, AgentId};
use crate::oauth::{chatgpt_account_id_from_token, decode_jwt_payload};

const CHATGPT_USAGE_URL: &str = "https://chatgpt.com/backend-api/wham/usage";
/// Codex desktop probe — rate limits arrive in `x-codex-*` response headers.
const CHATGPT_CODEX_RESPONSES_URL: &str = "https://chatgpt.com/backend-api/codex/responses";
const CLAUDE_USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
/// Cheap Codex model used by sub2api for header-only usage probes.
const CODEX_PROBE_MODEL: &str = "codex-auto-review";
/// User-Agent identity sent with the Codex `/responses` probe (matches Codex TUI).
const CODEX_PROBE_VERSION: &str = "0.146.0";
const CODEX_PROBE_ORIGINATOR: &str = "codex-tui";
/// `/wham/usage` impersonates Codex Desktop (sub2api `openaiQuotaCodexOriginator`).
const CHATGPT_WHAM_ORIGINATOR: &str = "Codex Desktop";

#[derive(Debug, Clone, Default, PartialEq)]
pub struct QuotaSnapshot {
    pub quota5h_pct: Option<f64>,
    pub quota7d_pct: Option<f64>,
    pub reset_5h_at: Option<DateTime<Utc>>,
    pub reset_7d_at: Option<DateTime<Utc>>,
    /// Kiro official credits (not a 5h/7d window).
    pub credit_used: Option<f64>,
    pub credit_limit: Option<f64>,
    pub credit_reset_at: Option<DateTime<Utc>>,
    pub plan_type: Option<String>,
    pub source: &'static str,
}

impl QuotaSnapshot {
    pub fn is_empty(&self) -> bool {
        self.quota5h_pct.is_none() && self.quota7d_pct.is_none() && self.credit_limit.is_none()
    }

    /// Prefer 5h reset text when available (matches UI QuotaBar on the 5h row).
    /// Never fall back to 7d remaining for the 5h row — that produced "9d" on weekly data.
    pub fn reset_in_label(&self, now: DateTime<Utc>) -> Option<String> {
        let at = self.reset_5h_at?;
        let rem = (at - now).num_seconds();
        // Hard cap: 5h bar cannot show more than ~5h remaining.
        Some(format_reset_in(clamp_reset_after(rem, 5 * 3600)))
    }

    pub fn reset_in_label_7d(&self, now: DateTime<Utc>) -> Option<String> {
        let at = self.reset_7d_at?;
        let rem = (at - now).num_seconds();
        Some(format_reset_in(clamp_reset_after(rem, 7 * 24 * 3600)))
    }
}

/// Recompute frozen reset labels from absolute timestamps.
/// Call on list so the countdown does not stick at the probe-time value.
pub fn refresh_quota_reset_label(account: &mut Account, now: DateTime<Utc>) -> bool {
    if !account.extra.is_object() {
        return false;
    }
    let at5 = account
        .extra
        .get("quota5hResetAt")
        .and_then(|v| v.as_str())
        .and_then(parse_rfc3339);
    let at7 = account
        .extra
        .get("quota7dResetAt")
        .and_then(|v| v.as_str())
        .and_then(parse_rfc3339);

    let label5 =
        at5.map(|at| format_reset_in(clamp_reset_after((at - now).num_seconds(), 5 * 3600)));
    let label7 =
        at7.map(|at| format_reset_in(clamp_reset_after((at - now).num_seconds(), 7 * 24 * 3600)));

    let hide_5h = at5.map(|t| t <= now).unwrap_or(false);
    let zero_7d = at7.map(|t| t <= now).unwrap_or(false);

    let Some(obj) = account.extra.as_object_mut() else {
        return false;
    };
    let mut dirty = false;
    if hide_5h {
        // Window ended and upstream has not returned a new 5h percent — hide, do not invent 0%.
        dirty |= clear_codex_5h_quota_fields(obj);
    } else if let Some(ref label) = label5 {
        let prev = obj
            .get("quotaResetIn")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if prev != label {
            obj.insert("quotaResetIn".into(), json!(label));
            dirty = true;
        }
    }
    if let Some(ref label) = label7 {
        let prev = obj
            .get("quota7dResetIn")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if prev != label {
            obj.insert("quota7dResetIn".into(), json!(label));
            dirty = true;
        }
    }
    if zero_7d && obj.get("quota7dPct").and_then(|v| v.as_i64()) != Some(0) {
        // Do not zero Grok weekly % just because period end passed without refresh;
        // only clear when we had an absolute reset and it's past.
        obj.insert("quota7dPct".into(), json!(0));
        dirty = true;
    }
    dirty
}

const CODEX_5H_QUOTA_KEYS: &[&str] = &[
    "quota5hPct",
    "quota5hResetAt",
    "codex_5h_reset_after_seconds",
    "quota5hResetAfterSec",
    "quotaResetIn",
];

fn clear_codex_5h_quota_fields(obj: &mut Map<String, Value>) -> bool {
    let mut dirty = false;
    for key in CODEX_5H_QUOTA_KEYS {
        if obj.remove(*key).is_some() {
            dirty = true;
        }
    }
    dirty
}

/// True when extra has no fresh quota snapshot (missing or older than cache TTL).
pub fn quota_is_stale(account: &Account, now: DateTime<Utc>) -> bool {
    if account.kind != AccountKind::Oauth {
        return false;
    }
    // Window already rolled over — cached used% is from the previous period.
    if extra_reset_elapsed(account, "quota5hResetAt", now)
        || extra_reset_elapsed(account, "quota7dResetAt", now)
    {
        return true;
    }
    let Some(raw) = account
        .extra
        .get("quotaUpdatedAt")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
    else {
        // No snapshot timestamp: probe even if leftover % fields exist,
        // otherwise Connections stays frozen on import leftovers forever.
        return true;
    };
    match DateTime::parse_from_rfc3339(raw) {
        Ok(dt) => {
            now.signed_duration_since(dt.with_timezone(&Utc))
                >= ChronoDuration::from_std(ACCOUNT_QUOTA_CACHE_TTL)
                    .unwrap_or_else(|_| ChronoDuration::minutes(10))
        }
        Err(_) => true,
    }
}

fn extra_reset_elapsed(account: &Account, key: &str, now: DateTime<Utc>) -> bool {
    account
        .extra
        .get(key)
        .and_then(|v| v.as_str())
        .and_then(parse_rfc3339)
        .map(|at| at <= now)
        .unwrap_or(false)
}

/// Best-effort network probe; updates `account.extra` on success.
/// Returns true when extra was modified.
pub fn refresh_account_quota(account: &mut Account, force: bool) -> Result<bool> {
    if account.kind != AccountKind::Oauth {
        return Ok(false);
    }
    let now = Utc::now();
    if !force && !quota_is_stale(account, now) {
        return Ok(false);
    }

    let snap = match account.agent_id {
        AgentId::Codex => fetch_codex_quota(account)?,
        AgentId::Claude => fetch_claude_quota(account)?,
        AgentId::Grok => fetch_grok_quota(account)?,
        AgentId::Kiro => fetch_kiro_quota(account)?,
        AgentId::Pi => {
            // Pi multi-provider routing lives in oauth::catalog (aliases → backend).
            let provider = account
                .credentials
                .get("provider")
                .and_then(|v| v.as_str())
                .or_else(|| account.extra.get("provider").and_then(|v| v.as_str()))
                .unwrap_or("");
            match crate::oauth::pi_provider_quota_backend(provider) {
                crate::oauth::PiQuotaBackend::Codex => fetch_codex_quota(account)?,
                crate::oauth::PiQuotaBackend::Grok => fetch_grok_quota(account)?,
                crate::oauth::PiQuotaBackend::None => return Ok(false),
            }
        }
        _ => return Ok(false),
    };

    if snap.is_empty() {
        if force {
            return Err(AppError::message(
                "account.quota",
                "quota probe returned no usage windows",
            ));
        }
        return Ok(false);
    }
    Ok(apply_quota_snapshot(account, &snap, now))
}

/// Soft variant for list paths — never returns Err.
pub fn try_refresh_account_quota(account: &mut Account, force: bool) -> bool {
    match refresh_account_quota(account, force) {
        Ok(changed) => changed,
        Err(e) => {
            tracing::debug!(
                module = targets::ACCOUNT,
                account_id = %account.id,
                agent = account.agent_id.as_str(),
                error = %e,
                "account quota probe skipped/failed"
            );
            false
        }
    }
}

pub fn apply_quota_snapshot(
    account: &mut Account,
    snap: &QuotaSnapshot,
    now: DateTime<Utc>,
) -> bool {
    if !account.extra.is_object() {
        account.extra = json!({});
    }
    let Some(obj) = account.extra.as_object_mut() else {
        return false;
    };
    let before = obj.clone();

    if let Some(p) = snap.quota5h_pct {
        obj.insert("quota5hPct".into(), json!(clamp_pct(p)));
        if let Some(at) = snap.reset_5h_at {
            obj.insert("quota5hResetAt".into(), json!(at.to_rfc3339()));
            let after = (at - now).num_seconds().max(0);
            obj.insert("codex_5h_reset_after_seconds".into(), json!(after));
            obj.insert("quota5hResetAfterSec".into(), json!(after));
        }
        if let Some(label) = snap.reset_in_label(now) {
            obj.insert("quotaResetIn".into(), json!(label));
        }
    } else {
        // Official snapshot omitted 5h — hide the bar until a later probe returns it.
        clear_codex_5h_quota_fields(obj);
    }
    if let Some(p) = snap.quota7d_pct {
        obj.insert("quota7dPct".into(), json!(clamp_pct(p)));
        if let Some(at) = snap.reset_7d_at {
            obj.insert("quota7dResetAt".into(), json!(at.to_rfc3339()));
            let after = (at - now).num_seconds().max(0);
            obj.insert("codex_7d_reset_after_seconds".into(), json!(after));
        }
        if let Some(label) = snap.reset_in_label_7d(now) {
            obj.insert("quota7dResetIn".into(), json!(label));
        }
    } else {
        for key in [
            "quota7dPct",
            "quota7dResetAt",
            "codex_7d_reset_after_seconds",
            "quota7dResetIn",
        ] {
            obj.remove(key);
        }
    }
    if let (Some(used), Some(limit)) = (snap.credit_used, snap.credit_limit) {
        obj.insert("creditUsed".into(), json!(used));
        obj.insert("creditLimit".into(), json!(limit));
        if let Some(at) = snap.credit_reset_at {
            obj.insert("creditResetAt".into(), json!(at.to_rfc3339()));
        } else {
            obj.remove("creditResetAt");
        }
    } else {
        for key in ["creditUsed", "creditLimit", "creditResetAt"] {
            obj.remove(key);
        }
    }
    obj.insert("codex_usage_updated_at".into(), json!(now.to_rfc3339()));
    if let Some(ref plan) = snap.plan_type {
        if obj
            .get("subscription")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .is_none()
        {
            obj.insert("subscription".into(), json!(plan));
        }
    }
    obj.insert("quotaUpdatedAt".into(), json!(now.to_rfc3339()));
    obj.insert("quotaSource".into(), json!(snap.source));

    *obj != before
}

// ── Fetchers ────────────────────────────────────────────────────────────────

fn fetch_codex_quota(account: &Account) -> Result<QuotaSnapshot> {
    let access = extract_access_token(account).ok_or_else(|| {
        AppError::message("account.quota", "no access_token for Codex quota probe")
    })?;
    let account_id = extract_chatgpt_account_id(account).ok_or_else(|| {
        AppError::message(
            "account.quota",
            "no chatgpt_account_id for Codex quota probe",
        )
    })?;
    let now = Utc::now();

    // 1) Preferred: Codex /responses probe → x-codex-* headers.
    match probe_codex_rate_limit_headers(&access, &account_id, now) {
        Ok(snap) if !snap.is_empty() => return Ok(snap),
        Ok(_) => {
            tracing::debug!(
                module = targets::ACCOUNT,
                "codex responses probe returned no x-codex headers; falling back to /wham/usage"
            );
        }
        Err(e) => {
            tracing::debug!(
                module = targets::ACCOUNT,
                error = %e,
                "codex responses probe failed; falling back to /wham/usage"
            );
        }
    }

    // 2) Fallback: ChatGPT /wham/usage body (same Normalize as headers).
    let body = http_get_json(
        CHATGPT_USAGE_URL,
        &[
            ("Authorization", &format!("Bearer {access}")),
            ("chatgpt-account-id", &account_id),
            ("openai-beta", "codex-1"),
            ("originator", CHATGPT_WHAM_ORIGINATOR),
            ("oai-language", "zh-CN"),
            ("Accept", "application/json"),
            ("sec-fetch-site", "none"),
            ("sec-fetch-mode", "no-cors"),
            ("sec-fetch-dest", "empty"),
            ("priority", "u=4, i"),
        ],
    )?;
    Ok(parse_openai_wham_usage(&body, now))
}

/// Payload for the Codex `/responses` usage probe (sub2api `createOpenAITestPayload`).
fn codex_responses_probe_payload() -> Value {
    json!({
        "model": CODEX_PROBE_MODEL,
        "input": [{
            "role": "user",
            "content": [{ "type": "input_text", "text": "hi" }]
        }],
        "stream": true,
        "store": false,
        "instructions": "You are a helpful assistant."
    })
}

/// Minimal Responses probe; rate-limit lives in response headers even on errors.
fn probe_codex_rate_limit_headers(
    access: &str,
    account_id: &str,
    now: DateTime<Utc>,
) -> Result<QuotaSnapshot> {
    let payload = codex_responses_probe_payload();
    let ua = format!("{CODEX_PROBE_ORIGINATOR}/{CODEX_PROBE_VERSION}");
    let mut req = ureq::post(CHATGPT_CODEX_RESPONSES_URL)
        .set("Authorization", &format!("Bearer {access}"))
        .set("chatgpt-account-id", account_id)
        .set("Content-Type", "application/json")
        .set("Accept", "text/event-stream")
        .set("OpenAI-Beta", "responses=experimental")
        .set("Originator", CODEX_PROBE_ORIGINATOR)
        .set("Version", CODEX_PROBE_VERSION)
        .set("User-Agent", &ua)
        .set("Host", "chatgpt.com");
    req = req.timeout(ACCOUNT_QUOTA_HTTP_TIMEOUT);

    let resp = match req.send_json(payload) {
        Ok(r) => r,
        Err(ureq::Error::Status(_, r)) => r, // 4xx/5xx may still carry x-codex-* headers
        Err(e) => {
            return Err(AppError::message(
                "account.quota",
                format!("codex responses probe failed: {e}"),
            ));
        }
    };

    let headers = extract_codex_headers_from_ureq(&resp);
    // Quota is in headers. Drop the SSE body so list()/refresh does not wait
    // on a streamed completion (and does not burn extra tokens).
    drop(resp);

    let Some(raw) = parse_codex_header_snapshot(&headers) else {
        return Ok(QuotaSnapshot {
            source: "codex_responses_headers",
            ..Default::default()
        });
    };
    Ok(normalize_codex_snapshot_to_quota(
        &raw,
        now,
        "codex_responses_headers",
    ))
}

fn extract_codex_headers_from_ureq(
    resp: &ureq::Response,
) -> std::collections::HashMap<String, String> {
    let mut m = std::collections::HashMap::new();
    for key in [
        "x-codex-primary-used-percent",
        "x-codex-primary-reset-after-seconds",
        "x-codex-primary-window-minutes",
        "x-codex-secondary-used-percent",
        "x-codex-secondary-reset-after-seconds",
        "x-codex-secondary-window-minutes",
        "x-codex-primary-over-secondary-limit-percent",
    ] {
        if let Some(v) = resp.header(key) {
            m.insert(key.to_string(), v.to_string());
        }
    }
    m
}

fn fetch_kiro_quota(account: &Account) -> Result<QuotaSnapshot> {
    let access = extract_access_token(account).ok_or_else(|| {
        AppError::message("account.quota", "no access_token for Kiro usage probe")
    })?;
    let region = kiro_region(account);
    let body = crate::adapters::kiro::http::get_usage_limits(&access, &region)?;
    let snap = parse_kiro_usage_limits(&body, Utc::now());
    if snap.is_empty() {
        return Err(AppError::message(
            "account.quota",
            "Kiro GetUsageLimits returned no credit window",
        ));
    }
    Ok(snap)
}

fn kiro_region(account: &Account) -> String {
    account
        .credentials
        .get("region")
        .or_else(|| account.extra.get("region"))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("us-east-1")
        .to_string()
}

/// Map `AmazonCodeWhispererService.GetUsageLimits` → credit used/limit.
///
/// Verified 2026-09 on Builder ID / KIRO FREE: `usageBreakdownList` CREDIT row
/// with `currentUsageWithPrecision` / `usageLimitWithPrecision`, plus
/// `subscriptionInfo.subscriptionTitle` and `nextDateReset` unix seconds.
pub fn parse_kiro_usage_limits(body: &Value, _now: DateTime<Utc>) -> QuotaSnapshot {
    let mut snap = QuotaSnapshot {
        source: "kiro_get_usage_limits",
        ..Default::default()
    };
    if let Some(title) = body
        .pointer("/subscriptionInfo/subscriptionTitle")
        .or_else(|| body.pointer("/subscription_info/subscription_title"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        snap.plan_type = Some(title.to_string());
    }
    let rows = body
        .get("usageBreakdownList")
        .or_else(|| body.get("usage_breakdown_list"))
        .and_then(Value::as_array);
    let credit = rows.and_then(|arr| {
        arr.iter()
            .find(|row| {
                row.get("resourceType")
                    .or_else(|| row.get("resource_type"))
                    .and_then(Value::as_str)
                    .is_some_and(|s| s.eq_ignore_ascii_case("CREDIT"))
            })
            .or_else(|| arr.first())
    });
    if let Some(row) = credit {
        let used = number_as_f64(
            row.get("currentUsageWithPrecision")
                .or_else(|| row.get("current_usage_with_precision"))
                .or_else(|| row.get("currentUsage"))
                .or_else(|| row.get("current_usage")),
        );
        let limit = number_as_f64(
            row.get("usageLimitWithPrecision")
                .or_else(|| row.get("usage_limit_with_precision"))
                .or_else(|| row.get("usageLimit"))
                .or_else(|| row.get("usage_limit")),
        );
        if let (Some(used), Some(limit)) = (used, limit) {
            if limit > 0.0 {
                snap.credit_used = Some(used.max(0.0));
                snap.credit_limit = Some(limit);
            }
        }
        let reset = unix_ts(
            row.get("nextDateReset")
                .or_else(|| row.get("next_date_reset")),
        )
        .or_else(|| {
            unix_ts(
                body.get("nextDateReset")
                    .or_else(|| body.get("next_date_reset")),
            )
        });
        snap.credit_reset_at = reset;
    }
    if snap.credit_reset_at.is_none() {
        snap.credit_reset_at = unix_ts(
            body.get("nextDateReset")
                .or_else(|| body.get("next_date_reset")),
        );
    }
    snap
}

fn unix_ts(v: Option<&Value>) -> Option<DateTime<Utc>> {
    let n = number_as_f64(v)?;
    if n <= 0.0 {
        return None;
    }
    let secs = if n >= 100_000_000_000.0 {
        n / 1000.0
    } else {
        n
    };
    DateTime::from_timestamp(secs as i64, 0)
}

fn fetch_claude_quota(account: &Account) -> Result<QuotaSnapshot> {
    let access = extract_access_token(account).ok_or_else(|| {
        AppError::message("account.quota", "no access_token for Claude quota probe")
    })?;
    let body = http_get_json(
        CLAUDE_USAGE_URL,
        &[
            ("Authorization", &format!("Bearer {access}")),
            ("Accept", "application/json"),
            ("anthropic-beta", "oauth-2025-04-20"),
        ],
    )?;
    Ok(parse_claude_oauth_usage(&body, Utc::now()))
}

// ── Grok / xAI billing ──────────────────────────────────────────────────────
// Weekly:  GET https://cli-chat-proxy.grok.com/v1/billing?format=credits
// Monthly: GET https://cli-chat-proxy.grok.com/v1/billing
// Does not consume model tokens (list-safe).

fn fetch_grok_quota(account: &Account) -> Result<QuotaSnapshot> {
    let access = extract_access_token(account).ok_or_else(|| {
        AppError::message(
            "account.quota",
            "no access token/key for Grok billing probe",
        )
    })?;
    let now = Utc::now();

    let weekly = http_get_json_grok_billing(
        &format!("{GROK_CLI_PROXY_BASE_URL}/billing?format=credits"),
        &access,
    );
    let monthly =
        http_get_json_grok_billing(&format!("{GROK_CLI_PROXY_BASE_URL}/billing"), &access);

    let weekly_body = weekly.ok();
    let monthly_body = monthly.ok();
    if weekly_body.is_none() && monthly_body.is_none() {
        return Err(AppError::message(
            "account.quota",
            "Grok billing weekly and monthly probes both failed",
        ));
    }

    let snap = parse_grok_billing(weekly_body.as_ref(), monthly_body.as_ref(), now);
    if snap.is_empty() && snap.plan_type.is_none() {
        return Err(AppError::message(
            "account.quota",
            "Grok billing returned no usage percent / plan",
        ));
    }
    Ok(snap)
}

fn http_get_json_grok_billing(url: &str, access: &str) -> Result<Value> {
    let mut req = ureq::get(url)
        .set("Authorization", &format!("Bearer {access}"))
        .set("Accept", "application/json")
        .set("Content-Type", "application/json");
    for (name, value) in grok_cli_identity_header_pairs() {
        req = req.set(name, &value);
    }
    req = req.timeout(ACCOUNT_QUOTA_HTTP_TIMEOUT);
    let resp = req.call().map_err(|e| {
        AppError::message("account.quota", format!("Grok billing request failed: {e}"))
    })?;
    let status = resp.status();
    let body: Value = resp.into_json().map_err(|e| {
        AppError::message("account.quota", format!("invalid Grok billing JSON: {e}"))
    })?;
    if !(200..300).contains(&status) {
        let msg = body
            .get("error")
            .and_then(|v| v.as_str())
            .or_else(|| body.pointer("/error/message").and_then(|v| v.as_str()))
            .unwrap_or("upstream rejected");
        return Err(AppError::message(
            "account.quota",
            format!("Grok billing {msg} (HTTP {status})"),
        ));
    }
    Ok(body)
}

/// Map xAI billing weekly/monthly payloads → UI 7d (weekly) / optional monthly %.
///
/// AgentHub only has 5h/7d bars: weekly credit usage → **7d**; monthly used% is
/// stored as plan context and used for 7d only when weekly is missing.
pub fn parse_grok_billing(
    weekly: Option<&Value>,
    monthly: Option<&Value>,
    now: DateTime<Utc>,
) -> QuotaSnapshot {
    let mut snap = QuotaSnapshot {
        source: "grok_billing",
        ..Default::default()
    };

    if let Some(w) = weekly {
        let cfg = w.get("config").unwrap_or(w);
        snap.quota7d_pct = grok_usage_percent(cfg);
        if let Some(end) = grok_period_end(cfg) {
            let rem = clamp_reset_after((end - now).num_seconds(), 7 * 24 * 3600);
            snap.reset_7d_at = Some(now + ChronoDuration::seconds(rem));
        }
    }

    if let Some(m) = monthly {
        let cfg = m.get("config").unwrap_or(m);
        let limit = grok_monthly_limit(cfg);
        let used = grok_monthly_used(cfg);
        if let (Some(lim), Some(u)) = (limit, used) {
            if lim > 0.0 && snap.quota7d_pct.is_none() {
                snap.quota7d_pct = Some((u / lim) * 100.0);
            }
            snap.plan_type = resolve_grok_plan(lim);
        }
        if snap.reset_7d_at.is_none() {
            if let Some(end) = grok_period_end(cfg) {
                let rem = clamp_reset_after((end - now).num_seconds(), 31 * 24 * 3600);
                if snap.quota7d_pct.is_some() {
                    snap.reset_7d_at = Some(now + ChronoDuration::seconds(rem.min(7 * 24 * 3600)));
                }
            }
        }
        if snap.plan_type.is_none() {
            if let Some(lim) = limit {
                snap.plan_type = resolve_grok_plan(lim);
            }
        }
    }

    // Unified billing / SuperGrok Heavy often publishes a period and no percent.
    // Keep the 7d bar visible (0%) instead of treating the snapshot as empty.
    if snap.quota7d_pct.is_none() {
        let end = weekly
            .and_then(|w| grok_period_end(w.get("config").unwrap_or(w)))
            .or_else(|| monthly.and_then(|m| grok_period_end(m.get("config").unwrap_or(m))));
        if let Some(end) = end {
            snap.quota7d_pct = Some(0.0);
            if snap.reset_7d_at.is_none() {
                let rem = clamp_reset_after((end - now).num_seconds(), 7 * 24 * 3600);
                snap.reset_7d_at = Some(now + ChronoDuration::seconds(rem));
            }
        }
    }

    snap
}

fn grok_usage_percent(cfg: &Value) -> Option<f64> {
    if let Some(p) = number_as_f64(
        cfg.get("creditUsagePercent")
            .or_else(|| cfg.get("credit_usage_percent")),
    ) {
        return Some(p);
    }
    if let Some(arr) = cfg.get("productUsage").and_then(|v| v.as_array()) {
        for item in arr {
            if let Some(p) = number_as_f64(
                item.get("usagePercent")
                    .or_else(|| item.get("usage_percent")),
            ) {
                return Some(p);
            }
        }
    }
    let cap = cent_value(cfg.get("onDemandCap").or_else(|| cfg.get("on_demand_cap")));
    let used = cent_value(
        cfg.get("onDemandUsed")
            .or_else(|| cfg.get("on_demand_used")),
    );
    match (cap, used) {
        (Some(lim), Some(u)) if lim > 0.0 => Some((u / lim) * 100.0),
        _ => None,
    }
}

fn grok_period_end(cfg: &Value) -> Option<DateTime<Utc>> {
    cfg.get("currentPeriod")
        .or_else(|| cfg.get("current_period"))
        .and_then(|period| {
            period
                .get("end")
                .and_then(|v| v.as_str())
                .and_then(parse_rfc3339)
        })
        .or_else(|| {
            cfg.get("billingPeriodEnd")
                .or_else(|| cfg.get("billing_period_end"))
                .and_then(|v| v.as_str())
                .and_then(parse_rfc3339)
        })
        .or_else(|| {
            cfg.pointer("/billingCycle/billingPeriodEnd")
                .and_then(|v| v.as_str())
                .and_then(parse_rfc3339)
        })
}

fn grok_monthly_limit(cfg: &Value) -> Option<f64> {
    cent_value(cfg.get("monthlyLimit").or_else(|| cfg.get("monthly_limit")))
}

fn grok_monthly_used(cfg: &Value) -> Option<f64> {
    cent_value(cfg.get("used"))
        .or_else(|| cent_value(cfg.pointer("/usage/totalUsed")))
        .or_else(|| cent_value(cfg.pointer("/usage/includedUsed")))
        .or_else(|| cent_value(cfg.get("totalUsed")))
}

fn cent_value(v: Option<&Value>) -> Option<f64> {
    let v = v?;
    if let Some(n) = v.as_f64() {
        return Some(n);
    }
    if let Some(n) = v.as_i64() {
        return Some(n as f64);
    }
    // { "val": 15000 }
    if let Some(n) = v.get("val").and_then(|x| x.as_f64()) {
        return Some(n);
    }
    if let Some(n) = v.get("val").and_then(|x| x.as_i64()) {
        return Some(n as f64);
    }
    if let Some(s) = v.as_str() {
        return s.trim().parse().ok();
    }
    None
}

fn resolve_grok_plan(monthly_limit_cents: f64) -> Option<String> {
    // Known Grok monthly credit limits (cents).
    if (monthly_limit_cents - 150_000.0).abs() < 1.0 {
        return Some("SuperGrok Heavy".into());
    }
    if (monthly_limit_cents - 15_000.0).abs() < 1.0 {
        return Some("SuperGrok".into());
    }
    if monthly_limit_cents > 0.0 {
        return Some(format!("plan ${:.0}", monthly_limit_cents / 100.0));
    }
    None
}

fn http_get_json(url: &str, headers: &[(&str, &str)]) -> Result<Value> {
    let mut req = ureq::get(url);
    for (k, v) in headers {
        req = req.set(k, v);
    }
    req = req.timeout(ACCOUNT_QUOTA_HTTP_TIMEOUT);
    let resp = req
        .call()
        .map_err(|e| AppError::message("account.quota", format!("quota request failed: {e}")))?;
    let status = resp.status();
    let body: Value = resp
        .into_json()
        .map_err(|e| AppError::message("account.quota", format!("invalid quota JSON: {e}")))?;
    if !(200..300).contains(&status) {
        let msg = body
            .get("error")
            .and_then(|v| v.as_str())
            .or_else(|| body.get("detail").and_then(|v| v.as_str()))
            .unwrap_or("upstream rejected");
        return Err(AppError::message(
            "account.quota",
            format!("{msg} (HTTP {status})"),
        ));
    }
    Ok(body)
}

// ── Parsers (unit-tested without network) ───────────────────────────────────

/// Raw primary/secondary fields (Codex header or /wham rate_limit shape).
/// Normalization is always by window size — never by the word "primary".
#[derive(Debug, Clone, Default)]
struct CodexRawSnapshot {
    primary_used: Option<f64>,
    primary_reset_after: Option<i64>,
    primary_window_mins: Option<i64>,
    secondary_used: Option<f64>,
    secondary_reset_after: Option<i64>,
    secondary_window_mins: Option<i64>,
}

#[derive(Debug, Clone, Default)]
struct CodexNormalized {
    used_5h: Option<f64>,
    reset_after_5h: Option<i64>,
    window_mins_5h: Option<i64>,
    used_7d: Option<f64>,
    reset_after_7d: Option<i64>,
    window_mins_7d: Option<i64>,
}

/// Classify Codex windows by duration: smaller → 5h, larger → 7d;
/// if only one window exists, fall back to primary=7d / secondary=5h.
fn normalize_codex_windows(raw: &CodexRawSnapshot) -> CodexNormalized {
    let mut out = CodexNormalized::default();
    let has_p = raw.primary_window_mins.is_some();
    let has_s = raw.secondary_window_mins.is_some();
    let p_mins = raw.primary_window_mins.unwrap_or(0);
    let s_mins = raw.secondary_window_mins.unwrap_or(0);

    // Prefer the shorter declared window as 5h, the longer as 7d.
    let (use_5h_from_primary, use_7d_from_primary) = if has_p && has_s {
        if p_mins < s_mins {
            (true, false)
        } else {
            (false, true)
        }
    } else if has_p {
        if p_mins <= 360 {
            (true, false)
        } else {
            (false, true)
        }
    } else if has_s {
        if s_mins <= 360 {
            // 5h is secondary → primary data (if any) is 7d
            (false, true)
        } else {
            (true, false)
        }
    } else {
        // No window lengths: legacy Codex headers assume primary=7d, secondary=5h.
        (false, true)
    };

    if use_5h_from_primary {
        out.used_5h = raw.primary_used;
        out.reset_after_5h = raw.primary_reset_after;
        out.window_mins_5h = raw.primary_window_mins;
        out.used_7d = raw.secondary_used;
        out.reset_after_7d = raw.secondary_reset_after;
        out.window_mins_7d = raw.secondary_window_mins;
    } else if use_7d_from_primary {
        out.used_7d = raw.primary_used;
        out.reset_after_7d = raw.primary_reset_after;
        out.window_mins_7d = raw.primary_window_mins;
        out.used_5h = raw.secondary_used;
        out.reset_after_5h = raw.secondary_reset_after;
        out.window_mins_5h = raw.secondary_window_mins;
    }
    out
}

fn normalize_codex_snapshot_to_quota(
    raw: &CodexRawSnapshot,
    now: DateTime<Utc>,
    source: &'static str,
) -> QuotaSnapshot {
    let n = normalize_codex_windows(raw);
    let mut snap = QuotaSnapshot {
        source,
        quota5h_pct: n.used_5h,
        quota7d_pct: n.used_7d,
        ..Default::default()
    };

    // reset_after is relative to probe time. Cap to window length so a 7d bar
    // can never show "9d remaining" (common when mixing absolute reset_at /
    // wrong field). Default caps: 5h / 7d.
    if let Some(after) = n.reset_after_5h {
        let cap = window_cap_secs(n.window_mins_5h, 5 * 3600);
        let after = clamp_reset_after(after, cap);
        snap.reset_5h_at = Some(now + ChronoDuration::seconds(after));
    }
    if let Some(after) = n.reset_after_7d {
        let cap = window_cap_secs(n.window_mins_7d, 7 * 24 * 3600);
        let after = clamp_reset_after(after, cap);
        snap.reset_7d_at = Some(now + ChronoDuration::seconds(after));
    }
    snap
}

fn window_cap_secs(window_mins: Option<i64>, default_secs: i64) -> i64 {
    window_mins
        .filter(|&m| m > 0)
        .map(|m| m.saturating_mul(60))
        .unwrap_or(default_secs)
}

/// Cap reset-after to the rolling window (+2 min clock skew). Never allow
/// "7d window, 9d remaining".
fn clamp_reset_after(after: i64, window_secs: i64) -> i64 {
    let cap = window_secs.max(0).saturating_add(120);
    after.max(0).min(cap)
}

fn parse_codex_header_snapshot(
    headers: &std::collections::HashMap<String, String>,
) -> Option<CodexRawSnapshot> {
    let get_f = |k: &str| headers.get(k).and_then(|s| s.trim().parse::<f64>().ok());
    let get_i = |k: &str| headers.get(k).and_then(|s| s.trim().parse::<i64>().ok());

    let raw = CodexRawSnapshot {
        primary_used: get_f("x-codex-primary-used-percent"),
        primary_reset_after: get_i("x-codex-primary-reset-after-seconds"),
        primary_window_mins: get_i("x-codex-primary-window-minutes"),
        secondary_used: get_f("x-codex-secondary-used-percent"),
        secondary_reset_after: get_i("x-codex-secondary-reset-after-seconds"),
        secondary_window_mins: get_i("x-codex-secondary-window-minutes"),
    };
    if raw.primary_used.is_none()
        && raw.secondary_used.is_none()
        && raw.primary_reset_after.is_none()
        && raw.secondary_reset_after.is_none()
        && raw.primary_window_mins.is_none()
        && raw.secondary_window_mins.is_none()
    {
        return None;
    }
    Some(raw)
}

/// Map ChatGPT `/wham/usage` rate_limit windows via the same Normalize as headers.
pub fn parse_openai_wham_usage(body: &Value, now: DateTime<Utc>) -> QuotaSnapshot {
    let plan = body
        .get("plan_type")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    let raw = rate_limit_json_to_codex_raw(pick_wham_rate_limit(body));
    let mut snap = normalize_codex_snapshot_to_quota(&raw, now, "chatgpt_wham_usage");
    snap.plan_type = plan;
    snap
}

fn rate_limit_has_windows(rate: &Value) -> bool {
    rate.get("primary_window").is_some_and(|v| v.is_object())
        || rate.get("secondary_window").is_some_and(|v| v.is_object())
}

/// Shared ChatGPT/Codex pool (`rate_limit`) wins. Extra Codex meters such as
/// Spark `codex_bengalfox` are used only when that pool has no windows —
/// otherwise a null additional meter used to wipe the real 5h/7d bars.
fn pick_wham_rate_limit(body: &Value) -> Option<&Value> {
    let top = body.get("rate_limit");
    if let Some(rate) = top.filter(|v| rate_limit_has_windows(v)) {
        return Some(rate);
    }
    let Some(arr) = body
        .get("additional_rate_limits")
        .and_then(|v| v.as_array())
    else {
        return top;
    };
    let mut first_codex: Option<&Value> = None;
    for item in arr {
        let feature = item
            .get("metered_feature")
            .or_else(|| item.get("limit_name"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let Some(rate) = item.get("rate_limit").filter(|v| rate_limit_has_windows(v)) else {
            continue;
        };
        if feature.eq_ignore_ascii_case("codex_bengalfox") {
            return Some(rate);
        }
        if first_codex.is_none() && feature.to_ascii_lowercase().contains("codex") {
            first_codex = Some(rate);
        }
    }
    first_codex.or(top)
}

fn rate_limit_json_to_codex_raw(rate: Option<&Value>) -> CodexRawSnapshot {
    let mut raw = CodexRawSnapshot::default();
    let Some(rate) = rate else {
        return raw;
    };
    if let Some(w) = rate.get("primary_window").filter(|v| !v.is_null()) {
        raw.primary_used = number_as_f64(w.get("used_percent"));
        raw.primary_reset_after = w.get("reset_after_seconds").and_then(|v| v.as_i64());
        raw.primary_window_mins = window_minutes_from_json(w);
        // Prefer reset_after; only fall back to absolute reset_at if after missing.
        if raw.primary_reset_after.is_none() {
            if let Some(at) = w
                .get("reset_at")
                .and_then(|v| v.as_i64())
                .and_then(parse_unix_timestamp)
            {
                let after = (at - Utc::now()).num_seconds();
                raw.primary_reset_after = Some(after.max(0));
            }
        }
    }
    if let Some(w) = rate.get("secondary_window").filter(|v| !v.is_null()) {
        raw.secondary_used = number_as_f64(w.get("used_percent"));
        raw.secondary_reset_after = w.get("reset_after_seconds").and_then(|v| v.as_i64());
        raw.secondary_window_mins = window_minutes_from_json(w);
        if raw.secondary_reset_after.is_none() {
            if let Some(at) = w
                .get("reset_at")
                .and_then(|v| v.as_i64())
                .and_then(parse_unix_timestamp)
            {
                let after = (at - Utc::now()).num_seconds();
                raw.secondary_reset_after = Some(after.max(0));
            }
        }
    }
    raw
}

fn window_minutes_from_json(w: &Value) -> Option<i64> {
    if let Some(m) = w.get("limit_window_minutes").and_then(|v| v.as_i64()) {
        if m > 0 {
            return Some(m);
        }
    }
    if let Some(s) = w.get("limit_window_seconds").and_then(|v| v.as_i64()) {
        if s > 0 {
            return Some((s + 59) / 60); // ceil seconds to minutes
        }
    }
    None
}

/// Unix seconds or milliseconds → UTC DateTime.
fn parse_unix_timestamp(n: i64) -> Option<DateTime<Utc>> {
    if n <= 0 {
        return None;
    }
    DateTime::from_timestamp(crate::utils::expiry::normalize_epoch_secs(n), 0)
}

fn parse_rfc3339(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s.trim())
        .ok()
        .map(|d| d.with_timezone(&Utc))
}

/// Map Claude `/api/oauth/usage` → 5h / 7d.
pub fn parse_claude_oauth_usage(body: &Value, now: DateTime<Utc>) -> QuotaSnapshot {
    let mut snap = QuotaSnapshot {
        source: "claude_oauth_usage",
        ..Default::default()
    };

    if let Some(w) = body.get("five_hour").or_else(|| body.get("fiveHour")) {
        snap.quota5h_pct = number_as_f64(
            w.get("utilization")
                .or_else(|| w.get("used_percent"))
                .or_else(|| w.get("usedPercent")),
        );
        snap.reset_5h_at = parse_reset_field(w, now);
    }
    if let Some(w) = body.get("seven_day").or_else(|| body.get("sevenDay")) {
        snap.quota7d_pct = number_as_f64(
            w.get("utilization")
                .or_else(|| w.get("used_percent"))
                .or_else(|| w.get("usedPercent")),
        );
        snap.reset_7d_at = parse_reset_field(w, now);
    }
    snap
}

fn parse_reset_field(w: &Value, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
    if let Some(s) = w
        .get("resets_at")
        .or_else(|| w.get("resetsAt"))
        .and_then(|v| v.as_str())
    {
        return DateTime::parse_from_rfc3339(s)
            .ok()
            .map(|d| d.with_timezone(&Utc));
    }
    if let Some(secs) = w
        .get("resets_in_seconds")
        .or_else(|| w.get("reset_after_seconds"))
        .and_then(|v| v.as_i64())
    {
        return Some(now + ChronoDuration::seconds(secs.max(0)));
    }
    None
}

// ── Credential helpers ──────────────────────────────────────────────────────

pub(crate) fn extract_access_token(account: &Account) -> Option<String> {
    let c = &account.credentials;
    c.get("access_token")
        .or_else(|| c.get("access"))
        .or_else(|| c.get("key"))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .or_else(|| {
            c.pointer("/body/tokens/access_token")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
        })
        .or_else(|| {
            c.pointer("/body/key")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
        })
        .or_else(|| {
            // Claude credentials_json
            c.pointer("/body/claudeAiOauth/accessToken")
                .or_else(|| c.pointer("/body/claude.ai_oauth/accessToken"))
                .or_else(|| c.pointer("/body/claudeAiOauth/access_token"))
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
        })
        .or_else(|| {
            // Pi multi-provider body
            let p = c.get("provider").and_then(|v| v.as_str())?;
            c.pointer(&format!("/body/{p}/access"))
                .or_else(|| c.pointer(&format!("/body/{p}/access_token")))
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
        })
        .or_else(|| extract_grok_profile_token(c))
}

/// Grok auth.json: `{ "https://auth.x.ai::clientId": { "key": "...", "refresh_token": "..." } }`
fn extract_grok_profile_token(credentials: &Value) -> Option<String> {
    let body = credentials.get("body").unwrap_or(credentials);
    let obj = body.as_object()?;
    for (k, entry) in obj {
        if !entry.is_object() {
            continue;
        }
        let looks = k.contains("auth.x.ai")
            || k == "xai"
            || entry.get("refresh_token").is_some()
            || entry.get("key").is_some();
        if !looks {
            continue;
        }
        if let Some(t) = entry
            .get("key")
            .or_else(|| entry.get("access"))
            .or_else(|| entry.get("access_token"))
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            return Some(t.to_string());
        }
    }
    None
}

pub(crate) fn extract_chatgpt_account_id(account: &Account) -> Option<String> {
    let c = &account.credentials;
    let extra = &account.extra;
    let provider = c
        .get("provider")
        .and_then(|v| v.as_str())
        .or_else(|| extra.get("provider").and_then(|v| v.as_str()));

    let mut candidates: Vec<Option<&Value>> = vec![
        c.get("account_id"),
        c.get("chatgpt_account_id"),
        extra.get("accountId"),
        extra.get("account_id"),
        extra.get("chatgpt_account_id"),
        c.pointer("/body/tokens/account_id"),
        c.pointer("/body/account/id"),
    ];
    if let Some(p) = provider {
        candidates.push(c.pointer(&format!("/body/{p}/account_id")));
        candidates.push(c.pointer(&format!("/body/{p}/chatgpt_account_id")));
        candidates.push(c.pointer(&format!("/body/{p}/accountId")));
    }
    if let Some(id) = candidates
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str())
        .map(str::trim)
        .find(|s| !s.is_empty())
    {
        return Some(id.to_string());
    }

    let mut tokens: Vec<Option<&Value>> = vec![
        c.get("id_token"),
        c.pointer("/body/tokens/id_token"),
        c.get("access_token"),
        c.get("access"),
        c.pointer("/body/tokens/access_token"),
    ];
    if let Some(p) = provider {
        for key in ["id_token", "idToken", "access", "access_token"] {
            tokens.push(c.pointer(&format!("/body/{p}/{key}")));
        }
    }
    tokens
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .find_map(chatgpt_account_id_from_token)
}

// ── Token expiry heal (local, no network) ───────────────────────────────────

/// Derive `expires_at` / `extra.expiresAt` / `tokenExpired` from credentials.
/// Safe for list(); returns true when account was modified.
pub fn heal_token_expiry(account: &mut Account) -> bool {
    if account.kind != AccountKind::Oauth {
        return false;
    }
    if !account.extra.is_object() {
        account.extra = json!({});
    }

    let before_creds = account.credentials.clone();
    let before_extra = account.extra.clone();

    // Ensure top-level tokens for agents with nested body shapes.
    if let Some(obj) = account.credentials.as_object_mut() {
        promote_nested_tokens(account.agent_id, obj);
    }

    let exp = resolve_expires_at(account.agent_id, &account.credentials);
    if let Some(ref rfc) = exp {
        // Always overwrite — prior heals may have stored id_token exp (~1h) for Codex.
        if let Some(obj) = account.credentials.as_object_mut() {
            obj.insert("expires_at".into(), json!(rfc));
        }
        if let Some(obj) = account.extra.as_object_mut() {
            obj.insert("expiresAt".into(), json!(rfc));
            obj.insert("tokenExpired".into(), json!(is_rfc3339_past(rfc)));
        }
    }

    account.credentials != before_creds || account.extra != before_extra
}

fn promote_nested_tokens(agent: AgentId, obj: &mut Map<String, Value>) {
    // Use a temporary Value view so we can JSON-pointer into nested body shapes.
    let view = Value::Object(obj.clone());
    if obj.get("access_token").and_then(|v| v.as_str()).is_none() {
        if let Some(a) = view
            .pointer("/body/tokens/access_token")
            .and_then(|v| v.as_str())
        {
            obj.insert("access_token".into(), json!(a));
        } else if let Some(a) = view
            .pointer("/body/claudeAiOauth/accessToken")
            .or_else(|| view.pointer("/body/claude.ai_oauth/accessToken"))
            .and_then(|v| v.as_str())
        {
            obj.insert("access_token".into(), json!(a));
        }
    }
    if obj.get("id_token").and_then(|v| v.as_str()).is_none() {
        if let Some(i) = view
            .pointer("/body/tokens/id_token")
            .and_then(|v| v.as_str())
        {
            obj.insert("id_token".into(), json!(i));
        }
    }
    if obj.get("refresh_token").and_then(|v| v.as_str()).is_none() {
        if let Some(r) = view
            .pointer("/body/tokens/refresh_token")
            .or_else(|| view.pointer("/body/claudeAiOauth/refreshToken"))
            .and_then(|v| v.as_str())
        {
            obj.insert("refresh_token".into(), json!(r));
        }
    }
    if agent == AgentId::Claude && obj.get("expires_at").is_none() {
        if let Some(exp) = view
            .pointer("/body/claudeAiOauth/expiresAt")
            .or_else(|| view.pointer("/body/claude.ai_oauth/expiresAt"))
            .cloned()
        {
            if let Some(rfc) = normalize_expires_value(&exp) {
                obj.insert("expires_at".into(), json!(rfc));
            }
        }
    }
    // Grok: promote profile key + expires_at from auth.x.ai entry.
    if agent == AgentId::Grok {
        if let Some(body) = view.get("body").and_then(|b| b.as_object()) {
            for (k, entry) in body {
                if !entry.is_object() {
                    continue;
                }
                if !(k.contains("auth.x.ai") || entry.get("key").is_some()) {
                    continue;
                }
                if obj.get("access_token").and_then(|v| v.as_str()).is_none() {
                    if let Some(a) = entry
                        .get("key")
                        .or_else(|| entry.get("access"))
                        .and_then(|v| v.as_str())
                    {
                        obj.insert("access_token".into(), json!(a));
                    }
                }
                if obj.get("refresh_token").and_then(|v| v.as_str()).is_none() {
                    if let Some(r) = entry.get("refresh_token").and_then(|v| v.as_str()) {
                        obj.insert("refresh_token".into(), json!(r));
                    }
                }
                if obj.get("expires_at").is_none() {
                    if let Some(exp) = entry.get("expires_at").and_then(|v| v.as_str()) {
                        if let Some(rfc) = normalize_expires_str(exp) {
                            obj.insert("expires_at".into(), json!(rfc));
                        }
                    }
                }
                break;
            }
        }
    }
}

fn resolve_expires_at(agent: AgentId, credentials: &Value) -> Option<String> {
    // Codex / OpenAI: access_token JWT exp is the API credential lifetime (~days).
    // id_token exp is only ~1h OIDC identity — MUST NOT drive "token remaining".
    // Always prefer access JWT when present (also overwrites stale stored expires_at).
    let access = credentials
        .get("access_token")
        .and_then(|v| v.as_str())
        .or_else(|| {
            credentials
                .pointer("/body/tokens/access_token")
                .and_then(|v| v.as_str())
        });
    if matches!(agent, AgentId::Codex | AgentId::Pi | AgentId::Grok) {
        if let Some(at) = access {
            if let Some(rfc) = jwt_exp_rfc3339(at) {
                return Some(rfc);
            }
        }
    } else if let Some(at) = access {
        if let Some(rfc) = jwt_exp_rfc3339(at) {
            return Some(rfc);
        }
    }

    if let Some(s) = credentials.get("expires_at").and_then(|v| v.as_str()) {
        if let Some(n) = normalize_expires_str(s) {
            return Some(n);
        }
    }
    if let Some(s) = credentials
        .pointer("/body/expires_at")
        .and_then(|v| v.as_str())
    {
        if let Some(n) = normalize_expires_str(s) {
            return Some(n);
        }
    }
    if let Some(v) = credentials.get("expires_at") {
        if let Some(n) = normalize_expires_value(v) {
            return Some(n);
        }
    }
    // Claude nested
    if let Some(v) = credentials
        .pointer("/body/claudeAiOauth/expiresAt")
        .or_else(|| credentials.pointer("/body/claude.ai_oauth/expiresAt"))
    {
        if let Some(n) = normalize_expires_value(v) {
            return Some(n);
        }
    }
    // Pi ms expires (when access is opaque / non-JWT)
    if let Some(p) = credentials.get("provider").and_then(|v| v.as_str()) {
        if let Some(ms) = credentials
            .pointer(&format!("/body/{p}/expires"))
            .and_then(|v| v.as_i64())
        {
            if let Some(dt) = DateTime::from_timestamp(ms / 1000, 0) {
                return Some(dt.to_rfc3339());
            }
        }
    }
    // Do NOT fall back to id_token exp — wrong semantics for Codex.
    None
}

fn jwt_exp_rfc3339(token: &str) -> Option<String> {
    let claims = decode_jwt_payload(token)?;
    let exp = claims.get("exp").and_then(|v| v.as_i64())?;
    DateTime::from_timestamp(exp, 0).map(|d| d.to_rfc3339())
}

fn normalize_expires_value(v: &Value) -> Option<String> {
    let secs = crate::utils::expiry::parse_expiry_epoch_secs(v)?;
    DateTime::from_timestamp(secs, 0).map(|d| d.to_rfc3339())
}

fn normalize_expires_str(s: &str) -> Option<String> {
    normalize_expires_value(&Value::String(s.to_string()))
}

fn is_rfc3339_past(s: &str) -> bool {
    match DateTime::parse_from_rfc3339(s) {
        Ok(dt) => dt.with_timezone(&Utc) <= Utc::now(),
        Err(_) => false,
    }
}

fn number_as_f64(v: Option<&Value>) -> Option<f64> {
    let v = v?;
    if let Some(n) = v.as_f64() {
        return Some(n);
    }
    if let Some(n) = v.as_i64() {
        return Some(n as f64);
    }
    if let Some(s) = v.as_str() {
        return s.trim().parse().ok();
    }
    None
}

fn clamp_pct(p: f64) -> i64 {
    if !p.is_finite() {
        return 0;
    }
    p.round().clamp(0.0, 100.0) as i64
}

fn format_reset_in(secs: i64) -> String {
    if secs <= 0 {
        return "即将重置".into();
    }
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    if h >= 24 {
        let d = h / 24;
        let rh = h % 24;
        return format!("{d}d{rh}h 后重置");
    }
    if h == 0 {
        return format!("{m}m 后重置");
    }
    format!("{h}h{m:02}m 后重置")
}

#[allow(dead_code)]
fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests;
