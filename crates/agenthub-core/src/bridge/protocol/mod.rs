//! Pure wire-protocol translation.  This module has no HTTP, credential, or runtime code.
//!
//! - [`chat`] / [`responses`]: existing Kimi local_bridge path (Responses downstream ↔ Chat).
//! - [`anthropic_messages`] + Responses IR helpers: Codex subscription → Claude Code kernel
//!   (Messages ↔ IR ↔ Responses). No network and no secrets.

pub mod anthropic_messages;
pub mod chat;
pub mod responses;

#[cfg(test)]
mod tests;
