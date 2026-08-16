//! Stream parser registry tests (separate from production modules).

use std::sync::Arc;

use crate::models::{AgentId, OutputStream, ProcessMode, ProcessStep};
use crate::platform::stream::{
    builtin_stream_registry, StreamParseError, StreamParser, StreamParserRegistry,
};
use crate::platform::AgentKey;
use crate::utils::stream_parse::{StreamOutput, StreamSession};

struct TestStreamParser {
    key: AgentKey,
    prefix: &'static str,
}

impl TestStreamParser {
    fn new(key: AgentKey, prefix: &'static str) -> Self {
        Self { key, prefix }
    }
}

impl StreamParser for TestStreamParser {
    fn agent_key(&self) -> AgentKey {
        self.key.clone()
    }

    fn parse_line(&self, line: &str) -> Option<Vec<ProcessStep>> {
        Some(vec![ProcessStep::Text {
            text: format!("{}:{line}", self.prefix),
        }])
    }
}

#[test]
fn builtin_registers_structured_agents_not_text_only() {
    let reg = builtin_stream_registry();
    let agents = [
        AgentId::Claude,
        AgentId::Codex,
        AgentId::Kimi,
        AgentId::Grok,
        AgentId::Pi,
    ];
    for agent in agents {
        assert!(reg.contains(agent), "{agent:?}");
        assert!(
            reg.get(&AgentKey::from_agent_id(agent)).is_some(),
            "{agent:?}"
        );
    }
    assert_eq!(
        reg.supported_keys(),
        agents
            .iter()
            .copied()
            .map(AgentKey::from_agent_id)
            .collect::<Vec<_>>()
    );
    assert_eq!(reg.supported_agents(), agents.to_vec());
    assert!(!reg.contains(AgentId::WorkBuddy));
    assert!(!reg.contains(AgentId::Cursor));
    assert!(reg.require(AgentId::Cursor).is_err());
}

#[test]
fn unknown_valid_key_registers_queries_and_executes() {
    let key = AgentKey::parse("third-party-stream").expect("valid unknown key");
    assert!(
        !AgentId::ALL
            .iter()
            .any(|agent| agent.as_str() == key.as_str()),
        "test identity must not borrow a real AgentId"
    );

    let mut reg = StreamParserRegistry::new();
    let unsupported = reg
        .require_key(&key)
        .err()
        .expect("unregistered key must be unsupported");
    assert_eq!(unsupported.agent_key, key.as_str());
    assert_eq!(unsupported.code, "unsupported");

    reg.register(Arc::new(TestStreamParser::new(key.clone(), "unknown")))
        .expect("register unknown parser");
    assert!(reg.contains_key(&key));
    assert_eq!(reg.supported_keys(), vec![key.clone()]);
    assert!(reg.supported_agents().is_empty());

    let parser = reg.get(&key).expect("query unknown parser");
    assert_eq!(parser.agent_key(), key);
    assert_eq!(
        parser.parse_line("payload"),
        Some(vec![ProcessStep::Text {
            text: "unknown:payload".to_string(),
        }])
    );
    assert!(reg.require_key(&parser.agent_key()).is_ok());
}

#[test]
fn unknown_key_parser_executes_through_stream_session_feed_and_flush() {
    let key = AgentKey::parse("third-party-stream-session").expect("valid unknown key");
    let mut reg = StreamParserRegistry::new();
    reg.register(Arc::new(TestStreamParser::new(key.clone(), "session")))
        .expect("register unknown parser");

    let mut session =
        StreamSession::for_agent_key(key.clone(), ProcessMode::Structured, true, &reg);
    assert_eq!(session.agent_key(), &key);
    assert!(session.is_structured());
    assert!(session.feed(OutputStream::Stdout, "payload").is_empty());

    let output = session.flush();
    assert!(output.iter().any(|item| matches!(
        item,
        StreamOutput::Chunk {
            stream: OutputStream::Stdout,
            text
        } if text == "session:payload"
    )));
    assert!(output.iter().any(|item| matches!(
        item,
        StreamOutput::Step(ProcessStep::Text { text }) if text == "session:payload"
    )));
    assert_eq!(session.assistant_text(), "session:payload");
}

#[test]
fn duplicate_registration_is_rejected_without_overwrite() {
    let key = AgentKey::parse("duplicate-stream").expect("valid key");
    let mut reg = StreamParserRegistry::new();
    reg.register(Arc::new(TestStreamParser::new(key.clone(), "first")))
        .expect("first registration");

    let error = reg
        .register(Arc::new(TestStreamParser::new(key.clone(), "second")))
        .expect_err("duplicate registration must fail");
    assert_eq!(error.agent_key, key);
    assert_eq!(reg.supported_keys(), vec![key.clone()]);
    assert_eq!(
        reg.get(&key)
            .expect("original parser retained")
            .parse_line("line"),
        Some(vec![ProcessStep::Text {
            text: "first:line".to_string(),
        }])
    );
}

#[test]
fn legacy_agent_id_helpers_delegate_to_key_native_lookup() {
    let key = AgentKey::from_agent_id(AgentId::Codex);
    let mut reg = StreamParserRegistry::new();
    reg.register(Arc::new(TestStreamParser::new(key.clone(), "legacy")))
        .expect("register known key");

    assert!(reg.get_agent_id(AgentId::Codex).is_some());
    assert!(reg.contains(AgentId::Codex));
    assert!(reg.require(AgentId::Codex).is_ok());
    assert!(!reg.contains(AgentId::Cursor));

    let error = reg
        .require(AgentId::Cursor)
        .err()
        .expect("missing legacy id must be unsupported");
    assert_eq!(error, StreamParseError::unsupported(AgentId::Cursor));
}
