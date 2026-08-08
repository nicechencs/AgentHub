//! Platform Projects capability: ProjectSource port + registry.
//!
//! Agent-specific filesystem discovery is registered as contributions.
//! [`crate::services::ProjectService`] merges, sorts, applies metadata, and deletes.
//!
//! TODO(P13): move source impls under integrations/agents/<key>/.

mod registry;
mod source;

pub use registry::{empty_registry, ProjectSourceRegistry};
pub use source::{ProjectScanContext, ProjectSource};

#[cfg(test)]
mod tests;
