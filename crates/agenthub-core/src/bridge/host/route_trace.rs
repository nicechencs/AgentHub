//! Credential-free route request traces for monitoring UI.
//!
//! Records the request lifecycle from local endpoint/auth through admission,
//! route resolution, connection selection, request/response conversion, upstream,
//! and client delivery — never bodies or secrets.
//!
//! When persistence is enabled (desktop GUI), traces go to a disposable sqlite
//! file next to — not inside — `agenthub.db`. Disk history is kept for
//! `log_retention_days` and includes token counts. The live UI ring stays at
//! [`ROUTE_TRACE_CAP`] per profile. Deleting the sqlite file does not touch
//! logins or routes.

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

mod persist;

use serde::{Deserialize, Serialize};

use super::surface::DownstreamSurface;
use super::transport::UpstreamChannel;
use crate::bridge::account::PickedMember;
use crate::bridge::route_index::DispatchCandidate;

pub const ROUTE_TRACE_CAP: usize = 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TraceStageStatus {
    Pending,
    Ok,
    Failed,
    Skipped,
    Interrupted,
}

impl Default for TraceStageStatus {
    fn default() -> Self {
        Self::Pending
    }
}

impl TraceStageStatus {
    /// Stable lowercase label for structured logs / UI stage names.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Ok => "ok",
            Self::Failed => "failed",
            Self::Skipped => "skipped",
            Self::Interrupted => "interrupted",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RouteTraceStageId {
    Received,
    LocalAuth,
    LocalEndpoint,
    Admission,
    RouteResolution,
    Pool,
    #[serde(alias = "conversion")]
    RequestConversion,
    UpstreamRequest,
    #[serde(alias = "upstream", alias = "upstream_auth")]
    UpstreamResponse,
    ResponseConversion,
    Delivery,
    #[serde(other)]
    Unknown,
}

impl RouteTraceStageId {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Received => "received",
            Self::LocalAuth => "local_auth",
            Self::LocalEndpoint => "local_endpoint",
            Self::Admission => "admission",
            Self::RouteResolution => "route_resolution",
            Self::Pool => "pool",
            Self::RequestConversion => "request_conversion",
            Self::UpstreamRequest => "upstream_request",
            Self::UpstreamResponse => "upstream_response",
            Self::ResponseConversion => "response_conversion",
            Self::Delivery => "delivery",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RouteTraceStep {
    pub status: TraceStageStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RouteTraceDelivery {
    pub status: TraceStageStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http_status: Option<u16>,
    #[serde(default)]
    pub stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completion: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteTraceMember {
    pub label: String,
    pub source_kind: String,
    pub source_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ticket_id: Option<String>,
    /// Last four characters of the selected upstream login only; never the secret.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key_last4: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteTracePoolAttempt {
    pub member: RouteTraceMember,
    pub status: TraceStageStatus,
    #[serde(default)]
    pub attempt_id: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default)]
    pub request_status: TraceStageStatus,
    #[serde(default)]
    pub response_status: TraceStageStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_result: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http_status: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conversion_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteTraceLocalAuth {
    pub status: TraceStageStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_id: Option<String>,
    /// Last four characters of the accepted local entry key; never the secret.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key_last4: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteTracePool {
    pub status: TraceStageStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected_member: Option<RouteTraceMember>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attempts: Vec<RouteTracePoolAttempt>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RouteTraceConversion {
    pub status: TraceStageStatus,
    /// Stable path id, e.g. `messages_to_openai_chat` or `passthrough`.
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RouteTraceUpstreamRequest {
    pub status: TraceStageStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub member: Option<RouteTraceMember>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteTraceUpstreamAuth {
    pub status: TraceStageStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http_status: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteTraceUpstream {
    pub status: TraceStageStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub member: Option<RouteTraceMember>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upstream_model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http_status: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// One completed local-route request trace for Activity / monitoring UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteRequestTrace {
    #[serde(default = "legacy_trace_version")]
    pub trace_version: u8,
    pub request_id: String,
    pub at_unix_ms: u128,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_id: Option<String>,
    pub method: String,
    pub path: String,
    pub http_status: u16,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ttft_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cached_input_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_tokens: Option<u64>,
    #[serde(default)]
    pub local_endpoint: RouteTraceStep,
    pub local_auth: RouteTraceLocalAuth,
    #[serde(default)]
    pub admission: RouteTraceStep,
    #[serde(default)]
    pub route_resolution: RouteTraceStep,
    pub pool: RouteTracePool,
    pub conversion: RouteTraceConversion,
    pub upstream_auth: RouteTraceUpstreamAuth,
    #[serde(default)]
    pub upstream_request: RouteTraceUpstreamRequest,
    pub upstream: RouteTraceUpstream,
    #[serde(default)]
    pub response_conversion: RouteTraceConversion,
    #[serde(default)]
    pub delivery: RouteTraceDelivery,
    /// First failed lifecycle stage id, used by monitoring summaries.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_stage: Option<RouteTraceStageId>,
}

#[derive(Default)]
struct ProfileTraces {
    recent: VecDeque<RouteRequestTrace>,
}

#[derive(Clone, Default)]
pub struct RouteTraceLog {
    inner: Arc<Mutex<TraceStore>>,
}

#[derive(Clone, Copy, Default)]
struct TraceUsagePatch {
    ttft_ms: Option<u64>,
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    cached_input_tokens: Option<u64>,
    reasoning_tokens: Option<u64>,
}

fn apply_usage_patch(trace: &mut RouteRequestTrace, patch: TraceUsagePatch) {
    if patch.ttft_ms.is_some() {
        trace.ttft_ms = patch.ttft_ms;
    }
    if patch.input_tokens.is_some() {
        trace.input_tokens = patch.input_tokens;
    }
    if patch.output_tokens.is_some() {
        trace.output_tokens = patch.output_tokens;
    }
    if patch.cached_input_tokens.is_some() {
        trace.cached_input_tokens = patch.cached_input_tokens;
    }
    if patch.reasoning_tokens.is_some() {
        trace.reasoning_tokens = patch.reasoning_tokens;
    }
}

struct TraceStore {
    by_profile: HashMap<String, ProfileTraces>,
    unauthenticated: VecDeque<RouteRequestTrace>,
    pending_usage: HashMap<String, TraceUsagePatch>,
    /// Disposable sqlite handle. Missing means in-memory ring only.
    persist: Option<persist::RouteTraceDb>,
}

impl Default for TraceStore {
    fn default() -> Self {
        Self {
            by_profile: HashMap::new(),
            unauthenticated: VecDeque::new(),
            pending_usage: HashMap::new(),
            persist: None,
        }
    }
}

impl RouteTraceLog {
    pub fn new() -> Self {
        Self::default()
    }

    /// Enable durable sqlite persistence at `path`. Loads the live UI ring if
    /// the file exists. Later calls are ignored (install once at process start).
    /// Best-effort: corrupt files are recreated; write failures never affect
    /// the request path. Disk retention follows settings `log_retention_days`.
    pub fn enable_persist(&self, path: PathBuf) {
        self.enable_persist_with_retention(
            path,
            crate::catalog::limits::DEFAULT_LOG_RETENTION_DAYS,
        );
    }

    pub fn enable_persist_with_retention(&self, path: PathBuf, retention_days: u32) {
        let Ok(mut store) = self.inner.lock() else {
            return;
        };
        if store.persist.is_some() {
            return;
        }
        let Some(db) = persist::RouteTraceDb::open_with_retention(&path, retention_days) else {
            return;
        };
        let empty = store.by_profile.is_empty() && store.unauthenticated.is_empty();
        if empty {
            apply_snapshot(&mut store, db.load_recent());
        }
        store.persist = Some(db);
    }

    pub fn push(&self, trace: RouteRequestTrace) {
        let Ok(mut store) = self.inner.lock() else {
            return;
        };
        let mut record = trace;
        if let Some(patch) = store.pending_usage.remove(&record.request_id) {
            apply_usage_patch(&mut record, patch);
        }
        flush_row(&store, &record);
        if let Some(profile_id) = record.profile_id.as_deref().filter(|id| !id.is_empty()) {
            let entry = store.by_profile.entry(profile_id.to_owned()).or_default();
            entry.recent.push_front(record);
            entry.recent.truncate(ROUTE_TRACE_CAP);
        } else {
            store.unauthenticated.push_front(record);
            store.unauthenticated.truncate(ROUTE_TRACE_CAP);
        }
    }

    pub fn patch_usage(
        &self,
        request_id: &str,
        ttft_ms: Option<u64>,
        input_tokens: Option<u64>,
        output_tokens: Option<u64>,
        cached_input_tokens: Option<u64>,
        reasoning_tokens: Option<u64>,
    ) {
        let Ok(mut store) = self.inner.lock() else {
            return;
        };
        let incoming = TraceUsagePatch {
            ttft_ms,
            input_tokens,
            output_tokens,
            cached_input_tokens,
            reasoning_tokens,
        };
        for entry in store.by_profile.values_mut() {
            if let Some(row) = entry
                .recent
                .iter_mut()
                .find(|row| row.request_id == request_id)
            {
                apply_usage_patch(row, incoming);
                let persisted = row.clone();
                flush_row(&store, &persisted);
                return;
            }
        }
        if let Some(row) = store
            .unauthenticated
            .iter_mut()
            .find(|row| row.request_id == request_id)
        {
            apply_usage_patch(row, incoming);
            let persisted = row.clone();
            flush_row(&store, &persisted);
            return;
        }
        if let Some(pending) = store.pending_usage.get_mut(request_id) {
            if incoming.ttft_ms.is_some() {
                pending.ttft_ms = incoming.ttft_ms;
            }
            if incoming.input_tokens.is_some() {
                pending.input_tokens = incoming.input_tokens;
            }
            if incoming.output_tokens.is_some() {
                pending.output_tokens = incoming.output_tokens;
            }
            if incoming.cached_input_tokens.is_some() {
                pending.cached_input_tokens = incoming.cached_input_tokens;
            }
            if incoming.reasoning_tokens.is_some() {
                pending.reasoning_tokens = incoming.reasoning_tokens;
            }
            return;
        }
        if store.pending_usage.len() >= ROUTE_TRACE_CAP {
            return;
        }
        store.pending_usage.insert(request_id.to_owned(), incoming);
    }

    pub fn patch_stream_completed(&self, request_id: &str, latency_ms: u64) {
        let Ok(mut store) = self.inner.lock() else {
            return;
        };
        let patch = |row: &mut RouteRequestTrace| {
            row.response_conversion.status = TraceStageStatus::Ok;
            row.response_conversion.result = Some("completed".to_owned());
            row.delivery.status = TraceStageStatus::Ok;
            row.delivery.completion = Some("stream_completed".to_owned());
            row.latency_ms = Some(latency_ms);
        };
        for entry in store.by_profile.values_mut() {
            if let Some(row) = entry
                .recent
                .iter_mut()
                .find(|row| row.request_id == request_id)
            {
                patch(row);
                let persisted = row.clone();
                flush_row(&store, &persisted);
                return;
            }
        }
        if let Some(row) = store
            .unauthenticated
            .iter_mut()
            .find(|row| row.request_id == request_id)
        {
            patch(row);
            let persisted = row.clone();
            flush_row(&store, &persisted);
        }
    }

    pub fn patch_stream_conversion_failed(&self, request_id: &str, latency_ms: u64) {
        let Ok(mut store) = self.inner.lock() else {
            return;
        };
        let patch = |row: &mut RouteRequestTrace| {
            row.ok = false;
            row.response_conversion.status = TraceStageStatus::Failed;
            row.response_conversion.result = Some("failed".to_owned());
            row.response_conversion.code = Some("stream_error".to_owned());
            row.delivery.status = TraceStageStatus::Failed;
            row.delivery.completion = Some("stream_error".to_owned());
            row.delivery.code = Some("stream_error".to_owned());
            row.latency_ms = Some(latency_ms);
            if row.failure_stage.is_none() {
                row.failure_stage = Some(RouteTraceStageId::ResponseConversion);
            }
        };
        for entry in store.by_profile.values_mut() {
            if let Some(row) = entry
                .recent
                .iter_mut()
                .find(|row| row.request_id == request_id)
            {
                patch(row);
                let persisted = row.clone();
                flush_row(&store, &persisted);
                return;
            }
        }
        if let Some(row) = store
            .unauthenticated
            .iter_mut()
            .find(|row| row.request_id == request_id)
        {
            patch(row);
            let persisted = row.clone();
            flush_row(&store, &persisted);
        }
    }

    pub fn patch_stream_disconnected(&self, request_id: &str, latency_ms: u64) {
        let Ok(mut store) = self.inner.lock() else {
            return;
        };
        let patch = |row: &mut RouteRequestTrace| {
            row.ok = false;
            row.response_conversion.status = TraceStageStatus::Interrupted;
            row.response_conversion.result = Some("interrupted".to_owned());
            row.delivery.status = TraceStageStatus::Failed;
            row.delivery.completion = Some("client_disconnected".to_owned());
            row.delivery.code = Some("client_disconnected".to_owned());
            row.latency_ms = Some(latency_ms);
            if row.failure_stage.is_none() {
                row.failure_stage = Some(RouteTraceStageId::Delivery);
            }
        };
        for entry in store.by_profile.values_mut() {
            if let Some(row) = entry
                .recent
                .iter_mut()
                .find(|row| row.request_id == request_id)
            {
                patch(row);
                let persisted = row.clone();
                flush_row(&store, &persisted);
                return;
            }
        }
        if let Some(row) = store
            .unauthenticated
            .iter_mut()
            .find(|row| row.request_id == request_id)
        {
            patch(row);
            let persisted = row.clone();
            flush_row(&store, &persisted);
        }
    }

    pub fn recent(&self, profile_id: &str) -> Vec<RouteRequestTrace> {
        self.inner
            .lock()
            .ok()
            .and_then(|store| {
                store
                    .by_profile
                    .get(profile_id)
                    .map(|entry| entry.recent.iter().cloned().collect())
            })
            .unwrap_or_default()
    }

    pub fn recent_unauthenticated(&self) -> Vec<RouteRequestTrace> {
        self.inner
            .lock()
            .ok()
            .map(|store| store.unauthenticated.iter().cloned().collect())
            .unwrap_or_default()
    }

    pub fn get(&self, request_id: &str) -> Option<RouteRequestTrace> {
        let store = self.inner.lock().ok()?;
        for entry in store.by_profile.values() {
            if let Some(found) = entry.recent.iter().find(|row| row.request_id == request_id) {
                return Some(found.clone());
            }
        }
        store
            .unauthenticated
            .iter()
            .find(|row| row.request_id == request_id)
            .cloned()
    }

    #[cfg(test)]
    fn persist_count(&self, profile_id: &str) -> usize {
        self.inner
            .lock()
            .ok()
            .and_then(|store| store.persist.as_ref().map(|db| db.count(profile_id)))
            .unwrap_or(0)
    }
}

fn apply_snapshot(store: &mut TraceStore, snapshot: persist::PersistSnapshot) {
    for (profile_id, traces) in snapshot.by_profile {
        if profile_id.trim().is_empty() {
            continue;
        }
        let entry = store.by_profile.entry(profile_id).or_default();
        entry.recent = traces.into_iter().take(ROUTE_TRACE_CAP).collect();
    }
    store.unauthenticated = snapshot
        .unauthenticated
        .into_iter()
        .take(ROUTE_TRACE_CAP)
        .collect();
}

fn flush_row(store: &TraceStore, trace: &RouteRequestTrace) {
    if let Some(db) = store.persist.as_ref() {
        db.upsert(trace);
    }
}

pub struct RouteTraceBuilder {
    trace: RouteRequestTrace,
    started: Instant,
    committed: bool,
}

impl RouteTraceBuilder {
    pub fn begin(
        request_id: impl Into<String>,
        method: impl Into<String>,
        path: impl Into<String>,
    ) -> Self {
        Self {
            trace: RouteRequestTrace {
                trace_version: 2,
                request_id: request_id.into(),
                at_unix_ms: now_unix_ms(),
                profile_id: None,
                method: method.into(),
                path: path.into(),
                http_status: 0,
                ok: false,
                model: None,
                latency_ms: None,
                ttft_ms: None,
                input_tokens: None,
                output_tokens: None,
                cached_input_tokens: None,
                reasoning_tokens: None,
                local_endpoint: RouteTraceStep::default(),
                local_auth: RouteTraceLocalAuth {
                    status: TraceStageStatus::Pending,
                    profile_id: None,
                    key_last4: None,
                    port: None,
                    code: None,
                    message: None,
                },
                admission: RouteTraceStep::default(),
                route_resolution: RouteTraceStep::default(),
                pool: RouteTracePool {
                    status: TraceStageStatus::Pending,
                    selected_member: None,
                    attempts: Vec::new(),
                    code: None,
                    message: None,
                },
                conversion: RouteTraceConversion {
                    status: TraceStageStatus::Pending,
                    path: String::new(),
                    result: None,
                    code: None,
                    message: None,
                },
                upstream_auth: RouteTraceUpstreamAuth {
                    status: TraceStageStatus::Pending,
                    http_status: None,
                    code: None,
                    message: None,
                },
                upstream_request: RouteTraceUpstreamRequest::default(),
                upstream: RouteTraceUpstream {
                    status: TraceStageStatus::Pending,
                    url: None,
                    member: None,
                    model: None,
                    upstream_model: None,
                    http_status: None,
                    code: None,
                    message: None,
                },
                response_conversion: RouteTraceConversion {
                    status: TraceStageStatus::Pending,
                    path: String::new(),
                    result: None,
                    code: None,
                    message: None,
                },
                delivery: RouteTraceDelivery::default(),
                failure_stage: None,
            },
            started: Instant::now(),
            committed: false,
        }
    }

    pub fn set_model(&mut self, model: Option<String>) {
        self.trace.model = model.filter(|value| !value.trim().is_empty());
    }

    #[cfg(test)]
    pub fn set_at_unix_ms(&mut self, at_unix_ms: u128) {
        self.trace.at_unix_ms = at_unix_ms;
    }

    pub fn local_endpoint_ok(&mut self) {
        self.trace.local_endpoint.status = TraceStageStatus::Ok;
    }

    pub fn admission_ok(&mut self) {
        self.trace.admission.status = TraceStageStatus::Ok;
    }

    pub fn admission_failed(&mut self, code: &str, message: &str) {
        self.trace.admission = RouteTraceStep {
            status: TraceStageStatus::Failed,
            code: Some(code.to_owned()),
            message: Some(message.to_owned()),
        };
        self.mark_failure(RouteTraceStageId::Admission);
        self.skip_after_admission();
    }

    pub fn route_resolution_ok(&mut self) {
        self.trace.route_resolution.status = TraceStageStatus::Ok;
    }

    pub fn route_resolution_failed(&mut self, code: &str, message: &str) {
        self.trace.route_resolution = RouteTraceStep {
            status: TraceStageStatus::Failed,
            code: Some(code.to_owned()),
            message: Some(message.to_owned()),
        };
        skip_if_pending(&mut self.trace.pool.status);
        self.mark_failure(RouteTraceStageId::RouteResolution);
        self.skip_after_pool();
    }

    pub fn local_auth_ok(&mut self, profile_id: &str, port: Option<u16>) {
        self.trace.profile_id = Some(profile_id.to_owned());
        self.trace.local_auth = RouteTraceLocalAuth {
            status: TraceStageStatus::Ok,
            profile_id: Some(profile_id.to_owned()),
            key_last4: None,
            port,
            code: None,
            message: None,
        };
    }

    pub fn local_auth_key_last4(&mut self, token: &str) {
        self.trace.local_auth.key_last4 = secret_last4(token);
    }

    pub fn local_auth_failed(&mut self, code: &str, message: &str) {
        self.trace.local_auth = RouteTraceLocalAuth {
            status: TraceStageStatus::Failed,
            profile_id: None,
            key_last4: None,
            port: None,
            code: Some(code.to_owned()),
            message: Some(message.to_owned()),
        };
        self.mark_failure(RouteTraceStageId::LocalAuth);
        skip_if_pending(&mut self.trace.local_endpoint.status);
        skip_if_pending(&mut self.trace.admission.status);
        skip_if_pending(&mut self.trace.route_resolution.status);
        self.skip_after_local_auth();
    }

    /// Token was accepted; this listener does not serve the requested path.
    /// Keep local auth Ok and fail the inbound endpoint, not the login step.
    pub fn local_path_failed(
        &mut self,
        profile_id: &str,
        port: Option<u16>,
        code: &str,
        message: &str,
    ) {
        self.trace.profile_id = Some(profile_id.to_owned());
        if self.trace.local_auth.status != TraceStageStatus::Ok {
            self.trace.local_auth = RouteTraceLocalAuth {
                status: TraceStageStatus::Ok,
                profile_id: Some(profile_id.to_owned()),
                key_last4: None,
                port,
                code: None,
                message: None,
            };
        } else if self.trace.local_auth.port.is_none() {
            self.trace.local_auth.port = port;
        }
        self.trace.local_endpoint = RouteTraceStep {
            status: TraceStageStatus::Failed,
            code: Some(code.to_owned()),
            message: Some(message.to_owned()),
        };
        self.mark_failure(RouteTraceStageId::LocalEndpoint);
        skip_if_pending(&mut self.trace.admission.status);
        skip_if_pending(&mut self.trace.route_resolution.status);
        self.skip_after_local_auth();
        self.trace.conversion.code = Some(code.to_owned());
        self.trace.conversion.message = Some(message.to_owned());
    }

    pub fn pool_failed(&mut self, code: &str, message: &str) {
        if !self.trace.pool.attempts.is_empty() {
            self.trace.pool.status = TraceStageStatus::Ok;
            return;
        }
        self.trace.pool = RouteTracePool {
            status: TraceStageStatus::Failed,
            selected_member: None,
            attempts: self.trace.pool.attempts.clone(),
            code: Some(code.to_owned()),
            message: Some(message.to_owned()),
        };
        self.mark_failure(RouteTraceStageId::Pool);
        self.skip_after_pool();
    }

    pub fn pool_selected(&mut self, member: &PickedMember, candidate: Option<&DispatchCandidate>) {
        let selected = trace_member(member);
        self.trace.pool.selected_member = Some(selected);
        self.trace.pool.status = TraceStageStatus::Ok;
        if let Some(candidate) = candidate {
            if !candidate.upstream_model.trim().is_empty() {
                self.trace.upstream.upstream_model = Some(candidate.upstream_model.clone());
            }
        }
    }

    pub fn upstream_model(&mut self, model: Option<&str>) {
        if self.trace.upstream.upstream_model.is_none() {
            self.trace.upstream.upstream_model = model
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned);
        }
    }

    pub fn upstream_attempt_started(
        &mut self,
        url: &str,
        member: &PickedMember,
        model: Option<&str>,
    ) -> u32 {
        let attempt_id = self.trace.pool.attempts.len() as u32 + 1;
        let member = trace_member(member);
        let url = sanitize_upstream_url(url);
        self.trace.upstream_request = RouteTraceUpstreamRequest {
            status: TraceStageStatus::Pending,
            url: Some(url.clone()),
            member: Some(member.clone()),
            model: model
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
                .or_else(|| self.trace.upstream.upstream_model.clone())
                .or_else(|| self.trace.model.clone()),
            code: None,
            message: None,
        };
        self.trace.pool.attempts.push(RouteTracePoolAttempt {
            member,
            status: TraceStageStatus::Pending,
            attempt_id,
            url: Some(url),
            request_status: TraceStageStatus::Pending,
            response_status: TraceStageStatus::Pending,
            auth_result: None,
            http_status: None,
            result: None,
            duration_ms: None,
            conversion_path: (!self.trace.conversion.path.is_empty())
                .then(|| self.trace.conversion.path.clone()),
            code: None,
            message: None,
        });
        attempt_id
    }

    pub fn upstream_attempt_response(
        &mut self,
        attempt_id: u32,
        http_status: u16,
        duration_ms: u64,
        code: Option<&str>,
    ) {
        let successful = (200..300).contains(&http_status);
        if let Some(attempt) = self
            .trace
            .pool
            .attempts
            .iter_mut()
            .find(|attempt| attempt.attempt_id == attempt_id)
        {
            attempt.status = if successful {
                TraceStageStatus::Ok
            } else {
                TraceStageStatus::Failed
            };
            attempt.request_status = TraceStageStatus::Ok;
            attempt.response_status = if successful {
                TraceStageStatus::Ok
            } else {
                TraceStageStatus::Failed
            };
            attempt.auth_result = Some(
                if successful {
                    "accepted"
                } else if http_status == 401 {
                    "rejected"
                } else {
                    "not_recorded"
                }
                .to_owned(),
            );
            attempt.http_status = Some(http_status);
            attempt.result = Some(if successful { "success" } else { "http_error" }.to_owned());
            attempt.duration_ms = Some(duration_ms);
            attempt.code = code.map(str::to_owned);
            attempt.message = code.map(safe_attempt_message);
        }
        self.trace.upstream_request.status = TraceStageStatus::Ok;
    }

    pub fn upstream_request_failed(
        &mut self,
        url: Option<&str>,
        member: &PickedMember,
        model: Option<&str>,
        code: &str,
    ) {
        self.trace.upstream_request = RouteTraceUpstreamRequest {
            status: TraceStageStatus::Failed,
            url: url
                .map(sanitize_upstream_url)
                .filter(|value| !value.is_empty()),
            member: Some(trace_member(member)),
            model: model
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
                .or_else(|| self.trace.upstream.upstream_model.clone())
                .or_else(|| self.trace.model.clone()),
            code: Some(code.to_owned()),
            message: Some(safe_attempt_message(code)),
        };
        self.mark_failure(RouteTraceStageId::UpstreamRequest);
        skip_if_pending(&mut self.trace.upstream_auth.status);
        skip_if_pending(&mut self.trace.upstream.status);
        skip_if_pending(&mut self.trace.response_conversion.status);
        skip_if_pending(&mut self.trace.delivery.status);
    }

    pub fn upstream_attempt_transport_failed(
        &mut self,
        attempt_id: u32,
        request_sent: bool,
        duration_ms: u64,
        code: &str,
    ) {
        if let Some(attempt) = self
            .trace
            .pool
            .attempts
            .iter_mut()
            .find(|attempt| attempt.attempt_id == attempt_id)
        {
            attempt.status = TraceStageStatus::Failed;
            attempt.request_status = if request_sent {
                TraceStageStatus::Ok
            } else {
                TraceStageStatus::Failed
            };
            attempt.response_status = if request_sent {
                TraceStageStatus::Failed
            } else {
                TraceStageStatus::Skipped
            };
            attempt.auth_result = Some("not_recorded".to_owned());
            attempt.result = Some(
                if request_sent {
                    "timeout"
                } else {
                    "transport_error"
                }
                .to_owned(),
            );
            attempt.duration_ms = Some(duration_ms);
            attempt.code = Some(code.to_owned());
            attempt.message = Some(safe_attempt_message(code));
        }
        self.trace.upstream_request.status = if request_sent {
            TraceStageStatus::Ok
        } else {
            TraceStageStatus::Failed
        };
        self.trace.upstream_request.code = (!request_sent).then(|| code.to_owned());
        self.trace.upstream_request.message = (!request_sent).then(|| safe_attempt_message(code));
    }

    pub fn pool_attempt_failed(&mut self, member: &PickedMember, code: &str, _message: &str) {
        if let Some(attempt) = self
            .trace
            .pool
            .attempts
            .last_mut()
            .filter(|attempt| attempt.member.source_id == member.source_id)
        {
            attempt.status = TraceStageStatus::Failed;
            attempt.code = Some(code.to_owned());
            attempt.message = Some(safe_attempt_message(code));
        }
    }

    pub fn conversion_prepared(
        &mut self,
        surface: DownstreamSurface,
        channel: UpstreamChannel,
        identity_relay: bool,
    ) {
        let path = conversion_path_id(surface, channel, identity_relay);
        let result = if identity_relay {
            "passthrough"
        } else {
            "converted"
        };
        self.trace.conversion = RouteTraceConversion {
            status: TraceStageStatus::Ok,
            path,
            result: Some(result.to_owned()),
            code: None,
            message: None,
        };
    }

    pub fn conversion_failed(&mut self, code: &str, message: &str) {
        self.trace.conversion = RouteTraceConversion {
            status: TraceStageStatus::Failed,
            path: self.trace.conversion.path.clone(),
            result: Some("failed".to_owned()),
            code: Some(code.to_owned()),
            message: Some(message.to_owned()),
        };
        self.mark_failure(RouteTraceStageId::RequestConversion);
        self.skip_after_conversion();
    }

    pub fn upstream_auth_result(
        &mut self,
        ok: bool,
        http_status: Option<u16>,
        code: Option<&str>,
        _message: Option<&str>,
    ) {
        self.trace.upstream_auth = RouteTraceUpstreamAuth {
            status: if ok {
                TraceStageStatus::Ok
            } else {
                TraceStageStatus::Failed
            },
            http_status,
            code: code.map(str::to_owned),
            message: code.map(safe_attempt_message),
        };
    }

    pub fn upstream_success(
        &mut self,
        url: &str,
        member: &PickedMember,
        http_status: u16,
        upstream_model: Option<&str>,
    ) {
        let member = trace_member(member);
        self.trace.pool.selected_member = Some(member.clone());
        self.trace.pool.status = TraceStageStatus::Ok;
        let has_success_attempt = self.trace.pool.attempts.last().is_some_and(|attempt| {
            attempt.member.source_id == member.source_id && attempt.status == TraceStageStatus::Ok
        });
        if !has_success_attempt {
            let attempt_id = self.trace.pool.attempts.len() as u32 + 1;
            self.trace.pool.attempts.push(RouteTracePoolAttempt {
                member: member.clone(),
                status: TraceStageStatus::Ok,
                attempt_id,
                url: Some(sanitize_upstream_url(url)),
                request_status: TraceStageStatus::Ok,
                response_status: TraceStageStatus::Ok,
                auth_result: Some("accepted".to_owned()),
                http_status: Some(http_status),
                result: Some("success".to_owned()),
                duration_ms: None,
                conversion_path: (!self.trace.conversion.path.is_empty())
                    .then(|| self.trace.conversion.path.clone()),
                code: None,
                message: None,
            });
        }
        self.trace.upstream_request.status = TraceStageStatus::Ok;
        self.trace.upstream_request.url = Some(sanitize_upstream_url(url));
        self.trace.upstream_request.member = Some(member.clone());
        self.trace.upstream_request.model = upstream_model
            .filter(|value| !value.trim().is_empty())
            .map(str::to_owned)
            .or_else(|| self.trace.upstream_request.model.clone())
            .or_else(|| self.trace.model.clone());
        self.trace.upstream = RouteTraceUpstream {
            status: TraceStageStatus::Ok,
            url: Some(sanitize_upstream_url(url)),
            member: Some(member),
            model: self.trace.model.clone(),
            upstream_model: upstream_model
                .filter(|value| !value.trim().is_empty())
                .map(str::to_owned)
                .or_else(|| self.trace.upstream.upstream_model.clone()),
            http_status: Some(http_status),
            code: None,
            message: None,
        };
        if self.trace.upstream_auth.status == TraceStageStatus::Pending {
            self.trace.upstream_auth.status = TraceStageStatus::Ok;
        }
        if self.trace.pool.status == TraceStageStatus::Pending {
            self.trace.pool.status = TraceStageStatus::Ok;
        }
    }

    pub fn upstream_failed(
        &mut self,
        url: &str,
        member: &PickedMember,
        http_status: Option<u16>,
        code: &str,
        _message: &str,
    ) {
        let failed_member = trace_member(member);
        if let Some(attempt) = self.trace.pool.attempts.last_mut().filter(|attempt| {
            attempt.member.source_kind == failed_member.source_kind
                && attempt.member.source_id == failed_member.source_id
        }) {
            attempt.status = TraceStageStatus::Failed;
            attempt.code = Some(code.to_owned());
            attempt.message = Some(safe_attempt_message(code));
        }
        if code != "unauthorized" {
            skip_if_pending(&mut self.trace.upstream_auth.status);
        }
        self.trace.upstream = RouteTraceUpstream {
            status: TraceStageStatus::Failed,
            url: Some(sanitize_upstream_url(url)),
            member: Some(failed_member),
            model: self.trace.model.clone(),
            upstream_model: self.trace.upstream.upstream_model.clone(),
            http_status,
            code: Some(code.to_owned()),
            message: Some(safe_attempt_message(code)),
        };
        skip_if_pending(&mut self.trace.response_conversion.status);
        skip_if_pending(&mut self.trace.delivery.status);
        if self.trace.upstream_request.status == TraceStageStatus::Failed {
            self.mark_failure(RouteTraceStageId::UpstreamRequest);
        } else {
            self.mark_failure(RouteTraceStageId::UpstreamResponse);
        }
    }

    pub fn attempts_exhausted(
        &mut self,
        url: Option<&str>,
        member: &PickedMember,
        http_status: Option<u16>,
        code: &str,
        _message: &str,
    ) {
        if self.trace.pool.attempts.is_empty() {
            self.pool_failed("pool_exhausted", "No healthy connection remained.");
            return;
        }
        let member = trace_member(member);
        self.trace.pool.status = TraceStageStatus::Ok;
        self.trace.pool.selected_member = Some(member.clone());
        self.trace.pool.code = None;
        self.trace.pool.message = None;
        if code == "unauthorized" {
            self.trace.upstream_auth = RouteTraceUpstreamAuth {
                status: TraceStageStatus::Failed,
                http_status,
                code: Some(code.to_owned()),
                message: Some(safe_attempt_message(code)),
            };
        } else {
            skip_if_pending(&mut self.trace.upstream_auth.status);
        }
        self.trace.upstream = RouteTraceUpstream {
            status: TraceStageStatus::Failed,
            url: url.map(sanitize_upstream_url),
            member: Some(member),
            model: self.trace.model.clone(),
            upstream_model: self.trace.upstream.upstream_model.clone(),
            http_status,
            code: Some(code.to_owned()),
            message: Some(safe_attempt_message(code)),
        };
        skip_if_pending(&mut self.trace.response_conversion.status);
        skip_if_pending(&mut self.trace.delivery.status);
        if self.trace.upstream_request.status == TraceStageStatus::Failed {
            self.mark_failure(RouteTraceStageId::UpstreamRequest);
        } else {
            self.mark_failure(RouteTraceStageId::UpstreamResponse);
        }
    }

    pub fn response_conversion_result(
        &mut self,
        stream: bool,
        http_status: u16,
        surface: DownstreamSurface,
        channel: UpstreamChannel,
    ) {
        self.trace.delivery.stream = stream;
        self.trace.response_conversion.path = format!("{}_to_{}", channel.name(), surface.op());
        if stream {
            self.trace.response_conversion.status = TraceStageStatus::Pending;
            self.trace.response_conversion.result = Some("streaming".to_owned());
        } else {
            self.trace.response_conversion.status = if (200..400).contains(&http_status) {
                TraceStageStatus::Ok
            } else {
                TraceStageStatus::Failed
            };
            self.trace.response_conversion.result = Some("completed".to_owned());
            if self.trace.response_conversion.status == TraceStageStatus::Failed {
                self.mark_failure(RouteTraceStageId::ResponseConversion);
            }
        }
    }

    pub fn finalize(&mut self, http_status: u16, log: &RouteTraceLog) {
        if self.committed {
            return;
        }
        self.trace.http_status = http_status;
        self.trace.ok = (200..400).contains(&http_status);
        self.trace.delivery.http_status = Some(http_status);
        if self.trace.delivery.status == TraceStageStatus::Pending {
            self.trace.delivery.status = if self.trace.delivery.stream {
                TraceStageStatus::Pending
            } else {
                TraceStageStatus::Ok
            };
            self.trace.delivery.completion = Some(
                if self.trace.delivery.stream {
                    "streaming"
                } else {
                    "response_returned"
                }
                .to_owned(),
            );
        }
        self.trace.latency_ms = Some(self.started.elapsed().as_millis() as u64);
        log_route_trace_finalized(&self.trace);
        log.push(self.trace.clone());
        self.committed = true;
    }

    fn mark_failure(&mut self, stage: RouteTraceStageId) {
        if self.trace.failure_stage.is_none() {
            self.trace.failure_stage = Some(stage);
        }
    }

    fn skip_after_local_auth(&mut self) {
        skip_if_pending(&mut self.trace.pool.status);
        skip_if_pending(&mut self.trace.conversion.status);
        skip_if_pending(&mut self.trace.upstream_request.status);
        skip_if_pending(&mut self.trace.upstream_auth.status);
        skip_if_pending(&mut self.trace.upstream.status);
        skip_if_pending(&mut self.trace.response_conversion.status);
        skip_if_pending(&mut self.trace.delivery.status);
    }

    fn skip_after_admission(&mut self) {
        skip_if_pending(&mut self.trace.route_resolution.status);
        self.skip_after_local_auth();
    }

    fn skip_after_pool(&mut self) {
        skip_if_pending(&mut self.trace.conversion.status);
        skip_if_pending(&mut self.trace.upstream_request.status);
        skip_if_pending(&mut self.trace.upstream_auth.status);
        skip_if_pending(&mut self.trace.upstream.status);
        skip_if_pending(&mut self.trace.response_conversion.status);
        skip_if_pending(&mut self.trace.delivery.status);
    }

    fn skip_after_conversion(&mut self) {
        skip_if_pending(&mut self.trace.upstream_request.status);
        skip_if_pending(&mut self.trace.upstream_auth.status);
        skip_if_pending(&mut self.trace.upstream.status);
        skip_if_pending(&mut self.trace.response_conversion.status);
        skip_if_pending(&mut self.trace.delivery.status);
    }
}

impl Drop for RouteTraceBuilder {
    fn drop(&mut self) {
        if !self.committed && self.trace.http_status > 0 {
            // Best-effort if caller forgot finalize after setting status.
        }
    }
}

/// One structured line per finished request so file logs share the monitor's
/// five stage names + request_id (no secrets / bodies).
fn log_route_trace_finalized(trace: &RouteRequestTrace) {
    tracing::info!(
        target: "core.adapter.route_trace",
        request_id = %trace.request_id,
        profile_id = trace.profile_id.as_deref().unwrap_or(""),
        method = %trace.method,
        path = %trace.path,
        http_status = trace.http_status,
        ok = trace.ok,
        latency_ms = trace.latency_ms.unwrap_or(0),
        local_auth = trace.local_auth.status.as_str(),
        pool = trace.pool.status.as_str(),
        conversion = trace.conversion.status.as_str(),
        upstream_request = trace.upstream_request.status.as_str(),
        upstream_auth = trace.upstream_auth.status.as_str(),
        upstream = trace.upstream.status.as_str(),
        failure_stage = trace.failure_stage.map(RouteTraceStageId::as_str).unwrap_or(""),
        "route trace finalized"
    );
}

pub fn trace_member(member: &PickedMember) -> RouteTraceMember {
    RouteTraceMember {
        label: if member.label.trim().is_empty() {
            member.source_id.clone()
        } else {
            member.label.clone()
        },
        source_kind: member.source_kind.clone(),
        source_id: member.source_id.clone(),
        ticket_id: if member.ticket_id.trim().is_empty() {
            None
        } else {
            Some(member.ticket_id.clone())
        },
        key_last4: secret_last4(&member.auth.token()),
    }
}

fn safe_attempt_message(code: &str) -> String {
    match code {
        "unauthorized" => "Upstream authorization failed.",
        "upstream_timeout" => "Timed out waiting for the upstream response.",
        "upstream_unavailable" => "Upstream transport unavailable.",
        "invalid_request" => "Upstream rejected the request.",
        _ => "Upstream attempt failed.",
    }
    .to_owned()
}

fn skip_if_pending(status: &mut TraceStageStatus) {
    if *status == TraceStageStatus::Pending {
        *status = TraceStageStatus::Skipped;
    }
}

fn legacy_trace_version() -> u8 {
    1
}

fn secret_last4(token: &str) -> Option<String> {
    let token = token.trim();
    (token.len() >= 4).then(|| token[token.len() - 4..].to_owned())
}

pub fn conversion_path_id(
    surface: DownstreamSurface,
    channel: UpstreamChannel,
    identity_relay: bool,
) -> String {
    if identity_relay {
        return "passthrough".to_owned();
    }
    let downstream = match surface {
        DownstreamSurface::Messages => "messages",
        DownstreamSurface::Responses => "responses",
        DownstreamSurface::ChatCompletions => "chat",
        DownstreamSurface::Models => "models",
    };
    let upstream = match channel {
        UpstreamChannel::OpenAiChat => "openai_chat",
        UpstreamChannel::Anthropic => "anthropic",
        UpstreamChannel::CodexResponses => "codex_responses",
        UpstreamChannel::Grok => "grok",
    };
    format!("{downstream}_to_{upstream}")
}

pub fn sanitize_upstream_url(url: &str) -> String {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let Ok(mut parsed) = reqwest::Url::parse(trimmed) else {
        return String::new();
    };
    let _ = parsed.set_username("");
    let _ = parsed.set_password(None);
    parsed.set_query(None);
    parsed.set_fragment(None);
    parsed.to_string()
}

fn now_unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_millis())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests;
