use std::path::Path;
use std::sync::Arc;

use crate::error::Result;
use crate::models::AgentId;
use crate::platform::AgentKey;

use super::{is_path_safe_session_id, SessionTitleRegistry, SessionTitleSource};

/// One known Agent title, keyed so tests can drive the registry without files.
struct StubTitleSource {
    key: AgentKey,
    title: Option<&'static str>,
}

impl StubTitleSource {
    fn new(key: AgentKey, title: Option<&'static str>) -> Self {
        Self { key, title }
    }
}

impl SessionTitleSource for StubTitleSource {
    fn agent_key(&self) -> AgentKey {
        self.key.clone()
    }

    fn title_for(&self, _home: &Path, session_id: &str) -> Result<Option<String>> {
        if session_id.is_empty() {
            return Ok(None);
        }
        Ok(self.title.map(ToOwned::to_owned))
    }
}

#[test]
fn unknown_valid_key_registers_and_resolves_a_title() {
    let key = AgentKey::parse("title-test-agent").unwrap();
    let mut registry = SessionTitleRegistry::new();
    registry
        .register(Arc::new(StubTitleSource::new(key.clone(), Some("side quest"))))
        .unwrap();

    let source = registry.get(&key).expect("unknown valid key is registered");
    assert_eq!(source.agent_key(), key);
    assert_eq!(
        source
            .title_for(Path::new("unused-title-test-home"), "session-1")
            .unwrap()
            .as_deref(),
        Some("side quest")
    );
    assert_eq!(registry.supported_keys(), vec![key]);
}

#[test]
fn duplicate_key_is_rejected_without_replacing_existing_source() {
    let key = AgentKey::parse("title-duplicate-agent").unwrap();
    let mut registry = SessionTitleRegistry::new();
    registry
        .register(Arc::new(StubTitleSource::new(key.clone(), Some("first"))))
        .unwrap();

    let error = registry
        .register(Arc::new(StubTitleSource::new(key.clone(), Some("second"))))
        .unwrap_err();

    assert_eq!(error.code(), "invalid_arg");
    assert_eq!(
        registry
            .get(&key)
            .unwrap()
            .title_for(Path::new("unused-title-test-home"), "session-1")
            .unwrap()
            .as_deref(),
        Some("first")
    );
}

#[test]
fn explicit_order_and_legacy_agent_id_helper_are_stable() {
    let first = AgentKey::parse("zeta-title-agent").unwrap();
    let second = AgentKey::parse("alpha-title-agent").unwrap();
    let claude = AgentKey::from_agent_id(AgentId::Claude);
    let mut registry = SessionTitleRegistry::new();
    registry
        .register(Arc::new(StubTitleSource::new(first.clone(), None)))
        .unwrap();
    registry
        .register(Arc::new(StubTitleSource::new(second.clone(), None)))
        .unwrap();
    registry
        .register(Arc::new(StubTitleSource::new(claude.clone(), None)))
        .unwrap();

    assert_eq!(registry.supported_keys(), vec![first, second, claude]);
    assert!(registry.get_agent_id(AgentId::Claude).is_some());
}

#[test]
fn rejects_session_ids_that_carry_path_syntax() {
    assert!(is_path_safe_session_id("019dab0b-373c-76e2-9900-e02a4b959f91"));
    assert!(is_path_safe_session_id("kiro-http:abc123"));
    assert!(is_path_safe_session_id("  thread-1  "));

    assert!(!is_path_safe_session_id(""));
    assert!(!is_path_safe_session_id("  "));
    assert!(!is_path_safe_session_id("../secrets"));
    assert!(!is_path_safe_session_id("a/b"));
    assert!(!is_path_safe_session_id(r"a\b"));
    assert!(!is_path_safe_session_id(".."));
}
