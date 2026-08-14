//! Agent-agnostic usage file collection (cursor + line loop).

use std::fs::{self, File};
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::Path;
use std::time::SystemTime;

use crate::error::{AppError, Result};
use crate::logging::targets;
use crate::models::AgentId;
use crate::storage::{UsageCursor, UsageRepo};
use crate::usage::session_jsonl::CollectStats;
use crate::utils::redact::redact_text;

use super::registry::builtin_usage_registry;
use super::source::{UsageLineOutcome, UsageSource};

/// Collect via builtin registry. Unknown / unsupported agents return empty
/// supported=false semantics at the service layer; here we error only if
/// called without a registered source.
pub fn collect_for_agent_id(agent: AgentId, repo: &UsageRepo) -> Result<CollectStats> {
    match builtin_usage_registry().get_agent_id(agent) {
        Some(source) => collect_with_source_for_agent_id(source.as_ref(), agent, repo),
        None => Ok(CollectStats {
            events: Vec::new(),
            cursors: Vec::new(),
            skipped: 0,
            failed: 0,
        }),
    }
}

/// Key-native execution path. Until usage persistence is migrated to AgentKey,
/// sources that discover files must enter through collect_with_source_for_agent_id.
pub fn collect_with_source(source: &dyn UsageSource, repo: &UsageRepo) -> Result<CollectStats> {
    collect_with_optional_agent_id(source, None, repo)
}

/// Legacy persistence boundary: the façade already knows the closed AgentId and
/// passes it separately; UsageSource itself remains AgentKey-native.
pub fn collect_with_source_for_agent_id(
    source: &dyn UsageSource,
    agent: AgentId,
    repo: &UsageRepo,
) -> Result<CollectStats> {
    collect_with_optional_agent_id(source, Some(agent), repo)
}

fn collect_with_optional_agent_id(
    source: &dyn UsageSource,
    persistence_agent: Option<AgentId>,
    repo: &UsageRepo,
) -> Result<CollectStats> {
    let files = source.discover_files()?;
    let mut events = Vec::new();
    let mut cursors = Vec::new();
    let mut skipped = 0u64;
    let mut failed = 0u64;

    for path in files {
        let agent = persistence_agent.ok_or_else(|| {
            AppError::InvalidArg(format!(
                "usage source '{}' requires a legacy AgentId at the persistence boundary",
                source.agent_key()
            ))
        })?;
        match parse_one_file(source, agent, &path, repo) {
            Ok(batch) => {
                skipped += batch.skipped;
                failed += batch.failed;
                events.extend(batch.events);
                cursors.push(batch.cursor);
            }
            Err(e) => {
                let path_s = path.to_string_lossy();
                let msg = redact_text(&e.to_string());
                tracing::warn!(
                    module = targets::USAGE,
                    code = e.code(),
                    op = "collect_file",
                    agent = agent.as_str(),
                    path = %path_s,
                    "{msg}"
                );
                failed += 1;
            }
        }
    }

    Ok(CollectStats {
        events,
        cursors,
        skipped,
        failed,
    })
}

pub(crate) struct FileBatch {
    pub events: Vec<crate::models::ParsedUsageEvent>,
    pub cursor: UsageCursor,
    pub skipped: u64,
    pub failed: u64,
}

/// Parse one session file through the registered UsageSource (unit tests only).
#[cfg(test)]
pub(crate) fn parse_file_for_agent_id(
    agent: AgentId,
    path: &Path,
    repo: &UsageRepo,
) -> Result<FileBatch> {
    let source = builtin_usage_registry()
        .get_agent_id(agent)
        .ok_or_else(|| {
            AppError::InvalidArg(format!(
                "no usage source registered for agent {}",
                agent.as_str()
            ))
        })?;
    parse_one_file(source.as_ref(), agent, path, repo)
}

fn parse_one_file(
    source: &dyn UsageSource,
    agent: AgentId,
    path: &Path,
    repo: &UsageRepo,
) -> Result<FileBatch> {
    let path_s = path.to_string_lossy().to_string();
    let meta = fs::metadata(path)?;
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let len = meta.len() as i64;

    let mut offset = 0i64;
    if let Some(cur) = repo.get_cursor(&path_s)? {
        if cur.file_mtime == mtime && cur.byte_offset <= len {
            offset = cur.byte_offset;
        }
        if cur.byte_offset > len {
            offset = 0;
        }
    }

    let session_id = crate::usage::session_jsonl::session_id_from_path(path);
    let mut parser = source.begin_file(path, offset as u64);

    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    if offset > 0 {
        reader.seek(SeekFrom::Start(offset as u64))?;
    }

    let mut events = Vec::new();
    let mut skipped = 0u64;
    let mut failed = 0u64;
    let mut buf = String::new();

    loop {
        buf.clear();
        let n = reader.read_line(&mut buf)?;
        if n == 0 {
            break;
        }
        let line = buf.trim();
        if line.is_empty() {
            continue;
        }
        match parser.on_line(line, session_id.as_deref()) {
            UsageLineOutcome::Event(ev) => events.push(ev),
            UsageLineOutcome::Events(batch) => events.extend(batch),
            UsageLineOutcome::Skipped => skipped += 1,
            UsageLineOutcome::Failed => failed += 1,
        }
    }

    let new_offset = {
        let mut f = File::open(path)?;
        f.seek(SeekFrom::End(0))? as i64
    };

    Ok(FileBatch {
        events,
        cursor: UsageCursor {
            path: path_s,
            agent_id: agent,
            byte_offset: new_offset,
            file_mtime: mtime,
        },
        skipped,
        failed,
    })
}
