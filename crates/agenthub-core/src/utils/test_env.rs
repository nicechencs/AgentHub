//! Test-only helpers for tests that mutate process environment variables.
//!
//! Several test modules (`services::install_service`, `platform::paths`, ...)
//! override the same keys (`PI_CODING_AGENT_DIR`, `CODEX_HOME`, ...). They
//! must share one lock so parallel tests cannot observe each other's
//! overrides; `EnvVarGuard` restores the previous value even when the test
//! panics before an explicit restore.

use std::ffi::OsString;
use std::path::Path;
use std::sync::{Mutex, MutexGuard, OnceLock};

static TEST_ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

/// Serialize environment-variable mutations across all test modules.
pub(crate) fn lock_test_env() -> MutexGuard<'static, ()> {
    TEST_ENV_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

/// RAII guard restoring one environment variable on drop, including unwind.
pub(crate) struct EnvVarGuard {
    key: &'static str,
    prev: Option<OsString>,
}

impl EnvVarGuard {
    pub(crate) fn set(key: &'static str, value: &Path) -> Self {
        let prev = std::env::var_os(key);
        std::env::set_var(key, value);
        Self { key, prev }
    }

    pub(crate) fn remove(key: &'static str) -> Self {
        let prev = std::env::var_os(key);
        std::env::remove_var(key);
        Self { key, prev }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        match &self.prev {
            Some(value) => std::env::set_var(self.key, value),
            None => std::env::remove_var(self.key),
        }
    }
}
