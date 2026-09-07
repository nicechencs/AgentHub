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
    let grok = list_oauth_options(AgentId::Grok);
    assert_eq!(grok.len(), 1);
    assert_eq!(grok[0].id, "xai");
    assert_eq!(grok[0].flow, OAuthFlowKind::DeviceCode);
    assert!(is_device_code_option(AgentId::Grok, None));
    assert!(is_device_code_option(AgentId::Grok, Some("xai")));
    assert!(is_device_code_option(AgentId::Grok, Some("grok")));
    assert!(!is_device_code_option(AgentId::Grok, Some("claude")));
    assert!(resolve_pkce_provider(AgentId::Grok, None).is_none());
    assert!(list_oauth_options(AgentId::Claude)
        .iter()
        .all(|o| !o.description.contains("auth.json") && !o.description.contains("OAuth")));
    assert_eq!(list_oauth_options(AgentId::Kimi).len(), 0);
    assert_eq!(list_oauth_options(AgentId::Cursor).len(), 0);
    assert_eq!(list_oauth_options(AgentId::Dsh).len(), 0);
    assert!(!oauth_supported(AgentId::Kimi));
    assert!(!oauth_supported(AgentId::Cursor));
    assert!(!oauth_supported(AgentId::Dsh));
    let kiro = list_oauth_options(AgentId::Kiro);
    assert_eq!(kiro.len(), 1);
    assert_eq!(kiro[0].id, "kiro");
    assert_eq!(kiro[0].flow, OAuthFlowKind::Cli);
    assert!(oauth_supported(AgentId::Kiro));
    assert!(is_cli_login_option(AgentId::Kiro, None));
    assert!(is_cli_login_option(AgentId::Kiro, Some("kiro")));
    assert!(!is_cli_login_option(AgentId::Kiro, Some("claude")));
    assert!(!is_device_code_option(AgentId::Kiro, None));
    assert!(resolve_pkce_provider(AgentId::Kiro, None).is_none());
    assert!(kiro.iter().all(|o| !o.description.contains("auth.json")
        && !o.description.contains("OAuth")
        && !o.description.contains("sqlite")));
}
