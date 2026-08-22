use std::time::Instant;

use axum::extract::Request;
use axum::http::HeaderMap;
use axum::response::Response;
use serde_json::Value;
use tokio::sync::OwnedSemaphorePermit;

use super::http::{
    has_valid_local_auth, overloaded_response, read_request_json, reject_invalid_local_auth,
    stopping_response, ListenerState,
};
use super::surface::DownstreamSurface;

pub(super) struct AdmittedRequest {
    pub state: ListenerState,
    pub request_id: String,
    pub started: Instant,
    pub permit: OwnedSemaphorePermit,
    pub headers: HeaderMap,
    pub body: Value,
}

/// Auth (after 404), shutdown, semaphore, read JSON. Logs op from surface.op().
pub(super) async fn admit_conversation(
    state: ListenerState,
    request: Request,
    surface: DownstreamSurface,
) -> Result<AdmittedRequest, Response> {
    let request_id = uuid::Uuid::new_v4().to_string();
    let started = Instant::now();
    let op = surface.op();
    // Do this before extracting JSON. Axum's Json extractor would otherwise read a potentially
    // slow or oversized body for an unauthenticated peer.
    if !has_valid_local_auth(request.headers(), &state.local_token) {
        return Err(reject_invalid_local_auth(
            &state,
            op,
            Some((&request_id, started)),
        ));
    }
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
    })
}
