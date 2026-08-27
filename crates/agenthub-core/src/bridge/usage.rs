//! Protocol `Usage` IR (`u64`, includes `reasoning_tokens`).
//!
//! This is not dashboard [`crate::models::UsageRecord`]. New usage fields
//! belong in `Usage::normalize`; Chat / Responses / Anthropic only map key names.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// Provider-neutral usage IR. Codex ResponseCompleted needs `reasoning_tokens`
/// (0 when the upstream omitted it). Dashboard rows do not store that column.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cached_input_tokens: Option<u64>,
    #[serde(default)]
    pub reasoning_tokens: u64,
}

impl Usage {
    /// Merge owner: missing total → `input.saturating_add(output)`; `Default` is all 0.
    fn normalize(
        input_tokens: u64,
        output_tokens: u64,
        total_tokens: Option<u64>,
        cached_input_tokens: Option<u64>,
        reasoning_tokens: u64,
    ) -> Self {
        Self {
            input_tokens,
            output_tokens,
            total_tokens: total_tokens.unwrap_or(input_tokens.saturating_add(output_tokens)),
            cached_input_tokens,
            reasoning_tokens,
        }
    }

    /// Chat Completions keys. Missing `prompt_tokens` / `input_tokens` → `None`.
    pub fn from_chat_usage(value: &Value) -> Option<Self> {
        let object = value.as_object()?;
        let input_tokens = object
            .get("prompt_tokens")
            .or_else(|| object.get("input_tokens"))?
            .as_u64()?;
        let output_tokens = object
            .get("completion_tokens")
            .or_else(|| object.get("output_tokens"))
            .and_then(Value::as_u64)
            .unwrap_or_default();
        let total_tokens = object.get("total_tokens").and_then(Value::as_u64);
        let cached_input_tokens = object
            .get("prompt_tokens_details")
            .and_then(Value::as_object)
            .and_then(|details| details.get("cached_tokens"))
            .and_then(Value::as_u64);
        Some(Self::normalize(
            input_tokens,
            output_tokens,
            total_tokens,
            cached_input_tokens,
            usage_reasoning_tokens(object),
        ))
    }

    /// Responses keys. Missing `input_tokens` → `None`.
    pub fn from_responses_usage(value: &Value) -> Option<Self> {
        let object = value.as_object()?;
        let input_tokens = object.get("input_tokens")?.as_u64()?;
        let output_tokens = object
            .get("output_tokens")
            .and_then(Value::as_u64)
            .unwrap_or_default();
        let total_tokens = object.get("total_tokens").and_then(Value::as_u64);
        let cached_input_tokens = object
            .get("input_tokens_details")
            .and_then(Value::as_object)
            .and_then(|details| details.get("cached_tokens"))
            .and_then(Value::as_u64);
        Some(Self::normalize(
            input_tokens,
            output_tokens,
            total_tokens,
            cached_input_tokens,
            usage_reasoning_tokens(object),
        ))
    }

    /// Anthropic Messages keys. Cached is OR not SUM; reasoning is always 0.
    /// Wire `total_tokens` is ignored.
    pub fn from_anthropic_usage(value: &Value) -> Option<Self> {
        let object = value.as_object()?;
        let input_tokens = object.get("input_tokens")?.as_u64()?;
        let output_tokens = object
            .get("output_tokens")
            .and_then(Value::as_u64)
            .unwrap_or_default();
        let cached_input_tokens = object
            .get("cache_read_input_tokens")
            .and_then(Value::as_u64)
            .or_else(|| {
                object
                    .get("cache_creation_input_tokens")
                    .and_then(Value::as_u64)
            });
        Some(Self::normalize(
            input_tokens,
            output_tokens,
            None,
            cached_input_tokens,
            0,
        ))
    }

    /// Responses JSON: top-level and `output_tokens_details` both carry reasoning.
    pub fn to_responses_json(&self) -> Value {
        serde_json::json!({
            "input_tokens": self.input_tokens,
            "input_tokens_details": {
                "cached_tokens": self.cached_input_tokens.unwrap_or(0),
            },
            "output_tokens": self.output_tokens,
            "output_tokens_details": {
                "reasoning_tokens": self.reasoning_tokens,
            },
            "total_tokens": self.total_tokens,
            "reasoning_tokens": self.reasoning_tokens,
        })
    }

    /// Completed Responses objects always carry usage so Codex can parse reasoning_tokens.
    pub fn completed_responses_json(usage: Option<&Self>) -> Value {
        usage.cloned().unwrap_or_default().to_responses_json()
    }

    /// Anthropic outbound: input/output and optional cache_read only (no reasoning/total).
    pub fn to_anthropic_usage_json(&self) -> Value {
        let mut usage = Map::new();
        usage.insert("input_tokens".to_owned(), Value::from(self.input_tokens));
        usage.insert("output_tokens".to_owned(), Value::from(self.output_tokens));
        if let Some(cached) = self.cached_input_tokens {
            usage.insert("cache_read_input_tokens".to_owned(), Value::from(cached));
        }
        Value::Object(usage)
    }
}

fn usage_reasoning_tokens(object: &Map<String, Value>) -> u64 {
    object
        .get("reasoning_tokens")
        .and_then(Value::as_u64)
        .or_else(|| {
            object
                .get("output_tokens_details")
                .or_else(|| object.get("completion_tokens_details"))
                .and_then(Value::as_object)
                .and_then(|details| details.get("reasoning_tokens"))
                .and_then(Value::as_u64)
        })
        .unwrap_or(0)
}
