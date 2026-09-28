//! Single source for Grok provider-managed TOML keys.

pub const PROVIDER_TOML_KEYS: &[&str] = &[
    "models",
    "model",
    "model_providers",
    "base_url",
    "api_key",
    "env_key",
];

/// Native TOML keys the projector writes. Must stay ⊆ [`PROVIDER_TOML_KEYS`].
///
/// `[auth]` is not listed. A provider switch only updates `preferred_method`;
/// replacing the table would drop OIDC and the other login settings.
// Referenced only from `tests.rs` in this crate; keep for test coverage.
#[allow(dead_code)]
pub const PROJECTOR_TOML_KEYS: &[&str] =
    &["models", "model", "model_providers", "base_url", "api_key"];
