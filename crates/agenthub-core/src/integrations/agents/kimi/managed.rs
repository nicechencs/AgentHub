//! Single source for Kimi provider-managed TOML keys.

pub const PROVIDER_TOML_KEYS: &[&str] = &["default_model", "default_provider", "providers"];

/// Native TOML keys the projector writes. Must stay ⊆ [`PROVIDER_TOML_KEYS`].
pub const PROJECTOR_TOML_KEYS: &[&str] = &["default_model", "default_provider", "providers"];
