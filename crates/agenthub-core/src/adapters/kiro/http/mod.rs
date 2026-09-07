//! AgentHub-owned Kiro HTTP client.
//!
//! Community protocol against CodeWhisperer / Kiro hosts — **not** an official
//! public REST. Builder ID login + `KIRO_API_KEY` first; enterprise `profileArn`
//! / `runtime.*.kiro.dev` deferred.
//! List-models prefers HTTP. Chat send uses HTTP only when `kiro-cli` is missing.

mod client;
mod creds;
mod eventstream;

#[cfg(test)]
mod tests;

pub(crate) use client::{
    chat_turn_http, chat_turn_with_access_token, get_usage_limits, list_models_http,
    try_http_run_result,
};
