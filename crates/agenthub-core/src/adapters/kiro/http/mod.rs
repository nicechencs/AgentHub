//! AgentHub-owned Kiro HTTP client (Chat-native first slice).
//!
//! Community protocol against CodeWhisperer / Kiro hosts — **not** an official
//! public REST. Builder ID login + `KIRO_API_KEY` first; enterprise `profileArn`
//! / `runtime.*.kiro.dev` deferred. CLI headless remains the fallback.

mod client;
mod creds;
mod eventstream;

#[cfg(test)]
mod tests;

pub(crate) use client::{chat_turn_http, list_models_http, try_http_run_result};
