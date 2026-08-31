//! Gateway usage spool ingest: JSONL files → `gateway_usage` rows.
//!
//! Per-file byte cursors live in the shared `usage_cursors` table (path =
//! spool file absolute path, `agent_id` = `gateway` sentinel). Rows and the
//! cursor advance in one transaction per file, mirroring
//! `usage_repo::insert_batch_and_cursors`, so a crash replays idempotently
//! (`request_id` primary key). Malformed lines are skipped with a warning.
//! Fully ingested files older than [`SPOOL_RETENTION_DAYS`] are deleted.

use std::fs::{self, File};
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::bridge::usage_capture::GatewayUsageEvent;
use crate::error::Result;
use crate::logging::targets;
use crate::models::GatewayUsageRow;
use crate::storage::gateway_usage_repo::{GatewaySpoolCursor, GatewayUsageRepo};
use crate::utils::redact::redact_text;

/// Spool files are deleted only after a full ingest and once older than this.
pub(crate) const SPOOL_RETENTION_DAYS: u64 = 7;

/// Counters for one spool-dir sweep (tracing only; `CollectResult` keeps its
/// public shape).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GatewaySpoolOutcome {
    pub files: usize,
    pub inserted: u64,
    pub malformed: u64,
    pub deleted_files: u64,
}

/// Ingest every `gateway-*.jsonl` spool file in `dir`, oldest first.
pub(crate) fn ingest_spool_dir(
    repo: &GatewayUsageRepo,
    dir: &Path,
) -> Result<GatewaySpoolOutcome> {
    let mut outcome = GatewaySpoolOutcome::default();
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(outcome),
        Err(error) => return Err(error.into()),
    };
    // `gateway-YYYYMMDD.jsonl` names sort oldest-first by day key.
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .collect();
    files.sort();
    for path in files {
        outcome.files += 1;
        match ingest_one_file(repo, &path) {
            Ok((inserted, malformed, deleted)) => {
                outcome.inserted += inserted;
                outcome.malformed += malformed;
                outcome.deleted_files += u64::from(deleted);
            }
            Err(error) => {
                let msg = redact_text(&error.to_string());
                tracing::warn!(
                    module = targets::USAGE,
                    op = "gateway_spool_file",
                    path = %path.to_string_lossy(),
                    "{msg}"
                );
            }
        }
    }
    Ok(outcome)
}

/// Ingest one spool file from its stored cursor; returns
/// (inserted, malformed, deleted).
fn ingest_one_file(
    repo: &GatewayUsageRepo,
    path: &Path,
) -> Result<(u64, u64, bool)> {
    let path_s = path.to_string_lossy().to_string();
    let meta = fs::metadata(path)?;
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let len = meta.len() as i64;

    // A cursor is reused only when the file was not rotated/truncated.
    let mut offset = 0i64;
    if let Some(cursor) = repo.get_spool_cursor(&path_s)? {
        if cursor.file_mtime == mtime && cursor.byte_offset <= len {
            offset = cursor.byte_offset;
        }
    }

    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    if offset > 0 {
        reader.seek(SeekFrom::Start(offset as u64))?;
    }

    let mut rows: Vec<GatewayUsageRow> = Vec::new();
    let mut malformed = 0u64;
    let mut consumed = offset;
    let mut buf = String::new();
    loop {
        buf.clear();
        let n = reader.read_line(&mut buf)?;
        if n == 0 {
            break;
        }
        consumed += n as i64;
        let line = buf.trim();
        if line.is_empty() {
            continue;
        }
        match serde_json::from_str::<GatewayUsageEvent>(line) {
            Ok(event) => rows.push(gateway_row(event)),
            Err(error) => {
                malformed += 1;
                tracing::warn!(
                    module = targets::USAGE,
                    op = "gateway_spool_line",
                    path = %path_s,
                    error = %redact_text(&error.to_string()),
                    "skipping malformed gateway usage spool line"
                );
            }
        }
    }

    let cursor = GatewaySpoolCursor {
        path: path_s,
        byte_offset: consumed,
        file_mtime: mtime,
    };
    // Deletion is decided before the transaction so the cursor row is cleaned
    // up atomically with the advance; the file itself is unlinked after commit.
    let reached_eof = consumed >= len;
    let expired = mtime > 0
        && SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|now| {
                now >= Duration::from_secs(mtime as u64) + retention_duration()
            })
            .unwrap_or(false);
    let delete_file = reached_eof && expired;
    let inserted =
        repo.insert_batch_and_cursor(&rows, &cursor, delete_file)?;
    if delete_file {
        match fs::remove_file(path) {
            Ok(()) => {}
            // The cursor row is already gone; a re-created file would simply
            // be re-read from 0 and deduplicated by request_id.
            Err(error) => tracing::warn!(
                module = targets::USAGE,
                op = "gateway_spool_file",
                path = %path.to_string_lossy(),
                error = %redact_text(&error.to_string()),
                "failed to delete expired gateway usage spool file"
            ),
        }
    }
    Ok((inserted, malformed, delete_file))
}

fn retention_duration() -> Duration {
    Duration::from_secs(SPOOL_RETENTION_DAYS * 24 * 60 * 60)
}

/// Map a captured spool event onto the persisted row shape.
fn gateway_row(event: GatewayUsageEvent) -> GatewayUsageRow {
    GatewayUsageRow {
        request_id: event.request_id,
        ts: event.ts,
        profile_id: event.profile_id,
        surface: event.surface,
        upstream_channel: event.upstream_channel,
        ticket_id: event.ticket_id,
        account_source_kind: event.account_source_kind,
        account_source_id: event.account_source_id,
        model: event.model,
        upstream_model: event.upstream_model,
        input_tokens: event.input_tokens as i64,
        output_tokens: event.output_tokens as i64,
        cached_input_tokens: event.cached_input_tokens.map(|v| v as i64),
        reasoning_tokens: event.reasoning_tokens.map(|v| v as i64),
        status: event.status,
        status_code: event.status_code.map(i64::from),
        error_class: event.error_class,
        latency_ms: event.latency_ms.map(|v| v as i64),
        ttft_ms: event.ttft_ms.map(|v| v as i64),
        attempts: event.attempts.map(i64::from),
        session_id: event.session_id,
    }
}

#[cfg(test)]
mod tests;
