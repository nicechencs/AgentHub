use std::time::Instant;

use axum::extract::Request;
use axum::http::HeaderMap;
use axum::response::Response;
use serde_json::Value;
use tokio::sync::OwnedSemaphorePermit;

use super::http::{overloaded_response, read_request_json, stopping_response, EdgeState};
use super::surface::DownstreamSurface;
use crate::bridge::account::PickedMember;

pub(super) struct AdmittedRequest {
    pub state: EdgeState,
    pub request_id: String,
    pub started: Instant,
    pub permit: OwnedSemaphorePermit,
    pub headers: HeaderMap,
    pub body: Value,
    pub member: Option<PickedMember>,
    pub affinity_key: Option<String>,
}

/// Shutdown, per-edge semaphore, read JSON. Auth has already bound the edge.
pub(super) async fn admit_conversation(
    state: EdgeState,
    request: Request,
    surface: DownstreamSurface,
    request_id: String,
    started: Instant,
) -> Result<AdmittedRequest, Response> {
    let op = surface.op();
    if state.force_shutdown.is_cancelled() {
        return Err(stopping_response());
    }
    let permit = match state.admission.clone().try_acquire_owned() {
        Ok(permit) => permit,
        Err(_) => {
            tracing::warn!(target: "core.adapter", profile_id = %state.profile_id, request_id = %request_id, op, code = "overloaded", status = 429_u16, elapsed_ms = started.elapsed().as_millis() as u64, "bridge profile is at request capacity");
            return Err(overloaded_response());
        }
    };
    let headers = request.headers().clone();
    let body = match read_request_json(request).await {
        Ok(body) => body,
        Err(response) => return Err(response),
    };
    Ok(AdmittedRequest {
        state,
        request_id,
        started,
        permit,
        headers,
        body,
        member: None,
        affinity_key: None,
    })
}
