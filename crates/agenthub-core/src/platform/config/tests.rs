//! Configuration platform unit tests (separate from production modules).

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use serde_json::{json, Value};
use tempfile::tempdir;

use crate::error::Result;
use crate::models::AgentId;
use crate::platform::config::{
    builtin_config_registry, AgentConfigProjector, AgentConfigSchema, ConfigApplyResult,
    ConfigChangePlan, ConfigFieldSchema, ConfigProjectorRegistry, ConfigValidationResult,
    ConfigValueType, ConfigurationService, FieldChange, NativeConfigFormat,
    NormalizedConfigDocument, SECRET_REDACTED,
};
use crate::platform::AgentKey;

struct FutureConfigProjector {
    key: AgentKey,
    greeting: &'static str,
}

impl FutureConfigProjector {
    fn new(greeting: &'static str) -> Self {
        Self {
            key: AgentKey::parse("future-agent").unwrap(),
            greeting,
        }
    }
}

impl AgentConfigProjector for FutureConfigProjector {
    fn agent_key(&self) -> AgentKey {
        self.key.clone()
    }

    fn schema(&self) -> AgentConfigSchema {
        AgentConfigSchema {
            agent_key: self.key.clone(),
            schema_version: 1,
            native_format: NativeConfigFormat::Json,
            relative_path: "future.json".into(),
            fields: vec![ConfigFieldSchema {
                key: "greeting".into(),
                label: "Greeting".into(),
                value_type: ConfigValueType::String,
                required: false,
                secret: false,
                default: Some(json!(self.greeting)),
                validation: None,
                help: None,
                capability: None,
            }],
        }
    }

    fn read_normalized(&self, agent_home: &Path) -> Result<NormalizedConfigDocument> {
        Ok(NormalizedConfigDocument {
            agent_key: self.key.clone(),
            schema_version: 1,
            values: BTreeMap::from([("greeting".into(), json!(self.greeting))]),
            unknown_native: json!({}),
            path: Some(agent_home.join("future.json")),
            missing: true,
        })
    }

    fn validate(&self, values: &BTreeMap<String, Value>) -> Result<ConfigValidationResult> {
        if values.keys().all(|key| key == "greeting") {
            Ok(ConfigValidationResult::success())
        } else {
            Ok(ConfigValidationResult::failure(vec![]))
        }
    }

    fn plan_apply(
        &self,
        current: &NormalizedConfigDocument,
        desired: &BTreeMap<String, Value>,
    ) -> Result<ConfigChangePlan> {
        Ok(ConfigChangePlan {
            agent_key: self.key.clone(),
            schema_version: 1,
            target_path: Path::new("future.json").to_path_buf(),
            field_changes: desired
                .iter()
                .map(|(key, value)| FieldChange {
                    field_key: key.clone(),
                    from: current.values.get(key).cloned(),
                    to: Some(value.clone()),
                    secret: false,
                })
                .collect(),
        })
    }

    fn apply(
        &self,
        agent_home: &Path,
        desired: &BTreeMap<String, Value>,
    ) -> Result<ConfigApplyResult> {
        let current = self.read_normalized(agent_home)?;
        Ok(ConfigApplyResult {
            plan: self.plan_apply(&current, desired)?,
            document: NormalizedConfigDocument {
                values: desired.clone(),
                ..current
            },
        })
    }

    fn materialize_settings_config(
        &self,
        _base_raw: Option<&Value>,
        desired: &BTreeMap<String, Value>,
    ) -> Result<Value> {
        Ok(Value::Object(desired.clone().into_iter().collect()))
    }
}

#[test]
fn unknown_valid_agent_key_runs_key_native_config_flow() {
    let key = AgentKey::parse("future-agent").unwrap();
    assert!(AgentId::parse(key.as_str()).is_none());
    let mut registry = ConfigProjectorRegistry::new();
    registry
        .register(Arc::new(FutureConfigProjector::new("hello")))
        .unwrap();
    let service = ConfigurationService::with_registry(registry);
    let home = tempdir().unwrap();

    assert_eq!(service.schema_for_agent_key(&key).unwrap().agent_key, key);
    let document = service.read_for_agent_key(&key, home.path()).unwrap();
    assert_eq!(document.values["greeting"], "hello");
    let desired = BTreeMap::from([("greeting".into(), json!("hi"))]);
    assert!(service.validate_for_agent_key(&key, &desired).unwrap().ok);
    let plan = service
        .plan_apply_for_agent_key(&key, &desired, home.path())
        .unwrap();
    assert_eq!(plan.agent_key, key);
    assert_eq!(plan.field_changes.len(), 1);
}

#[test]
fn config_registry_rejects_duplicate_key_without_replacing_first() {
    let key = AgentKey::parse("future-agent").unwrap();
    let mut registry = ConfigProjectorRegistry::new();
    registry
        .register(Arc::new(FutureConfigProjector::new("first")))
        .unwrap();
    let err = registry
        .register(Arc::new(FutureConfigProjector::new("second")))
        .unwrap_err();
    assert_eq!(err.code(), "invalid_arg");
    assert_eq!(registry.supported_agent_keys(), vec![key.clone()]);
    assert_eq!(
        registry.get(&key).unwrap().schema().fields[0].default,
        Some(json!("first"))
    );
}

#[test]
fn config_legacy_agent_id_helpers_delegate_to_agent_key() {
    let registry = builtin_config_registry();
    let key = AgentKey::from_agent_id(AgentId::Claude);
    assert!(registry.contains(AgentId::Claude));
    assert!(registry.get_agent_id(AgentId::Claude).is_some());
    assert_eq!(registry.get(&key).unwrap().agent_key(), key);

    let service = ConfigurationService::new();
    assert_eq!(
        service.schema(AgentId::Claude).unwrap(),
        service.schema_for_agent_key(&key).unwrap()
    );
}

#[test]
fn registry_covers_supported_not_cursor() {
    let reg = builtin_config_registry();
    assert!(reg.contains(AgentId::Claude));
    assert!(reg.contains(AgentId::Codex));
    assert!(reg.contains(AgentId::Kimi));
    assert!(reg.contains(AgentId::Grok));
    assert!(!reg.contains(AgentId::Cursor));
    assert!(!reg.contains(AgentId::Pi));
    assert!(!reg.contains(AgentId::WorkBuddy));
    assert_eq!(reg.supported_agents().len(), 4);
}

#[test]
fn claude_read_normalize_roundtrip_preserves_unknown() {
    let dir = tempdir().unwrap();
    let home = dir.path();
    let path = home.join("settings.json");
    std::fs::write(
        &path,
        r#"{
  "env": {
    "ANTHROPIC_BASE_URL": "https://example.com",
    "ANTHROPIC_AUTH_TOKEN": "sk-secret-token",
    "ANTHROPIC_MODEL": "claude-sonnet"
  },
  "model": "claude-sonnet",
  "customPlugin": { "enabled": true },
  "extraTop": 42
}
"#,
    )
    .unwrap();

    let svc = ConfigurationService::new();
    let doc = svc.read_at(AgentId::Claude, Some(home)).unwrap();
    assert!(!doc.missing);
    assert_eq!(
        doc.values.get("baseUrl").and_then(|v| v.as_str()),
        Some("https://example.com")
    );
    assert_eq!(
        doc.values.get("model").and_then(|v| v.as_str()),
        Some("claude-sonnet")
    );
    assert_eq!(
        doc.values.get("apiKey").and_then(|v| v.as_str()),
        Some(SECRET_REDACTED)
    );
    // Secret scrubbed in unknown_native
    assert_eq!(
        doc.unknown_native["env"]["ANTHROPIC_AUTH_TOKEN"],
        SECRET_REDACTED
    );
    assert_eq!(doc.unknown_native["extraTop"], 42);
    assert_eq!(doc.unknown_native["customPlugin"]["enabled"], true);

    let mut desired = BTreeMap::new();
    desired.insert("baseUrl".into(), json!("https://new.example.com"));
    desired.insert("apiKey".into(), json!(SECRET_REDACTED)); // unchanged
    desired.insert("claudeAuthEnv".into(), json!("ANTHROPIC_AUTH_TOKEN"));
    desired.insert("model".into(), json!("claude-opus"));
    desired.insert("modelOpus".into(), json!(""));
    desired.insert("modelSonnet".into(), json!(""));
    desired.insert("modelHaiku".into(), json!(""));
    desired.insert("modelFable".into(), json!(""));
    desired.insert("modelSubagent".into(), json!(""));

    let applied = svc.apply_at(AgentId::Claude, &desired, Some(home)).unwrap();
    assert_eq!(
        applied
            .document
            .values
            .get("baseUrl")
            .and_then(|v| v.as_str()),
        Some("https://new.example.com")
    );
    assert_eq!(
        applied
            .document
            .values
            .get("model")
            .and_then(|v| v.as_str()),
        Some("claude-opus")
    );

    // Secret preserved on disk
    let raw: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(raw["env"]["ANTHROPIC_AUTH_TOKEN"], "sk-secret-token");
    assert_eq!(raw["extraTop"], 42);
    assert_eq!(raw["customPlugin"]["enabled"], true);
}

#[test]
fn claude_invalid_json_errors() {
    let dir = tempdir().unwrap();
    let home = dir.path();
    std::fs::write(home.join("settings.json"), "{not-json").unwrap();
    let svc = ConfigurationService::new();
    let err = svc.read_at(AgentId::Claude, Some(home)).unwrap_err();
    assert_eq!(err.code(), "invalid_arg");
}

#[test]
fn claude_validate_rejects_unknown_field() {
    let svc = ConfigurationService::new();
    let mut values = BTreeMap::new();
    values.insert("baseUrl".into(), json!("https://x"));
    values.insert("nope".into(), json!("x"));
    let r = svc.validate(AgentId::Claude, &values).unwrap();
    assert!(!r.ok);
    assert!(r.issues.iter().any(|i| i.code == "unknown_field"));
}

#[test]
fn codex_toml_roundtrip_and_auth() {
    let dir = tempdir().unwrap();
    let home = dir.path();
    std::fs::write(
        home.join("config.toml"),
        r#"model = "gpt-4"
model_provider = "custom"

[model_providers.custom]
name = "custom"
base_url = "https://api.example.com"
wire_api = "responses"
"#,
    )
    .unwrap();
    std::fs::write(
        home.join("auth.json"),
        r#"{ "OPENAI_API_KEY": "sk-live-key" }
"#,
    )
    .unwrap();

    let svc = ConfigurationService::new();
    let doc = svc.read_at(AgentId::Codex, Some(home)).unwrap();
    assert_eq!(
        doc.values.get("model").and_then(|v| v.as_str()),
        Some("gpt-4")
    );
    assert_eq!(
        doc.values.get("baseUrl").and_then(|v| v.as_str()),
        Some("https://api.example.com")
    );
    assert_eq!(
        doc.values.get("apiKey").and_then(|v| v.as_str()),
        Some(SECRET_REDACTED)
    );

    let mut desired = BTreeMap::new();
    desired.insert("model".into(), json!("gpt-5"));
    desired.insert("baseUrl".into(), json!("https://new.api"));
    desired.insert("apiKey".into(), json!("sk-new-key"));
    desired.insert("reasoningEffort".into(), json!("high"));
    desired.insert("wireApi".into(), json!("responses"));
    desired.insert("providerSlug".into(), json!("custom"));

    svc.apply_at(AgentId::Codex, &desired, Some(home)).unwrap();
    let text = std::fs::read_to_string(home.join("config.toml")).unwrap();
    assert!(text.contains("gpt-5"));
    assert!(text.contains("https://new.api"));
    assert!(text.contains("high"));
    // key not in toml
    assert!(!text.contains("sk-new-key"));
    let auth: Value =
        serde_json::from_str(&std::fs::read_to_string(home.join("auth.json")).unwrap()).unwrap();
    assert_eq!(auth["OPENAI_API_KEY"], "sk-new-key");
}

#[test]
fn codex_invalid_toml_errors() {
    let dir = tempdir().unwrap();
    let home = dir.path();
    std::fs::write(home.join("config.toml"), "[[[not valid").unwrap();
    let svc = ConfigurationService::new();
    let err = svc.read_at(AgentId::Codex, Some(home)).unwrap_err();
    assert_eq!(err.code(), "invalid_arg");
}

#[test]
fn missing_file_read_is_ok() {
    let dir = tempdir().unwrap();
    let svc = ConfigurationService::new();
    let doc = svc.read_at(AgentId::Claude, Some(dir.path())).unwrap();
    assert!(doc.missing);
    assert_eq!(doc.values.get("model").and_then(|v| v.as_str()), Some(""));
}

#[test]
fn unsupported_agent_errors() {
    let svc = ConfigurationService::new();
    let err = svc.schema(AgentId::Cursor).unwrap_err();
    assert_eq!(err.code(), "unsupported");
}

#[test]
fn materialize_claude_pool_settings_preserves_unknown() {
    let svc = ConfigurationService::new();
    let base = json!({
        "env": { "ANTHROPIC_AUTH_TOKEN": "sk-old", "CUSTOM_FLAG": "1" },
        "extraTop": true
    });
    let mut desired = BTreeMap::new();
    desired.insert("baseUrl".into(), json!("https://proxy.example"));
    desired.insert("apiKey".into(), json!(SECRET_REDACTED));
    desired.insert("claudeAuthEnv".into(), json!("ANTHROPIC_AUTH_TOKEN"));
    desired.insert("model".into(), json!("claude-opus"));
    desired.insert("modelOpus".into(), json!(""));
    desired.insert("modelSonnet".into(), json!(""));
    desired.insert("modelHaiku".into(), json!(""));
    desired.insert("modelFable".into(), json!(""));
    desired.insert("modelSubagent".into(), json!(""));
    let raw = svc
        .materialize_settings_config(AgentId::Claude, &desired, Some(&base))
        .unwrap();
    assert_eq!(raw["env"]["ANTHROPIC_BASE_URL"], "https://proxy.example");
    assert_eq!(raw["env"]["ANTHROPIC_AUTH_TOKEN"], "sk-old");
    assert_eq!(raw["env"]["CUSTOM_FLAG"], "1");
    assert_eq!(raw["extraTop"], true);
    assert_eq!(raw["model"], "claude-opus");
}

#[test]
fn projector_schema_versions_are_stable() {
    let reg = builtin_config_registry();
    for agent in [
        AgentId::Claude,
        AgentId::Codex,
        AgentId::Kimi,
        AgentId::Grok,
    ] {
        let p = reg.get_agent_id(agent).unwrap();
        let expected = if agent == AgentId::Grok { 2 } else { 1 };
        assert_eq!(p.schema().schema_version, expected);
        assert!(!p.schema().fields.is_empty());
    }
}

#[test]
fn grok_apply_and_secret_unchanged() {
    let dir = tempdir().unwrap();
    let home = dir.path();
    std::fs::write(
        home.join("config.toml"),
        "model = \"grok-2\"\nbase_url = \"https://x\"\napi_key = \"sk-old\"\n",
    )
    .unwrap();
    let svc = ConfigurationService::new();
    let mut desired = BTreeMap::new();
    desired.insert("model".into(), json!("grok-3"));
    desired.insert("baseUrl".into(), json!("https://y"));
    desired.insert("apiKey".into(), json!(SECRET_REDACTED));
    svc.apply_at(AgentId::Grok, &desired, Some(home)).unwrap();
    let text = std::fs::read_to_string(home.join("config.toml")).unwrap();
    assert!(text.contains("[models]"));
    assert!(text.contains("[model."));
    assert!(text.contains("grok-3"));
    assert!(text.contains("sk-old"));
}

#[test]
fn grok_registry_roundtrip_preserves_native_model_options() {
    let dir = tempdir().unwrap();
    let home = dir.path();
    std::fs::write(
        home.join("config.toml"),
        r#"[models]
default = "grok"
web_search = "grok"

[model."grok"]
model = "grok-4.5"
base_url = "https://relay.example.com/v1"
name = "Grok 4.5"
api_key = "sk-native-secret"
api_backend = "responses"
context_window = 1000000
supports_backend_search = true
"#,
    )
    .unwrap();

    let svc = ConfigurationService::new();
    let read = svc.read_at(AgentId::Grok, Some(home)).unwrap();
    assert_eq!(
        read.values.get("model").and_then(Value::as_str),
        Some("grok-4.5")
    );
    assert_eq!(
        read.values.get("baseUrl").and_then(Value::as_str),
        Some("https://relay.example.com/v1")
    );
    assert_eq!(
        read.values.get("apiKey").and_then(Value::as_str),
        Some(SECRET_REDACTED)
    );
    let safe_content = read
        .unknown_native
        .get("content")
        .and_then(Value::as_str)
        .unwrap();
    assert!(safe_content.contains("api_backend = \"responses\""));
    assert!(safe_content.contains("context_window = 1000000"));
    assert!(safe_content.contains("supports_backend_search = true"));
    assert!(!safe_content.contains("sk-native-secret"));
    assert!(safe_content.contains("api_key = \"***\""));

    let mut desired = BTreeMap::new();
    desired.insert("model".into(), json!("grok-4.5-latest"));
    desired.insert("baseUrl".into(), json!("https://new-relay.example.com/v1"));
    desired.insert("apiKey".into(), json!(SECRET_REDACTED));
    svc.apply_at(AgentId::Grok, &desired, Some(home)).unwrap();
    let text = std::fs::read_to_string(home.join("config.toml")).unwrap();
    assert!(text.contains("model = \"grok-4.5-latest\""));
    assert!(text.contains("base_url = \"https://new-relay.example.com/v1\""));
    assert!(text.contains("name = \"Grok 4.5\""));
    assert!(text.contains("api_backend = \"responses\""));
    assert!(text.contains("context_window = 1000000"));
    assert!(text.contains("supports_backend_search = true"));
    assert!(text.contains("api_key = \"sk-native-secret\""));
}

#[test]
fn kimi_toml_roundtrip_and_secret_unchanged() {
    let dir = tempdir().unwrap();
    let home = dir.path();
    std::fs::write(
        home.join("config.toml"),
        r#"default_model = "kimi-k2"
default_provider = "moonshot"

[providers.moonshot]
base_url = "https://api.moonshot.cn/v1"
api_key = "sk-kimi-old"
"#,
    )
    .unwrap();

    let svc = ConfigurationService::new();
    let doc = svc.read_at(AgentId::Kimi, Some(home)).unwrap();
    assert_eq!(
        doc.values.get("model").and_then(|v| v.as_str()),
        Some("kimi-k2")
    );
    assert_eq!(
        doc.values.get("baseUrl").and_then(|v| v.as_str()),
        Some("https://api.moonshot.cn/v1")
    );
    assert_eq!(
        doc.values.get("apiKey").and_then(|v| v.as_str()),
        Some(SECRET_REDACTED)
    );
    assert_eq!(
        doc.values.get("providerSlug").and_then(|v| v.as_str()),
        Some("moonshot")
    );

    // Apply non-secret fields + redacted apiKey → keep native secret.
    let mut desired = BTreeMap::new();
    desired.insert("model".into(), json!("kimi-k2.5"));
    desired.insert("baseUrl".into(), json!("https://api.example.kimi"));
    desired.insert("apiKey".into(), json!(SECRET_REDACTED));
    desired.insert("providerSlug".into(), json!("moonshot"));
    svc.apply_at(AgentId::Kimi, &desired, Some(home)).unwrap();

    let text = std::fs::read_to_string(home.join("config.toml")).unwrap();
    assert!(text.contains("kimi-k2.5"), "{text}");
    assert!(text.contains("https://api.example.kimi"), "{text}");
    assert!(text.contains("sk-kimi-old"), "secret must be preserved; {text}");
    assert!(!text.contains(SECRET_REDACTED), "{text}");

    // Materialize pool settings from base content + new secret value.
    let base = json!({ "format": "toml", "content": text });
    let mut mat = BTreeMap::new();
    mat.insert("model".into(), json!("kimi-k2.5"));
    mat.insert("baseUrl".into(), json!("https://api.example.kimi"));
    mat.insert("apiKey".into(), json!("sk-kimi-new"));
    mat.insert("providerSlug".into(), json!("moonshot"));
    let raw = svc
        .materialize_settings_config(AgentId::Kimi, &mat, Some(&base))
        .unwrap();
    let content = raw["content"].as_str().unwrap_or("");
    assert!(content.contains("sk-kimi-new"), "{content}");
    assert!(content.contains("kimi-k2.5"), "{content}");
}

#[test]
fn kimi_invalid_toml_errors() {
    let dir = tempdir().unwrap();
    let home = dir.path();
    std::fs::write(home.join("config.toml"), "[[[not valid").unwrap();
    let svc = ConfigurationService::new();
    let err = svc.read_at(AgentId::Kimi, Some(home)).unwrap_err();
    assert_eq!(err.code(), "invalid_arg");
}
