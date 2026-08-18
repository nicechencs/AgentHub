//! Protocol planning graph: agent bind table + adapter capability matrix.
//!
//! Pure lookup / decide tables. Route classify (I/O + repo) stays in
//! [`crate::services::adapter_route_service`].

mod adapter_capability_matrix;
mod agent_capability;

pub use adapter_capability_matrix::*;
pub use agent_capability::*;
