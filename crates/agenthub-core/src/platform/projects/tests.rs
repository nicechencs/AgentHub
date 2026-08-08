use std::path::Path;
use std::sync::Arc;

use crate::error::Result;
use crate::models::{AgentId, AgentProject, AgentSession};
use crate::platform::AgentKey;
use crate::services::project_service::{
    list_projects_for_key_home, list_sessions_for_key_home, list_sessions_for_project_key_home,
};

use super::{ProjectScanContext, ProjectSource, ProjectSourceRegistry};

struct EmptyProjectSource {
    key: AgentKey,
}

impl EmptyProjectSource {
    fn new(key: AgentKey) -> Self {
        Self { key }
    }
}

impl ProjectSource for EmptyProjectSource {
    fn agent_key(&self) -> AgentKey {
        self.key.clone()
    }

    fn list_projects(&self, _ctx: &ProjectScanContext<'_>) -> Result<Vec<AgentProject>> {
        Ok(Vec::new())
    }

    fn list_sessions(&self, _ctx: &ProjectScanContext<'_>) -> Result<Vec<AgentSession>> {
        Ok(Vec::new())
    }

    fn list_sessions_in_project(
        &self,
        _ctx: &ProjectScanContext<'_>,
        _project_id: &str,
        _key: &str,
    ) -> Result<Vec<AgentSession>> {
        Ok(Vec::new())
    }
}

#[test]
fn unknown_valid_key_registers_queries_and_executes() {
    let key = AgentKey::parse("project-test-agent").unwrap();
    let mut registry = ProjectSourceRegistry::new();
    registry
        .register(Arc::new(EmptyProjectSource::new(key.clone())))
        .unwrap();

    let source = registry.get(&key).expect("unknown valid key is registered");
    assert_eq!(source.agent_key(), key);

    let home = Path::new("unused-project-test-home");
    assert!(list_projects_for_key_home(&registry, &key, home, None)
        .unwrap()
        .is_empty());
    assert!(list_sessions_for_key_home(&registry, &key, home, None)
        .unwrap()
        .is_empty());
    assert!(list_sessions_for_project_key_home(
        &registry,
        &key,
        home,
        "project-test-agent:proj:any",
        "any",
        None,
    )
    .unwrap()
    .is_empty());
    assert_eq!(registry.supported_keys(), vec![key]);
}

#[test]
fn duplicate_key_is_rejected_without_replacing_existing_source() {
    let key = AgentKey::parse("project-duplicate-agent").unwrap();
    let mut registry = ProjectSourceRegistry::new();
    registry
        .register(Arc::new(EmptyProjectSource::new(key.clone())))
        .unwrap();

    let error = registry
        .register(Arc::new(EmptyProjectSource::new(key.clone())))
        .unwrap_err();

    assert_eq!(error.code(), "invalid_arg");
    assert!(registry.get(&key).is_some());
    assert_eq!(registry.supported_keys(), vec![key]);
}

#[test]
fn explicit_order_and_legacy_agent_id_helpers_are_stable() {
    let first = AgentKey::parse("zeta-project-agent").unwrap();
    let second = AgentKey::parse("alpha-project-agent").unwrap();
    let claude = AgentKey::from_agent_id(AgentId::Claude);
    let mut registry = ProjectSourceRegistry::new();
    registry
        .register(Arc::new(EmptyProjectSource::new(first.clone())))
        .unwrap();
    registry
        .register(Arc::new(EmptyProjectSource::new(second.clone())))
        .unwrap();
    registry
        .register(Arc::new(EmptyProjectSource::new(claude.clone())))
        .unwrap();

    assert_eq!(registry.supported_keys(), vec![first, second, claude]);
    assert!(registry.get_agent_id(AgentId::Claude).is_some());
    assert!(registry.contains(AgentId::Claude));
}
