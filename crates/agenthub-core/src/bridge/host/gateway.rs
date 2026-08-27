//! In-process loopback gateway: sockets, edges, and local-bearer lookup.
//!
//! One [`Gateway`] per [`super::BridgeRuntimeHost`]. Local bearer is the only
//! request-path identity; AccountPicker hangs off [`EdgeState`].

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use axum::http::{header, HeaderMap};
use reqwest::Url;
use tokio::sync::{watch, Semaphore};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use thiserror::Error;

use crate::bridge::account::{AccountPicker, PickedMember};
use crate::bridge::auth_reload::AuthReloadCoordinator;
use crate::bridge::grok_cli::GrokReasoningReplay;
use crate::bridge::route_index::DispatchCandidate;
use crate::bridge::runtime::{
    BridgeRuntimeState, BridgeRuntimeStatus, BridgeStartSpec, BridgeUpstreamStatus,
};

use super::surface::DownstreamSurface;
use super::{MAX_IN_FLIGHT_REQUESTS_PER_PROFILE, UPSTREAM_CONNECT_TIMEOUT};

#[derive(Debug, Error)]
pub enum BridgeHostError {
    #[error("bridge profile id must not be empty")]
    EmptyProfileId,
    #[error("bridge local bearer token must not be empty")]
    EmptyLocalToken,
    #[error("bridge upstream base URL must not be empty")]
    EmptyUpstreamUrl,
    #[error("bridge upstream base URL is invalid or not permitted")]
    InvalidUpstreamUrl,
    #[error("bridge upstream bearer token must not be empty")]
    EmptyUpstreamToken,
    #[error("bridge host is shutting down and cannot start listeners")]
    HostClosing,
    #[error("bridge instance already runs with a different configuration")]
    ConflictingStart,
    #[error("bridge instance is stopping")]
    Stopping,
    #[error("bridge instance is not running")]
    NotRunning,
    #[error("bridge host state is unavailable")]
    StatePoisoned,
    #[error("failed to bind loopback bridge listener: {0}")]
    Bind(#[from] std::io::Error),
    #[error("gateway port must be a non-zero loopback port")]
    InvalidGatewayPort,
}

/// A tiny cancellation-safe completion primitive. The cleanup task, rather than an RPC caller,
/// owns listener JoinHandles; callers can therefore be dropped without abandoning a stop.
pub struct CleanupCompletion {
    /// `watch` retains the terminal result. A waiter that subscribes after `finish`, or while
    /// `finish` races registration, observes the latest value instead of depending on an edge-
    /// triggered notification that can be lost.
    result: watch::Sender<Option<bool>>,
}

impl CleanupCompletion {
    pub fn new() -> Self {
        let (result, _receiver) = watch::channel(None);
        Self { result }
    }

    pub fn finish(&self, failed: bool) {
        self.result.send_replace(Some(failed));
    }

    pub async fn wait(&self) -> Result<(), BridgeHostError> {
        let mut result = self.result.subscribe();
        loop {
            if let Some(failed) = *result.borrow_and_update() {
                return if failed {
                    Err(BridgeHostError::StatePoisoned)
                } else {
                    Ok(())
                };
            }
            result
                .changed()
                .await
                .map_err(|_| BridgeHostError::StatePoisoned)?;
        }
    }
}

/// Shared axum state. All loopback sockets serve this table.
#[derive(Clone)]
pub(super) struct Gateway {
    pub(super) registry: Arc<Mutex<GatewayRegistry>>,
    pub(super) auth_reload: AuthReloadCoordinator,
}

pub(super) struct GatewayRegistry {
    pub(super) sockets: HashMap<u16, SocketInstance>,
    pub(super) runtimes: HashMap<String, EdgeRuntime>,
    pub(super) primary_port: Option<u16>,
}

pub(super) struct SocketInstance {
    pub(super) accept_shutdown: CancellationToken,
    pub(super) task: Option<JoinHandle<Result<(), ()>>>,
}

pub(super) struct EdgeRuntime {
    pub(super) spec: BridgeStartSpec,
    pub(super) cited_port: u16,
    pub(super) started_at: std::time::SystemTime,
    pub(super) lifecycle: BridgeRuntimeState,
    pub(super) state: EdgeState,
    pub(super) stop_completion: Option<Arc<CleanupCompletion>>,
}

/// Per-profile edge. AccountPicker (C2) is the member poller; A4 keeps a single
/// lead [`crate::bridge::ResolvedAuth`] on `upstream.auth` for single-member
/// in-place 401 reload.
#[derive(Clone)]
pub(super) struct EdgeState {
    pub(super) profile_id: Arc<str>,
    pub(super) local_token: Arc<str>,
    pub(super) upstream: crate::bridge::runtime::BridgeUpstreamConfig,
    pub(super) upstream_url: Url,
    pub(super) client: reqwest::Client,
    pub(super) force_shutdown: CancellationToken,
    /// New requests 503 once stop begins; in-flight keep going until drain timeout.
    pub(super) stopping: Arc<AtomicBool>,
    pub(super) admission: Arc<Semaphore>,
    pub(super) observed_upstream: Arc<Mutex<BridgeUpstreamStatus>>,
    pub(super) grok_replay: Arc<GrokReasoningReplay>,
    pub(super) listed_models: Arc<[String]>,
    // Written from the spec; read path currently unused (kept for tests).
    #[allow(dead_code)]
    pub(super) reload_upstream_auth: Option<crate::bridge::UpstreamAuthReload>,
    pub(super) account_picker: AccountPicker,
    pub(super) mapping_source: Option<crate::models::AdapterSourceProduct>,
    pub(super) mapping_target: Option<crate::models::AgentId>,
    pub(super) custom_openai: bool,
    pub(super) route_index: Option<crate::bridge::route_index::EffectiveRouteIndex>,
    pub(super) auth_reload: AuthReloadCoordinator,
    pub(super) codex_ingress_grok_upstream: bool,
    pub(super) grok_ingress_codex_upstream: bool,
    pub(super) continuations: std::sync::Arc<super::continuation::ContinuationBindings>,
}

pub(super) enum GatewayAuthError {
    Unauthorized,
    Stopping,
    Poisoned,
}

impl Gateway {
    pub(super) fn new() -> Self {
        Self {
            registry: Arc::new(Mutex::new(GatewayRegistry {
                sockets: HashMap::new(),
                runtimes: HashMap::new(),
                primary_port: None,
            })),
            auth_reload: AuthReloadCoordinator::new(),
        }
    }

    pub(super) fn lock(&self) -> Result<MutexGuard<'_, GatewayRegistry>, BridgeHostError> {
        self.registry
            .lock()
            .map_err(|_| BridgeHostError::StatePoisoned)
    }

    /// Constant-time compare against every live local bearer. A miss is 401;
    /// a hit whose edge is draining is 503. Does not reveal whether the path
    /// would have been 404.
    pub(super) fn authenticate(&self, headers: &HeaderMap) -> Result<EdgeState, GatewayAuthError> {
        let presented = presented_local_token(headers);
        let registry = self
            .registry
            .lock()
            .map_err(|_| GatewayAuthError::Poisoned)?;
        let presented_bytes = presented.unwrap_or("");
        let mut matched: Option<EdgeState> = None;
        for runtime in registry.runtimes.values() {
            if constant_time_eq(
                presented_bytes.as_bytes(),
                runtime.state.local_token.as_bytes(),
            ) {
                matched = Some(runtime.state.clone());
            }
        }
        let Some(edge) = matched else {
            return Err(GatewayAuthError::Unauthorized);
        };
        if edge.stopping.load(Ordering::SeqCst) || edge.force_shutdown.is_cancelled() {
            return Err(GatewayAuthError::Stopping);
        }
        Ok(edge)
    }

    /// List models for GET /v1/models. When a running custom OpenAI-compat
    /// backup exists for the same target, include stealth/ox-alpha.
    pub(super) fn listed_models_with_backup(&self, state: &EdgeState) -> Vec<String> {
        let include = state.custom_openai || self.has_running_custom_openai_backup(state);
        crate::models::with_openrouter_backup_model(state.listed_models.to_vec(), include)
    }

    fn has_running_custom_openai_backup(&self, lead: &EdgeState) -> bool {
        let Ok(registry) = self.lock() else {
            return false;
        };
        registry.runtimes.values().any(|runtime| {
            let state = &runtime.state;
            state.custom_openai
                && state.mapping_target == lead.mapping_target
                && !state.stopping.load(Ordering::SeqCst)
                && !state.force_shutdown.is_cancelled()
        })
    }

    /// After the body model is known: if the lead mapping misses and another
    /// running edge can serve it, switch for this request only.
    pub(super) fn switch_edge_for_model(
        &self,
        lead: &EdgeState,
        model: &str,
    ) -> ModelSwitchOutcome {
        use crate::models::{decide_model_switch, ModelSwitchCandidate, ModelSwitchDecision};
        let Some(source) = lead.mapping_source else {
            return ModelSwitchOutcome::Stay;
        };
        let Some(target) = lead.mapping_target else {
            return ModelSwitchOutcome::Stay;
        };
        let Ok(registry) = self.registry.lock() else {
            return ModelSwitchOutcome::Unavailable;
        };
        let lead_surface = lead.upstream.local_surface;
        let lead_candidate = ModelSwitchCandidate {
            profile_id: lead.profile_id.to_string(),
            source,
            target,
            custom_openai_compat: lead.custom_openai,
            same_surface: true,
            running: true,
            listed_models: lead.listed_models.to_vec(),
        };
        let mut others = Vec::new();
        let mut other_states = std::collections::HashMap::new();
        for runtime in registry.runtimes.values() {
            let state = &runtime.state;
            if state.profile_id.as_ref() == lead.profile_id.as_ref() {
                continue;
            }
            let Some(src) = state.mapping_source else {
                continue;
            };
            let Some(tgt) = state.mapping_target else {
                continue;
            };
            let running =
                !state.stopping.load(Ordering::SeqCst) && !state.force_shutdown.is_cancelled();
            others.push(ModelSwitchCandidate {
                profile_id: state.profile_id.to_string(),
                source: src,
                target: tgt,
                custom_openai_compat: state.custom_openai,
                same_surface: state.upstream.local_surface == lead_surface,
                running,
                listed_models: state.listed_models.to_vec(),
            });
            other_states.insert(state.profile_id.to_string(), state.clone());
        }
        match decide_model_switch(&lead_candidate, model, &others) {
            ModelSwitchDecision::Stay => ModelSwitchOutcome::Stay,
            ModelSwitchDecision::SwitchTo { profile_id } => other_states
                .remove(&profile_id)
                .map(ModelSwitchOutcome::Switched)
                .unwrap_or(ModelSwitchOutcome::Unavailable),
            ModelSwitchDecision::Unavailable => ModelSwitchOutcome::Unavailable,
        }
    }
}

pub(super) enum ModelSwitchOutcome {
    Stay,
    Switched(EdgeState),
    Unavailable,
}

impl GatewayRegistry {
    pub(super) fn sockets_live(&self) -> bool {
        self.sockets
            .values()
            .any(|socket| socket.task.as_ref().is_some_and(|task| !task.is_finished()))
    }

    pub(super) fn token_owned_by_other(&self, spec: &BridgeStartSpec) -> bool {
        self.runtimes.values().any(|runtime| {
            runtime.spec.profile_id != spec.profile_id
                && runtime.state.local_token.as_ref() == spec.local_token
        })
    }

    pub(super) fn remaining_citers(&self, port: u16, except_profile: Option<&str>) -> usize {
        self.runtimes
            .values()
            .filter(|runtime| {
                runtime.cited_port == port
                    && except_profile.is_none_or(|id| runtime.spec.profile_id != id)
            })
            .count()
    }
}

impl EdgeRuntime {
    pub(super) fn status(&self, sockets_live: bool) -> BridgeRuntimeStatus {
        let state = match self.lifecycle {
            BridgeRuntimeState::Stopping => BridgeRuntimeState::Stopping,
            BridgeRuntimeState::Running | BridgeRuntimeState::Starting if !sockets_live => {
                BridgeRuntimeState::Error
            }
            state => state,
        };
        BridgeRuntimeStatus {
            profile_id: self.spec.profile_id.clone(),
            port: self.cited_port,
            running: matches!(
                state,
                BridgeRuntimeState::Running | BridgeRuntimeState::Degraded
            ),
            started_at: self.started_at,
            source_connection_id: self.spec.upstream.source_connection_id.clone(),
            state,
            upstream_status: self.public_upstream_status(state),
        }
    }

    pub(super) fn stopped_status(&self) -> BridgeRuntimeStatus {
        BridgeRuntimeStatus {
            profile_id: self.spec.profile_id.clone(),
            port: self.cited_port,
            running: false,
            started_at: self.started_at,
            source_connection_id: self.spec.upstream.source_connection_id.clone(),
            state: BridgeRuntimeState::Stopped,
            upstream_status: BridgeUpstreamStatus::Stopped,
        }
    }

    fn public_upstream_status(&self, state: BridgeRuntimeState) -> BridgeUpstreamStatus {
        match state {
            BridgeRuntimeState::Stopped | BridgeRuntimeState::Stopping => {
                BridgeUpstreamStatus::Stopped
            }
            BridgeRuntimeState::Error => BridgeUpstreamStatus::Unavailable,
            _ => self.state.observed_upstream(),
        }
    }
}

impl EdgeState {
    pub(super) fn from_spec(
        spec: &BridgeStartSpec,
        upstream_url: Url,
        force_shutdown: CancellationToken,
        auth_reload: AuthReloadCoordinator,
    ) -> Self {
        let state = Self {
            profile_id: Arc::from(spec.profile_id.clone()),
            local_token: Arc::from(spec.local_token.clone()),
            upstream: spec.upstream.clone(),
            upstream_url,
            client: reqwest::Client::builder()
                // Streaming requests deliberately have no reqwest-wide total timeout: a healthy
                // long-running SSE response is bounded by per-chunk idle time instead.
                .connect_timeout(UPSTREAM_CONNECT_TIMEOUT)
                .build()
                .expect("reqwest client builder uses static valid settings"),
            force_shutdown,
            stopping: Arc::new(AtomicBool::new(false)),
            admission: Arc::new(Semaphore::new(MAX_IN_FLIGHT_REQUESTS_PER_PROFILE)),
            observed_upstream: Arc::new(Mutex::new(BridgeUpstreamStatus::Unknown)),
            grok_replay: Arc::new(GrokReasoningReplay::new()),
            listed_models: spec.listed_models.clone().into(),
            reload_upstream_auth: spec.reload_upstream_auth.clone(),
            account_picker: spec.account_picker(),
            mapping_source: spec.mapping_source,
            mapping_target: spec.mapping_target,
            custom_openai: spec.custom_openai,
            route_index: spec.route_index.clone(),
            auth_reload,
            codex_ingress_grok_upstream: spec.codex_ingress_grok_upstream,
            grok_ingress_codex_upstream: spec.grok_ingress_codex_upstream,
            continuations: std::sync::Arc::new(super::continuation::ContinuationBindings::new()),
        };
        // stop+start is how production rotates a login; host-wide 401
        // isolation must not outlive the old picker.
        state.admit_started_members();
        state
    }

    fn admit_started_members(&self) {
        for member in self.account_picker.members() {
            if member.is_eligible() {
                self.auth_reload
                    .clear_isolated(&member.authorization_fingerprint());
            }
        }
    }

    /// Indexed pick: shrink candidates, skip isolated / cooling / this-request exclusions.
    /// Mixed-provider indexes pick a lane first, then a member inside that lane.
    pub(super) fn pick_v2(
        &self,
        candidates: &[DispatchCandidate],
        model: &str,
        extra_excluded: &[String],
        affinity_key: Option<&str>,
    ) -> Option<PickedMember> {
        self.pick_v2_in_lane(candidates, model, extra_excluded, None, affinity_key)
    }

    pub(super) fn pick_v2_in_lane(
        &self,
        candidates: &[DispatchCandidate],
        model: &str,
        extra_excluded: &[String],
        last_member_id: Option<&str>,
        affinity_key: Option<&str>,
    ) -> Option<PickedMember> {
        let mut excluded = extra_excluded.to_vec();
        excluded.extend(self.account_picker.cooldown_exclusions(model));
        for member in self.account_picker.members() {
            if self
                .auth_reload
                .is_isolated(&member.authorization_fingerprint())
            {
                excluded.push(member.source_id.clone());
                excluded.push(member.ticket_id.clone());
            }
        }
        let lane_candidates = match &self.route_index {
            Some(index) => index.schedule_lane(
                DownstreamSurface::endpoint_key(self.upstream.local_surface),
                model,
                candidates,
                &excluded,
                last_member_id,
            ),
            None => candidates.to_vec(),
        };
        self.account_picker
            .pick_from_candidates(&lane_candidates, affinity_key, &excluded)
    }

    pub(super) fn affinity_key_for(
        &self,
        body: &serde_json::Value,
        headers: &HeaderMap,
    ) -> Option<String> {
        let session = super::continuation::session_identifier(body, headers)?;
        let route_id = self
            .route_index
            .as_ref()
            .map(|index| index.route_id.as_str())
            .unwrap_or(self.profile_id.as_ref());
        let dialect = self
            .mapping_target
            .map(crate::models::RouteDownstreamDialect::for_agent)
            .unwrap_or(crate::models::RouteDownstreamDialect::Generic)
            .as_str();
        Some(crate::bridge::account::route_scoped_affinity_key(
            route_id, dialect, &session,
        ))
    }

    pub(super) fn isolate_authorization(&self, member: &PickedMember) {
        self.account_picker.isolate(&member.source_id);
        self.auth_reload
            .isolate(&member.authorization_fingerprint());
    }

    pub(super) fn observed_upstream(&self) -> BridgeUpstreamStatus {
        self.observed_upstream
            .lock()
            .map(|status| *status)
            .unwrap_or(BridgeUpstreamStatus::Unavailable)
    }

    pub(super) fn record_upstream(&self, status: BridgeUpstreamStatus) {
        if let Ok(mut observed) = self.observed_upstream.lock() {
            *observed = status;
        }
    }

    pub(super) fn record_upstream_success(&self) {
        self.record_upstream(BridgeUpstreamStatus::Connected);
    }

    pub(super) fn record_upstream_failure(&self) {
        self.record_upstream(BridgeUpstreamStatus::Degraded);
    }
}

fn presented_local_token(headers: &HeaderMap) -> Option<&str> {
    let bearer = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "));
    let api_key = headers
        .get("x-api-key")
        .and_then(|value| value.to_str().ok());
    bearer.or(api_key)
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    let mut mismatch = left.len() ^ right.len();
    let width = left.len().max(right.len());
    for index in 0..width {
        mismatch |= usize::from(*left.get(index).unwrap_or(&0) ^ *right.get(index).unwrap_or(&0));
    }
    mismatch == 0
}
