//! OAuth login options exposed to CLI/GUI.
//!
//! Most agents have a single PKCE option. Pi exposes multiple upstream providers
//! (anthropic / openai-codex / xai / …) that all write into `~/.pi/agent/auth.json`.
//!
//! Provider aliases, refresh support, and quota backends are table-driven so the
//! various match arms do not drift.

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

/// Upstream quota probe backend for a Pi provider entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PiQuotaBackend {
    /// No quota probe implemented.
    None,
    /// ChatGPT / Codex 5h+7d windows.
    Codex,
    /// Grok / xAI billing.
    Grok,
}

/// Built-in Pi OAuth flow kind for a canonical provider key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PiProviderFlow {
    Pkce,
    DeviceCode,
}

/// Single source of truth for Pi provider aliases → canonical auth.json key,
/// AgentHub OAuth flow, token refresh, and quota routing.
#[derive(Debug, Clone, Copy)]
struct PiProviderSpec {
    /// Canonical key written under `~/.pi/agent/auth.json`.
    canonical: &'static str,
    /// Accepted aliases (including the canonical form).
    aliases: &'static [&'static str],
    /// AgentHub-implemented login flow, if any.
    flow: Option<PiProviderFlow>,
    /// Whether `refresh_pi_provider` can renew tokens for this key.
    refreshable: bool,
    /// Quota probe routing for multi-provider Pi accounts.
    quota: PiQuotaBackend,
}

/// Table of known Pi providers. Keep alias lists exhaustive — helpers only scan this.
const PI_PROVIDER_SPECS: &[PiProviderSpec] = &[
    PiProviderSpec {
        canonical: "anthropic",
        aliases: &["anthropic", "claude"],
        flow: Some(PiProviderFlow::Pkce),
        refreshable: true,
        quota: PiQuotaBackend::None,
    },
    PiProviderSpec {
        canonical: "openai-codex",
        aliases: &["openai-codex", "codex", "openai"],
        flow: Some(PiProviderFlow::Pkce),
        refreshable: true,
        quota: PiQuotaBackend::Codex,
    },
    PiProviderSpec {
        canonical: "xai",
        aliases: &["xai", "grok"],
        flow: Some(PiProviderFlow::DeviceCode),
        refreshable: true,
        quota: PiQuotaBackend::Grok,
    },
    PiProviderSpec {
        canonical: "github-copilot",
        aliases: &["github-copilot", "copilot"],
        // Known Pi key, but AgentHub does not implement PKCE or device-code for it.
        flow: None,
        refreshable: false,
        quota: PiQuotaBackend::None,
    },
    PiProviderSpec {
        canonical: "openrouter",
        aliases: &["openrouter"],
        flow: None,
        refreshable: false,
        quota: PiQuotaBackend::None,
    },
    PiProviderSpec {
        canonical: "kimi-coding",
        aliases: &["kimi-coding", "kimi"],
        // Known Pi key, but AgentHub does not implement PKCE or device-code for it.
        flow: None,
        refreshable: false,
        quota: PiQuotaBackend::None,
    },
    PiProviderSpec {
        canonical: "radius",
        aliases: &["radius"],
        flow: None,
        refreshable: false,
        quota: PiQuotaBackend::None,
    },
];

fn lookup_pi_provider(provider_key: &str) -> Option<&'static PiProviderSpec> {
    let key = provider_key.trim();
    if key.is_empty() {
        return None;
    }
    PI_PROVIDER_SPECS.iter().find(|spec| {
        spec.aliases
            .iter()
            .any(|alias| alias.eq_ignore_ascii_case(key))
    })
}

/// List login options for an agent (empty ⇒ OAuth unsupported).
pub fn list_oauth_options(agent: AgentId) -> Vec<OAuthLoginOption> {
    match agent {
        AgentId::Claude => vec![single_pkce(
            agent,
            "claude",
            "Claude Pro/Max",
            "用浏览器登录 Claude 订阅",
        )],
        AgentId::Codex => vec![single_pkce(
            agent,
            "codex",
            "ChatGPT Plus/Pro",
            "用浏览器登录 ChatGPT 订阅",
        )],
        AgentId::Grok => vec![single_pkce(
            agent,
            "xai",
            "Grok / xAI",
            "用浏览器登录 Grok 订阅",
        )],
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
        AgentId::Pi => {
            let spec = lookup_pi_provider(provider_key.unwrap_or(""))?;
            match (spec.flow, spec.canonical) {
                (Some(PiProviderFlow::Pkce), "anthropic") => Some(&PI_ANTHROPIC),
                (Some(PiProviderFlow::Pkce), "openai-codex") => Some(&PI_OPENAI_CODEX),
                _ => None,
            }
        }
        other => {
            // Single-agent OAuth ignores provider_key (or accepts its own id / aliases).
            if let Some(key) = provider_key {
                let key = key.trim();
                if !key.is_empty() && !single_agent_accepts_provider_key(other, key) {
                    return None;
                }
            }
            oauth_provider_for(other)
        }
    }
}

fn single_agent_accepts_provider_key(agent: AgentId, key: &str) -> bool {
    if key.eq_ignore_ascii_case(agent.as_str()) {
        return true;
    }
    match agent {
        AgentId::Claude => key.eq_ignore_ascii_case("claude"),
        AgentId::Codex => {
            key.eq_ignore_ascii_case("codex") || key.eq_ignore_ascii_case("openai-codex")
        }
        AgentId::Grok => key.eq_ignore_ascii_case("xai") || key.eq_ignore_ascii_case("grok"),
        _ => false,
    }
}

/// Whether this option uses the implemented device-code flow (Pi xAI only).
pub fn is_device_code_option(agent: AgentId, provider_key: Option<&str>) -> bool {
    match agent {
        AgentId::Pi => matches!(
            lookup_pi_provider(provider_key.unwrap_or("")).map(|s| s.flow),
            Some(Some(PiProviderFlow::DeviceCode))
        ),
        _ => false,
    }
}

/// Known Pi login keys that are not implemented in AgentHub (not PKCE, not device-code).
pub fn is_unimplemented_pi_oauth(provider_key: Option<&str>) -> bool {
    matches!(
        lookup_pi_provider(provider_key.unwrap_or("")).map(|s| s.flow),
        Some(None)
    )
}

/// Map Pi provider key (or alias) to the canonical auth.json key.
pub fn pi_auth_json_key(provider_key: &str) -> Option<&'static str> {
    lookup_pi_provider(provider_key).map(|s| s.canonical)
}

/// Whether AgentHub can refresh tokens for this Pi provider key / alias.
pub fn pi_provider_refreshable(provider_key: &str) -> bool {
    lookup_pi_provider(provider_key)
        .map(|s| s.refreshable)
        .unwrap_or(false)
}

/// All aliases (including canonical keys) for Pi providers AgentHub can refresh.
///
/// Sorted, de-duplicated. Keep the TS mirror
/// (`src/lib/backend/contracts/oauth-constants.ts` `PI_REFRESH_PROVIDERS`) in lockstep —
/// both sides are asserted by unit tests.
pub fn pi_refreshable_provider_aliases() -> Vec<&'static str> {
    let mut aliases: Vec<&'static str> = PI_PROVIDER_SPECS
        .iter()
        .filter(|spec| spec.refreshable)
        .flat_map(|spec| spec.aliases.iter().copied())
        .collect();
    aliases.sort_unstable();
    aliases.dedup();
    aliases
}

/// Resolve Pi provider alias → quota probe backend (None when unsupported / unknown).
pub fn pi_provider_quota_backend(provider_key: &str) -> PiQuotaBackend {
    lookup_pi_provider(provider_key)
        .map(|s| s.quota)
        .unwrap_or(PiQuotaBackend::None)
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
    // UI only lists the three subscription logins AgentHub implements end-to-end.
    vec![
        OAuthLoginOption {
            id: "anthropic".into(),
            agent_id: AgentId::Pi,
            label: "Claude Pro/Max".into(),
            description: "浏览器登录，写入 Pi 的登录列表".into(),
            flow: OAuthFlowKind::Pkce,
            auth_json_key: Some("anthropic".into()),
        },
        OAuthLoginOption {
            id: "openai-codex".into(),
            agent_id: AgentId::Pi,
            label: "ChatGPT Plus/Pro (Codex)".into(),
            description: "浏览器登录，写入 Pi 的登录列表".into(),
            flow: OAuthFlowKind::Pkce,
            auth_json_key: Some("openai-codex".into()),
        },
        OAuthLoginOption {
            id: "xai".into(),
            agent_id: AgentId::Pi,
            label: "xAI (Grok 订阅)".into(),
            description: "设备码登录，写入 Pi 的登录列表".into(),
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
        assert_eq!(opts.len(), 3);
        assert!(opts.iter().any(|o| o.id == "anthropic"));
        assert!(opts.iter().any(|o| o.id == "openai-codex"));
        assert!(opts
            .iter()
            .any(|o| o.id == "xai" && o.flow == OAuthFlowKind::DeviceCode));
        assert!(opts
            .iter()
            .all(|o| !o.description.contains("auth.json") && !o.label.contains("auth.json")));
        for dead in ["github-copilot", "openrouter", "kimi-coding", "radius"] {
            assert!(
                !opts.iter().any(|o| o.id == dead),
                "unimplemented Pi key {dead} must not be a clickable login option"
            );
        }
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
    fn pi_aliases_normalize_and_route_capabilities() {
        assert_eq!(pi_auth_json_key("OPENAI"), Some("openai-codex"));
        assert_eq!(pi_auth_json_key("grok"), Some("xai"));
        assert!(pi_provider_refreshable("anthropic"));
        assert!(pi_provider_refreshable("openai"));
        assert!(pi_provider_refreshable("xai"));
        assert!(!pi_provider_refreshable("openrouter"));
        // Frozen set mirrored by TS PI_REFRESH_PROVIDERS — update both when the table changes.
        assert_eq!(
            pi_refreshable_provider_aliases(),
            vec![
                "anthropic",
                "claude",
                "codex",
                "grok",
                "openai",
                "openai-codex",
                "xai",
            ]
        );
        assert_eq!(pi_provider_quota_backend("codex"), PiQuotaBackend::Codex);
        assert_eq!(pi_provider_quota_backend("grok"), PiQuotaBackend::Grok);
        assert_eq!(pi_provider_quota_backend("anthropic"), PiQuotaBackend::None);
        assert!(!is_device_code_option(AgentId::Pi, Some("github-copilot")));
        assert!(!is_device_code_option(AgentId::Pi, Some("kimi-coding")));
        assert!(is_unimplemented_pi_oauth(Some("github-copilot")));
        assert!(is_unimplemented_pi_oauth(Some("kimi-coding")));
        assert!(is_unimplemented_pi_oauth(Some("openrouter")));
        assert!(!is_unimplemented_pi_oauth(Some("xai")));
        assert!(is_device_code_option(AgentId::Pi, Some("xai")));
        assert!(!is_device_code_option(AgentId::Pi, Some("anthropic")));
    }

    #[test]
    fn single_agent_options() {
        assert_eq!(list_oauth_options(AgentId::Claude).len(), 1);
        assert_eq!(list_oauth_options(AgentId::Codex).len(), 1);
        assert_eq!(list_oauth_options(AgentId::Grok).len(), 1);
        assert!(list_oauth_options(AgentId::Claude)
            .iter()
            .all(|o| !o.description.contains("auth.json") && !o.description.contains("OAuth")));
        assert_eq!(list_oauth_options(AgentId::Kimi).len(), 0);
        assert_eq!(list_oauth_options(AgentId::Cursor).len(), 0);
        assert_eq!(list_oauth_options(AgentId::Dsh).len(), 0);
        assert!(!oauth_supported(AgentId::Kimi));
        assert!(!oauth_supported(AgentId::Cursor));
        assert!(!oauth_supported(AgentId::Dsh));
    }
}
