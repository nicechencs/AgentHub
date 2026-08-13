//! Shared constants for the Kimi membership → Claude native route.
//!
//! Plan (`AdapterRouteService`) and apply (`AdapterApplyService` /
//! `AdapterSecretResolver`) import from here so endpoint and env key strings
//! cannot drift.

/// Official Kimi coding Anthropic-compatible endpoint projected into Claude.
pub const KIMI_CLAUDE_BASE_URL: &str = "https://api.kimi.com/coding/";

/// Claude env key for the Anthropic-compatible base URL.
pub const ANTHROPIC_BASE_URL_ENV: &str = "ANTHROPIC_BASE_URL";

/// Claude env key that carries the membership API key (or connection marker).
pub const ANTHROPIC_AUTH_TOKEN_ENV: &str = "ANTHROPIC_AUTH_TOKEN";

/// Stored in generated reference providers instead of the source API key.
pub const CONNECTION_SECRET_MARKER: &str = "$AGENTHUB_CONNECTION_SECRET$";
