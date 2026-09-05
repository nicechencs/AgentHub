//! JSONL collect cursor.
//!
//! A file is unchanged only when **both** size and mtime match the cached
//! snapshot. Either changing means the file was modified — re-read from
//! byte 0. Cached usage rows still dedupe at insert, so a full rescan of an
//! unchanged-in-content file does not count as new rows.

use std::time::UNIX_EPOCH;

pub(crate) fn mtime_secs(meta: &std::fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Byte offset to start reading from.
///
/// Returns the stored offset only when size and mtime both match and the
/// offset still falls inside the file. Otherwise `0` (whole-file rescan).
pub(crate) fn resume_offset(
    stored_offset: i64,
    stored_mtime: i64,
    stored_size: i64,
    mtime: i64,
    len: i64,
) -> i64 {
    let unchanged = stored_mtime == mtime && stored_size == len;
    if unchanged && stored_offset >= 0 && stored_offset <= len {
        stored_offset
    } else {
        0
    }
}
