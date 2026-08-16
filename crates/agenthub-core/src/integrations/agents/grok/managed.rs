//! Single source for Grok provider-managed TOML keys.

pub const PROVIDER_TOML_KEYS: &[&str] = &["models", "model", "base_url", "api_key", "env_key"];

/// Native TOML keys the projector writes. Must stay ⊆ [`PROVIDER_TOML_KEYS`].
pub const PROJECTOR_TOML_KEYS: &[&str] = &["models", "model", "base_url", "api_key"];
