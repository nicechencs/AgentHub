use std::fs;
use std::io::BufRead;

use crate::utils::dsh_log_fixture::{write_zstd_log, zstd_frames};
use crate::utils::test_temp::real_tempdir;
use crate::utils::zstd_jsonl::{
    is_zstd_jsonl, open_lines, read_decoded_head, read_decoded_head_capped,
};

fn read_all(path: &std::path::Path) -> Vec<String> {
    let lines = open_lines(path, 0).expect("open log");
    read_all_from(lines)
}

fn read_all_from(lines: crate::utils::zstd_jsonl::LogLines) -> Vec<String> {
    let mut reader = lines.reader;
    let mut out = Vec::new();
    let mut line = String::new();
    while reader.read_line(&mut line).expect("read line") > 0 {
        let trimmed = line.trim_end_matches(['\n', '\r']).to_string();
        if !trimmed.is_empty() {
            out.push(trimmed);
        }
        line.clear();
    }
    out
}

#[test]
fn detects_compressed_jsonl_suffix() {
    assert!(is_zstd_jsonl(std::path::Path::new("session.v3.jsonl.zstd")));
    assert!(is_zstd_jsonl(std::path::Path::new("SESSION.V1.JSONL.ZSTD")));
    assert!(!is_zstd_jsonl(std::path::Path::new("session.v3.jsonl")));
    assert!(!is_zstd_jsonl(std::path::Path::new("session.v3.jsonl.zst")));
}

#[test]
fn decodes_every_concatenated_frame() {
    let dir = real_tempdir();
    let path = dir.path().join("session.v3.jsonl.zstd");
    write_zstd_log(&path, &["{\"seq\":1}", "{\"seq\":2}", "{\"seq\":3}"]);
    assert_eq!(
        read_all(&path),
        vec!["{\"seq\":1}", "{\"seq\":2}", "{\"seq\":3}"]
    );
}

#[test]
fn resumes_at_an_exact_frame_boundary() {
    let dir = real_tempdir();
    let path = dir.path().join("session.v3.jsonl.zstd");
    let first = zstd_frames(&["{\"seq\":1}"]);
    let second = zstd_frames(&["{\"seq\":2}"]);
    let third = zstd_frames(&["{\"seq\":3}"]);
    let mut bytes = first.clone();
    bytes.extend_from_slice(&second);
    bytes.extend_from_slice(&third);
    fs::write(&path, &bytes).expect("write log");

    let reader = open_lines(&path, first.len() as u64).expect("open log");
    assert!(!reader.errors.is_set());
    assert_eq!(read_all_from(reader), vec!["{\"seq\":2}", "{\"seq\":3}"]);
}

#[test]
fn mid_frame_offset_redecodes_from_the_start() {
    let dir = real_tempdir();
    let path = dir.path().join("session.v3.jsonl.zstd");
    let first = zstd_frames(&["{\"seq\":1}"]);
    let second = zstd_frames(&["{\"seq\":2}"]);
    let mut bytes = first.clone();
    bytes.extend_from_slice(&second);
    fs::write(&path, &bytes).expect("write log");

    // An offset inside a frame is not a resume point: replay everything so no
    // row is skipped (insert-time dedupe absorbs the duplicates).
    let reader = open_lines(&path, first.len() as u64 + 3).expect("open log");
    assert_eq!(read_all_from(reader), vec!["{\"seq\":1}", "{\"seq\":2}"]);
}

#[test]
fn offset_at_end_of_log_yields_nothing() {
    let dir = real_tempdir();
    let path = dir.path().join("session.v3.jsonl.zstd");
    let frames = zstd_frames(&["{\"seq\":1}", "{\"seq\":2}"]);
    fs::write(&path, &frames).expect("write log");
    let mut reader = open_lines(&path, frames.len() as u64)
        .expect("open log")
        .reader;
    let mut line = String::new();
    assert_eq!(reader.read_line(&mut line).expect("read line"), 0);
}

#[test]
fn decoded_head_counts_decoded_bytes_and_keeps_lines_whole() {
    let dir = real_tempdir();
    let path = dir.path().join("session.v3.jsonl.zstd");
    write_zstd_log(&path, &["{\"n\":1}", "{\"n\":2}", "{\"n\":3}"]);
    let head = read_decoded_head(&path, 16).expect("head");
    assert_eq!(head, "{\"n\":1}\n{\"n\":2}\n");
    let (capped, truncated) = read_decoded_head_capped(&path, 16).expect("head");
    assert_eq!(capped, head);
    assert!(truncated, "budget smaller than the log reports truncation");
    let full = read_decoded_head(&path, 4096).expect("head");
    assert_eq!(full, "{\"n\":1}\n{\"n\":2}\n{\"n\":3}\n");
    let (_, truncated) = read_decoded_head_capped(&path, 4096).expect("head");
    assert!(!truncated, "whole log is not truncated");
}

#[test]
fn plain_logs_keep_byte_offset_semantics() {
    let dir = real_tempdir();
    let path = dir.path().join("session.v3.jsonl");
    fs::write(&path, "{\"seq\":1}\n{\"seq\":2}\n").expect("write log");
    assert_eq!(read_all(&path), vec!["{\"seq\":1}", "{\"seq\":2}"]);

    let mut reader = open_lines(&path, 10).expect("open log").reader;
    let mut line = String::new();
    reader.read_line(&mut line).expect("read line");
    assert_eq!(line.trim_end_matches(['\n', '\r']), "{\"seq\":2}");
}

#[test]
fn torn_trailing_frame_keeps_complete_rows() {
    let dir = real_tempdir();
    let path = dir.path().join("session.v3.jsonl.zstd");
    let frames = zstd_frames(&["{\"seq\":1}", "{\"seq\":2}", "{\"seq\":3}"]);
    fs::write(&path, &frames[..frames.len() - 5]).expect("write torn log");

    let lines = open_lines(&path, 0).expect("open log");
    let errors = lines.errors.clone();
    let rows = read_all_from(lines);
    assert!(rows.len() >= 2, "complete frames survive: {rows:?}");
    assert_eq!(rows[0], "{\"seq\":1}");
    assert_eq!(rows[1], "{\"seq\":2}");
    assert!(rows.len() <= 3, "torn row never becomes a full line: {rows:?}");
    // A torn tail is reported like damage: the two are indistinguishable from
    // the decoded stream, and the caller must not treat a short read as a
    // complete log. The next collect rescans once the append lands.
    assert!(errors.is_set(), "a short read is signalled");
}

#[test]
fn damaged_middle_frame_is_reported_not_swallowed() {
    let dir = real_tempdir();
    let path = dir.path().join("session.v3.jsonl.zstd");
    let first = zstd_frames(&["{\"seq\":1}"]);
    let mut second = zstd_frames(&["{\"seq\":2}"]);
    let third = zstd_frames(&["{\"seq\":3}"]);
    // Corrupt the middle frame's own header while a later frame still follows:
    // the decoder must stop there *and* say so.
    second[..4].copy_from_slice(&[0x00, 0x00, 0x00, 0x00]);
    let mut bytes = first.clone();
    bytes.extend_from_slice(&second);
    bytes.extend_from_slice(&third);
    fs::write(&path, &bytes).expect("write damaged log");

    let lines = open_lines(&path, 0).expect("open log");
    let errors = lines.errors.clone();
    let rows = read_all_from(lines);
    assert!(
        errors.is_set(),
        "a damaged committed frame must be signalled"
    );
    assert_eq!(rows, vec!["{\"seq\":1}"], "rows before the damage survive");

    // The history path reports it the same way (and still returns the head).
    let (head, _) = read_decoded_head_capped(&path, 4096).expect("head");
    assert!(head.contains("{\"seq\":1}"), "{head:?}");
}
