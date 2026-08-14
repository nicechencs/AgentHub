//! Grok Build CLI usage parser (ccusage `adapter-grok`).
//!
//! Source: `$GROK_HOME/sessions/**/updates.jsonl` (else `~/.grok`).
//! Only `sessionUpdate == "turn_completed"` rows with a usable breakdown count.
//! `logs/unified.jsonl` is not a source — it has no per-request model id.
//!
//! Token mapping (OpenAI-style input includes cache):
//! - `inputTokens − cachedReadTokens − cacheCreationTokens` → stored input
//! - `cachedReadTokens` → cache read
//! - `cacheCreationTokens` → cache create (carved from the uncached remainder)
//! - `outputTokens` → output (includes reasoning; do not add `reasoningTokens`)
//! - `costUsdTicks` → invoice USD (`ticks / 1e10`)

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::error::Result;
use crate::models::{AgentId, ParsedUsageEvent};
use crate::utils::paths::agent_home;

/// `costUsdTicks` are fixed-point USD: one tick is 1e-10 USD.
pub(crate) const COST_USD_TICKS_PER_USD: f64 = 1e10;

#[derive(Debug, Clone, Default)]
pub(crate) struct GrokSessionMeta {
    pub session_id: Option<String>,
    pub default_model: Option<String>,
}

#[derive(Debug, Clone, Copy, Default)]
struct GrokTokenRow {
    input_tokens: i64,
    output_tokens: i64,
    cached_read_tokens: i64,
    cache_creation_tokens: i64,
    reasoning_tokens: i64,
    cost_usd_ticks: u64,
}

/// Per-file Grok parse session: summary metadata + in-file dedupe.
pub(crate) struct GrokParser {
    meta: GrokSessionMeta,
    seen: HashSet<String>,
}

impl GrokParser {
    pub(crate) fn new(updates_path: &Path) -> Self {
        Self {
            meta: load_session_meta(updates_path),
            seen: HashSet::new(),
        }
    }

    pub(crate) fn extract_line(
        &mut self,
        line: &str,
        path_session_id: Option<&str>,
    ) -> std::result::Result<Vec<ParsedUsageEvent>, ()> {
        let events = extract_grok_events(
            line,
            path_session_id,
            self.meta.session_id.as_deref(),
            self.meta.default_model.as_deref(),
        )?;
        let mut out = Vec::with_capacity(events.len());
        for ev in events {
            if self.seen.insert(ev.raw_hash.clone()) {
                out.push(ev);
            }
        }
        Ok(out)
    }
}

pub(crate) fn line_might_have_usage_grok(line: &str) -> bool {
    line.contains("\"turn_completed\"")
}

/// Discover `sessions/**/updates.jsonl` under the Grok home.
pub(crate) fn discover_grok_files() -> Result<Vec<PathBuf>> {
    let home = agent_home(AgentId::Grok)?;
    Ok(discover_grok_files_in(&home))
}

pub(crate) fn discover_grok_files_in(home: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let sessions = home.join("sessions");
    walk_updates_jsonl(&sessions, &mut out);
    out.sort();
    out.dedup();
    out
}

pub(crate) fn session_id_from_updates_path(path: &Path) -> Option<String> {
    // Grok session dirs are often authored on Windows; normalize separators so
    // Linux CI / cross-platform string paths still resolve the parent session id.
    let raw = path.to_string_lossy();
    let normalized = raw.replace('\\', "/");
    let path = Path::new(normalized.as_str());
    if path.file_name().and_then(|n| n.to_str()) != Some("updates.jsonl") {
        return None;
    }
    path.parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

/// Pricing lookup candidates for a raw Grok model id (e.g. `grok-4.5-build`).
pub(crate) fn pricing_candidates(raw_model: &str) -> Vec<String> {
    let mut candidates = Vec::new();
    let mut push = |value: String| {
        if !candidates.iter().any(|existing| existing == &value) {
            candidates.push(value);
        }
    };

    let stripped = raw_model
        .strip_prefix("[grok] ")
        .unwrap_or(raw_model)
        .trim();
    if stripped.is_empty() {
        return candidates;
    }

    let normalized = stripped
        .strip_suffix("-build")
        .unwrap_or(stripped)
        .to_string();

    push(stripped.to_string());
    push(format!("xai/{stripped}"));
    push(format!("x-ai/{stripped}"));
    push(normalized.clone());
    push(format!("xai/{normalized}"));
    push(format!("x-ai/{normalized}"));
    candidates
}

pub(crate) fn grok_model_has_pricing(model: &str) -> bool {
    use crate::usage::has_embedded_pricing;
    pricing_candidates(model)
        .iter()
        .any(|c| has_embedded_pricing(c))
        || has_embedded_pricing(model)
}

/// Convert Grok's fixed-point `costUsdTicks` into USD, if the record carried any.
pub(crate) fn cost_usd_from_ticks(ticks: u64) -> Option<f64> {
    (ticks > 0).then(|| ticks as f64 / COST_USD_TICKS_PER_USD)
}

/// Split `inputTokens` into uncached, cache-read and cache-write parts.
///
/// `cachedReadTokens` is a subset of `inputTokens`. `cacheCreationTokens` is
/// treated as a sibling subset of the uncached remainder so the three parts
/// sum back to `inputTokens`.
pub(crate) fn split_input_tokens(
    input: i64,
    cached_read: i64,
    cache_creation: i64,
) -> (i64, i64, i64) {
    let input = input.max(0);
    let cache_read = cached_read.max(0).min(input);
    let uncached = input - cache_read;
    let cache_creation = cache_creation.max(0).min(uncached);
    (uncached - cache_creation, cache_read, cache_creation)
}

pub(crate) fn extract_grok_events(
    line: &str,
    path_session_id: Option<&str>,
    summary_session_id: Option<&str>,
    default_model: Option<&str>,
) -> std::result::Result<Vec<ParsedUsageEvent>, ()> {
    let v: serde_json::Value = serde_json::from_str(line).map_err(|_| ())?;
    let params = v.get("params").filter(|p| p.is_object());
    let Some(params) = params else {
        return Ok(Vec::new());
    };
    let Some(update) = params.get("update").filter(|u| u.is_object()) else {
        return Ok(Vec::new());
    };
    if update.get("sessionUpdate").and_then(|s| s.as_str()) != Some("turn_completed") {
        return Ok(Vec::new());
    }
    let Some(usage) = update.get("usage").filter(|u| u.is_object()) else {
        return Ok(Vec::new());
    };

    let event_id = params
        .pointer("/_meta/eventId")
        .and_then(|x| x.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    let session_id = string_field(params, &["sessionId", "session_id"])
        .or_else(|| summary_session_id.map(str::to_string))
        .or_else(|| path_session_id.map(str::to_string));

    let ts = resolve_timestamp(&v, params.get("_meta"));

    let mut events = Vec::new();
    for (raw_model, row) in model_usage_rows(usage, default_model) {
        let (uncached, cache_read, cache_create) = split_input_tokens(
            row.input_tokens,
            row.cached_read_tokens,
            row.cache_creation_tokens,
        );
        if uncached == 0
            && cache_read == 0
            && cache_create == 0
            && row.output_tokens == 0
            && row.reasoning_tokens == 0
        {
            continue;
        }
        let cost_usd = cost_usd_from_ticks(row.cost_usd_ticks);
        let raw_hash = grok_dedupe_key(
            event_id.as_deref(),
            session_id.as_deref(),
            &ts,
            &raw_model,
            uncached,
            row.output_tokens,
            cache_read,
            cache_create,
            row.reasoning_tokens,
        );
        events.push(ParsedUsageEvent {
            agent_id: AgentId::Grok,
            model: raw_model,
            input_tokens: uncached,
            output_tokens: row.output_tokens.max(0),
            cache_creation_tokens: cache_create,
            cache_read_tokens: cache_read,
            session_id: session_id.clone(),
            ts: ts.clone(),
            raw_hash,
            cost_usd,
        });
    }
    Ok(events)
}

fn model_usage_rows(
    usage: &serde_json::Value,
    default_model: Option<&str>,
) -> Vec<(String, GrokTokenRow)> {
    if let Some(map) = usage.get("modelUsage").or_else(|| usage.get("model_usage")) {
        if let Some(obj) = map.as_object() {
            if !obj.is_empty() {
                let mut rows: Vec<_> = obj
                    .iter()
                    .map(|(model, value)| (model.clone(), token_row(value)))
                    .collect();
                rows.sort_by(|a, b| a.0.cmp(&b.0));
                return rows;
            }
        }
    }
    let model = default_model
        .map(str::to_string)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string());
    vec![(model, token_row(usage))]
}

fn token_row(usage: &serde_json::Value) -> GrokTokenRow {
    GrokTokenRow {
        input_tokens: token_i64(usage, &["inputTokens", "input_tokens"]),
        output_tokens: token_i64(usage, &["outputTokens", "output_tokens"]),
        cached_read_tokens: token_i64(usage, &["cachedReadTokens", "cached_read_tokens"]),
        cache_creation_tokens: token_i64(usage, &["cacheCreationTokens", "cache_creation_tokens"]),
        reasoning_tokens: token_i64(usage, &["reasoningTokens", "reasoning_tokens"]),
        cost_usd_ticks: token_u64(usage, &["costUsdTicks", "cost_usd_ticks"]),
    }
}

fn load_session_meta(updates_path: &Path) -> GrokSessionMeta {
    let summary_path = updates_path.with_file_name("summary.json");
    let Ok(text) = fs::read_to_string(summary_path) else {
        return GrokSessionMeta::default();
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else {
        return GrokSessionMeta::default();
    };
    let session_id = v
        .pointer("/info/id")
        .and_then(|x| x.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let default_model = v
        .get("current_model_id")
        .or_else(|| v.get("currentModelId"))
        .and_then(|x| x.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    GrokSessionMeta {
        session_id,
        default_model,
    }
}

fn resolve_timestamp(root: &serde_json::Value, meta: Option<&serde_json::Value>) -> String {
    if let Some(ms) = meta
        .and_then(|m| {
            m.get("agentTimestampMs")
                .or_else(|| m.get("agent_timestamp_ms"))
        })
        .and_then(value_as_i64)
    {
        if ms > 0 {
            return rfc3339_from_millis(ms);
        }
    }
    if let Some(seconds) = root.get("timestamp").and_then(value_as_i64) {
        if seconds > 0 {
            // Envelope timestamp is Unix seconds.
            let ms = if seconds >= 1_000_000_000_000 {
                seconds
            } else {
                seconds.saturating_mul(1000)
            };
            return rfc3339_from_millis(ms);
        }
    }
    rfc3339_from_millis(0)
}

fn rfc3339_from_millis(ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ms)
        .or_else(|| chrono::DateTime::from_timestamp(0, 0))
        .unwrap_or_else(|| chrono::DateTime::UNIX_EPOCH)
        .to_rfc3339()
}

fn grok_dedupe_key(
    event_id: Option<&str>,
    session_id: Option<&str>,
    ts: &str,
    model: &str,
    input: i64,
    output: i64,
    cache_read: i64,
    cache_create: i64,
    reasoning: i64,
) -> String {
    if let Some(event_id) = event_id.filter(|s| !s.is_empty()) {
        return format!("{event_id}|{model}");
    }
    format!(
        "{}|{ts}|{model}|{input}|{output}|{cache_read}|{cache_create}|{reasoning}",
        session_id.unwrap_or(""),
    )
}

fn walk_updates_jsonl(dir: &Path, out: &mut Vec<PathBuf>) {
    if !dir.exists() {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for ent in entries.flatten() {
        let path = ent.path();
        if path.is_dir() {
            walk_updates_jsonl(&path, out);
        } else if path.file_name().and_then(|n| n.to_str()) == Some("updates.jsonl") {
            out.push(path);
        }
    }
}

fn string_field(v: &serde_json::Value, keys: &[&str]) -> Option<String> {
    for k in keys {
        if let Some(s) = v.get(*k).and_then(|x| x.as_str()) {
            if !s.is_empty() {
                return Some(s.to_string());
            }
        }
    }
    None
}

fn token_i64(v: &serde_json::Value, keys: &[&str]) -> i64 {
    for k in keys {
        if let Some(n) = v.get(*k).and_then(value_as_i64) {
            return n.max(0);
        }
    }
    0
}

fn token_u64(v: &serde_json::Value, keys: &[&str]) -> u64 {
    for k in keys {
        if let Some(n) = v.get(*k).and_then(value_as_u64) {
            return n;
        }
    }
    0
}

fn value_as_i64(value: &serde_json::Value) -> Option<i64> {
    value
        .as_i64()
        .or_else(|| value.as_u64().and_then(|n| i64::try_from(n).ok()))
        .or_else(|| value.as_f64().map(|n| n as i64))
}

fn value_as_u64(value: &serde_json::Value) -> Option<u64> {
    value
        .as_u64()
        .or_else(|| value.as_i64().and_then(|n| u64::try_from(n).ok()))
        .or_else(|| value.as_f64().and_then(|n| (n >= 0.0).then_some(n as u64)))
}

#[cfg(test)]
mod tests;
