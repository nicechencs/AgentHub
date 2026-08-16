//! Platform Projects capability: ProjectSource port + registry + builtin sources.
//!
//! Agent-specific filesystem discovery is registered as contributions.
//! [`crate::services::ProjectService`] merges, sorts, applies metadata, and deletes.
//!
//! Scan helpers remain under `services/project_service/scan`.
//! Per-agent sources live in [`crate::integrations`].

mod registry;
mod source;
mod sources;

pub use registry::{empty_registry, ProjectSourceRegistry};
pub use source::{ProjectScanContext, ProjectSource};
pub use sources::builtin_project_registry;

#[cfg(test)]
mod tests;
