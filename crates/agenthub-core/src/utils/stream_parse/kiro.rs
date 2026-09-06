//! Parser for `kiro-cli chat --output-format stream-json`.
//!
//! Verified on kiro-cli 2.21.1 with `--agent-engine v2` (v1 rejects stream-json).
//! Lines are `{type, data}` envelopes; `sessionUpdate.data` is ACP. Do not emit
//! `runFinished.finalText` as a second assistant bubble.

use serde_json::Value;

use super::acp::decode_session_update;
use crate::models::ProcessStep;

pub fn parse_line(line: &str) -> Option<Vec<ProcessStep>> {
    let v: Value = serde_json::from_str(line).ok()?;
    let ty = v.get("type").and_then(|t| t.as_str()).unwrap_or("");
    let data = v.get("data").unwrap_or(&v);

    match ty {
        "runStarted" => {
            let engine = data
                .get("engine")
                .and_then(|e| e.as_str())
                .unwrap_or("starting");
            Some(vec![ProcessStep::Status {
                phase: "starting".into(),
                detail: Some(engine.into()),
            }])
        }
        "metadata" => Some(vec![]),
        "sessionUpdate" | "session_update" => Some(decode_session_update(data)),
        "runFinished" => Some(run_finished(data)),
        "runError" => {
            let message = data
                .get("message")
                .and_then(|m| m.as_str())
                .or_else(|| v.get("message").and_then(|m| m.as_str()))
                .unwrap_or("error")
                .to_string();
            Some(vec![ProcessStep::Error { message }])
        }
        _ => {
            if data.get("sessionUpdate").is_some()
                || data.get("session_update").is_some()
                || data
                    .get("update")
                    .and_then(|u| u.get("sessionUpdate").or_else(|| u.get("session_update")))
                    .is_some()
            {
                return Some(decode_session_update(data));
            }
            None
        }
    }
}

fn run_finished(data: &Value) -> Vec<ProcessStep> {
    let status = data
        .get("status")
        .and_then(|s| s.as_str())
        .unwrap_or("success");
    let reason = data
        .get("stopReason")
        .or_else(|| data.get("stop_reason"))
        .and_then(|s| s.as_str())
        .unwrap_or(status);
    if status.eq_ignore_ascii_case("success")
        || status.eq_ignore_ascii_case("ok")
        || status.eq_ignore_ascii_case("completed")
    {
        vec![ProcessStep::Status {
            phase: "result".into(),
            detail: Some(reason.into()),
        }]
    } else {
        let message = data
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or(reason)
            .to_string();
        vec![ProcessStep::Error { message }]
    }
}

#[cfg(test)]
mod tests;
