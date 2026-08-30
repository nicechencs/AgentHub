//! WorkBuddy usage: `projects/**/*.jsonl` with `providerData.usage`, plus the
//! Claude-isomorphic `message.usage` shape used by older fixtures.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::Value;

use crate::error::Result;
use crate::models::{AgentId, ParsedUsageEvent};
use crate::platform::usage::{UsageFileParser, UsageLineOutcome, UsageSource};
use crate::platform::AgentKey;
use crate::usage::grok::split_input_tokens;
use crate::usage::session_jsonl::{
    discover_workbuddy_files, extract_claude_like, line_might_have_usage_claude_like,
};

pub fn register(ctx: &mut crate::integrations::IntegrationContext<'_>) {
    ctx.usage
        .register(Arc::new(WorkBuddyUsageSource))
        .expect("unique built-in usage source");
}

struct WorkBuddyUsageSource;

struct WorkBuddyParser;

impl UsageSource for WorkBuddyUsageSource {
    fn agent_key(&self) -> AgentKey {
        AgentKey::parse("workbuddy").expect("builtin usage source key must be valid")
    }

    fn discover_files(&self) -> Result<Vec<PathBuf>> {
        discover_workbuddy_files()
    }

    fn begin_file(&self, _path: &Path, _byte_offset: u64) -> Box<dyn UsageFileParser> {
        Box::new(WorkBuddyParser)
    }
}

impl UsageFileParser for WorkBuddyParser {
    fn on_line(&mut self, line: &str, session_id: Option<&str>) -> UsageLineOutcome {
        if !line_might_have_usage_claude_like(line) && !line.contains("\"inputTokens\"") {
            return UsageLineOutcome::Skipped;
        }
        match extract_workbuddy(line, session_id) {
            Ok(Some(ev)) => UsageLineOutcome::Event(ev),
            Ok(None) => match extract_claude_like(AgentId::WorkBuddy, line, session_id) {
                Ok(Some(ev)) => UsageLineOutcome::Event(ev),
                Ok(None) => UsageLineOutcome::Skipped,
                Err(()) => UsageLineOutcome::Failed,
            },
            Err(()) => UsageLineOutcome::Failed,
        }
    }
}

fn extract_workbuddy(
    line: &str,
    session_id: Option<&str>,
) -> std::result::Result<Option<ParsedUsageEvent>, ()> {
    let v: Value = serde_json::from_str(line).map_err(|_| ())?;
    let Some(pd) = v.get("providerData").filter(|p| p.is_object()) else {
        return Ok(None);
    };
    let Some(usage) = pd.get("usage").filter(|u| u.is_object()) else {
        return Ok(None);
    };

    let input = token_num(usage, &["inputTokens", "input_tokens", "prompt_tokens"]);
    let output = token_num(
        usage,
        &["outputTokens", "output_tokens", "completion_tokens"],
    );
    let mut cache_read = token_num(
        usage,
        &[
            "cache_read_input_tokens",
            "cached_input_tokens",
            "cacheReadTokens",
        ],
    );
    if cache_read == 0 {
        cache_read = details_cached(usage);
    }
    if input == 0 && output == 0 && cache_read == 0 {
        return Ok(None);
    }
    let (input, cache_read, cache_create) = split_input_tokens(input, cache_read, 0);

    let model = str_field(pd, &["model", "requestModelName", "requestModelId"])
        .or_else(|| str_field(&v, &["model"]))
        .unwrap_or_else(|| "unknown".into());
    let ts = timestamp_of(&v).unwrap_or_else(|| chrono::Utc::now().to_rfc3339());
    let sid =
        str_field(&v, &["sessionId", "session_id"]).or_else(|| session_id.map(|s| s.to_string()));
    let message_id = str_field(pd, &["messageId", "message_id"]);
    let request_id = str_field(pd, &["conversationRequestId", "requestId"]);
    let raw_hash = match (message_id.as_deref(), request_id.as_deref()) {
        (Some(m), Some(r)) => format!("workbuddy:{m}:{r}"),
        (Some(m), None) => format!("workbuddy:{m}"),
        (None, Some(r)) => format!("workbuddy:{r}"),
        (None, None) => format!(
            "workbuddy:{ts}:{}:{input}:{output}",
            sid.as_deref().unwrap_or("-")
        ),
    };

    Ok(Some(ParsedUsageEvent {
        agent_id: AgentId::WorkBuddy,
        model,
        input_tokens: input,
        output_tokens: output.max(0),
        cache_creation_tokens: cache_create,
        cache_creation_1h_tokens: 0,
        cache_read_tokens: cache_read,
        session_id: sid,
        ts,
        raw_hash,
        cost_usd: None,
        fast: false,
    }))
}

fn details_cached(usage: &Value) -> i64 {
    let details = usage
        .get("inputTokensDetails")
        .or_else(|| usage.get("input_tokens_details"));
    match details {
        Some(Value::Array(arr)) => arr
            .iter()
            .map(|item| token_num(item, &["cached_tokens", "cachedTokens"]))
            .sum(),
        Some(obj) if obj.is_object() => token_num(obj, &["cached_tokens", "cachedTokens"]),
        _ => 0,
    }
}

fn token_num(v: &Value, keys: &[&str]) -> i64 {
    for k in keys {
        if let Some(n) = v.get(*k).and_then(|x| x.as_i64()) {
            return n.max(0);
        }
        if let Some(n) = v.get(*k).and_then(|x| x.as_u64()) {
            return n.min(i64::MAX as u64) as i64;
        }
        if let Some(n) = v.get(*k).and_then(|x| x.as_f64()) {
            return n.max(0.0) as i64;
        }
    }
    0
}

fn str_field(v: &Value, keys: &[&str]) -> Option<String> {
    for k in keys {
        if let Some(s) = v.get(*k).and_then(|x| x.as_str()) {
            let t = s.trim();
            if !t.is_empty() {
                return Some(t.to_string());
            }
        }
    }
    None
}

fn timestamp_of(v: &Value) -> Option<String> {
    if let Some(s) = v.get("timestamp").and_then(|x| x.as_str()) {
        let t = s.trim();
        if !t.is_empty() {
            return Some(t.to_string());
        }
    }
    if let Some(n) = v.get("timestamp").and_then(|x| x.as_i64()) {
        return Some(crate::integrations::shared::sqlite::epoch_to_rfc3339(n));
    }
    if let Some(n) = v.get("timestamp").and_then(|x| x.as_f64()) {
        return Some(crate::integrations::shared::sqlite::epoch_to_rfc3339(
            n as i64,
        ));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_provider_data_usage_and_peels_cached_tokens() {
        let line = r#"{"type":"function_call","timestamp":"2026-08-29T00:00:00.000Z","sessionId":"sess-wb","providerData":{"model":"hy3-x","messageId":"m1","conversationRequestId":"r1","usage":{"inputTokens":100,"outputTokens":20,"inputTokensDetails":[{"cached_tokens":10}]}}}"#;
        let ev = extract_workbuddy(line, None).unwrap().unwrap();
        assert_eq!(ev.agent_id, AgentId::WorkBuddy);
        assert_eq!(ev.model, "hy3-x");
        assert_eq!(ev.input_tokens, 90);
        assert_eq!(ev.cache_read_tokens, 10);
        assert_eq!(ev.output_tokens, 20);
        assert_eq!(ev.session_id.as_deref(), Some("sess-wb"));
        assert_eq!(ev.raw_hash, "workbuddy:m1:r1");
    }

    #[test]
    fn falls_back_to_claude_message_usage() {
        let line = r#"{"timestamp":"2026-01-09T10:00:00.000Z","sessionId":"wb1","message":{"id":"m1","model":"claude-sonnet-4","usage":{"input_tokens":10,"output_tokens":5,"cache_read_input_tokens":1}}}"#;
        let mut parser = WorkBuddyParser;
        match parser.on_line(line, None) {
            UsageLineOutcome::Event(ev) => {
                assert_eq!(ev.input_tokens, 10);
                assert_eq!(ev.output_tokens, 5);
                assert_eq!(ev.cache_read_tokens, 1);
            }
            other => panic!("expected event, got {other:?}"),
        }
    }

    #[test]
    fn skips_lines_without_usage() {
        let line = r#"{"type":"message","timestamp":"2026-08-29T00:00:00.000Z","sessionId":"sess-wb","content":"hi"}"#;
        let mut parser = WorkBuddyParser;
        assert!(matches!(
            parser.on_line(line, None),
            UsageLineOutcome::Skipped
        ));
    }
}
