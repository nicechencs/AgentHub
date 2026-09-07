//! Minimal AWS event-stream parser for Kiro GenerateAssistantResponse bodies.
//!
//! Spec shape: big-endian prelude (total_len, headers_len, prelude_crc) +
//! headers + payload + message_crc. First slice skips CRC verification.

use std::collections::HashMap;

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

/// Parse concatenated AWS event-stream frames. Stops at the first truncated frame.
pub(crate) fn parse_event_stream(data: &[u8]) -> Vec<EventMessage> {
    let mut out = Vec::new();
    let mut i = 0usize;
    while i + 16 <= data.len() {
        let total_len = u32::from_be_bytes(data[i..i + 4].try_into().unwrap()) as usize;
        let headers_len = u32::from_be_bytes(data[i + 4..i + 8].try_into().unwrap()) as usize;
        if total_len < 16 || i + total_len > data.len() {
            break;
        }
        if 12 + headers_len + 4 > total_len {
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
    out
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

/// Collect assistant text deltas from GenerateAssistantResponse event-stream bytes.
pub(crate) fn collect_assistant_text(data: &[u8]) -> (String, Option<String>) {
    let mut text = String::new();
    let mut conversation_id = None;
    for event in parse_event_stream(data) {
        let et = event.event_type().unwrap_or("");
        let Ok(payload) = serde_json::from_slice::<serde_json::Value>(&event.payload) else {
            continue;
        };
        match et {
            "assistantResponseEvent" => {
                if let Some(chunk) = payload.get("content").and_then(|v| v.as_str()) {
                    text.push_str(chunk);
                }
            }
            "initial-response" => {
                if conversation_id.is_none() {
                    conversation_id = payload
                        .get("conversationId")
                        .and_then(|v| v.as_str())
                        .map(str::to_owned);
                }
            }
            _ => {}
        }
    }
    (text, conversation_id)
}

#[cfg(test)]
mod tests;
