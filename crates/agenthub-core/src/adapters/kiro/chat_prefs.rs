//! Chat model + effort prefs for headless Kiro (`kiro-cli chat --model` / `--effort`).
//!
//! Listing uses `kiro-cli chat --list-models -f json`. Selected values persist in
//! `~/.kiro/agenthub-chat-prefs.json` (AgentHub-owned; Kiro has no config-write contract).

use serde::{Deserialize, Serialize};

use crate::error::{AppError, Result};
use crate::models::{AgentId, LiveChatModel};
use crate::utils::atomic::atomic_write;
use crate::utils::paths::agent_home;
use crate::utils::process::run_capture;

use super::detect_installation;

/// Documented `kiro-cli chat --effort` values.
pub(crate) const KIRO_CHAT_EFFORTS: &[&str] = &["low", "medium", "high", "xhigh", "max"];

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct KiroChatPrefs {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    effort: Option<String>,
}

fn prefs_path() -> Result<std::path::PathBuf> {
    Ok(agent_home(AgentId::Kiro)?.join("agenthub-chat-prefs.json"))
}

fn read_prefs() -> KiroChatPrefs {
    let Ok(path) = prefs_path() else {
        return KiroChatPrefs::default();
    };
    let Ok(raw) = std::fs::read_to_string(&path) else {
        return KiroChatPrefs::default();
    };
    serde_json::from_str(&raw).unwrap_or_default()
}

fn write_prefs(prefs: &KiroChatPrefs) -> Result<()> {
    let path = prefs_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let body = serde_json::to_vec_pretty(prefs)
        .map_err(|e| AppError::InvalidArg(format!("serialize kiro chat prefs: {e}")))?;
    atomic_write(&path, &body)
}

fn normalize_effort(raw: &str) -> Option<String> {
    let value = raw.trim();
    if KIRO_CHAT_EFFORTS.iter().any(|item| *item == value) {
        Some(value.to_string())
    } else {
        None
    }
}

/// Parse `kiro-cli chat --list-models -f json` stdout.
///
/// Shape: `{ "models": [{ "model_id": "…", … }], "default_model": "…" }`.
pub(crate) fn parse_kiro_list_models_json(stdout: &str) -> (Option<String>, Vec<String>) {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(stdout.trim()) else {
        return (None, Vec::new());
    };
    let default = value
        .get("default_model")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned);
    let mut models = Vec::new();
    let mut seen = std::collections::HashSet::new();
    if let Some(arr) = value.get("models").and_then(|v| v.as_array()) {
        for entry in arr {
            let id = entry
                .get("model_id")
                .or_else(|| entry.get("modelId"))
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty());
            let Some(id) = id else { continue };
            if seen.insert(id.to_string()) {
                models.push(id.to_string());
            }
        }
    }
    if let Some(id) = default.as_ref() {
        if seen.insert(id.clone()) {
            models.insert(0, id.clone());
        }
    }
    (default, models)
}

fn list_kiro_cli_models() -> (Option<String>, Vec<String>) {
    // Prefer AgentHub-owned HTTP list when creds work; CLI remains fallback.
    if let Ok(listed) = super::http::list_models_http() {
        if !listed.models.is_empty() {
            return (listed.default_model, listed.models);
        }
    }
    let Some(bin) = detect_installation().binary_path else {
        return (None, Vec::new());
    };
    let Ok(out) = run_capture(&bin, &["chat", "--list-models", "-f", "json"]) else {
        return (None, Vec::new());
    };
    if !out.status.success() {
        return (None, Vec::new());
    }
    parse_kiro_list_models_json(&String::from_utf8_lossy(&out.stdout))
}

fn merge_kiro_chat_models(cli: &[String], current: Option<&str>) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for id in cli.iter().map(|s| s.as_str()).chain(current.into_iter()) {
        let id = id.trim();
        if id.is_empty() || !seen.insert(id.to_string()) {
            continue;
        }
        out.push(id.to_string());
    }
    out
}

fn kiro_efforts() -> Vec<String> {
    KIRO_CHAT_EFFORTS
        .iter()
        .map(|item| (*item).to_string())
        .collect()
}

/// Prefs injected into [`crate::models::RunOptions`] for each headless send.
pub(crate) fn kiro_send_prefs() -> (Option<String>, Option<String>) {
    let live = kiro_live_chat_model();
    (live.model, live.effort)
}

/// Live Chat model chip + picker for Kiro.
pub(crate) fn kiro_live_chat_model() -> LiveChatModel {
    let stored = read_prefs();
    let (cli_default, cli_models) = list_kiro_cli_models();
    let models = merge_kiro_chat_models(&cli_models, stored.model.as_deref());
    let model = stored
        .model
        .filter(|id| models.iter().any(|item| item == id))
        .or_else(|| {
            cli_default
                .filter(|id| models.iter().any(|item| item == id))
                .or_else(|| models.first().cloned())
        });
    let efforts = kiro_efforts();
    let effort = stored
        .effort
        .and_then(|value| normalize_effort(&value))
        .filter(|value| efforts.iter().any(|item| item == value))
        .or_else(|| efforts.iter().find(|item| *item == "high").cloned())
        .or_else(|| efforts.first().cloned());
    LiveChatModel {
        model,
        models,
        effort,
        efforts,
    }
}

pub(crate) fn set_kiro_default_model(model: &str) -> Result<()> {
    let model = model.trim();
    if model.is_empty() {
        return Err(AppError::InvalidArg("model must not be empty".into()));
    }
    let mut prefs = read_prefs();
    prefs.model = Some(model.to_string());
    write_prefs(&prefs)
}

pub(crate) fn set_kiro_default_effort(effort: &str) -> Result<()> {
    let Some(effort) = normalize_effort(effort) else {
        return Err(AppError::InvalidArg(format!(
            "不支持的思考等级: {}",
            effort.trim()
        )));
    };
    let mut prefs = read_prefs();
    prefs.effort = Some(effort);
    write_prefs(&prefs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_list_models_json_reads_ids_and_default() {
        let stdout = r#"{
          "models": [
            {"model_name":"auto","model_id":"auto"},
            {"model_name":"claude-haiku-4.5","model_id":"claude-haiku-4.5"}
          ],
          "default_model":"auto"
        }"#;
        let (default, models) = parse_kiro_list_models_json(stdout);
        assert_eq!(default.as_deref(), Some("auto"));
        assert_eq!(
            models,
            vec!["auto".to_string(), "claude-haiku-4.5".to_string()]
        );
    }

    #[test]
    fn parse_list_models_json_inserts_missing_default() {
        let stdout = r#"{"models":[{"model_id":"a"}],"default_model":"b"}"#;
        let (default, models) = parse_kiro_list_models_json(stdout);
        assert_eq!(default.as_deref(), Some("b"));
        assert_eq!(models, vec!["b".to_string(), "a".to_string()]);
    }

    #[test]
    fn parse_list_models_json_rejects_garbage() {
        assert_eq!(parse_kiro_list_models_json("not-json"), (None, Vec::new()));
    }

    #[test]
    fn normalize_effort_accepts_documented_values() {
        assert_eq!(normalize_effort("medium").as_deref(), Some("medium"));
        assert_eq!(normalize_effort(" max ").as_deref(), Some("max"));
        assert_eq!(normalize_effort("turbo"), None);
    }
}
