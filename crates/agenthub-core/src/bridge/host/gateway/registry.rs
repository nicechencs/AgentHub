//! Socket and runtime tables owned by [`super::Gateway`].
//!
//! Request-body parse and vendor Chat rendering do not live here.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::bridge::runtime::{
    BridgeRuntimeState, BridgeRuntimeStatus, BridgeStartSpec, BridgeUpstreamStatus,
};

use super::edge::EdgeState;
use super::CleanupCompletion;

pub(in crate::bridge::host) struct GatewayRegistry {
    pub(in crate::bridge::host) sockets: HashMap<u16, SocketInstance>,
    pub(in crate::bridge::host) runtimes: HashMap<String, EdgeRuntime>,
    pub(in crate::bridge::host) primary_port: Option<u16>,
}

pub(in crate::bridge::host) struct SocketInstance {
    pub(in crate::bridge::host) accept_shutdown: CancellationToken,
    pub(in crate::bridge::host) task: Option<JoinHandle<Result<(), ()>>>,
}

pub(in crate::bridge::host) struct EdgeRuntime {
    pub(in crate::bridge::host) spec: BridgeStartSpec,
    pub(in crate::bridge::host) cited_port: u16,
    pub(in crate::bridge::host) started_at: std::time::SystemTime,
    pub(in crate::bridge::host) lifecycle: BridgeRuntimeState,
    pub(in crate::bridge::host) state: EdgeState,
    pub(in crate::bridge::host) stop_completion: Option<Arc<CleanupCompletion>>,
}

impl GatewayRegistry {
    pub(in crate::bridge::host) fn new() -> Self {
        Self {
            sockets: HashMap::new(),
            runtimes: HashMap::new(),
            primary_port: None,
        }
    }

    pub(in crate::bridge::host) fn sockets_live(&self) -> bool {
        self.sockets
            .values()
            .any(|socket| socket.task.as_ref().is_some_and(|task| !task.is_finished()))
    }

    pub(in crate::bridge::host) fn token_owned_by_other(&self, spec: &BridgeStartSpec) -> bool {
        self.runtimes.values().any(|runtime| {
            runtime.spec.profile_id != spec.profile_id
                && runtime.state.local_token.as_ref() == spec.local_token
        })
    }

    pub(in crate::bridge::host) fn remaining_citers(
        &self,
        port: u16,
        except_profile: Option<&str>,
    ) -> usize {
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
    pub(in crate::bridge::host) fn status(&self, sockets_live: bool) -> BridgeRuntimeStatus {
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
            source_id: self.spec.upstream.source_id.clone(),
            state,
            upstream_status: self.public_upstream_status(state),
        }
    }

    pub(in crate::bridge::host) fn stopped_status(&self) -> BridgeRuntimeStatus {
        BridgeRuntimeStatus {
            profile_id: self.spec.profile_id.clone(),
            port: self.cited_port,
            running: false,
            started_at: self.started_at,
            source_id: self.spec.upstream.source_id.clone(),
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
