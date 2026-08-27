//! Pure wire-protocol translation.  This module has no HTTP, credential, or runtime code.
//!
//! - [`chat`] / [`responses`]: Kimi local_bridge path (Responses downstream ↔ Chat)
//!   and Codex official login → Grok / Kimi / DSH (Chat local ↔ Responses upstream).
//! - [`anthropic_messages`] + Responses IR helpers:
//!   - Codex subscription → Claude Code kernel (Messages downstream ↔ IR ↔ Responses upstream).
//!   - Anthropic API Key / Claude subscription → Codex (Responses downstream ↔ IR ↔ Messages upstream).
//!   No network and no secrets.

pub mod anthropic_messages;
pub mod chat;
pub mod pair;
pub mod responses;

pub use responses::{
    apply_official_codex_model, encode_responses_from_ir, is_leftover_bridge_model,
    parse_responses_request, prepare_official_codex_request, responses_output_to_ir,
    to_grok_chat_request, to_grok_responses_request, to_kimi_chat_request, to_responses_request,
    translate_responses_request, IrToResponsesSse, ResponsesStreamToIr,
};

/// Claude subscription → Codex kernel fixtures.
#[cfg(test)]
mod claude_codex_tests;
#[cfg(test)]
mod fixture_loader;
#[cfg(test)]
mod tests;
