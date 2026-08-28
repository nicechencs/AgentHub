//! AgentAdapter trait + registry. One module per agent.
//!
//! Shared helpers live in sibling files so this module stays a façade:
//! trait, registry, detect, auth revision, and config writers.

mod adapter_trait;
pub(crate) mod auth_revision;
mod config_write;
mod detect_binary;
mod registry;
pub(crate) mod session_resume;

mod claude;
mod codex;
mod codex_copies;
pub(crate) mod cursor;
pub(crate) mod dsh;
mod grok;
mod kimi;
pub(crate) mod pi;
pub mod pi_auth;
pub(crate) mod workbuddy;

// Free install probes for platform detectors (P1-3) — no AdapterRegistry required.
pub(crate) use claude::detect_installation as detect_claude_installation;
pub(crate) use codex::detect_installation as detect_codex_installation;
pub(crate) use cursor::detect_installation as detect_cursor_installation;
pub(crate) use dsh::detect_installation as detect_dsh_installation;
pub(crate) use grok::detect_installation as detect_grok_installation;
pub(crate) use kimi::detect_installation as detect_kimi_installation;
pub(crate) use pi::detect_installation as detect_pi_installation;
pub(crate) use workbuddy::detect_installation as detect_workbuddy_installation;

// Codex / Claude OAuth PKCE historically stored flat token bundles; adapters +
// oauth finish both need the same conversion into the live apply shape.
pub(crate) use claude::normalize_oauth_credentials as normalize_claude_oauth_credentials;
pub(crate) use codex::normalize_oauth_credentials as normalize_codex_oauth_credentials;
pub(crate) use grok::{
    expand_grok_auth_to_live_accounts, grok_live_has_leftover_api_key_field,
    grok_live_uses_default_auth_slot, read_grok_live_api_key_tail, read_grok_live_base_url,
};
pub(crate) use kimi::kimi_live_has_leftover_api_key_when_oauth;

pub use adapter_trait::{default_authorization_key, default_identity_label, AgentAdapter};
pub(crate) use auth_revision::{
    auth_file_revision, auth_files_revision, inspect_auth_credentials, oauth_auth_health,
};
pub(crate) use config_write::{
    api_key_live_account, require_api_key, write_json_config, write_toml_config,
    write_verified_json_object,
};
pub(crate) use detect_binary::{
    detect_binary, detect_binary_with_env, extract_version_token,
    is_under_agenthub_user_npm_prefix, looks_like_version_line, user_writable_npm_prefix,
    NOT_FOUND_FIREFIGHTING_NOTE,
};
pub use registry::{
    register_all, supports_structured_stream, wants_structured_for, AdapterRegistry,
};
pub use session_resume::{plan_native_resume, supports_print_resume, NativeResumePlan};

#[cfg(test)]
mod tests;
