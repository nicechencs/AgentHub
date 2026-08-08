//! Claude / WorkBuddy / Grok usage sources (Claude-like JSONL fields).

use std::path::{Path, PathBuf};

use crate::error::Result;
use crate::models::AgentId;
use crate::platform::usage::source::{UsageFileParser, UsageLineOutcome, UsageSource};
use crate::platform::AgentKey;
use crate::usage::session_jsonl::{
    discover_claude_files, discover_grok_files, discover_workbuddy_files, extract_claude_like,
    line_might_have_usage_claude_like,
};

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

macro_rules! claude_like_source {
    ($name:ident, $key:literal, $agent:expr, $discover:path) => {
        pub struct $name;

        impl UsageSource for $name {
            fn agent_key(&self) -> AgentKey {
                AgentKey::parse($key).expect("builtin usage source key must be valid")
            }

            fn discover_files(&self) -> Result<Vec<PathBuf>> {
                $discover()
            }

            fn begin_file(&self, _path: &Path, _byte_offset: u64) -> Box<dyn UsageFileParser> {
                Box::new(ClaudeLikeParser { agent: $agent })
            }
        }
    };
}

claude_like_source!(
    ClaudeUsageSource,
    "claude",
    AgentId::Claude,
    discover_claude_files
);
claude_like_source!(
    WorkBuddyUsageSource,
    "workbuddy",
    AgentId::WorkBuddy,
    discover_workbuddy_files
);
claude_like_source!(GrokUsageSource, "grok", AgentId::Grok, discover_grok_files);
