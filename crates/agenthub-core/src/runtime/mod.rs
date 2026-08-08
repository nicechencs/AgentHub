//! Shared runtimes (Node/npm/PowerShell/Git) — decoupled from Agent adapters.

mod bootstrap;
mod detect;
mod nodejs;

pub use bootstrap::*;
pub use detect::*;
pub use nodejs::*;
