//! Single source for Kimi provider-managed TOML keys.

pub const PROVIDER_TOML_KEYS: &[&str] = &["default_model", "default_provider", "providers"];

/// Native TOML keys the projector writes. Must stay ⊆ [`PROVIDER_TOML_KEYS`].
// Referenced only from `tests.rs` in this crate; keep for test coverage.
#[allow(dead_code)]
pub const PROJECTOR_TOML_KEYS: &[&str] = &["default_model", "default_provider", "providers"];
