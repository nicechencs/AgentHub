//! Single source for Codex provider-managed TOML keys.
//!
//! Provider switch (`write_toml_config`) replaces these top-level keys.
//! The projector may only write a subset (plus auth.json, which is exempt).

/// Top-level `config.toml` keys owned by the provider/connection write path.
pub const PROVIDER_TOML_KEYS: &[&str] = &[
    "model",
    "review_model",
    "model_provider",
    "model_reasoning_effort",
    "model_reasoning_summary",
    "model_verbosity",
    "model_providers",
    "disable_response_storage",
    "preferred_auth_method",
    "network_access",
    "windows_wsl_setup_acknowledged",
    "features",
];

/// Native TOML keys the GenericConfigForm projector writes (not UI field ids).
/// Must stay ⊆ [`PROVIDER_TOML_KEYS`].
pub const PROJECTOR_TOML_KEYS: &[&str] = &[
    "model",
    "model_provider",
    "model_providers",
    "model_reasoning_effort",
];
