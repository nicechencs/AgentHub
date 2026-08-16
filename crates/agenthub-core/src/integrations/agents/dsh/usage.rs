//! DeepSeek Harness session JSONL usage source.

use std::path::{Path, PathBuf};

use crate::error::Result;
use crate::platform::usage::{UsageFileParser, UsageLineOutcome, UsageSource};
use crate::platform::AgentKey;
use crate::usage::session_jsonl::{
    discover_dsh_files, extract_dsh, line_might_have_usage_dsh, note_dsh_model_from_line,
};

pub struct DshUsageSource;

struct DshParser {
    model: Option<String>,
}

impl UsageFileParser for DshParser {
    fn on_line(&mut self, line: &str, session_id: Option<&str>) -> UsageLineOutcome {
        note_dsh_model_from_line(line, &mut self.model);
        if !line_might_have_usage_dsh(line) {
            return UsageLineOutcome::Skipped;
        }
        match extract_dsh(line, session_id, self.model.as_deref()) {
            Ok(Some(ev)) => {
                self.model = Some(ev.model.clone());
                UsageLineOutcome::Event(ev)
            }
            Ok(None) => UsageLineOutcome::Skipped,
            Err(()) => UsageLineOutcome::Failed,
        }
    }
}

impl UsageSource for DshUsageSource {
    fn agent_key(&self) -> AgentKey {
        AgentKey::parse("dsh").expect("builtin usage source key must be valid")
    }

    fn discover_files(&self) -> Result<Vec<PathBuf>> {
        discover_dsh_files()
    }

    fn begin_file(&self, _path: &Path, _byte_offset: u64) -> Box<dyn UsageFileParser> {
        Box::new(DshParser { model: None })
    }
}

pub fn register(ctx: &mut crate::integrations::IntegrationContext<'_>) {
    ctx.usage
        .register(std::sync::Arc::new(DshUsageSource))
        .expect("unique built-in usage source");
}
