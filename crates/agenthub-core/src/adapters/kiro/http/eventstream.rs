//! Minimal AWS event-stream parser for Kiro GenerateAssistantResponse bodies.
//!
//! Spec shape: big-endian prelude (total_len, headers_len, prelude_crc) +
//! headers + payload + message_crc. First slice skips CRC verification.
//!
//! Incremental decoding yields a frame as soon as its bytes are complete; it
//! does not wait for EOF. That is the only honest "chunk" boundary this client
//! can see on the Kiro HTTP body.

use std::collections::HashMap;
use std::io::Read;

const BODY_LIMIT: u64 = 8 * 1024 * 1024;
const PREVIEW_LIMIT: usize = 300;
const READ_BUF: usize = 8192;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EventMessage {
    pub headers: HashMap<String, String>,
    pub payload: Vec<u8>,
}

impl EventMessage {
    pub(crate) fn event_type(&self) -> Option<&str> {
        self.headers.get(":event-type").map(String::as_str)
    }
}

/// Bytes already pulled from the socket plus assistant text seen so far.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AssistantRead {
    pub text: String,
    pub conversation_id: Option<String>,
    pub preview: String,
}

/// Holds a partial AWS event-stream frame until later `push` calls complete it.
#[derive(Debug, Default)]
pub(crate) struct EventStreamDecoder {
    buf: Vec<u8>,
}

impl EventStreamDecoder {
    pub(crate) fn new() -> Self {
        Self { buf: Vec::new() }
    }

    /// Append bytes and return every newly completed frame. Incomplete tail
    /// stays buffered; a later `push` (or EOF) is required to finish it.
    pub(crate) fn push(&mut self, data: &[u8]) -> Vec<EventMessage> {
        if data.is_empty() {
            return Vec::new();
        }
        self.buf.extend_from_slice(data);
        let (events, consumed) = parse_event_stream_consumed(&self.buf);
        if consumed > 0 {
            self.buf.drain(..consumed);
        }
        events
    }

    #[cfg(test)]
    pub(crate) fn leftover(&self) -> &[u8] {
        &self.buf
    }
}

/// Parse concatenated AWS event-stream frames. Stops at the first truncated frame.
#[cfg(test)]
pub(crate) fn parse_event_stream(data: &[u8]) -> Vec<EventMessage> {
    parse_event_stream_consumed(data).0
}

fn parse_event_stream_consumed(data: &[u8]) -> (Vec<EventMessage>, usize) {
    let mut out = Vec::new();
    let mut i = 0usize;
    while i + 16 <= data.len() {
        let total_len = u32::from_be_bytes(data[i..i + 4].try_into().unwrap()) as usize;
        let headers_len = u32::from_be_bytes(data[i + 4..i + 8].try_into().unwrap()) as usize;
        if total_len < 16 || 12 + headers_len + 4 > total_len {
            break;
        }
        if i + total_len > data.len() {
            break;
        }
        let message = &data[i..i + total_len];
        let headers_bytes = &message[12..12 + headers_len];
        let payload = message[12 + headers_len..total_len - 4].to_vec();
        let Some(headers) = parse_headers(headers_bytes) else {
            break;
        };
        out.push(EventMessage { headers, payload });
        i += total_len;
    }
    (out, i)
}

fn parse_headers(bytes: &[u8]) -> Option<HashMap<String, String>> {
    let mut out = HashMap::new();
    let mut j = 0usize;
    while j < bytes.len() {
        if j >= bytes.len() {
            break;
        }
        let name_len = bytes[j] as usize;
        j += 1;
        if j + name_len > bytes.len() {
            return None;
        }
        let name = String::from_utf8_lossy(&bytes[j..j + name_len]).into_owned();
        j += name_len;
        if j >= bytes.len() {
            return None;
        }
        let htype = bytes[j];
        j += 1;
        let value = match htype {
            0 => "true".to_string(),
            1 => "false".to_string(),
            2 => {
                if j >= bytes.len() {
                    return None;
                }
                let v = bytes[j];
                j += 1;
                v.to_string()
            }
            3 => {
                if j + 2 > bytes.len() {
                    return None;
                }
                let v = i16::from_be_bytes(bytes[j..j + 2].try_into().ok()?);
                j += 2;
                v.to_string()
            }
            4 => {
                if j + 4 > bytes.len() {
                    return None;
                }
                let v = i32::from_be_bytes(bytes[j..j + 4].try_into().ok()?);
                j += 4;
                v.to_string()
            }
            5 | 8 => {
                if j + 8 > bytes.len() {
                    return None;
                }
                let v = i64::from_be_bytes(bytes[j..j + 8].try_into().ok()?);
                j += 8;
                v.to_string()
            }
            6 => {
                if j + 2 > bytes.len() {
                    return None;
                }
                let vlen = u16::from_be_bytes(bytes[j..j + 2].try_into().ok()?) as usize;
                j += 2;
                if j + vlen > bytes.len() {
                    return None;
                }
                let v = format!("<{} bytes>", vlen);
                j += vlen;
                v
            }
            7 => {
                if j + 2 > bytes.len() {
                    return None;
                }
                let vlen = u16::from_be_bytes(bytes[j..j + 2].try_into().ok()?) as usize;
                j += 2;
                if j + vlen > bytes.len() {
                    return None;
                }
                let v = String::from_utf8_lossy(&bytes[j..j + vlen]).into_owned();
                j += vlen;
                v
            }
            9 => {
                if j + 16 > bytes.len() {
                    return None;
                }
                j += 16;
                "<uuid>".to_string()
            }
            _ => return None,
        };
        out.insert(name, value);
    }
    Some(out)
}

enum AssistantEvent {
    TextDelta(String),
    ConversationId(String),
}

fn assistant_event(event: &EventMessage) -> Option<AssistantEvent> {
    let et = event.event_type().unwrap_or("");
    let payload = serde_json::from_slice::<serde_json::Value>(&event.payload).ok()?;
    match et {
        "assistantResponseEvent" => payload
            .get("content")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| AssistantEvent::TextDelta(s.to_owned())),
        "initial-response" => payload
            .get("conversationId")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| AssistantEvent::ConversationId(s.to_owned())),
        _ => None,
    }
}

fn apply_assistant_event(
    event: &EventMessage,
    text: &mut String,
    conversation_id: &mut Option<String>,
    mut on_delta: impl FnMut(&str),
) {
    match assistant_event(event) {
        Some(AssistantEvent::TextDelta(chunk)) => {
            text.push_str(&chunk);
            on_delta(&chunk);
        }
        Some(AssistantEvent::ConversationId(id)) if conversation_id.is_none() => {
            *conversation_id = Some(id);
        }
        _ => {}
    }
}

/// Collect assistant text deltas from GenerateAssistantResponse event-stream bytes.
pub(crate) fn collect_assistant_text(data: &[u8]) -> (String, Option<String>) {
    let mut decoder = EventStreamDecoder::new();
    let mut text = String::new();
    let mut conversation_id = None;
    for event in decoder.push(data) {
        apply_assistant_event(&event, &mut text, &mut conversation_id, |_| {});
    }
    (text, conversation_id)
}

/// Read an AWS event-stream body and invoke `on_delta` as each assistant
/// content frame completes — before the reader reaches EOF.
pub(crate) fn read_assistant_events(
    reader: impl Read,
    mut on_delta: impl FnMut(&str),
) -> std::io::Result<AssistantRead> {
    let mut reader = reader.take(BODY_LIMIT);
    let mut decoder = EventStreamDecoder::new();
    let mut text = String::new();
    let mut conversation_id = None;
    let mut preview = Vec::new();
    let mut buf = [0u8; READ_BUF];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        if preview.len() < PREVIEW_LIMIT {
            let take = (PREVIEW_LIMIT - preview.len()).min(n);
            preview.extend_from_slice(&buf[..take]);
        }
        for event in decoder.push(&buf[..n]) {
            apply_assistant_event(&event, &mut text, &mut conversation_id, &mut on_delta);
        }
    }
    Ok(AssistantRead {
        text,
        conversation_id,
        preview: String::from_utf8_lossy(&preview).into_owned(),
    })
}

#[cfg(test)]
mod tests;
