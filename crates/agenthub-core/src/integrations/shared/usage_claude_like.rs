//! Claude-like JSONL usage source (Claude / WorkBuddy).

use std::path::{Path, PathBuf};

use crate::error::Result;
use crate::models::AgentId;
use crate::platform::usage::{UsageFileParser, UsageLineOutcome, UsageSource};
use crate::platform::AgentKey;
use crate::usage::session_jsonl::{extract_claude_like, line_might_have_usage_claude_like};

struct ClaudeLikeParser {
    agent: AgentId,
}

impl UsageFileParser for ClaudeLikeParser {
    fn on_line(&mut self, line: &str, session_id: Option<&str>) -> UsageLineOutcome {
        if !line_might_have_usage_claude_like(line) {
            return UsageLineOutcome::Skipped;
        }
        match extract_claude_like(self.agent, line, session_id) {
            Ok(Some(ev)) => UsageLineOutcome::Event(ev),
            Ok(None) => UsageLineOutcome::Skipped,
            Err(()) => UsageLineOutcome::Failed,
        }
    }
}

pub(crate) struct ClaudeLikeUsageSource {
    key: &'static str,
    agent: AgentId,
    discover: fn() -> Result<Vec<PathBuf>>,
}

impl ClaudeLikeUsageSource {
    pub(crate) fn new(
        key: &'static str,
        agent: AgentId,
        discover: fn() -> Result<Vec<PathBuf>>,
    ) -> Self {
        Self {
            key,
            agent,
            discover,
        }
    }
}

impl UsageSource for ClaudeLikeUsageSource {
    fn agent_key(&self) -> AgentKey {
        AgentKey::parse(self.key).expect("builtin usage source key must be valid")
    }

    fn discover_files(&self) -> Result<Vec<PathBuf>> {
        (self.discover)()
    }

    fn begin_file(&self, _path: &Path, _byte_offset: u64) -> Box<dyn UsageFileParser> {
        Box::new(ClaudeLikeParser { agent: self.agent })
    }
}
