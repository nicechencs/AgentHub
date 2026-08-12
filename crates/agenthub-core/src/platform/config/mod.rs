//! Platform Configuration: schema, projector port, registry, service.
//!
//! Agent-specific native JSON/TOML knowledge lives in [`sources`].
//! TODO(P13): move projectors under integrations/agents/<key>/.

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
