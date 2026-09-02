//! Per-profile edge: start-time config plus request-path state.
//!
//! Start-time fields (profile / token / url / mapping / listed_models /
//! protocol flags / route_index) are read-only after start. Request state
//! (stopping / admission / picker / continuations / grok_replay /
//! observed_upstream / auth_reload) is not mixed into the socket HashMap.

use std::collections::HashSet;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use axum::http::HeaderMap;
use reqwest::Url;
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;

use crate::bridge::account::{AccountPicker, PickedMember};
use crate::bridge::auth_reload::AuthReloadCoordinator;
use crate::bridge::grok_cli::GrokReasoningReplay;
use crate::bridge::route_index::DispatchCandidate;
use crate::bridge::runtime::{BridgeStartSpec, BridgeUpstreamStatus, DownstreamResponsesProfile};

use super::super::surface::DownstreamSurface;
use super::super::{MAX_IN_FLIGHT_REQUESTS_PER_PROFILE, UPSTREAM_CONNECT_TIMEOUT};

/// Per-profile edge. AccountPicker (C2) is the member poller; A4 keeps a single
/// lead [`crate::bridge::ResolvedAuth`] on `upstream.auth` for single-member
/// in-place 401 reload.
#[derive(Clone)]
pub(in crate::bridge::host) struct EdgeState {
    pub(in crate::bridge::host) profile_id: Arc<str>,
    pub(in crate::bridge::host) local_token: Arc<str>,
    pub(in crate::bridge::host) upstream: crate::bridge::runtime::BridgeUpstreamConfig,
    pub(in crate::bridge::host) upstream_url: Url,
    pub(in crate::bridge::host) client: reqwest::Client,
    pub(in crate::bridge::host) force_shutdown: CancellationToken,
    /// New requests 503 once stop begins; in-flight keep going until drain timeout.
    pub(in crate::bridge::host) stopping: Arc<AtomicBool>,
    pub(in crate::bridge::host) admission: Arc<Semaphore>,
    pub(in crate::bridge::host) observed_upstream: Arc<Mutex<BridgeUpstreamStatus>>,
    pub(in crate::bridge::host) grok_replay: Arc<GrokReasoningReplay>,
    pub(in crate::bridge::host) listed_models: Arc<[String]>,
    // Written from the spec; read path currently unused (kept for tests).
    #[allow(dead_code)]
    pub(in crate::bridge::host) reload_upstream_auth: Option<crate::bridge::UpstreamAuthReload>,
    pub(in crate::bridge::host) account_picker: AccountPicker,
    pub(in crate::bridge::host) mapping_source: Option<crate::models::AdapterSourceProduct>,
    pub(in crate::bridge::host) mapping_target: Option<crate::models::AgentId>,
    pub(in crate::bridge::host) downstream_responses_profile: Option<DownstreamResponsesProfile>,
    pub(in crate::bridge::host) custom_openai: bool,
    pub(in crate::bridge::host) route_index:
        Option<crate::bridge::route_index::EffectiveRouteIndex>,
    pub(in crate::bridge::host) auth_reload: AuthReloadCoordinator,
    pub(in crate::bridge::host) codex_ingress_grok_upstream: bool,
    pub(in crate::bridge::host) grok_ingress_codex_upstream: bool,
    pub(in crate::bridge::host) continuations:
        std::sync::Arc<super::super::continuation::ContinuationBindings>,
    /// Runtime evidence: (member_id, public_model) pairs that 403/404 entitlement
    /// marked unsupported. Availability only; next `/models` omits the id when
    /// no other candidate remains.
    pub(in crate::bridge::host) member_model_denials: Arc<Mutex<HashSet<(String, String)>>>,
    /// Host-level optional gateway usage spool (clone of the shared slot).
    pub(in crate::bridge::host) usage_spool: crate::bridge::usage_capture::UsageSpoolSlot,
    pub(in crate::bridge::host) route_traces: crate::bridge::host::RouteTraceLog,
}

impl EdgeState {
    pub(in crate::bridge::host) fn from_spec(
        spec: &BridgeStartSpec,
        upstream_url: Url,
        force_shutdown: CancellationToken,
        auth_reload: AuthReloadCoordinator,
        usage_spool: crate::bridge::usage_capture::UsageSpoolSlot,
        route_traces: crate::bridge::host::RouteTraceLog,
    ) -> Self {
        let state = Self {
            profile_id: Arc::from(spec.profile_id.clone()),
            local_token: Arc::from(spec.local_token.clone()),
            upstream: spec.upstream.clone(),
            upstream_url,
            client: reqwest::Client::builder()
                // Streaming requests deliberately have no reqwest-wide total timeout: a healthy
                // long-running SSE response is bounded by per-chunk idle time instead.
                // Never follow an upstream redirect: the transport may attach an API key
                // header, and reqwest must not carry that secret to a different origin.
                .redirect(reqwest::redirect::Policy::none())
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
            downstream_responses_profile: spec.downstream_responses_profile,
            custom_openai: spec.custom_openai,
            route_index: spec.route_index.clone(),
            auth_reload,
            codex_ingress_grok_upstream: spec.codex_ingress_grok_upstream,
            grok_ingress_codex_upstream: spec.grok_ingress_codex_upstream,
            continuations: std::sync::Arc::new(
                super::super::continuation::ContinuationBindings::new(),
            ),
            member_model_denials: Arc::new(Mutex::new(HashSet::new())),
            usage_spool,
            route_traces,
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
    pub(in crate::bridge::host) fn pick_v2(
        &self,
        candidates: &[DispatchCandidate],
        model: &str,
        extra_excluded: &[String],
        affinity_key: Option<&str>,
    ) -> Option<PickedMember> {
        self.pick_v2_in_lane(candidates, model, extra_excluded, None, affinity_key)
    }

    pub(in crate::bridge::host) fn pick_v2_in_lane(
        &self,
        candidates: &[DispatchCandidate],
        model: &str,
        extra_excluded: &[String],
        last_member_id: Option<&str>,
        affinity_key: Option<&str>,
    ) -> Option<PickedMember> {
        let mut excluded = extra_excluded.to_vec();
        excluded.extend(self.account_picker.cooldown_exclusions(model));
        excluded.extend(self.denied_member_ids(model));
        for member in self.account_picker.members() {
            if self
                .auth_reload
                .is_isolated(&member.authorization_fingerprint())
            {
                excluded.push(member.source_id.clone());
                excluded.push(member.ticket_id.clone());
            }
        }
        if let Some(picked) = self
            .account_picker
            .try_sticky(candidates, affinity_key, &excluded)
        {
            return Some(picked);
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

    pub(in crate::bridge::host) fn affinity_key_for(
        &self,
        body: &serde_json::Value,
        headers: &HeaderMap,
    ) -> Option<String> {
        let session = super::super::continuation::session_identifier(body, headers)?;
        let route_id = self
            .route_index
            .as_ref()
            .map(|index| index.route_id.as_str())
            .unwrap_or(self.profile_id.as_ref());
        let dialect = match self.upstream.local_surface {
            crate::bridge::runtime::BridgeLocalSurface::Responses => self
                .downstream_responses_profile
                .map(|profile| profile.dialect.as_str())
                .unwrap_or("generic"),
            _ => self
                .mapping_target
                .map(crate::models::RouteDownstreamDialect::for_agent)
                .unwrap_or(crate::models::RouteDownstreamDialect::Generic)
                .as_str(),
        };
        Some(crate::bridge::account::route_scoped_affinity_key(
            route_id, dialect, &session,
        ))
    }

    pub(in crate::bridge::host) fn deny_member_model(&self, member_id: &str, public_model: &str) {
        let member_id = member_id.trim();
        let public_model = public_model.trim();
        if member_id.is_empty() || public_model.is_empty() {
            return;
        }
        if let Ok(mut guard) = self.member_model_denials.lock() {
            guard.insert((member_id.to_owned(), public_model.to_owned()));
        }
    }

    pub(in crate::bridge::host) fn denied_member_ids(&self, public_model: &str) -> Vec<String> {
        let Ok(guard) = self.member_model_denials.lock() else {
            return Vec::new();
        };
        guard
            .iter()
            .filter(|(_, model)| model == public_model)
            .map(|(member_id, _)| member_id.clone())
            .collect()
    }

    pub(in crate::bridge::host) fn models_after_denials(&self, listed: Vec<String>) -> Vec<String> {
        let Some(index) = &self.route_index else {
            return listed;
        };
        let endpoint = DownstreamSurface::endpoint_key(self.upstream.local_surface);
        listed
            .into_iter()
            .filter(|model| {
                let Ok(candidates) = index.resolve(endpoint, model) else {
                    return false;
                };
                let denied = self.denied_member_ids(model);
                candidates
                    .iter()
                    .any(|candidate| !denied.iter().any(|id| id == &candidate.member_id))
            })
            .collect()
    }

    pub(in crate::bridge::host) fn isolate_authorization(&self, member: &PickedMember) {
        self.account_picker.isolate(&member.source_id);
        self.auth_reload
            .isolate(&member.authorization_fingerprint());
    }

    pub(in crate::bridge::host) fn observed_upstream(&self) -> BridgeUpstreamStatus {
        self.observed_upstream
            .lock()
            .map(|status| *status)
            .unwrap_or(BridgeUpstreamStatus::Unavailable)
    }

    pub(in crate::bridge::host) fn record_upstream(&self, status: BridgeUpstreamStatus) {
        if let Ok(mut observed) = self.observed_upstream.lock() {
            *observed = status;
        }
    }

    pub(in crate::bridge::host) fn record_upstream_success(&self) {
        self.record_upstream(BridgeUpstreamStatus::Connected);
    }

    pub(in crate::bridge::host) fn record_upstream_failure(&self) {
        self.record_upstream(BridgeUpstreamStatus::Degraded);
    }
}
