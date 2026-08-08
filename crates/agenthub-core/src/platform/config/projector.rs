//! AgentConfigProjector port — native read/normalize/validate/project.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::Value;

use crate::error::Result;
use crate::platform::AgentKey;

use super::document::{ConfigApplyResult, ConfigChangePlan, NormalizedConfigDocument};
use super::schema::{AgentConfigSchema, ConfigValidationResult};

/// Sparse extension: one agent contributes native config knowledge.
///
/// Platform [`super::ConfigurationService`] owns paths policy, atomic write
/// orchestration, and error mapping. Implementations must preserve unknown
/// native fields unless the user explicitly submits a replacement for them.
pub trait AgentConfigProjector: Send + Sync {
    fn agent_key(&self) -> AgentKey;

    fn schema(&self) -> AgentConfigSchema;

    /// Read and normalize native config under `agent_home`.
    fn read_normalized(&self, agent_home: &Path) -> Result<NormalizedConfigDocument>;

    /// Validate desired known-field values (object keys = field keys).
    fn validate(&self, values: &BTreeMap<String, Value>) -> Result<ConfigValidationResult>;

    /// Build a change plan without writing.
    fn plan_apply(
        &self,
        current: &NormalizedConfigDocument,
        desired: &BTreeMap<String, Value>,
    ) -> Result<ConfigChangePlan>;

    /// Apply desired values: merge into native, atomic write, re-read.
    ///
    /// Secret fields set to [`super::schema::SECRET_REDACTED`] or omitted must
    /// keep the existing native secret.
    fn apply(
        &self,
        agent_home: &Path,
        desired: &BTreeMap<String, Value>,
    ) -> Result<ConfigApplyResult>;

    /// Merge `desired` into optional existing pool/live raw settings without writing files.
    ///
    /// Output shape matches historical provider `settings_config` (Claude: JSON object;
    /// TOML agents: `{ format: "toml", content, auth? }`).
    fn materialize_settings_config(
        &self,
        base_raw: Option<&Value>,
        desired: &BTreeMap<String, Value>,
    ) -> Result<Value>;
}
