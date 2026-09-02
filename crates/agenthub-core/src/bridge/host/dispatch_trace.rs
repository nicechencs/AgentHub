//! Helpers wiring route traces into dispatch return paths.

use axum::response::Response;

use super::route_trace::{RouteTraceBuilder, RouteTraceLog};

pub(super) fn trace_response(
    trace: &mut RouteTraceBuilder,
    log: &RouteTraceLog,
    response: Response,
) -> Response {
    trace.finalize(response.status().as_u16(), log);
    response
}
