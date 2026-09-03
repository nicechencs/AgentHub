//! Credential-free route request traces for monitoring UI.
//!
//! Records localhost auth, pool member selection, conversion path/result,
//! upstream auth outcome, and upstream URL/account — never bodies or secrets.
//!
//! When persistence is enabled (desktop GUI), the last [`ROUTE_TRACE_CAP`]
//! traces per profile are flushed to a JSON ring under the data dir and
//! restored on process restart so Activity/monitor history survives crashes.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

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
}

impl TraceStageStatus {
    /// Stable lowercase label for structured logs / UI stage names.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Ok => "ok",
            Self::Failed => "failed",
            Self::Skipped => "skipped",
        }
    }
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    pub local_auth: RouteTraceLocalAuth,
    pub pool: RouteTracePool,
    pub conversion: RouteTraceConversion,
    pub upstream_auth: RouteTraceUpstreamAuth,
    pub upstream: RouteTraceUpstream,
    /// First failed stage id for list summaries: `local_auth` | `pool` | `conversion` | `upstream_auth` | `upstream`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_stage: Option<String>,
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
}

/// On-disk ring snapshot (credential-free). Restored on GUI/process restart.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RouteTracePersistFile {
    version: u32,
    #[serde(default)]
    by_profile: HashMap<String, Vec<RouteRequestTrace>>,
    #[serde(default)]
    unauthenticated: Vec<RouteRequestTrace>,
}

struct TraceStore {
    by_profile: HashMap<String, ProfileTraces>,
    unauthenticated: VecDeque<RouteRequestTrace>,
    pending_usage: HashMap<String, TraceUsagePatch>,
    /// When set, finalized traces are flushed as a capped JSON ring under the
    /// data dir so restart/crash does not wipe the monitoring list.
    persist_path: Option<PathBuf>,
}

impl Default for TraceStore {
    fn default() -> Self {
        Self {
            by_profile: HashMap::new(),
            unauthenticated: VecDeque::new(),
            pending_usage: HashMap::new(),
            persist_path: None,
        }
    }
}

impl RouteTraceLog {
    pub fn new() -> Self {
        Self::default()
    }

    /// Enable durable ring persistence at `path`. Loads last N traces if the
    /// file exists. Later calls are ignored (install once at process start).
    /// Best-effort: corrupt/missing files start empty; write failures never
    /// affect the request path.
    pub fn enable_persist(&self, path: PathBuf) {
        let Ok(mut store) = self.inner.lock() else {
            return;
        };
        if store.persist_path.is_some() {
            return;
        }
        let empty = store.by_profile.is_empty() && store.unauthenticated.is_empty();
        if empty {
            if let Some(snapshot) = load_persist_file(&path) {
                apply_snapshot(&mut store, snapshot);
            }
        }
        store.persist_path = Some(path);
    }

    pub fn push(&self, trace: RouteRequestTrace) {
        let Ok(mut store) = self.inner.lock() else {
            return;
        };
        let mut record = trace;
        if let Some(patch) = store.pending_usage.remove(&record.request_id) {
            apply_usage_patch(&mut record, patch);
        }
        if let Some(profile_id) = record.profile_id.as_deref().filter(|id| !id.is_empty()) {
            let entry = store.by_profile.entry(profile_id.to_owned()).or_default();
            entry.recent.push_front(record);
            entry.recent.truncate(ROUTE_TRACE_CAP);
        } else {
            store.unauthenticated.push_front(record);
            store.unauthenticated.truncate(ROUTE_TRACE_CAP);
        }
        flush_persist(&store);
    }

    pub fn patch_usage(
        &self,
        request_id: &str,
        ttft_ms: Option<u64>,
        input_tokens: Option<u64>,
        output_tokens: Option<u64>,
    ) {
        let Ok(mut store) = self.inner.lock() else {
            return;
        };
        let incoming = TraceUsagePatch {
            ttft_ms,
            input_tokens,
            output_tokens,
        };
        for entry in store.by_profile.values_mut() {
            if let Some(row) = entry
                .recent
                .iter_mut()
                .find(|row| row.request_id == request_id)
            {
                apply_usage_patch(row, incoming);
                flush_persist(&store);
                return;
            }
        }
        if let Some(row) = store
            .unauthenticated
            .iter_mut()
            .find(|row| row.request_id == request_id)
        {
            apply_usage_patch(row, incoming);
            flush_persist(&store);
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
            return;
        }
        if store.pending_usage.len() >= ROUTE_TRACE_CAP {
            return;
        }
        store.pending_usage.insert(request_id.to_owned(), incoming);
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
}

fn apply_snapshot(store: &mut TraceStore, snapshot: RouteTracePersistFile) {
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

fn load_persist_file(path: &Path) -> Option<RouteTracePersistFile> {
    let bytes = std::fs::read(path).ok()?;
    if bytes.is_empty() {
        return None;
    }
    match serde_json::from_slice::<RouteTracePersistFile>(&bytes) {
        Ok(snapshot) if snapshot.version == 1 => Some(snapshot),
        Ok(_) => {
            tracing::warn!(
                target: "core.adapter",
                path = %path.display(),
                "route trace persist file has unsupported version; starting empty"
            );
            None
        }
        Err(error) => {
            tracing::warn!(
                target: "core.adapter",
                path = %path.display(),
                error = %error,
                "route trace persist file unreadable; starting empty"
            );
            None
        }
    }
}

fn flush_persist(store: &TraceStore) {
    let Some(path) = store.persist_path.as_ref() else {
        return;
    };
    let snapshot = RouteTracePersistFile {
        version: 1,
        by_profile: store
            .by_profile
            .iter()
            .map(|(id, entry)| (id.clone(), entry.recent.iter().cloned().collect()))
            .collect(),
        unauthenticated: store.unauthenticated.iter().cloned().collect(),
    };
    let Ok(bytes) = serde_json::to_vec(&snapshot) else {
        return;
    };
    if let Err(error) = crate::utils::atomic::atomic_write(path, &bytes) {
        tracing::warn!(
            target: "core.adapter",
            path = %path.display(),
            error = %error,
            "failed to persist route traces"
        );
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
                local_auth: RouteTraceLocalAuth {
                    status: TraceStageStatus::Pending,
                    profile_id: None,
                    key_last4: None,
                    port: None,
                    code: None,
                    message: None,
                },
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
                failure_stage: None,
            },
            started: Instant::now(),
            committed: false,
        }
    }

    pub fn set_model(&mut self, model: Option<String>) {
        self.trace.model = model.filter(|value| !value.trim().is_empty());
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
        self.mark_failure("local_auth");
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
        self.mark_failure("local_endpoint");
        self.skip_after_local_auth();
        self.trace.conversion.code = Some(code.to_owned());
        self.trace.conversion.message = Some(message.to_owned());
    }

    pub fn pool_failed(&mut self, code: &str, message: &str) {
        self.trace.pool = RouteTracePool {
            status: TraceStageStatus::Failed,
            selected_member: None,
            attempts: self.trace.pool.attempts.clone(),
            code: Some(code.to_owned()),
            message: Some(message.to_owned()),
        };
        self.mark_failure("pool");
        self.skip_after_pool();
    }

    pub fn pool_selected(&mut self, member: &PickedMember, candidate: Option<&DispatchCandidate>) {
        let selected = trace_member(member);
        self.trace.pool.selected_member = Some(selected.clone());
        self.trace.pool.status = TraceStageStatus::Ok;
        if self.trace.pool.attempts.is_empty() {
            self.trace.pool.attempts.push(RouteTracePoolAttempt {
                member: selected,
                status: TraceStageStatus::Ok,
                code: None,
                message: None,
            });
        }
        if let Some(candidate) = candidate {
            if !candidate.upstream_model.trim().is_empty() {
                self.trace.upstream.upstream_model = Some(candidate.upstream_model.clone());
            }
        }
    }

    pub fn pool_attempt_failed(&mut self, member: &PickedMember, code: &str, message: &str) {
        self.trace.pool.attempts.push(RouteTracePoolAttempt {
            member: trace_member(member),
            status: TraceStageStatus::Failed,
            code: Some(code.to_owned()),
            message: Some(message.to_owned()),
        });
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
        self.mark_failure("conversion");
        self.skip_after_conversion();
    }

    pub fn upstream_auth_result(
        &mut self,
        ok: bool,
        http_status: Option<u16>,
        code: Option<&str>,
        message: Option<&str>,
    ) {
        self.trace.upstream_auth = RouteTraceUpstreamAuth {
            status: if ok {
                TraceStageStatus::Ok
            } else {
                TraceStageStatus::Failed
            },
            http_status,
            code: code.map(str::to_owned),
            message: message.map(str::to_owned),
        };
        if !ok {
            self.mark_failure("upstream_auth");
        }
    }

    pub fn upstream_success(
        &mut self,
        url: &str,
        member: &PickedMember,
        http_status: u16,
        upstream_model: Option<&str>,
    ) {
        let member = trace_member(member);
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
        message: &str,
    ) {
        self.trace.upstream = RouteTraceUpstream {
            status: TraceStageStatus::Failed,
            url: Some(sanitize_upstream_url(url)),
            member: Some(trace_member(member)),
            model: self.trace.model.clone(),
            upstream_model: self.trace.upstream.upstream_model.clone(),
            http_status,
            code: Some(code.to_owned()),
            message: Some(message.to_owned()),
        };
        self.mark_failure("upstream");
    }

    pub fn finalize(&mut self, http_status: u16, log: &RouteTraceLog) {
        if self.committed {
            return;
        }
        self.trace.http_status = http_status;
        self.trace.ok = (200..400).contains(&http_status);
        self.trace.latency_ms = Some(self.started.elapsed().as_millis() as u64);
        log_route_trace_finalized(&self.trace);
        log.push(self.trace.clone());
        self.committed = true;
    }

    fn mark_failure(&mut self, stage: &str) {
        if self.trace.failure_stage.is_none() {
            self.trace.failure_stage = Some(stage.to_owned());
        }
    }

    fn skip_after_local_auth(&mut self) {
        self.trace.pool.status = TraceStageStatus::Skipped;
        self.trace.conversion.status = TraceStageStatus::Skipped;
        self.trace.upstream_auth.status = TraceStageStatus::Skipped;
        self.trace.upstream.status = TraceStageStatus::Skipped;
    }

    fn skip_after_pool(&mut self) {
        self.trace.conversion.status = TraceStageStatus::Skipped;
        self.trace.upstream_auth.status = TraceStageStatus::Skipped;
        self.trace.upstream.status = TraceStageStatus::Skipped;
    }

    fn skip_after_conversion(&mut self) {
        self.trace.upstream_auth.status = TraceStageStatus::Skipped;
        self.trace.upstream.status = TraceStageStatus::Skipped;
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
        upstream_auth = trace.upstream_auth.status.as_str(),
        upstream = trace.upstream.status.as_str(),
        failure_stage = trace.failure_stage.as_deref().unwrap_or(""),
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
    trimmed
        .split(['?', '#'])
        .next()
        .unwrap_or(trimmed)
        .to_owned()
}

fn now_unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_millis())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests;
