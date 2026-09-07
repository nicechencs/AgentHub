//! Kiro CLI session usage (`~/.kiro/sessions/cli/*.json`).
//!
//! Verified on kiro-cli 2.21.1: each session is a pretty-printed JSON snapshot
//! rewritten per turn (not JSONL). Token fields live on
//! `session_state.conversation_metadata.user_turn_metadatas[]`. Companion
//! `.jsonl` transcripts and Kiro editor session trees have no verified token
//! path. SQLite `data.sqlite3` has no usage tables.
//!
//! Current CLI often writes `0` token counts while still ending the turn;
//! those rows are kept so Kiro shows up after collect. Do not treat
//! `metering_usage` credits as USD.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::Value;

use crate::error::Result;
use crate::integrations::shared::projects::builtin_key;
use crate::models::{AgentId, ParsedUsageEvent};
use crate::platform::usage::{UsageFileParser, UsageLineOutcome, UsageSource};

struct KiroUsageSource;

struct NoopParser;

impl UsageFileParser for NoopParser {
    fn on_line(&mut self, _line: &str, _session_id: Option<&str>) -> UsageLineOutcome {
        UsageLineOutcome::Skipped
    }
}

impl UsageSource for KiroUsageSource {
    fn agent_key(&self) -> crate::platform::AgentKey {
        builtin_key("kiro")
    }

    fn discover_files(&self) -> Result<Vec<PathBuf>> {
        Ok(Vec::new())
    }

    fn begin_file(&self, _path: &Path, _byte_offset: u64) -> Box<dyn UsageFileParser> {
        Box::new(NoopParser)
    }

    fn harvest_events(&self) -> Result<Vec<ParsedUsageEvent>> {
        let home = match crate::utils::paths::agent_home(AgentId::Kiro) {
            Ok(h) => h,
            Err(_) => return Ok(Vec::new()),
        };
        Ok(collect_kiro_usage(&home))
    }
}

pub fn register(ctx: &mut crate::integrations::IntegrationContext<'_>) {
    ctx.usage
        .register(Arc::new(KiroUsageSource))
        .expect("unique built-in usage source");
}

pub(crate) fn collect_kiro_usage(home: &Path) -> Vec<ParsedUsageEvent> {
    let dir = home.join("sessions").join("cli");
    let Ok(entries) = fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| {
            path.is_file()
                && path
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
        })
        .collect();
    files.sort();
    let mut out = Vec::new();
    for path in files {
        out.extend(parse_session_file(&path));
    }
    out
}

fn parse_session_file(path: &Path) -> Vec<ParsedUsageEvent> {
    let Ok(text) = fs::read_to_string(path) else {
        return Vec::new();
    };
    let Ok(root) = serde_json::from_str::<Value>(&text) else {
        return Vec::new();
    };
    let session_id = str_field(&root, &["session_id", "sessionId"])
        .or_else(|| {
            path.file_stem()
                .and_then(|s| s.to_str())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        })
        .unwrap_or_else(|| "unknown".into());
    let fallback_model = root
        .pointer("/session_state/rts_model_state/model_info/model_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned);
    let Some(turns) = root
        .pointer("/session_state/conversation_metadata/user_turn_metadatas")
        .and_then(Value::as_array)
    else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (idx, turn) in turns.iter().enumerate() {
        if let Some(ev) = event_from_turn(turn, &session_id, fallback_model.as_deref(), idx) {
            out.push(ev);
        }
    }
    out
}

fn event_from_turn(
    turn: &Value,
    session_id: &str,
    fallback_model: Option<&str>,
    idx: usize,
) -> Option<ParsedUsageEvent> {
    let ts = str_field(turn, &["end_timestamp", "endTimestamp"])?;
    let input = token_num(turn, &["input_token_count", "inputTokenCount"]);
    let output = token_num(turn, &["output_token_count", "outputTokenCount"]);
    let cache_read = token_num(
        turn,
        &["cache_read_input_token_count", "cacheReadInputTokenCount"],
    );
    let cache_write = token_num(
        turn,
        &["cache_write_input_token_count", "cacheWriteInputTokenCount"],
    );
    let model = str_field(turn, &["model", "modelId", "model_id"])
        .or_else(|| fallback_model.map(ToOwned::to_owned))
        .unwrap_or_else(|| "unknown".into());
    let raw_hash = turn_hash(turn, session_id, &ts, idx);
    Some(ParsedUsageEvent {
        agent_id: AgentId::Kiro,
        model,
        input_tokens: input,
        output_tokens: output,
        cache_creation_tokens: cache_write,
        cache_creation_1h_tokens: 0,
        cache_read_tokens: cache_read,
        session_id: Some(session_id.to_string()),
        ts,
        raw_hash,
        cost_usd: None,
        fast: false,
    })
}

fn turn_hash(turn: &Value, session_id: &str, ts: &str, idx: usize) -> String {
    if let Some(ids) = turn.get("message_ids").or_else(|| turn.get("messageIds")) {
        if let Some(first) = ids.as_array().and_then(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .map(str::trim)
                .find(|s| !s.is_empty())
        }) {
            return format!("kiro:{session_id}:{first}");
        }
        if let Some(first) = ids.as_str().map(str::trim).filter(|s| !s.is_empty()) {
            return format!("kiro:{session_id}:{first}");
        }
    }
    format!("kiro:{session_id}:{ts}:{idx}")
}

fn token_num(v: &Value, keys: &[&str]) -> i64 {
    for k in keys {
        if let Some(n) = v.get(*k).and_then(json_i64) {
            return n.max(0);
        }
    }
    0
}

fn json_i64(v: &Value) -> Option<i64> {
    if let Some(n) = v.as_i64() {
        return Some(n);
    }
    if let Some(n) = v.as_u64() {
        return Some(n.min(i64::MAX as u64) as i64);
    }
    if let Some(n) = v.as_f64() {
        return Some(n.max(0.0) as i64);
    }
    None
}

fn str_field(v: &Value, keys: &[&str]) -> Option<String> {
    for k in keys {
        if let Some(s) = v.get(*k).and_then(Value::as_str) {
            let t = s.trim();
            if !t.is_empty() {
                return Some(t.to_string());
            }
        }
    }
    None
}

#[cfg(test)]
#[path = "usage/tests.rs"]
mod tests;
