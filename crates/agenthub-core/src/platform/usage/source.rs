//! UsageSource extension port — Agent-specific discovery + parse only.

use std::fs::File;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

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
    /// One log line can yield several events (Grok `modelUsage` map).
    Events(Vec<RawUsageEvent>),
    Skipped,
    Failed,
}

/// Line reader over one physical log plus its decode-failure signal.
pub struct UsageLogReader {
    pub reader: Box<dyn BufRead + Send>,
    /// Set when the log could not be fully decoded, so a short read is not
    /// mistaken for a complete one (compressed logs only).
    pub decode_error: Option<Arc<AtomicBool>>,
}

impl UsageLogReader {
    /// True when decoding stopped at damaged data instead of end of log.
    pub fn had_decode_error(&self) -> bool {
        self.decode_error
            .as_ref()
            .is_some_and(|flag| flag.load(std::sync::atomic::Ordering::Relaxed))
    }
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

    /// Line reader over the physical log, positioned at `byte_offset`.
    ///
    /// The default streams plain UTF-8 bytes, which is what every agent except
    /// DeepSeek Harness writes. DSH persists `*.jsonl.zstd` (concatenated zstd
    /// frames), so it overrides this with a decoding reader and the platform
    /// keeps one cursor plus one line loop for all agents.
    fn open_lines(&self, path: &Path, byte_offset: u64) -> Result<UsageLogReader> {
        let mut file = File::open(path)?;
        if byte_offset > 0 {
            file.seek(SeekFrom::Start(byte_offset))?;
        }
        Ok(UsageLogReader {
            reader: Box::new(BufReader::new(file)),
            decode_error: None,
        })
    }

    /// Start incremental parse for one file (`byte_offset` already resolved).
    fn begin_file(&self, path: &Path, byte_offset: u64) -> Box<dyn UsageFileParser>;

    /// Optional whole-store harvest (SQLite indexes). Default is empty — file
    /// discovery is the only path. Dedup still uses `raw_hash`.
    fn harvest_events(&self) -> Result<Vec<ParsedUsageEvent>> {
        Ok(Vec::new())
    }
}
