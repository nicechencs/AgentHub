//! UsageSource extension port — Agent-specific discovery + parse only.

use std::path::{Path, PathBuf};

use crate::error::Result;
use crate::models::ParsedUsageEvent;
use crate::platform::AgentKey;

/// Raw usage event from a source parser (platform persists after pricing).
///
/// Alias of the existing wire/DTO shape so historical fixtures stay equivalent.
pub type RawUsageEvent = ParsedUsageEvent;

/// How stored input/cache tokens should be interpreted for cost recompute.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TokenAccounting {
    /// Input is non-cached (or cache is separate); default for most agents.
    #[default]
    Standard,
    /// OpenAI/Codex layout: legacy rows may include cache inside input.
    CodexBillable,
}

/// Result of feeding one log line to a file parser session.
#[derive(Debug)]
pub enum UsageLineOutcome {
    Event(RawUsageEvent),
    Skipped,
    Failed,
}

/// Per-file parse session (holds model inheritance and other agent state).
pub trait UsageFileParser: Send {
    fn on_line(&mut self, line: &str, session_id: Option<&str>) -> UsageLineOutcome;
}

/// Agent integration contribution for usage collection.
///
/// Platform owns cursors, dedupe insert, pricing, and queries.
pub trait UsageSource: Send + Sync {
    fn agent_key(&self) -> AgentKey;

    /// Stable parser identity for future rescan (not stored in DB this task).
    fn parser_version(&self) -> u32 {
        1
    }

    fn token_accounting(&self) -> TokenAccounting {
        TokenAccounting::Standard
    }

    /// Discover usage log files for this agent (read-only).
    fn discover_files(&self) -> Result<Vec<PathBuf>>;

    /// Start incremental parse for one file (`byte_offset` already resolved).
    fn begin_file(&self, path: &Path, byte_offset: u64) -> Box<dyn UsageFileParser>;
}
