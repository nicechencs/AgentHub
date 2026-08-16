//! Codex session JSONL usage source.

use std::path::{Path, PathBuf};

use crate::error::Result;
use crate::logging::targets;
use crate::models::AgentId;
use crate::platform::usage::{TokenAccounting, UsageFileParser, UsageLineOutcome, UsageSource};
use crate::platform::AgentKey;
use crate::usage::session_jsonl::{
    bootstrap_codex_prefix, discover_codex_files, extract_codex, line_might_have_usage_codex,
    read_codex_default_model, CodexParseState,
};
use crate::utils::paths::agent_home;
use crate::utils::redact::redact_text;

pub struct CodexUsageSource;

struct CodexParser {
    path: String,
    state: CodexParseState,
}

impl Drop for CodexParser {
    fn drop(&mut self) {
        // Per-file parse summary (ccusage alignment diagnostics). Only when something happened.
        if self.state.emitted == 0
            && self.state.skipped_dup_total == 0
            && self.state.skipped_burst == 0
            && !self.state.forkish
        {
            return;
        }
        let path = redact_text(&self.path);
        tracing::debug!(
            module = targets::USAGE,
            op = "codex_parse_file",
            path = %path,
            forkish = self.state.forkish,
            burst_skip = self.state.burst_skip_active
                || self.state.skipped_burst > 0,
            emitted = self.state.emitted,
            skipped_dup_total = self.state.skipped_dup_total,
            skipped_burst = self.state.skipped_burst,
            "codex session usage parse summary"
        );
    }
}

impl UsageFileParser for CodexParser {
    fn on_line(&mut self, line: &str, session_id: Option<&str>) -> UsageLineOutcome {
        // turn_context is included in prefilter so model inheritance still runs.
        if !line_might_have_usage_codex(line) {
            return UsageLineOutcome::Skipped;
        }
        match extract_codex(line, session_id, &mut self.state) {
            Ok(Some(ev)) => UsageLineOutcome::Event(ev),
            Ok(None) => UsageLineOutcome::Skipped,
            Err(()) => UsageLineOutcome::Failed,
        }
    }
}

impl UsageSource for CodexUsageSource {
    fn agent_key(&self) -> AgentKey {
        AgentKey::parse("codex").expect("builtin usage source key must be valid")
    }

    fn token_accounting(&self) -> TokenAccounting {
        TokenAccounting::CodexBillable
    }

    fn discover_files(&self) -> Result<Vec<PathBuf>> {
        discover_codex_files()
    }

    fn begin_file(&self, path: &Path, byte_offset: u64) -> Box<dyn UsageFileParser> {
        // Prefer: turn_context inheritance → ~/.codex/config.toml `model` → "unknown".
        let from_cfg = agent_home(AgentId::Codex)
            .ok()
            .and_then(|root| read_codex_default_model(&root));
        // Full-file scan (offset 0): detect fork/subagent rewritten-history burst.
        // Incremental resume: inherit model + previous total_token_usage; no burst skip.
        let state = if byte_offset == 0 {
            CodexParseState::init_from_file(path, from_cfg)
        } else {
            let (prefix_model, previous_total) = bootstrap_codex_prefix(path, byte_offset);
            let model = prefix_model.or(from_cfg);
            tracing::debug!(
                module = targets::USAGE,
                op = "codex_begin_file",
                path = %redact_text(&path.to_string_lossy()),
                byte_offset,
                has_previous_total = previous_total.is_some(),
                model = model.as_deref().unwrap_or("unknown"),
                "codex usage file resume mid-cursor"
            );
            CodexParseState::resume_from_prefix(model, previous_total)
        };
        if byte_offset == 0 && (state.forkish || state.burst_skip_active) {
            let path_s = redact_text(&path.to_string_lossy());
            tracing::debug!(
                module = targets::USAGE,
                op = "codex_begin_file",
                path = %path_s,
                byte_offset,
                forkish = state.forkish,
                burst_skip_active = state.burst_skip_active,
                model = state.model.as_deref().unwrap_or("unknown"),
                "codex usage file open (fork/burst state)"
            );
        }
        Box::new(CodexParser {
            path: path.to_string_lossy().into_owned(),
            state,
        })
    }
}

pub fn register(ctx: &mut crate::integrations::IntegrationContext<'_>) {
    ctx.usage
        .register(std::sync::Arc::new(CodexUsageSource))
        .expect("unique built-in usage source");
}
