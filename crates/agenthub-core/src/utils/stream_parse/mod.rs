//! Line-oriented structured stdout decoding for Chat process UI.
//!
//! Platform owns line buffering + StreamOutput mapping ([`StreamSession`]).
//! Agent-specific NDJSON decoding is registered as
//! [`crate::platform::stream::StreamParser`] contributions.
//!
//! Agents with structured capability emit NDJSON under `ProcessMode::Auto`:
//! Claude `stream-json`, Codex `--json`, Kimi `stream-json`, Pi `--mode json`,
//! Grok `streaming-json`. Text-only agents / CLI multi-run stay passthrough.

pub(crate) mod claude;
pub(crate) mod codex;
pub(crate) mod grok;
pub(crate) mod kimi;
pub(crate) mod pi;

use std::sync::Arc;

use crate::logging::targets;
use crate::models::{AgentId, OutputStream, ProcessMode, ProcessStep};
use crate::platform::stream::{builtin_stream_registry, StreamParser, StreamParserRegistry};
use crate::platform::AgentKey;

const MAX_LINE_BYTES: usize = 256 * 1024;
const MAX_ASSISTANT_CHARS: usize = 2 * 1024 * 1024;
/// Cap emitted process steps per turn (still decode text for the body).
const MAX_EMITTED_STEPS: usize = 2000;

/// One unit emitted after feeding a chunk (or flush).
#[derive(Debug, Clone, PartialEq)]
pub enum StreamOutput {
    Chunk { stream: OutputStream, text: String },
    Step(ProcessStep),
}

/// Streaming session: line buffer + optional registered StreamParser.
///
/// Does **not** match on concrete Agent names for decoding — parsers come
/// from [`builtin_stream_registry`].
pub struct StreamSession {
    agent_key: AgentKey,
    structured: bool,
    parser: Option<Arc<dyn StreamParser>>,
    line_buf: String,
    assistant_text: String,
    step_count: usize,
    raw_fallback_lines: usize,
    native_session_id: Option<String>,
}

impl StreamSession {
    pub fn new(agent: AgentId, process_mode: ProcessMode) -> Self {
        let structured_requested = crate::adapters::wants_structured_for(process_mode, agent);
        Self::for_agent_key(
            AgentKey::from_agent_id(agent),
            process_mode,
            structured_requested,
            builtin_stream_registry(),
        )
    }

    /// Key-native construction path with an injectable parser registry.
    pub fn for_agent_key(
        agent_key: AgentKey,
        process_mode: ProcessMode,
        structured_requested: bool,
        registry: &StreamParserRegistry,
    ) -> Self {
        let parser = registry.get(&agent_key);
        // Text mode always remains passthrough. Other modes require both the
        // caller's capability decision and a registered parser.
        let structured =
            process_mode != ProcessMode::Text && structured_requested && parser.is_some();
        if structured {
            tracing::debug!(
                module = targets::RUN,
                op = "stream_session",
                agent = agent_key.as_str(),
                process_mode = process_mode.as_str(),
                "structured stream session open"
            );
        } else if structured_requested && process_mode != ProcessMode::Text && parser.is_none() {
            // Caller asked for structured decoding but no parser is registered —
            // fall back to text chunks so the session still works.
            tracing::warn!(
                module = targets::RUN,
                op = "stream_session",
                agent = agent_key.as_str(),
                process_mode = process_mode.as_str(),
                "structured stream requested but no parser registered; falling back to text"
            );
        }
        Self {
            agent_key,
            structured,
            parser,
            line_buf: String::new(),
            assistant_text: String::new(),
            step_count: 0,
            raw_fallback_lines: 0,
            native_session_id: None,
        }
    }

    pub fn agent_key(&self) -> &AgentKey {
        &self.agent_key
    }

    pub fn is_structured(&self) -> bool {
        self.structured
    }

    pub fn assistant_text(&self) -> &str {
        &self.assistant_text
    }

    pub fn step_count(&self) -> usize {
        self.step_count
    }

    pub fn native_session_id(&self) -> Option<&str> {
        self.native_session_id.as_deref()
    }

    /// Feed a raw process chunk; returns decoded outputs for the UI / chat content.
    pub fn feed(&mut self, stream: OutputStream, text: &str) -> Vec<StreamOutput> {
        if !self.structured {
            return vec![StreamOutput::Chunk {
                stream,
                text: text.to_string(),
            }];
        }

        if stream == OutputStream::Stderr {
            return vec![StreamOutput::Chunk {
                stream: OutputStream::Stderr,
                text: text.to_string(),
            }];
        }

        self.line_buf.push_str(text);
        let mut out = Vec::new();
        while let Some(idx) = self.line_buf.find('\n') {
            let mut line = self.line_buf.drain(..=idx).collect::<String>();
            if line.ends_with('\n') {
                line.pop();
                if line.ends_with('\r') {
                    line.pop();
                }
            }
            out.extend(self.handle_line(&line));
        }
        // Cap incomplete line buffer to avoid unbounded growth on binary garbage.
        if self.line_buf.len() > MAX_LINE_BYTES {
            let truncated: String = self.line_buf.chars().take(200).collect();
            self.line_buf.clear();
            self.raw_fallback_lines += 1;
            out.push(StreamOutput::Step(ProcessStep::Raw {
                text: truncated,
                note: Some("line buffer overflow; discarded incomplete line".into()),
            }));
            self.step_count += 1;
        }
        out
    }

    /// Flush trailing partial line (if any), then optional parser end-state.
    pub fn flush(&mut self) -> Vec<StreamOutput> {
        let mut out = Vec::new();
        if self.structured && !self.line_buf.trim().is_empty() {
            let line = std::mem::take(&mut self.line_buf);
            out.extend(self.handle_line(line.trim_end_matches(['\r', '\n'])));
        } else {
            self.line_buf.clear();
        }
        if let Some(parser) = self.parser.as_ref() {
            if let Some(steps) = parser.flush() {
                out.extend(self.map_steps(steps));
            }
        }
        out
    }

    fn handle_line(&mut self, line: &str) -> Vec<StreamOutput> {
        let line = line.trim();
        if line.is_empty() {
            return Vec::new();
        }
        if self.native_session_id.is_none() {
            if let Some(id) = extract_native_session_id(self.agent_key.as_str(), line) {
                self.native_session_id = Some(id);
            }
        }
        if line.len() > MAX_LINE_BYTES {
            self.raw_fallback_lines += 1;
            self.step_count += 1;
            return vec![StreamOutput::Step(ProcessStep::Raw {
                text: line.chars().take(200).collect(),
                note: Some("line too long".into()),
            })];
        }

        // Agent-specific decode via registry — no concrete AgentId match here.
        let parsed = self.parser.as_ref().and_then(|p| p.parse_line(line));

        match parsed {
            Some(events) if !events.is_empty() => self.map_steps(events),
            Some(_) => Vec::new(),
            None => {
                self.raw_fallback_lines += 1;
                // If it looks like JSON but unknown shape, keep as raw step (not chat body).
                if line.starts_with('{') {
                    if self.step_count < MAX_EMITTED_STEPS {
                        self.step_count += 1;
                        vec![StreamOutput::Step(ProcessStep::Raw {
                            text: line.chars().take(400).collect(),
                            note: Some("unrecognized structured line".into()),
                        })]
                    } else {
                        Vec::new()
                    }
                } else {
                    // Non-JSON line in structured mode: still surface as text (compat).
                    append_assistant(&mut self.assistant_text, line);
                    append_assistant(&mut self.assistant_text, "\n");
                    let mut out = vec![StreamOutput::Chunk {
                        stream: OutputStream::Stdout,
                        text: format!("{line}\n"),
                    }];
                    if self.step_count < MAX_EMITTED_STEPS {
                        self.step_count += 1;
                        out.push(StreamOutput::Step(ProcessStep::Raw {
                            text: line.chars().take(400).collect(),
                            note: Some("non-json line in structured mode".into()),
                        }));
                    }
                    out
                }
            }
        }
    }

    /// Map decoded ProcessSteps into StreamOutput (text chunks + capped steps).
    fn map_steps(&mut self, events: Vec<ProcessStep>) -> Vec<StreamOutput> {
        let mut out = Vec::with_capacity(events.len());
        for step in events {
            if let ProcessStep::Text { text } = &step {
                append_assistant(&mut self.assistant_text, text);
                out.push(StreamOutput::Chunk {
                    stream: OutputStream::Stdout,
                    text: text.clone(),
                });
            }
            // Always accumulate text; only cap Step emissions to the UI.
            let emit_step = self.step_count < MAX_EMITTED_STEPS
                || matches!(step, ProcessStep::Error { .. } | ProcessStep::Tool { .. });
            if emit_step {
                tracing::trace!(
                    module = targets::RUN,
                    op = "stream_step",
                    agent = self.agent_key.as_str(),
                    step = step.kind(),
                    "decoded process step"
                );
                self.step_count += 1;
                out.push(StreamOutput::Step(step));
            }
        }
        out
    }

    pub fn log_summary(&self) {
        if !self.structured {
            return;
        }
        tracing::debug!(
            module = targets::RUN,
            op = "stream_session",
            agent = self.agent_key.as_str(),
            steps = self.step_count,
            raw_fallback_lines = self.raw_fallback_lines,
            assistant_chars = self.assistant_text.chars().count(),
            "structured stream session closed"
        );
    }
}

/// Pull an official session/thread id from one structured stdout line.
pub fn extract_native_session_id(agent_key: &str, line: &str) -> Option<String> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    let v: serde_json::Value = serde_json::from_str(line).ok()?;
    let raw = match agent_key {
        "claude" => first_json_str(&v, &["session_id", "sessionId"]),
        "codex" => first_json_str(&v, &["thread_id", "session_id", "sessionId"])
            .or_else(|| v.pointer("/thread/id").and_then(|x| x.as_str()).map(str::to_string)),
        _ => None,
    }?;
    crate::adapters::session_resume::valid_session_id(&raw).map(str::to_string)
}

fn first_json_str(v: &serde_json::Value, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(s) = v.get(*key).and_then(|x| x.as_str()) {
            let t = s.trim();
            if !t.is_empty() {
                return Some(t.to_string());
            }
        }
    }
    None
}

fn append_assistant(dest: &mut String, chunk: &str) {
    if dest.len() >= MAX_ASSISTANT_CHARS {
        return;
    }
    let room = MAX_ASSISTANT_CHARS - dest.len();
    if chunk.len() <= room {
        dest.push_str(chunk);
        return;
    }
    let mut end = room;
    while end > 0 && !chunk.is_char_boundary(end) {
        end -= 1;
    }
    dest.push_str(&chunk[..end]);
}

#[cfg(test)]
mod tests;
