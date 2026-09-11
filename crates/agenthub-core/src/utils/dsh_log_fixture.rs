//! Test-only writer for DeepSeek Harness style zstd session logs.
//!
//! DSH persists one zstd frame per append batch, so a fixture must be a
//! concatenation of independent frames — a single-frame file would not exercise
//! the reader that walks frame boundaries.
#![cfg(test)]

use std::fs;
use std::io::Write;
use std::path::Path;

/// Encodes every row in its own zstd frame.
pub fn zstd_frames(rows: &[&str]) -> Vec<u8> {
    let mut out = Vec::new();
    for row in rows {
        let mut encoder =
            zstd::stream::write::Encoder::new(Vec::new(), 3).expect("zstd encoder");
        encoder.write_all(row.as_bytes()).expect("write row");
        encoder.write_all(b"\n").expect("write newline");
        out.extend_from_slice(&encoder.finish().expect("finish frame"));
    }
    out
}

/// Writes `rows` as a multi-frame zstd log, creating parent directories.
pub fn write_zstd_log(path: &Path, rows: &[&str]) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create log dir");
    }
    fs::write(path, zstd_frames(rows)).expect("write zstd log");
}
