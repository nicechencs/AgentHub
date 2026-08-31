//! Durable JSONL spool for per-request gateway usage capture.
//!
//! The local bridge appends one [`GatewayUsageEvent`] per completed upstream
//! exchange to a per-day `gateway-YYYYMMDD.jsonl` file. The usage collect
//! pipeline ingests those files into the separate `gateway_usage` SQLite table
//! (migration 00024) with byte-offset cursors, so crash replay is idempotent
//! (`request_id` primary key). Capture is best-effort: a spool failure is only
//! logged and never propagates into the request path.
//!
//! This data deliberately stays separate from `usage_records`: agent log
//! collection already records the same spend, and merging the two would
//! double count.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::bridge::account::PickedMember;
use crate::bridge::usage::Usage;

/// One per-request gateway usage event. Field names mirror the
/// `gateway_usage` table columns; the spool wire is one JSON object per line.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GatewayUsageEvent {
    pub request_id: String,
    /// RFC3339 UTC timestamp taken when the outcome was recorded.
    pub ts: String,
    pub profile_id: String,
    /// Downstream surface op name (`responses` / `messages` / `chat`).
    pub surface: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upstream_channel: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ticket_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_source_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_source_id: Option<String>,
    /// Public model string from the client request body.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upstream_model: Option<String>,
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cached_input_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_tokens: Option<u64>,
    /// `ok` or `failed`.
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_code: Option<u16>,
    /// Short failure class (bridge error code / upstream error class name).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_class: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<u64>,
    /// Time to first forwarded stream payload; `None` for non-stream requests.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ttft_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attempts: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
}

/// Per-request capture identity threaded from dispatch into the stream
/// completion handlers. Only fields the dispatch path already knows.
#[derive(Debug, Clone, Default)]
pub(crate) struct CaptureContext {
    /// Downstream surface op name (`responses` / `messages` / `chat`).
    pub surface: &'static str,
    /// Public model string from the client request body.
    pub model: String,
    /// Session identifier when already extracted for affinity / continuation.
    pub session_id: Option<String>,
    /// Actual upstream channel (resolved after failover picked an attempt).
    pub channel: Option<&'static str>,
}

impl GatewayUsageEvent {
    /// Identity skeleton shared by every capture site; outcome fields are
    /// set by the caller before [`emit`].
    pub(crate) fn base(request_id: &str, started: Instant, profile_id: &str) -> Self {
        Self {
            request_id: request_id.to_owned(),
            ts: now_rfc3339(),
            profile_id: profile_id.to_owned(),
            surface: String::new(),
            upstream_channel: None,
            ticket_id: None,
            account_source_kind: None,
            account_source_id: None,
            model: None,
            upstream_model: None,
            input_tokens: 0,
            output_tokens: 0,
            cached_input_tokens: None,
            reasoning_tokens: None,
            status: "ok".to_owned(),
            status_code: None,
            error_class: None,
            latency_ms: Some(elapsed_ms(started)),
            ttft_ms: None,
            // v1 = 1. v2 final attempt counts are not surfaced on the send
            // outcome; keeping 1 avoids inventing plumbing in the hot path.
            attempts: Some(1),
            session_id: None,
        }
    }

    pub(crate) fn with_member(mut self, member: &PickedMember) -> Self {
        self.ticket_id = non_empty(&member.ticket_id);
        self.account_source_kind = non_empty(&member.source_kind);
        self.account_source_id = non_empty(&member.source_id);
        self
    }

    pub(crate) fn with_capture(mut self, capture: &CaptureContext) -> Self {
        self.surface = capture.surface.to_owned();
        self.model = non_empty(&capture.model);
        self.session_id = capture.session_id.clone();
        self.upstream_channel = capture.channel.map(str::to_owned);
        self
    }

    pub(crate) fn with_upstream_model(mut self, model: Option<&str>) -> Self {
        self.upstream_model = model.map(str::to_owned);
        self
    }

    /// Token fields from the protocol Usage IR.
    pub(crate) fn with_usage(mut self, usage: &Usage) -> Self {
        self.input_tokens = usage.input_tokens;
        self.output_tokens = usage.output_tokens;
        self.cached_input_tokens = usage.cached_input_tokens;
        self.reasoning_tokens = Some(usage.reasoning_tokens);
        self
    }

    pub(crate) fn ok(mut self, status_code: Option<u16>, ttft_ms: Option<u64>) -> Self {
        self.status = "ok".to_owned();
        self.status_code = status_code;
        self.ttft_ms = ttft_ms;
        self.error_class = None;
        self
    }

    pub(crate) fn failed(mut self, status_code: Option<u16>, error_class: &str) -> Self {
        self.status = "failed".to_owned();
        self.status_code = status_code;
        self.error_class = Some(error_class.to_owned());
        self
    }
}

/// Append one event to the spool when capture is installed; no-op otherwise.
pub(crate) fn emit(slot: &UsageSpoolSlot, event: GatewayUsageEvent) {
    if let Some(spool) = slot.get() {
        spool.record(&event);
    }
}

/// One stream pump's capture guard. Stream generators own it: the success tail
/// calls [`Self::succeed`], and every error `return` (or a dropped generator
/// after a client disconnect) records the failure exactly once via `Drop`.
/// Error class is uniform (`stream_error`), matching the stream warning path.
pub(crate) struct StreamCaptureGuard {
    slot: UsageSpoolSlot,
    event: GatewayUsageEvent,
    started: Instant,
    status_code: Option<u16>,
    armed: bool,
}

impl StreamCaptureGuard {
    pub(crate) fn new(
        slot: &UsageSpoolSlot,
        event: GatewayUsageEvent,
        started: Instant,
        status_code: Option<u16>,
    ) -> Self {
        Self {
            slot: slot.clone(),
            event,
            started,
            status_code,
            armed: true,
        }
    }

    /// Success tail: disarm and record the `ok` outcome.
    pub(crate) fn succeed(mut self, ttft_ms: Option<u64>, usage: Option<&Usage>) {
        self.armed = false;
        let mut event = self.event.clone();
        event.latency_ms = Some(elapsed_ms(self.started));
        event.ttft_ms = ttft_ms;
        if let Some(usage) = usage {
            event = event.with_usage(usage);
        }
        emit(&self.slot, event.ok(self.status_code, ttft_ms));
    }
}

impl Drop for StreamCaptureGuard {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        let mut event = self.event.clone();
        event.latency_ms = Some(elapsed_ms(self.started));
        emit(&self.slot, event.failed(self.status_code, "stream_error"));
    }
}

/// Host-level, cloneable slot holding the optional spool. The shell installs
/// it once (before edges start); unset keeps capture disabled so CLI runs and
/// existing tests never write spool files.
#[derive(Clone, Default)]
pub struct UsageSpoolSlot(Arc<OnceLock<Arc<UsageSpool>>>);

impl UsageSpoolSlot {
    /// Install the spool. Later calls are ignored (`true` on first install).
    pub fn set(&self, spool: Arc<UsageSpool>) -> bool {
        self.0.set(spool).is_ok()
    }

    pub(crate) fn get(&self) -> Option<Arc<UsageSpool>> {
        self.0.get().cloned()
    }
}

/// Append-only JSONL spool: one line per [`GatewayUsageEvent`], one file per
/// UTC day. Writes are flushed immediately; any error is logged (once per
/// record) and dropped so capture can never affect a response.
pub struct UsageSpool {
    dir: PathBuf,
    writer: Mutex<Option<SpoolFile>>,
}

struct SpoolFile {
    day: String,
    file: std::fs::File,
}

impl UsageSpool {
    pub fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            writer: Mutex::new(None),
        }
    }

    pub fn record(&self, event: &GatewayUsageEvent) {
        let line = match serde_json::to_string(event) {
            Ok(line) => line,
            Err(error) => {
                warn_capture("serialize", &error.to_string());
                return;
            }
        };
        if let Err(error) = self.append_line(&day_key(&event.ts), &line) {
            warn_capture("write", &error.to_string());
        }
    }

    fn append_line(&self, day: &str, line: &str) -> std::io::Result<()> {
        let mut guard = self.writer.lock().map_err(|_| {
            std::io::Error::other("gateway usage spool writer lock poisoned")
        })?;
        if guard.as_ref().map(|file| file.day.as_str()) != Some(day) {
            fs::create_dir_all(&self.dir)?;
            let path = self.dir.join(format!("gateway-{day}.jsonl"));
            let file = OpenOptions::new().create(true).append(true).open(path)?;
            *guard = Some(SpoolFile {
                day: day.to_owned(),
                file,
            });
        }
        let Some(file) = guard.as_mut() else {
            return Ok(());
        };
        file.file.write_all(line.as_bytes())?;
        file.file.write_all(b"\n")?;
        file.file.flush()?;
        Ok(())
    }
}

fn warn_capture(op: &str, error: &str) {
    tracing::warn!(
        target: "core.adapter",
        op = "usage_capture",
        code = "spool_write_failed",
        stage = op,
        error = %error,
        "gateway usage spool write failed; capture continues best-effort"
    );
}

fn non_empty(value: &str) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

/// UTC day key (`YYYYMMDD`) from an RFC3339 timestamp's date part.
fn day_key(ts: &str) -> String {
    let date = ts.get(..10).unwrap_or("1970-01-01");
    date.replace('-', "")
}

pub(crate) fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}

#[cfg(test)]
mod tests;
