//! Claude Code live env for gateway model IDs.
//!
//! Writes that take over `settings.json` pin the model (and role slots).
//! Context window is never inferred from a model-id catalog: callers pass an
//! override from route/provider config, or the id itself carries Claude Code's
//! `[1m]` marker. Strip that marker before listed-model matching and before
//! forwarding upstream.

use serde_json::{Map, Value};

/// Claude Code's `[1m]` marker means a 1,048,576 token window.
pub const CLAUDE_WINDOW_1M: u32 = 1_048_576;
pub const CLAUDE_WINDOW_200K: u32 = 200_000;

pub const CLAUDE_CODE_MAX_CONTEXT_TOKENS: &str = "CLAUDE_CODE_MAX_CONTEXT_TOKENS";
pub const CLAUDE_CODE_AUTO_COMPACT_WINDOW: &str = "CLAUDE_CODE_AUTO_COMPACT_WINDOW";

/// Strip a trailing Claude Code `[1m]` / `[1M]` context marker.
pub fn strip_claude_context_marker(model: &str) -> &str {
    let trimmed = model.trim();
    for suffix in ["[1m]", "[1M]"] {
        if let Some(head) = trimmed.strip_suffix(suffix) {
            return head.trim_end();
        }
    }
    trimmed
}

/// True when `listed` and `requested` are the same model id, ignoring a
/// Claude Code `[1m]` marker and ASCII case.
pub fn listed_model_matches(listed: &str, requested: &str) -> bool {
    strip_claude_context_marker(listed).eq_ignore_ascii_case(strip_claude_context_marker(requested))
}

/// Window implied by a `[1m]` marker on the raw model string.
pub fn window_from_context_marker(model: &str) -> Option<u32> {
    let trimmed = model.trim();
    if trimmed.ends_with("[1m]") || trimmed.ends_with("[1M]") {
        Some(CLAUDE_WINDOW_1M)
    } else {
        None
    }
}

/// Window to declare on Claude Code for this model.
///
/// Priority: explicit override, then a `[1m]` marker on the id, then omit
/// (official Claude ids use the client catalog; other ids need the caller to
/// pass an override from route/provider config).
pub fn claude_context_window_for(model: &str, override_tokens: Option<u32>) -> Option<u32> {
    let override_tokens = override_tokens.filter(|tokens| *tokens > 0);
    if let Some(tokens) = override_tokens {
        return Some(tokens);
    }
    window_from_context_marker(model)
}

/// Parse a projector/form `contextWindow` value.
///
/// `auto` / empty omit. `1000000` is accepted as the 1M alias. Any other
/// positive integer is kept as-is so apply can still write a typed override.
pub fn parse_claude_context_window_override(raw: &str) -> Option<u32> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("auto") {
        return None;
    }
    if trimmed == "1000000" {
        return Some(CLAUDE_WINDOW_1M);
    }
    trimmed.parse().ok().filter(|tokens: &u32| *tokens > 0)
}

/// Normalize an env/form raw value onto the projector enum (`auto` / `200000` / `1048576`).
pub fn claude_context_window_choice(raw: &str) -> &'static str {
    match parse_claude_context_window_override(raw) {
        Some(tokens) if tokens == CLAUDE_WINDOW_200K => "200000",
        Some(tokens) if tokens == CLAUDE_WINDOW_1M => "1048576",
        _ => "auto",
    }
}

fn insert_env(env: &mut Map<String, Value>, key: &str, value: &str) {
    env.insert(key.to_owned(), Value::String(value.to_owned()));
}

/// Pin the live Claude model (and role slots) plus window env for a generated
/// or materialized `settings.json` `env` object.
pub fn apply_claude_live_model_env(
    env: &mut Map<String, Value>,
    model: &str,
    override_tokens: Option<u32>,
) {
    let id = strip_claude_context_marker(model);
    if id.is_empty() {
        env.remove("ANTHROPIC_MODEL");
        for key in [
            "ANTHROPIC_DEFAULT_OPUS_MODEL",
            "ANTHROPIC_DEFAULT_SONNET_MODEL",
            "ANTHROPIC_DEFAULT_HAIKU_MODEL",
            "ANTHROPIC_DEFAULT_FABLE_MODEL",
            "CLAUDE_CODE_SUBAGENT_MODEL",
        ] {
            env.remove(key);
        }
        env.remove(CLAUDE_CODE_MAX_CONTEXT_TOKENS);
        env.remove(CLAUDE_CODE_AUTO_COMPACT_WINDOW);
        return;
    }
    insert_env(env, "ANTHROPIC_MODEL", id);
    insert_env(env, "ANTHROPIC_DEFAULT_OPUS_MODEL", id);
    insert_env(env, "ANTHROPIC_DEFAULT_SONNET_MODEL", id);
    insert_env(env, "ANTHROPIC_DEFAULT_HAIKU_MODEL", id);
    insert_env(env, "ANTHROPIC_DEFAULT_FABLE_MODEL", id);
    insert_env(env, "CLAUDE_CODE_SUBAGENT_MODEL", id);
    match claude_context_window_for(model, override_tokens) {
        Some(tokens) => {
            let value = tokens.to_string();
            insert_env(env, CLAUDE_CODE_MAX_CONTEXT_TOKENS, &value);
            insert_env(env, CLAUDE_CODE_AUTO_COMPACT_WINDOW, &value);
        }
        None => {
            env.remove(CLAUDE_CODE_MAX_CONTEXT_TOKENS);
            env.remove(CLAUDE_CODE_AUTO_COMPACT_WINDOW);
        }
    }
}

#[cfg(test)]
mod tests;
