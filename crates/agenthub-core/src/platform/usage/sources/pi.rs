//! Pi coding-agent session JSONL usage source.

use std::path::{Path, PathBuf};

use crate::error::Result;
use crate::platform::usage::source::{UsageFileParser, UsageLineOutcome, UsageSource};
use crate::platform::AgentKey;
use crate::usage::session_jsonl::{
    bootstrap_pi_model, discover_pi_files, extract_pi, line_might_have_usage_pi,
    note_pi_model_from_line, read_pi_default_model,
};
use crate::utils::paths::home_dir;

pub struct PiUsageSource;

struct PiParser {
    model: Option<String>,
}

impl UsageFileParser for PiParser {
    fn on_line(&mut self, line: &str, session_id: Option<&str>) -> UsageLineOutcome {
        if !line_might_have_usage_pi(line) {
            note_pi_model_from_line(line, &mut self.model);
            return UsageLineOutcome::Skipped;
        }
        note_pi_model_from_line(line, &mut self.model);
        match extract_pi(line, session_id, self.model.as_deref()) {
            Ok(Some(ev)) => {
                self.model = Some(ev.model.clone());
                UsageLineOutcome::Event(ev)
            }
            Ok(None) => UsageLineOutcome::Skipped,
            Err(()) => UsageLineOutcome::Failed,
        }
    }
}

impl UsageSource for PiUsageSource {
    fn agent_key(&self) -> AgentKey {
        AgentKey::parse("pi").expect("builtin usage source key must be valid")
    }

    fn discover_files(&self) -> Result<Vec<PathBuf>> {
        discover_pi_files()
    }

    fn begin_file(&self, path: &Path, byte_offset: u64) -> Box<dyn UsageFileParser> {
        // settings.json defaultModel, then model_change lines in the session log.
        let from_settings = path
            .parent()
            .and_then(|sessions| sessions.parent()) // agent/
            .and_then(read_pi_default_model)
            .or_else(|| {
                home_dir()
                    .ok()
                    .and_then(|h| read_pi_default_model(&h.join(".pi").join("agent")))
            });
        let model = if byte_offset > 0 {
            bootstrap_pi_model(path, byte_offset).or(from_settings)
        } else {
            from_settings
        };
        Box::new(PiParser { model })
    }
}
