//! Upstream subscription quota windows (5h / 7d) for OAuth accounts.
//!
//! Codex (aligned with sub2api):
//! 1. Preferred: `POST https://chatgpt.com/backend-api/codex/responses` with
//!    `codex-auto-review` + `stream: true`; 5h/7d come from `x-codex-*` headers.
//! 2. Fallback: `GET https://chatgpt.com/backend-api/wham/usage` (Codex Desktop
//!    identity). Top-level `rate_limit` is the shared ChatGPT/Codex pool;
//!    `additional_rate_limits` (e.g. Spark `codex_bengalfox`) is used only when
//!    that pool has no windows.
//!
//! Claude OAuth: `GET https://api.anthropic.com/api/oauth/usage`.
//!
//! Results are written into `account.extra` for the existing UI fields
//! (`quota5hPct`, `quota7dPct`, `quotaResetIn`). List probes are best-effort;
//! an explicit Connections refresh surfaces probe failures.

use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use serde_json::{json, Map, Value};

use crate::bridge::grok_cli::{grok_cli_identity_header_pairs, GROK_CLI_PROXY_BASE_URL};
use crate::catalog::limits::{ACCOUNT_QUOTA_CACHE_TTL, ACCOUNT_QUOTA_HTTP_TIMEOUT};
use crate::error::{AppError, Result};
use crate::logging::targets;
use crate::models::{Account, AccountKind, AgentId};
use crate::oauth::decode_jwt_payload;

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
    pub plan_type: Option<String>,
    pub source: &'static str,
}

impl QuotaSnapshot {
    pub fn is_empty(&self) -> bool {
        self.quota5h_pct.is_none() && self.quota7d_pct.is_none()
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
                "quota probe returned no 5h/7d windows",
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
        // creditUsagePercent is 0–100 for the weekly credits window.
        if let Some(p) = number_as_f64(
            cfg.get("creditUsagePercent")
                .or_else(|| cfg.get("credit_usage_percent")),
        ) {
            snap.quota7d_pct = Some(p);
        }
        // productUsage[].usagePercent fallback (e.g. Api product).
        if snap.quota7d_pct.is_none() {
            if let Some(arr) = cfg.get("productUsage").and_then(|v| v.as_array()) {
                for item in arr {
                    if let Some(p) = number_as_f64(
                        item.get("usagePercent")
                            .or_else(|| item.get("usage_percent")),
                    ) {
                        snap.quota7d_pct = Some(p);
                        break;
                    }
                }
            }
        }
        if let Some(period) = cfg
            .get("currentPeriod")
            .or_else(|| cfg.get("current_period"))
        {
            if let Some(end) = period
                .get("end")
                .and_then(|v| v.as_str())
                .and_then(parse_rfc3339)
            {
                // Clamp remaining to 7d even if period is longer (billing windows vary).
                let rem = (end - now).num_seconds();
                let rem = clamp_reset_after(rem, 7 * 24 * 3600);
                snap.reset_7d_at = Some(now + ChronoDuration::seconds(rem));
            }
        }
    }

    if let Some(m) = monthly {
        let cfg = m.get("config").unwrap_or(m);
        let limit = cent_value(cfg.get("monthlyLimit").or_else(|| cfg.get("monthly_limit")));
        let used = cent_value(cfg.get("used"));
        if let (Some(lim), Some(u)) = (limit, used) {
            if lim > 0.0 {
                let pct = (u / lim) * 100.0;
                // Prefer weekly for 7d bar; monthly only if weekly missing.
                if snap.quota7d_pct.is_none() {
                    snap.quota7d_pct = Some(pct);
                }
            }
            // Infer plan name from the monthly credit limit.
            snap.plan_type = resolve_grok_plan(lim);
        }
        if snap.reset_7d_at.is_none() {
            if let Some(end) = cfg
                .get("billingPeriodEnd")
                .or_else(|| cfg.get("billing_period_end"))
                .and_then(|v| v.as_str())
                .and_then(parse_rfc3339)
            {
                let rem = clamp_reset_after((end - now).num_seconds(), 31 * 24 * 3600);
                // Still map to 7d bar reset label only when we used monthly %.
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

    snap
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
    rate.get("primary_window")
        .is_some_and(|v| v.is_object())
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
    let Some(arr) = body.get("additional_rate_limits").and_then(|v| v.as_array()) else {
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

fn extract_chatgpt_account_id(account: &Account) -> Option<String> {
    let c = &account.credentials;
    let extra = &account.extra;
    [
        c.get("account_id").and_then(|v| v.as_str()),
        c.get("chatgpt_account_id").and_then(|v| v.as_str()),
        extra.get("accountId").and_then(|v| v.as_str()),
        c.pointer("/body/tokens/account_id")
            .and_then(|v| v.as_str()),
        c.pointer("/body/account/id").and_then(|v| v.as_str()),
    ]
    .into_iter()
    .flatten()
    .map(str::trim)
    .find(|s| !s.is_empty())
    .map(|s| s.to_string())
    .or_else(|| {
        // From id_token JWT claims
        let tok = c
            .get("id_token")
            .or_else(|| c.pointer("/body/tokens/id_token"))
            .and_then(|v| v.as_str())?;
        let claims = decode_jwt_payload(tok)?;
        claims
            .pointer("/https:~1~1api.openai.com~1auth/chatgpt_account_id")
            .or_else(|| {
                claims
                    .get("https://api.openai.com/auth")
                    .and_then(|a| a.get("chatgpt_account_id"))
            })
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
    })
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
mod tests {
    use super::*;
    use crate::models::AccountKind;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;

    fn make_jwt(claims: Value) -> String {
        let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"none"}"#);
        let payload = URL_SAFE_NO_PAD.encode(claims.to_string().as_bytes());
        format!("{header}.{payload}.sig")
    }

    #[test]
    fn parse_wham_classifies_by_window_length_not_primary_name() {
        let now = Utc::now();
        // Real Codex/ChatGPT shape: primary = 7d, secondary = 5h.
        let body = json!({
            "plan_type": "prolite",
            "rate_limit": {
                "primary_window": {
                    "used_percent": 18.0,
                    "limit_window_seconds": 604800,
                    "reset_after_seconds": 86400
                },
                "secondary_window": {
                    "used_percent": 42.5,
                    "limit_window_seconds": 18000,
                    "reset_after_seconds": 7200
                }
            }
        });
        let snap = parse_openai_wham_usage(&body, now);
        assert_eq!(
            snap.quota5h_pct,
            Some(42.5),
            "5h must come from secondary (18000s)"
        );
        assert_eq!(
            snap.quota7d_pct,
            Some(18.0),
            "7d must come from primary (604800s)"
        );
        assert_eq!(snap.plan_type.as_deref(), Some("prolite"));
        let r5 = snap.reset_5h_at.expect("5h reset");
        let r7 = snap.reset_7d_at.expect("7d reset");
        // reset_after is relative to probe `now`, clamped to window
        assert!(((r5 - now).num_seconds() - 7200).abs() <= 1);
        assert!(((r7 - now).num_seconds() - 86400).abs() <= 1);
        let label = snap.reset_in_label(now).unwrap();
        assert!(label.contains("后重置"));
        // 5h row uses only 5h reset (~2h), never weekly remaining.
        assert!(label.starts_with("2h"), "label={label}");
        let label7 = snap.reset_in_label_7d(now).unwrap();
        assert!(
            label7.starts_with("1d")
                || label7.contains("24h")
                || label7.starts_with("1d0h")
                || label7.contains("后重置")
        );
        // 86400s = 1d exactly
        assert!(
            label7.starts_with("1d") || label7 == "24h00m 后重置" || label7.starts_with("1d0h")
        );
    }

    #[test]
    fn seven_day_remaining_never_exceeds_window() {
        let now = Utc::now();
        // Malicious/wrong reset_after of 9 days on a 7d window must clamp.
        let raw = CodexRawSnapshot {
            primary_used: Some(10.0),
            primary_reset_after: Some(9 * 24 * 3600), // 9 days
            primary_window_mins: Some(10080),         // 7 days
            secondary_used: Some(1.0),
            secondary_reset_after: Some(10 * 3600), // 10h on 5h window → clamp to 5h
            secondary_window_mins: Some(300),
            ..Default::default()
        };
        let snap = normalize_codex_snapshot_to_quota(&raw, now, "test");
        let rem7 = (snap.reset_7d_at.unwrap() - now).num_seconds();
        assert!(
            rem7 <= 7 * 24 * 3600 + 120,
            "7d remaining {rem7}s must not exceed 7d window"
        );
        assert!(
            rem7 >= 7 * 24 * 3600 - 5,
            "should clamp near full 7d, got {rem7}"
        );
        let rem5 = (snap.reset_5h_at.unwrap() - now).num_seconds();
        assert!(
            rem5 <= 5 * 3600 + 120,
            "5h remaining {rem5}s must not exceed 5h window"
        );
    }

    #[test]
    fn parse_wham_without_window_length_uses_openai_primary_as_7d() {
        let now = Utc::now();
        let body = json!({
            "rate_limit": {
                "primary_window": { "used_percent": 10.0, "reset_after_seconds": 1000 },
                "secondary_window": { "used_percent": 20.0, "reset_after_seconds": 500 }
            }
        });
        let snap = parse_openai_wham_usage(&body, now);
        assert_eq!(snap.quota7d_pct, Some(10.0));
        assert_eq!(snap.quota5h_pct, Some(20.0));
    }

    #[test]
    fn parse_grok_billing_weekly_and_monthly() {
        let now = DateTime::parse_from_rfc3339("2026-07-10T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let weekly = json!({
            "config": {
                "currentPeriod": {
                    "type": "WEEKLY",
                    "start": "2026-07-09T03:25:00Z",
                    "end": "2026-07-16T03:25:00Z"
                },
                "creditUsagePercent": 12.5,
                "productUsage": [{ "product": "Api", "usagePercent": 12.5 }]
            }
        });
        let monthly = json!({
            "config": {
                "monthlyLimit": { "val": 15000 },
                "used": { "val": 1500 },
                "billingPeriodStart": "2026-07-01T00:00:00Z",
                "billingPeriodEnd": "2026-08-01T00:00:00Z"
            }
        });
        let snap = parse_grok_billing(Some(&weekly), Some(&monthly), now);
        assert_eq!(snap.quota7d_pct, Some(12.5));
        assert_eq!(snap.plan_type.as_deref(), Some("SuperGrok"));
        let r7 = snap.reset_7d_at.expect("weekly period end");
        // end - now = ~6d3h, within 7d cap
        let rem = (r7 - now).num_seconds();
        assert!(rem > 5 * 24 * 3600);
        assert!(rem <= 7 * 24 * 3600 + 120);
        assert!(snap.reset_in_label_7d(now).unwrap().contains("后重置"));
        // 5h unused for Grok billing
        assert!(snap.quota5h_pct.is_none());
        assert!(snap.reset_in_label(now).is_none());
    }

    #[test]
    fn parse_grok_billing_monthly_only() {
        let now = Utc::now();
        let monthly = json!({
            "config": {
                "monthlyLimit": { "val": 15000 },
                "used": { "val": 7500 },
                "billingPeriodEnd": (now + ChronoDuration::days(10)).to_rfc3339()
            }
        });
        let snap = parse_grok_billing(None, Some(&monthly), now);
        assert_eq!(snap.quota7d_pct, Some(50.0));
        assert_eq!(snap.plan_type.as_deref(), Some("SuperGrok"));
        // monthly remaining clamped to 7d for the 7d bar
        let rem = (snap.reset_7d_at.unwrap() - now).num_seconds();
        assert!(rem <= 7 * 24 * 3600 + 120);
    }

    #[test]
    fn codex_token_expiry_ignores_short_id_token() {
        // Real Codex shape: access exp ~10d, id_token exp ~1h (often already past).
        let access_exp = (Utc::now() + ChronoDuration::hours(200)).timestamp();
        let id_exp = (Utc::now() - ChronoDuration::hours(100)).timestamp();
        let access = {
            use base64::engine::general_purpose::URL_SAFE_NO_PAD;
            use base64::Engine;
            let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"none"}"#);
            let payload =
                URL_SAFE_NO_PAD.encode(json!({"sub":"u","exp": access_exp}).to_string().as_bytes());
            format!("{header}.{payload}.sig")
        };
        let id_token = {
            use base64::engine::general_purpose::URL_SAFE_NO_PAD;
            use base64::Engine;
            let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"none"}"#);
            let payload =
                URL_SAFE_NO_PAD.encode(json!({"sub":"u","exp": id_exp}).to_string().as_bytes());
            format!("{header}.{payload}.sig")
        };
        let mut acc = Account {
            id: "c1".into(),
            agent_id: AgentId::Codex,
            kind: AccountKind::Oauth,
            label: "x".into(),
            credentials: json!({
                "format": "auth_json",
                "body": { "tokens": { "access_token": access, "id_token": id_token } },
                // Stale value from a previous id_token-based heal:
                "expires_at": DateTime::from_timestamp(id_exp, 0).unwrap().to_rfc3339(),
            }),
            extra: json!({
                "expiresAt": DateTime::from_timestamp(id_exp, 0).unwrap().to_rfc3339(),
                "tokenExpired": true,
            }),
            status: "active".into(),
            is_current: true,
            created_at: "t".into(),
            updated_at: "t".into(),
        };
        assert!(heal_token_expiry(&mut acc));
        assert_eq!(
            acc.extra.get("tokenExpired").and_then(|v| v.as_bool()),
            Some(false),
            "must use access_token exp, not expired id_token"
        );
        let exp = acc.extra.get("expiresAt").and_then(|v| v.as_str()).unwrap();
        let rem = (DateTime::parse_from_rfc3339(exp)
            .unwrap()
            .with_timezone(&Utc)
            - Utc::now())
        .num_seconds();
        assert!(
            rem > 100 * 3600,
            "remaining should be ~200h from access, got {rem}"
        );
    }

    #[test]
    fn refresh_quota_reset_label_ticks_from_absolute_time() {
        let mut acc = Account {
            id: "c1".into(),
            agent_id: AgentId::Codex,
            kind: AccountKind::Oauth,
            label: "x".into(),
            credentials: json!({}),
            extra: json!({
                "quota5hPct": 10,
                "quota5hResetAt": (Utc::now() + ChronoDuration::hours(1) + ChronoDuration::minutes(5)).to_rfc3339(),
                "quotaResetIn": "9h00m 后重置"
            }),
            status: "active".into(),
            is_current: true,
            created_at: "t".into(),
            updated_at: "t".into(),
        };
        assert!(refresh_quota_reset_label(&mut acc, Utc::now()));
        let label = acc
            .extra
            .get("quotaResetIn")
            .and_then(|v| v.as_str())
            .unwrap();
        assert!(
            label.starts_with("1h"),
            "expected ~1h remaining, got {label}"
        );
        assert!(label.contains("后重置"));
    }

    #[test]
    fn parse_claude_five_hour_seven_day() {
        let now = Utc::now();
        let reset5 = (now + ChronoDuration::hours(3)).to_rfc3339();
        let body = json!({
            "five_hour": { "utilization": 12.0, "resets_at": reset5 },
            "seven_day": { "utilization": 55.5, "resets_in_seconds": 100000 }
        });
        let snap = parse_claude_oauth_usage(&body, now);
        assert_eq!(snap.quota5h_pct, Some(12.0));
        assert_eq!(snap.quota7d_pct, Some(55.5));
        assert!(snap.reset_5h_at.is_some());
        assert!(snap.reset_7d_at.is_some());
    }

    #[test]
    fn heal_token_expiry_from_jwt_exp() {
        let exp = (Utc::now() + ChronoDuration::hours(6)).timestamp();
        let access = make_jwt(json!({ "sub": "u1", "exp": exp }));
        let mut acc = Account {
            id: "c1".into(),
            agent_id: AgentId::Codex,
            kind: AccountKind::Oauth,
            label: "codex-oauth".into(),
            credentials: json!({
                "format": "auth_json",
                "body": { "tokens": { "access_token": access, "refresh_token": "rt" } }
            }),
            extra: json!({ "source": "live" }),
            status: "active".into(),
            is_current: true,
            created_at: "t".into(),
            updated_at: "t".into(),
        };
        assert!(heal_token_expiry(&mut acc));
        assert!(acc
            .extra
            .get("expiresAt")
            .and_then(|v| v.as_str())
            .is_some());
        assert_eq!(
            acc.extra.get("tokenExpired").and_then(|v| v.as_bool()),
            Some(false)
        );
        assert!(acc
            .credentials
            .get("access_token")
            .and_then(|v| v.as_str())
            .is_some());
    }

    #[test]
    fn apply_snapshot_writes_ui_fields() {
        let mut acc = Account {
            id: "c1".into(),
            agent_id: AgentId::Codex,
            kind: AccountKind::Oauth,
            label: "x".into(),
            credentials: json!({}),
            extra: json!({}),
            status: "active".into(),
            is_current: true,
            created_at: "t".into(),
            updated_at: "t".into(),
        };
        let now = Utc::now();
        let snap = QuotaSnapshot {
            quota5h_pct: Some(62.4),
            quota7d_pct: Some(10.1),
            reset_5h_at: Some(now + ChronoDuration::hours(2)),
            reset_7d_at: None,
            plan_type: Some("plus".into()),
            source: "test",
        };
        assert!(apply_quota_snapshot(&mut acc, &snap, now));
        assert_eq!(
            acc.extra.get("quota5hPct").and_then(|v| v.as_i64()),
            Some(62)
        );
        assert_eq!(
            acc.extra.get("quota7dPct").and_then(|v| v.as_i64()),
            Some(10)
        );
        assert!(acc
            .extra
            .get("quota5hResetAt")
            .and_then(|v| v.as_str())
            .is_some());
        assert!(acc
            .extra
            .get("quotaResetIn")
            .and_then(|v| v.as_str())
            .unwrap()
            .contains("后重置"));
        assert_eq!(
            acc.extra.get("subscription").and_then(|v| v.as_str()),
            Some("plus")
        );
        assert!(!quota_is_stale(&acc, now));
    }

    #[test]
    fn apply_snapshot_clears_omitted_5h_when_upstream_is_weekly_only() {
        let mut acc = oauth_account_with_extra(json!({
            "quota5hPct": 40,
            "quota5hResetAt": "2026-08-01T00:00:00Z",
            "quotaResetIn": "即将重置",
            "quota7dPct": 10
        }));
        let now = Utc::now();
        let snap = QuotaSnapshot {
            quota5h_pct: None,
            quota7d_pct: Some(22.0),
            reset_5h_at: None,
            reset_7d_at: Some(now + ChronoDuration::days(2)),
            plan_type: None,
            source: "test",
        };
        assert!(apply_quota_snapshot(&mut acc, &snap, now));
        assert!(acc.extra.get("quota5hPct").is_none());
        assert!(acc.extra.get("quotaResetIn").is_none());
        assert_eq!(
            acc.extra.get("quota7dPct").and_then(|v| v.as_i64()),
            Some(22)
        );
    }

    #[test]
    fn codex_probe_payload_matches_sub2api_cheap_stream() {
        let payload = codex_responses_probe_payload();
        assert_eq!(payload["model"], "codex-auto-review");
        assert_eq!(payload["stream"], true);
        assert_eq!(payload["store"], false);
        assert!(payload.get("instructions").and_then(|v| v.as_str()).is_some());
    }

    #[test]
    fn parse_wham_keeps_top_level_pool_when_additional_codex_meter_is_empty() {
        let now = Utc::now();
        let body = json!({
            "plan_type": "plus",
            "rate_limit": {
                "primary_window": {
                    "used_percent": 18.0,
                    "limit_window_seconds": 604800,
                    "reset_after_seconds": 86400
                },
                "secondary_window": {
                    "used_percent": 42.5,
                    "limit_window_seconds": 18000,
                    "reset_after_seconds": 7200
                }
            },
            "additional_rate_limits": [{
                "metered_feature": "codex_bengalfox",
                "rate_limit": serde_json::Value::Null
            }]
        });
        let snap = parse_openai_wham_usage(&body, now);
        assert_eq!(snap.quota5h_pct, Some(42.5));
        assert_eq!(snap.quota7d_pct, Some(18.0));
    }

    #[test]
    fn parse_wham_uses_bengalfox_only_when_shared_pool_has_no_windows() {
        let now = Utc::now();
        let body = json!({
            "rate_limit": serde_json::Value::Null,
            "additional_rate_limits": [{
                "metered_feature": "codex_bengalfox",
                "rate_limit": {
                    "primary_window": {
                        "used_percent": 7.0,
                        "limit_window_seconds": 604800,
                        "reset_after_seconds": 1000
                    },
                    "secondary_window": {
                        "used_percent": 3.0,
                        "limit_window_seconds": 18000,
                        "reset_after_seconds": 500
                    }
                }
            }]
        });
        let snap = parse_openai_wham_usage(&body, now);
        assert_eq!(snap.quota5h_pct, Some(3.0));
        assert_eq!(snap.quota7d_pct, Some(7.0));
    }

    fn oauth_account_with_extra(extra: Value) -> Account {
        Account {
            id: "c1".into(),
            agent_id: AgentId::Codex,
            kind: AccountKind::Oauth,
            label: "x".into(),
            credentials: json!({}),
            extra,
            status: "active".into(),
            is_current: true,
            created_at: "t".into(),
            updated_at: "t".into(),
        }
    }

    #[test]
    fn quota_is_stale_when_updated_at_missing_even_if_pct_fields_exist() {
        let acc = oauth_account_with_extra(json!({
            "quota5hPct": 40,
            "quota7dPct": 10
        }));
        assert!(quota_is_stale(&acc, Utc::now()));
    }

    #[test]
    fn quota_is_stale_when_5h_reset_has_elapsed() {
        let now = Utc::now();
        let acc = oauth_account_with_extra(json!({
            "quota5hPct": 40,
            "quotaUpdatedAt": now.to_rfc3339(),
            "quota5hResetAt": (now - ChronoDuration::seconds(1)).to_rfc3339()
        }));
        assert!(quota_is_stale(&acc, now));
    }

    #[test]
    fn elapsed_5h_reset_hides_bar_instead_of_inventing_zero() {
        let now = Utc::now();
        let mut acc = oauth_account_with_extra(json!({
            "quota5hPct": 40,
            "quota5hResetAt": (now - ChronoDuration::seconds(1)).to_rfc3339(),
            "quotaResetIn": "即将重置",
            "quota7dPct": 10
        }));
        assert!(refresh_quota_reset_label(&mut acc, now));
        assert!(acc.extra.get("quota5hPct").is_none());
        assert!(acc.extra.get("quotaResetIn").is_none());
        assert_eq!(
            acc.extra.get("quota7dPct").and_then(|v| v.as_i64()),
            Some(10)
        );
    }
}
