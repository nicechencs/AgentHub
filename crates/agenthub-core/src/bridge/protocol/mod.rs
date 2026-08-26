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
pub mod responses;

/// Claude subscription → Codex kernel fixtures.
#[cfg(test)]
mod claude_codex_tests;
#[cfg(test)]
mod fixture_loader;
#[cfg(test)]
mod tests;
