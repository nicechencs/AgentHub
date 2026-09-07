//! AgentHub-owned Kiro HTTP client.
//!
//! Community protocol against CodeWhisperer / Kiro hosts — **not** an official
//! public REST. Builder ID login + `KIRO_API_KEY` first; enterprise `profileArn`
//! / `runtime.*.kiro.dev` deferred.
//! List-models prefers HTTP. Chat print path prefers HTTP when creds work;
//! multi-turn resumes via namespaced `kiro-http:<conversationId>`. CLI
//! `--resume-id` stays a separate namespace.

mod client;
mod creds;
mod eventstream;

#[cfg(test)]
mod tests;

pub(crate) use client::{
    chat_turn_with_access_token, get_usage_limits, list_models_http, parse_http_native_session_id,
    try_http_run_result,
};
pub(crate) use creds::KiroHttpRouteParams;
