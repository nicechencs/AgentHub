//! Kimi Code wire.jsonl usage source.

use std::path::{Path, PathBuf};

use crate::error::Result;
use crate::platform::usage::source::{UsageFileParser, UsageLineOutcome, UsageSource};
use crate::platform::AgentKey;
use crate::usage::session_jsonl::{
    bootstrap_kimi_model, discover_kimi_files, extract_kimi, kimi_root_from_wire_path,
    line_might_have_usage_kimi, note_kimi_model_from_line, read_kimi_default_model,
};

pub struct KimiUsageSource;

struct KimiParser {
    model: Option<String>,
}

impl UsageFileParser for KimiParser {
    fn on_line(&mut self, line: &str, session_id: Option<&str>) -> UsageLineOutcome {
        if !line_might_have_usage_kimi(line) {
            note_kimi_model_from_line(line, &mut self.model);
            return UsageLineOutcome::Skipped;
        }
        note_kimi_model_from_line(line, &mut self.model);
        match extract_kimi(line, session_id, self.model.as_deref()) {
            Ok(Some(ev)) => {
                self.model = Some(ev.model.clone());
                UsageLineOutcome::Event(ev)
            }
            Ok(None) => UsageLineOutcome::Skipped,
            Err(()) => UsageLineOutcome::Failed,
        }
    }
}

impl UsageSource for KimiUsageSource {
    fn agent_key(&self) -> AgentKey {
        AgentKey::parse("kimi").expect("builtin usage source key must be valid")
    }

    fn discover_files(&self) -> Result<Vec<PathBuf>> {
        discover_kimi_files()
    }

    fn begin_file(&self, path: &Path, byte_offset: u64) -> Box<dyn UsageFileParser> {
        let from_cfg = kimi_root_from_wire_path(path)
            .as_ref()
            .and_then(|root| read_kimi_default_model(root));
        let model = if byte_offset > 0 {
            bootstrap_kimi_model(path, byte_offset).or(from_cfg)
        } else {
            from_cfg
        };
        Box::new(KimiParser { model })
    }
}
