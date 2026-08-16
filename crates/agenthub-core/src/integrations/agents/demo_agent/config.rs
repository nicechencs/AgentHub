use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use crate::error::Result;
use crate::integrations::IntegrationContext;
use crate::models::Capability;
use crate::platform::config::{
    AgentConfigProjector, AgentConfigSchema, ConfigApplyResult, ConfigChangePlan,
    ConfigFieldSchema, ConfigValidationIssue, ConfigValidationResult, ConfigValueType, FieldChange,
    NativeConfigFormat, NormalizedConfigDocument,
};
use crate::platform::AgentKey;

use super::key;

struct DemoConfigProjector {
    key: AgentKey,
}

impl AgentConfigProjector for DemoConfigProjector {
    fn agent_key(&self) -> AgentKey {
        self.key.clone()
    }

    fn schema(&self) -> AgentConfigSchema {
        AgentConfigSchema {
            agent_key: self.key.clone(),
            schema_version: 1,
            native_format: NativeConfigFormat::Json,
            relative_path: "demo.json".into(),
            fields: vec![ConfigFieldSchema {
                key: "greeting".into(),
                label: "Greeting".into(),
                value_type: ConfigValueType::String,
                required: false,
                secret: false,
                default: Some(serde_json::json!("hello")),
                validation: None,
                help: None,
                capability: Some(Capability::ConfigWrite.as_str().into()),
            }],
        }
    }

    fn read_normalized(&self, _agent_home: &Path) -> Result<NormalizedConfigDocument> {
        Ok(NormalizedConfigDocument {
            agent_key: self.key.clone(),
            schema_version: 1,
            values: BTreeMap::from([("greeting".into(), serde_json::json!("hello"))]),
            unknown_native: serde_json::json!({}),
            path: None,
            missing: true,
        })
    }

    fn validate(
        &self,
        values: &BTreeMap<String, serde_json::Value>,
    ) -> Result<ConfigValidationResult> {
        if values.contains_key("nope") {
            return Ok(ConfigValidationResult::failure(vec![
                ConfigValidationIssue {
                    field_key: "nope".into(),
                    code: "unknown_field".into(),
                    message: "unknown".into(),
                },
            ]));
        }
        Ok(ConfigValidationResult::success())
    }

    fn plan_apply(
        &self,
        current: &NormalizedConfigDocument,
        desired: &BTreeMap<String, serde_json::Value>,
    ) -> Result<ConfigChangePlan> {
        Ok(ConfigChangePlan {
            agent_key: self.key.clone(),
            schema_version: 1,
            target_path: std::path::PathBuf::from("demo.json"),
            field_changes: desired
                .keys()
                .map(|k| FieldChange {
                    field_key: k.clone(),
                    from: current.values.get(k).cloned(),
                    to: desired.get(k).cloned(),
                    secret: false,
                })
                .collect(),
        })
    }

    fn apply(
        &self,
        agent_home: &Path,
        desired: &BTreeMap<String, serde_json::Value>,
    ) -> Result<ConfigApplyResult> {
        let current = self.read_normalized(agent_home)?;
        let plan = self.plan_apply(&current, desired)?;
        Ok(ConfigApplyResult {
            document: NormalizedConfigDocument {
                values: desired.clone(),
                ..current
            },
            plan,
        })
    }

    fn materialize_settings_config(
        &self,
        _base_raw: Option<&serde_json::Value>,
        desired: &BTreeMap<String, serde_json::Value>,
    ) -> Result<serde_json::Value> {
        Ok(serde_json::Value::Object(
            desired
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
        ))
    }
}

pub fn register(ctx: &mut IntegrationContext<'_>) {
    ctx.config
        .register(Arc::new(DemoConfigProjector { key: key() }))
        .expect("unique demo-agent config projector");
}
