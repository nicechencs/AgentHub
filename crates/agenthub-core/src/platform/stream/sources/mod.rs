//! StreamParser contributions (agent-specific NDJSON decoders).
//!
//! Implementations call into `utils/stream_parse::{claude,codex,...}` helpers.
//! TODO(P13): relocate under integrations/agents/<key>/.

use std::sync::Arc;

use super::parser::StreamParser;
use super::registry::StreamParserRegistry;
use crate::models::ProcessStep;
use crate::platform::AgentKey;
use crate::utils::stream_parse::{claude, codex, grok, kimi, pi};

macro_rules! line_parser {
    ($name:ident, $agent:expr, $parse:path) => {
        struct $name;

        impl StreamParser for $name {
            fn agent_key(&self) -> AgentKey {
                AgentKey::parse($agent).expect("built-in stream parser key must be valid")
            }

            fn parse_line(&self, line: &str) -> Option<Vec<ProcessStep>> {
                $parse(line)
            }
        }
    };
}

line_parser!(ClaudeStreamParser, "claude", claude::parse_line);
line_parser!(CodexStreamParser, "codex", codex::parse_line);
line_parser!(KimiStreamParser, "kimi", kimi::parse_line);
line_parser!(PiStreamParser, "pi", pi::parse_line);
line_parser!(GrokStreamParser, "grok", grok::parse_line);

pub fn build_registry() -> StreamParserRegistry {
    let mut reg = StreamParserRegistry::new();
    reg.register(Arc::new(ClaudeStreamParser))
        .expect("unique built-in stream parser key");
    reg.register(Arc::new(CodexStreamParser))
        .expect("unique built-in stream parser key");
    reg.register(Arc::new(KimiStreamParser))
        .expect("unique built-in stream parser key");
    reg.register(Arc::new(PiStreamParser))
        .expect("unique built-in stream parser key");
    reg.register(Arc::new(GrokStreamParser))
        .expect("unique built-in stream parser key");
    // WorkBuddy / Cursor: no structured parser (text mode).
    reg
}
