//! Platform Configuration: schema, projector port, registry, service.
//!
//! Shared schema/projector ports live here; per-agent projectors live in
//! [`crate::integrations`].

mod document;
mod projector;
mod registry;
mod schema;
mod service;
pub mod sources;

pub use document::{ConfigApplyResult, ConfigChangePlan, FieldChange, NormalizedConfigDocument};
pub use projector::AgentConfigProjector;
pub use registry::{builtin_config_registry, ConfigProjectorRegistry};
pub use schema::{
    AgentConfigSchema, ConfigFieldSchema, ConfigValidationIssue, ConfigValidationResult,
    ConfigValueType, FieldValidation, NativeConfigFormat, SECRET_REDACTED,
};
pub use service::ConfigurationService;

#[cfg(test)]
mod tests;
