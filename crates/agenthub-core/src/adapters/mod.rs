//! AgentAdapter trait + registry. One module per agent.
//!
//! Shared helpers live in sibling files so this module stays a façade:
//! trait, registry, detect, auth revision, and config writers.

mod adapter_trait;
mod auth_revision;
mod config_write;
mod detect_binary;
mod registry;
pub(crate) mod session_resume;

mod claude;
mod codex;
pub(crate) mod cursor;
pub(crate) mod dsh;
mod grok;
mod kimi;
mod pi;
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

// Codex OAuth PKCE historically stored flat token bundles; adapters + oauth finish
// both need the same conversion into live `auth_json` shape.
pub(crate) use codex::normalize_oauth_credentials as normalize_codex_oauth_credentials;

pub use adapter_trait::{default_authorization_key, default_identity_label, AgentAdapter};
pub(crate) use auth_revision::{
    auth_file_revision, auth_files_revision, inspect_auth_credentials, oauth_auth_health,
    AuthCredentialMetadata,
};
pub(crate) use config_write::{
    api_key_live_account, require_api_key, write_json_config, write_toml_config,
    write_verified_json_object,
};
pub(crate) use detect_binary::{
    detect_binary, detect_binary_with_env, expand_binary_names, extract_version_token,
    infer_channel, looks_like_version_line, well_known_bin_paths, NOT_FOUND_FIREFIGHTING_NOTE,
};
pub use registry::{
    register_all, supports_structured_stream, wants_structured_for, AdapterRegistry,
};
pub use session_resume::{plan_native_resume, supports_print_resume, NativeResumePlan};

#[cfg(test)]
mod tests;
