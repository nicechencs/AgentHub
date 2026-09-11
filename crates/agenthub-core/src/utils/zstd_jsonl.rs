//! Zstandard-compressed JSONL logs (`*.jsonl.zstd`).
//!
//! DeepSeek Harness stores every session as a concatenation of **independent**
//! zstd frames: one frame holding the header row, then one frame per persisted
//! append batch (`@deepseek-ai/dsh-session-persistence-jsonl`). Two consequences
//! drive this module:
//!
//! - line-oriented readers cannot read the file at all; it has to be decoded;
//! - byte offsets address the *compressed* stream, so a resumed reader must
//!   start on a frame boundary rather than trusting a stored offset blindly.
//!
//! A torn trailing frame is expected while a session is being written (crash /
//! in-flight append). Per the persistence contract the complete frames decoded
//! before it stay authoritative, so a decode error ends the stream instead of
//! failing the whole file — but it is **reported**: a damaged committed frame
//! would otherwise look like a short, complete log (see [`DecodeErrors`]).
//!
//! Scope note: DSH writes checksummed frames, so damage inside a frame is
//! caught. A frame that is merely *truncated* while further data follows cannot
//! be distinguished from a torn tail by the decoder — DSH rolls the file length
//! back on a failed write, so that shape only appears in hand-edited logs.

use std::fs::File;
use std::io::{self, BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::error::Result;
use crate::logging::targets;

/// `ZSTD_MAGICNUMBER` as stored (little endian).
const ZSTD_MAGIC: [u8; 4] = [0x28, 0xb5, 0x2f, 0xfd];

/// File-name suffix of a compressed JSONL log.
pub(crate) const ZSTD_SUFFIX: &str = ".jsonl.zstd";

/// Set when a compressed log could not be fully decoded.
///
/// The persistence contract lets a torn trailing frame contribute its complete
/// rows, so the reader stops there. That happens both for a damaged committed
/// frame and for an append that is still in flight, and the two are not
/// distinguishable from the decoded stream alone. Without this signal a short
/// read is indistinguishable from a short log: the caller would report a
/// plausible but too-small token sum and advance its cursor past the damage.
#[derive(Clone, Default)]
pub(crate) struct DecodeErrors(Arc<AtomicBool>);

impl DecodeErrors {
    pub(crate) fn is_set(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }

    pub(crate) fn shared(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.0)
    }

    fn mark(&self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

/// Decoded line reader plus its decode-failure signal.
pub(crate) struct LogLines {
    pub reader: Box<dyn BufRead + Send>,
    pub errors: DecodeErrors,
}

/// True when `path` is a zstd-compressed JSONL log.
pub(crate) fn is_zstd_jsonl(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.to_ascii_lowercase().ends_with(ZSTD_SUFFIX))
        .unwrap_or(false)
}

/// Opens a line reader over a JSONL log.
///
/// Plain logs stream from `byte_offset` as before. Compressed logs only resume
/// when `byte_offset` is already a frame boundary (the cursor stores the end of
/// the last decoded frame); an offset in the middle of a frame means the log
/// changed under us, so decoding restarts at the beginning and callers dedupe.
pub(crate) fn open_lines(path: &Path, byte_offset: u64) -> Result<LogLines> {
    let mut file = File::open(path)?;
    if !is_zstd_jsonl(path) {
        if byte_offset > 0 {
            file.seek(SeekFrom::Start(byte_offset))?;
        }
        return Ok(LogLines {
            reader: Box::new(BufReader::new(file)),
            errors: DecodeErrors::default(),
        });
    }
    if byte_offset > 0 {
        let len = file.metadata()?.len();
        if byte_offset >= len {
            return Ok(LogLines {
                reader: Box::new(io::Cursor::new(Vec::new())),
                errors: DecodeErrors::default(),
            });
        }
        let boundary = has_frame_magic_at(&mut file, byte_offset)?;
        file.seek(SeekFrom::Start(if boundary { byte_offset } else { 0 }))?;
    }
    let decoder = zstd::stream::read::Decoder::new(file)?;
    let errors = DecodeErrors::default();
    Ok(LogLines {
        reader: Box::new(BufReader::new(TailTolerant {
            inner: decoder,
            errors: errors.clone(),
        })),
        errors,
    })
}

/// Reads at most `max_plain_bytes` of **decoded** text from the head of `path`.
///
/// Plain logs read those bytes raw. Compressed logs decode frame by frame until
/// the budget is reached, so callers can keep thinking in transcript bytes.
pub(crate) fn read_decoded_head(path: &Path, max_plain_bytes: u64) -> Option<String> {
    read_decoded_head_capped(path, max_plain_bytes).map(|(text, _)| text)
}

/// [`read_decoded_head`] plus whether the budget cut the transcript short.
pub(crate) fn read_decoded_head_capped(
    path: &Path,
    max_plain_bytes: u64,
) -> Option<(String, bool)> {
    if max_plain_bytes == 0 {
        return Some((String::new(), false));
    }
    if !is_zstd_jsonl(path) {
        let file = File::open(path).ok()?;
        let mut buf = Vec::new();
        file.take(max_plain_bytes).read_to_end(&mut buf).ok()?;
        let truncated = buf.len() as u64 >= max_plain_bytes;
        return Some((String::from_utf8_lossy(&buf).into_owned(), truncated));
    }
    let lines = open_lines(path, 0).ok()?;
    let errors = lines.errors;
    let mut reader = lines.reader;
    let mut buf = Vec::new();
    reader
        .by_ref()
        .take(max_plain_bytes)
        .read_to_end(&mut buf)
        .ok()?;
    if errors.is_set() {
        // Debug, not warn: an in-flight append trips this too, and the project
        // scan runs often. The usage collector turns the same signal into a
        // failed file plus a warning, where a short read would distort totals.
        tracing::debug!(
            module = targets::PROJECT,
            op = "decode",
            code = "session_log_incomplete",
            path = %path.display(),
            "compressed session log could not be fully decoded; later rows were skipped"
        );
    }
    let truncated = buf.len() as u64 >= max_plain_bytes;
    if truncated {
        // A budget cut can land mid-line; drop the partial tail so line counts
        // and first-line parsing stay honest.
        if let Some(last_nl) = buf.iter().rposition(|&b| b == b'\n') {
            buf.truncate(last_nl + 1);
        }
    }
    Some((String::from_utf8_lossy(&buf).into_owned(), truncated))
}

/// True when a zstd frame (standard or skippable) starts exactly at `offset`.
fn has_frame_magic_at(file: &mut File, offset: u64) -> Result<bool> {
    file.seek(SeekFrom::Start(offset))?;
    let mut magic = [0u8; 4];
    match file.read_exact(&mut magic) {
        Ok(()) => Ok(is_frame_magic(&magic)),
        Err(_) => Ok(false),
    }
}

fn is_frame_magic(bytes: &[u8]) -> bool {
    if bytes.len() < 4 {
        return false;
    }
    if bytes[..4] == ZSTD_MAGIC {
        return true;
    }
    // Skippable frames: 0x184D2A50..=0x184D2A5F, little endian on disk.
    (0x50..=0x5f).contains(&bytes[0])
        && bytes[1] == 0x2a
        && bytes[2] == 0x4d
        && bytes[3] == 0x18
}

/// Ends the decoded stream at a torn/unreadable frame, recording that it did.
struct TailTolerant<R> {
    inner: R,
    errors: DecodeErrors,
}

impl<R: Read> Read for TailTolerant<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self.inner.read(buf) {
            Ok(n) => Ok(n),
            // Complete frames already decoded stay valid; the rest is dropped
            // and reported through `errors`.
            Err(_) => {
                self.errors.mark();
                Ok(0)
            }
        }
    }
}
