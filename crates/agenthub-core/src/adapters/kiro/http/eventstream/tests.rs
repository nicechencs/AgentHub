use super::*;

fn frame(headers: &[(&str, &str)], payload: &[u8]) -> Vec<u8> {
    let mut header_bytes = Vec::new();
    for (name, value) in headers {
        header_bytes.push(name.len() as u8);
        header_bytes.extend_from_slice(name.as_bytes());
        header_bytes.push(7); // string
        let vb = value.as_bytes();
        header_bytes.extend_from_slice(&(vb.len() as u16).to_be_bytes());
        header_bytes.extend_from_slice(vb);
    }
    let headers_len = header_bytes.len() as u32;
    let total_len = 12 + header_bytes.len() + payload.len() + 4;
    let mut out = Vec::with_capacity(total_len);
    out.extend_from_slice(&(total_len as u32).to_be_bytes());
    out.extend_from_slice(&headers_len.to_be_bytes());
    out.extend_from_slice(&0u32.to_be_bytes()); // prelude crc ignored
    out.extend_from_slice(&header_bytes);
    out.extend_from_slice(payload);
    out.extend_from_slice(&0u32.to_be_bytes()); // message crc ignored
    out
}

#[test]
fn parse_assistant_deltas_and_conversation_id() {
    let mut bytes = frame(
        &[
            (":event-type", "initial-response"),
            (":message-type", "event"),
        ],
        br#"{"conversationId":"cid-1"}"#,
    );
    bytes.extend(frame(
        &[
            (":event-type", "assistantResponseEvent"),
            (":message-type", "event"),
        ],
        br#"{"content":"po"}"#,
    ));
    bytes.extend(frame(
        &[
            (":event-type", "assistantResponseEvent"),
            (":message-type", "event"),
        ],
        br#"{"content":"ng"}"#,
    ));
    let (text, cid) = collect_assistant_text(&bytes);
    assert_eq!(text, "pong");
    assert_eq!(cid.as_deref(), Some("cid-1"));
}
