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

#[test]
fn decoder_holds_truncated_frame_until_complete() {
    let complete = frame(
        &[(":event-type", "assistantResponseEvent")],
        br#"{"content":"hi"}"#,
    );
    assert!(complete.len() > 8, "fixture must be large enough to split");
    let mut decoder = EventStreamDecoder::new();
    let first = decoder.push(&complete[..8]);
    assert!(first.is_empty(), "truncated prelude must not yield a frame");
    assert_eq!(decoder.leftover(), &complete[..8]);
    let rest = decoder.push(&complete[8..]);
    assert_eq!(rest.len(), 1);
    assert!(decoder.leftover().is_empty());
    let (text, _) = collect_assistant_text(&complete);
    assert_eq!(text, "hi");
}

#[test]
fn decoder_yields_first_frame_before_later_bytes() {
    let first = frame(
        &[(":event-type", "assistantResponseEvent")],
        br#"{"content":"po"}"#,
    );
    let second = frame(
        &[(":event-type", "assistantResponseEvent")],
        br#"{"content":"ng"}"#,
    );
    let mut decoder = EventStreamDecoder::new();
    let got = decoder.push(&first);
    assert_eq!(got.len(), 1);
    let mut text = String::new();
    apply_assistant_event(&got[0], &mut text, &mut None, |_| {});
    assert_eq!(text, "po");
    let got = decoder.push(&second);
    assert_eq!(got.len(), 1);
    apply_assistant_event(&got[0], &mut text, &mut None, |_| {});
    assert_eq!(text, "pong");
}

#[test]
fn read_assistant_events_emits_first_delta_before_reader_eof() {
    use std::io::{self, Read};
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

    struct GatedReader {
        first: Vec<u8>,
        rest: Vec<u8>,
        pos: usize,
        gate: mpsc::Receiver<()>,
        released: bool,
    }

    impl Read for GatedReader {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            if self.pos < self.first.len() {
                let n = (self.first.len() - self.pos).min(buf.len());
                buf[..n].copy_from_slice(&self.first[self.pos..self.pos + n]);
                self.pos += n;
                return Ok(n);
            }
            if !self.released {
                self.gate
                    .recv_timeout(Duration::from_secs(5))
                    .map_err(|_| {
                        io::Error::new(io::ErrorKind::TimedOut, "gated reader timed out")
                    })?;
                self.released = true;
                self.pos = 0;
            }
            if self.pos >= self.rest.len() {
                return Ok(0);
            }
            let n = (self.rest.len() - self.pos).min(buf.len());
            buf[..n].copy_from_slice(&self.rest[self.pos..self.pos + n]);
            self.pos += n;
            Ok(n)
        }
    }

    let first = frame(
        &[(":event-type", "assistantResponseEvent")],
        br#"{"content":"po"}"#,
    );
    let rest = frame(
        &[(":event-type", "assistantResponseEvent")],
        br#"{"content":"ng"}"#,
    );
    let (gate_tx, gate_rx) = mpsc::channel();
    let (delta_tx, delta_rx) = mpsc::channel();
    let reader = GatedReader {
        first,
        rest,
        pos: 0,
        gate: gate_rx,
        released: false,
    };
    let worker = thread::spawn(move || {
        read_assistant_events(reader, |delta| {
            let _ = delta_tx.send(delta.to_owned());
        })
    });

    let first_delta = delta_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("first assistant delta must arrive before the rest of the body");
    assert_eq!(first_delta, "po");
    assert!(
        delta_rx.recv_timeout(Duration::from_millis(50)).is_err(),
        "second delta must not arrive until later bytes are released"
    );
    gate_tx.send(()).expect("release rest");
    let second = delta_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("second delta after release");
    assert_eq!(second, "ng");
    let read = worker.join().expect("worker").expect("read");
    assert_eq!(read.text, "pong");
}
