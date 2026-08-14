//! Grok Build CLI `updates.jsonl` usage source (ccusage adapter-grok).

use std::path::{Path, PathBuf};

use crate::error::Result;
use crate::platform::usage::source::{UsageFileParser, UsageLineOutcome, UsageSource};
use crate::platform::AgentKey;
use crate::usage::grok::{discover_grok_files, line_might_have_usage_grok, GrokParser};

pub struct GrokUsageSource;

impl UsageSource for GrokUsageSource {
    fn agent_key(&self) -> AgentKey {
        AgentKey::parse("grok").expect("builtin usage source key must be valid")
    }

    fn parser_version(&self) -> u32 {
        2
    }

    fn discover_files(&self) -> Result<Vec<PathBuf>> {
        discover_grok_files()
    }

    fn begin_file(&self, path: &Path, _byte_offset: u64) -> Box<dyn UsageFileParser> {
        Box::new(GrokFileParser {
            inner: GrokParser::new(path),
        })
    }
}

struct GrokFileParser {
    inner: GrokParser,
}

impl UsageFileParser for GrokFileParser {
    fn on_line(&mut self, line: &str, session_id: Option<&str>) -> UsageLineOutcome {
        if !line_might_have_usage_grok(line) {
            return UsageLineOutcome::Skipped;
        }
        match self.inner.extract_line(line, session_id) {
            Ok(events) if events.is_empty() => UsageLineOutcome::Skipped,
            Ok(mut events) if events.len() == 1 => {
                UsageLineOutcome::Event(events.pop().expect("len == 1"))
            }
            Ok(events) => UsageLineOutcome::Events(events),
            Err(()) => UsageLineOutcome::Failed,
        }
    }
}
