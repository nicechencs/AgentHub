//! Shared runtimes (Node/npm/PowerShell/Git) — decoupled from Agent adapters.

mod bootstrap;
mod detect;
mod nodejs;

pub use bootstrap::*;
pub use detect::*;
pub use nodejs::*;

use crate::models::RuntimeId;

/// Runtime prerequisites for the platform-native install channel.
///
/// Windows runs allowlisted `.ps1` via PowerShell. macOS/Linux run allowlisted
/// `install.sh` with bash/sh and must not advertise PowerShell.
pub fn native_install_requires() -> Vec<RuntimeId> {
    #[cfg(windows)]
    {
        vec![RuntimeId::PowerShell]
    }
    #[cfg(not(windows))]
    {
        Vec::new()
    }
}
