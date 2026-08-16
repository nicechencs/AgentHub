//! Stream parser wrapper used by agents that decode via `utils::stream_parse`.

use crate::models::ProcessStep;
use crate::platform::stream::StreamParser;
use crate::platform::AgentKey;

pub(crate) struct FnStreamParser {
    key: AgentKey,
    parse: fn(&str) -> Option<Vec<ProcessStep>>,
}

impl FnStreamParser {
    pub(crate) fn new(key: &'static str, parse: fn(&str) -> Option<Vec<ProcessStep>>) -> Self {
        Self {
            key: AgentKey::parse(key).expect("built-in stream parser key must be valid"),
            parse,
        }
    }
}

impl StreamParser for FnStreamParser {
    fn agent_key(&self) -> AgentKey {
        self.key.clone()
    }

    fn parse_line(&self, line: &str) -> Option<Vec<ProcessStep>> {
        (self.parse)(line)
    }
}
