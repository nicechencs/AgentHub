//! In-process loopback gateway: sockets, edges, and local-bearer lookup.
//!
//! One [`Gateway`] per [`super::BridgeRuntimeHost`]. Local bearer is the only
//! request-path identity; AccountPicker hangs off [`EdgeState`].
//!
//! Socket / runtime tables live in [`registry`]; per-profile config and request
//! state live in [`edge`]. Listener start/stop stays on [`super::BridgeRuntimeHost`].

mod edge;
mod registry;

use std::sync::{Arc, Mutex, MutexGuard};

use axum::http::{header, HeaderMap};
use std::sync::atomic::Ordering;

use thiserror::Error;
use tokio::sync::watch;

use crate::bridge::auth_reload::AuthReloadCoordinator;

pub(super) use edge::EdgeState;
pub(super) use registry::{EdgeRuntime, GatewayRegistry, SocketInstance};

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

pub(super) enum GatewayAuthError {
    Unauthorized,
    Stopping,
    Poisoned,
}

impl Gateway {
    pub(super) fn new() -> Self {
        Self {
            registry: Arc::new(Mutex::new(GatewayRegistry::new())),
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
        use crate::bridge::{decide_model_switch, ModelSwitchCandidate, ModelSwitchDecision};
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
