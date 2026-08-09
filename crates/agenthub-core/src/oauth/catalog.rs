//! OAuth login options exposed to CLI/GUI.
//!
//! Most agents have a single PKCE option. Pi exposes multiple upstream providers
//! (anthropic / openai-codex / xai / …) that all write into `~/.pi/agent/auth.json`.

use serde::{Deserialize, Serialize};

use crate::models::AgentId;

use super::providers::{oauth_provider_for, OAuthProvider, PI_ANTHROPIC, PI_OPENAI_CODEX};

/// How the user authenticates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OAuthFlowKind {
    /// Browser PKCE + loopback callback (AgentHub default).
    Pkce,
    /// Device code (user opens a URL and enters a short code).
    DeviceCode,
}

/// One selectable OAuth login target.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuthLoginOption {
    /// Stable id for start_oauth(providerKey=…).
    pub id: String,
    pub agent_id: AgentId,
    pub label: String,
    pub description: String,
    pub flow: OAuthFlowKind,
    /// Pi auth.json key (e.g. `anthropic`, `xai`). None for single-agent OAuth.
    pub auth_json_key: Option<String>,
}

/// List login options for an agent (empty ⇒ OAuth unsupported).
pub fn list_oauth_options(agent: AgentId) -> Vec<OAuthLoginOption> {
    match agent {
        AgentId::Claude => vec![single_pkce(agent, "claude", "Claude Pro/Max", "Anthropic 订阅 OAuth")],
        AgentId::Codex => vec![single_pkce(
            agent,
            "codex",
            "ChatGPT Plus/Pro",
            "OpenAI Codex OAuth",
        )],
        AgentId::Grok => vec![single_pkce(agent, "xai", "Grok / xAI", "xAI OAuth (Grok CLI)")],
        AgentId::Pi => pi_options(),
        _ => vec![],
    }
}

pub fn oauth_supported(agent: AgentId) -> bool {
    !list_oauth_options(agent).is_empty()
}

/// Resolve the PKCE provider for an agent + optional provider key.
pub fn resolve_pkce_provider(
    agent: AgentId,
    provider_key: Option<&str>,
) -> Option<&'static OAuthProvider> {
    match agent {
        AgentId::Pi => match provider_key.unwrap_or("").trim() {
            "anthropic" | "claude" => Some(&PI_ANTHROPIC),
            "openai-codex" | "codex" | "openai" => Some(&PI_OPENAI_CODEX),
            // xAI for Pi is device-code only in upstream pi-ai.
            "xai" | "grok" => None,
            "" => None,
            _ => None,
        },
        other => {
            // Single-agent OAuth ignores provider_key (or accepts its own id).
            if let Some(key) = provider_key {
                let key = key.trim();
                if !key.is_empty()
                    && key != other.as_str()
                    && !(other == AgentId::Claude && key == "claude")
                    && !(other == AgentId::Codex && (key == "codex" || key == "openai-codex"))
                    && !(other == AgentId::Grok && (key == "xai" || key == "grok"))
                {
                    return None;
                }
            }
            oauth_provider_for(other)
        }
    }
}

/// Whether this option uses device-code flow.
pub fn is_device_code_option(agent: AgentId, provider_key: Option<&str>) -> bool {
    match agent {
        AgentId::Pi => matches!(
            provider_key.unwrap_or("").trim(),
            "xai" | "grok" | "github-copilot" | "kimi-coding"
        ),
        _ => false,
    }
}

/// Map Pi provider key for auth.json writes.
pub fn pi_auth_json_key(provider_key: &str) -> Option<&'static str> {
    match provider_key.trim() {
        "anthropic" | "claude" => Some("anthropic"),
        "openai-codex" | "codex" | "openai" => Some("openai-codex"),
        "xai" | "grok" => Some("xai"),
        "github-copilot" | "copilot" => Some("github-copilot"),
        "openrouter" => Some("openrouter"),
        "kimi-coding" | "kimi" => Some("kimi-coding"),
        "radius" => Some("radius"),
        _ => None,
    }
}

fn single_pkce(agent: AgentId, id: &str, label: &str, description: &str) -> OAuthLoginOption {
    OAuthLoginOption {
        id: id.into(),
        agent_id: agent,
        label: label.into(),
        description: description.into(),
        flow: OAuthFlowKind::Pkce,
        auth_json_key: None,
    }
}

fn pi_options() -> Vec<OAuthLoginOption> {
    vec![
        OAuthLoginOption {
            id: "anthropic".into(),
            agent_id: AgentId::Pi,
            label: "Claude Pro/Max".into(),
            description: "写入 Pi auth.json → anthropic".into(),
            flow: OAuthFlowKind::Pkce,
            auth_json_key: Some("anthropic".into()),
        },
        OAuthLoginOption {
            id: "openai-codex".into(),
            agent_id: AgentId::Pi,
            label: "ChatGPT Plus/Pro (Codex)".into(),
            description: "写入 Pi auth.json → openai-codex".into(),
            flow: OAuthFlowKind::Pkce,
            auth_json_key: Some("openai-codex".into()),
        },
        OAuthLoginOption {
            id: "xai".into(),
            agent_id: AgentId::Pi,
            label: "xAI (Grok 订阅)".into(),
            description: "设备码登录 → Pi auth.json → xai".into(),
            flow: OAuthFlowKind::DeviceCode,
            auth_json_key: Some("xai".into()),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pi_has_multi_provider_options() {
        let opts = list_oauth_options(AgentId::Pi);
        assert!(opts.len() >= 3);
        assert!(opts.iter().any(|o| o.id == "anthropic"));
        assert!(opts.iter().any(|o| o.id == "openai-codex"));
        assert!(opts.iter().any(|o| o.id == "xai" && o.flow == OAuthFlowKind::DeviceCode));
        assert!(oauth_supported(AgentId::Pi));
    }

    #[test]
    fn resolve_pi_pkce_providers() {
        assert!(resolve_pkce_provider(AgentId::Pi, Some("anthropic")).is_some());
        assert!(resolve_pkce_provider(AgentId::Pi, Some("openai-codex")).is_some());
        assert!(resolve_pkce_provider(AgentId::Pi, Some("xai")).is_none());
        assert_eq!(pi_auth_json_key("claude"), Some("anthropic"));
    }

    #[test]
    fn single_agent_options() {
        assert_eq!(list_oauth_options(AgentId::Claude).len(), 1);
        assert_eq!(list_oauth_options(AgentId::Kimi).len(), 0);
        assert!(!oauth_supported(AgentId::Kimi));
    }
}
