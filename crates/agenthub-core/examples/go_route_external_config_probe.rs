//! Real-store config source for the isolated external-upstream policy probe.
//!
//! Successful cases write the exact core-generated runtime config to stdout so
//! the shell probe can pipe it directly to Go. The document contains synthetic
//! login information and must never be redirected to a file or log.

use std::ffi::OsStr;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use agenthub_core::models::{
    AccountInput, AccountKind, AdapterSourceKind, AgentId, ProviderInput, RouteDownstreamSurface,
    FEATURE_CODEX_INGRESS_GROK_UPSTREAM, FEATURE_GROK_INGRESS_CODEX_UPSTREAM,
};
use agenthub_core::AgentHub;
use serde_json::json;

const PROVIDER_ID: &str = "external-policy-provider";
const API_KEY: &str = "sk_probe_external_policy_do_not_use";
const ACCESS_TOKEN: &str = "oauth_probe_external_policy_access_do_not_use";
const REFRESH_TOKEN: &str = "oauth_probe_external_policy_refresh_do_not_use";
const CHATGPT_ACCOUNT_ID: &str = "acct_probe_external_policy_do_not_use";

type ProbeResult<T> = Result<T, ProbeError>;

enum ProbeError {
    Rejected,
    Fatal(&'static str),
}

struct SeededSource {
    kind: AdapterSourceKind,
    id: String,
    target: AgentId,
    surface: RouteDownstreamSurface,
}

fn main() {
    match run() {
        Ok(()) => {}
        Err(ProbeError::Rejected) => {
            eprintln!("external policy case rejected");
            std::process::exit(3);
        }
        Err(ProbeError::Fatal(message)) => {
            eprintln!("external policy probe failed: {message}");
            std::process::exit(1);
        }
    }
}

fn run() -> ProbeResult<()> {
    let mut args = std::env::args_os().skip(1);
    let case_id = args
        .next()
        .and_then(|value| value.into_string().ok())
        .ok_or(ProbeError::Fatal(
            "usage: go_route_external_config_probe <case-id> <scratch>",
        ))?;
    let scratch = args.next().map(PathBuf::from).ok_or(ProbeError::Fatal(
        "usage: go_route_external_config_probe <case-id> <scratch>",
    ))?;
    if args.next().is_some() {
        return Err(ProbeError::Fatal("unexpected extra arguments"));
    }
    let (data, skills) = validate_scratch(&scratch)?;
    let hub = AgentHub::open_with_skills_root(Some(&data), Some(&skills))
        .map_err(|_| ProbeError::Fatal("open isolated AgentHub failed"))?;
    match case_id.as_str() {
        "grok_official_login" => hub
            .db()
            .set_setting(FEATURE_CODEX_INGRESS_GROK_UPSTREAM, "on")
            .map_err(|_| ProbeError::Fatal("enable Codex to Grok pair adapter failed"))?,
        "codex_official_login_to_grok" => hub
            .db()
            .set_setting(FEATURE_GROK_INGRESS_CODEX_UPSTREAM, "on")
            .map_err(|_| ProbeError::Fatal("enable Grok to Codex pair adapter failed"))?,
        _ => {}
    }
    let source = seed_case(&hub, &case_id)?;
    let pool = hub
        .route_pools()
        .ensure_default_pool(source.target, source.surface)
        .map_err(|_| ProbeError::Fatal("create isolated route pool failed"))?;
    hub.route_pools()
        .add_member(&pool.id, source.kind, &source.id)
        .map_err(|_| ProbeError::Fatal("attach isolated source failed"))?;
    let pools = hub
        .route_pools()
        .list_gateway_listener_pools()
        .map_err(|_| ProbeError::Fatal("list isolated route pools failed"))?;
    let config = hub
        .adapter_bridge()
        .build_go_route_isolated_config(&pools)
        .map_err(|_| ProbeError::Rejected)?;
    io::stdout()
        .write_all(&config)
        .map_err(|_| ProbeError::Fatal("write generated config failed"))?;
    Ok(())
}

fn seed_case(hub: &AgentHub, case_id: &str) -> ProbeResult<SeededSource> {
    match case_id {
        "anthropic_api_key" => provider(
            hub,
            AgentId::Claude,
            json!({
                "apiKey": API_KEY,
                "env": {
                    "ANTHROPIC_BASE_URL": "https://api.anthropic.com/v1",
                    "ANTHROPIC_AUTH_TOKEN": API_KEY,
                },
                "model": "claude-probe-model",
            }),
            json!({"preset": "anthropic"}),
            AgentId::Claude,
            RouteDownstreamSurface::Messages,
        ),
        "openai_api_key" => provider(
            hub,
            AgentId::Codex,
            json!({
                "apiKey": API_KEY,
                "base_url": "https://api.openai.com/v1",
                "model": "gpt-probe-model",
            }),
            json!({"preset": "openai"}),
            AgentId::Codex,
            RouteDownstreamSurface::Responses,
        ),
        "openai_chat_api_key" => provider(
            hub,
            AgentId::Codex,
            json!({
                "apiKey": API_KEY,
                "base_url": "https://api.openai.com/v1",
                "model": "gpt-probe-model",
            }),
            json!({"preset": "openai"}),
            AgentId::Dsh,
            RouteDownstreamSurface::ChatCompletions,
        ),
        "kimi_api_key" => provider(
            hub,
            AgentId::Kimi,
            json!({
                "apiKey": API_KEY,
                "baseUrl": "https://api.kimi.com/coding/v1",
                "model": "kimi-k2.5",
            }),
            json!({"preset": "kimi-code-membership"}),
            AgentId::Codex,
            RouteDownstreamSurface::Responses,
        ),
        "kimi_chat_api_key" => provider(
            hub,
            AgentId::Kimi,
            json!({
                "apiKey": API_KEY,
                "baseUrl": "https://api.kimi.com/coding/v1",
                "model": "kimi-k2.5",
            }),
            json!({"preset": "kimi-code-membership"}),
            AgentId::Dsh,
            RouteDownstreamSurface::ChatCompletions,
        ),
        "custom_relay" => provider(
            hub,
            AgentId::Claude,
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://relay.invalid/v1",
                    "ANTHROPIC_AUTH_TOKEN": API_KEY,
                },
                "model": "claude-probe-model",
            }),
            json!({"preset": "custom"}),
            AgentId::Claude,
            RouteDownstreamSurface::Messages,
        ),
        "moonshot_api_key" => provider(
            hub,
            AgentId::Kimi,
            json!({
                "api_key": API_KEY,
                "base_url": "https://api.moonshot.cn/v1",
                "model": "moonshot-probe-model",
            }),
            json!({"preset": "moonshot"}),
            AgentId::Codex,
            RouteDownstreamSurface::Responses,
        ),
        "xai_api_key" => provider(
            hub,
            AgentId::Grok,
            json!({
                "api_key": API_KEY,
                "base_url": "https://api.x.ai/v1",
                "model": "grok-probe-model",
            }),
            json!({"preset": "xai-api"}),
            AgentId::Codex,
            RouteDownstreamSurface::Responses,
        ),
        "anthropic_evil_host" => provider(
            hub,
            AgentId::Claude,
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://api.anthropic.com.evil.invalid/v1",
                    "ANTHROPIC_AUTH_TOKEN": API_KEY,
                },
                "model": "claude-probe-model",
            }),
            json!({"preset": "anthropic"}),
            AgentId::Claude,
            RouteDownstreamSurface::Messages,
        ),
        "anthropic_wrong_port" => provider(
            hub,
            AgentId::Claude,
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://api.anthropic.com:8443/v1",
                    "ANTHROPIC_AUTH_TOKEN": API_KEY,
                },
                "model": "claude-probe-model",
            }),
            json!({"preset": "anthropic"}),
            AgentId::Claude,
            RouteDownstreamSurface::Messages,
        ),
        "openai_evil_host" => provider(
            hub,
            AgentId::Codex,
            json!({
                "apiKey": API_KEY,
                "base_url": "https://api.openai.com.evil.invalid/v1",
                "model": "gpt-probe-model",
            }),
            json!({"preset": "openai"}),
            AgentId::Codex,
            RouteDownstreamSurface::Responses,
        ),
        "openai_query" => provider(
            hub,
            AgentId::Codex,
            json!({
                "apiKey": API_KEY,
                "base_url": "https://api.openai.com/v1?probe=1",
                "model": "gpt-probe-model",
            }),
            json!({"preset": "openai"}),
            AgentId::Codex,
            RouteDownstreamSurface::Responses,
        ),
        "codex_official_login" => account(
            hub,
            AgentId::Codex,
            json!({
                "format": "auth_json",
                "account_id": CHATGPT_ACCOUNT_ID,
                "tokens": {
                    "access_token": ACCESS_TOKEN,
                    "refresh_token": REFRESH_TOKEN,
                }
            }),
            json!({}),
            AgentId::Codex,
            RouteDownstreamSurface::Responses,
        ),
        "codex_official_login_missing_account_id" => account(
            hub,
            AgentId::Codex,
            json!({
                "format": "auth_json",
                "tokens": {
                    "access_token": ACCESS_TOKEN,
                    "refresh_token": REFRESH_TOKEN,
                }
            }),
            json!({}),
            AgentId::Codex,
            RouteDownstreamSurface::Responses,
        ),
        "codex_official_login_to_grok" | "codex_official_login_to_grok_flag_off" => account(
            hub,
            AgentId::Codex,
            json!({
                "format": "auth_json",
                "account_id": CHATGPT_ACCOUNT_ID,
                "tokens": {
                    "access_token": ACCESS_TOKEN,
                    "refresh_token": REFRESH_TOKEN,
                }
            }),
            json!({}),
            AgentId::Grok,
            RouteDownstreamSurface::Responses,
        ),
        "grok_official_login" => account(
            hub,
            AgentId::Grok,
            json!({
                "format": "oauth",
                "provider": "xai",
                "access_token": ACCESS_TOKEN,
                "refresh_token": REFRESH_TOKEN,
            }),
            json!({"source": "oauth_pkce"}),
            AgentId::Codex,
            RouteDownstreamSurface::Responses,
        ),
        "grok_official_login_flag_off" => account(
            hub,
            AgentId::Grok,
            json!({
                "format": "oauth",
                "provider": "xai",
                "access_token": ACCESS_TOKEN,
                "refresh_token": REFRESH_TOKEN,
            }),
            json!({"source": "oauth_pkce"}),
            AgentId::Codex,
            RouteDownstreamSurface::Responses,
        ),
        "kimi_oauth" => account(
            hub,
            AgentId::Kimi,
            json!({
                "format": "oauth",
                "provider": "kimi-code-membership",
                "access_token": ACCESS_TOKEN,
                "refresh_token": REFRESH_TOKEN,
            }),
            json!({"provider": "kimi-code-membership"}),
            AgentId::Codex,
            RouteDownstreamSurface::Responses,
        ),
        _ => Err(ProbeError::Fatal("unknown policy case")),
    }
}

fn provider(
    hub: &AgentHub,
    agent_id: AgentId,
    settings_config: serde_json::Value,
    meta: serde_json::Value,
    target: AgentId,
    surface: RouteDownstreamSurface,
) -> ProbeResult<SeededSource> {
    hub.providers()
        .create(&ProviderInput {
            id: PROVIDER_ID.into(),
            agent_id,
            name: "External policy probe provider".into(),
            settings_config,
            meta,
            is_current: false,
        })
        .map_err(|_| ProbeError::Fatal("create isolated provider failed"))?;
    Ok(SeededSource {
        kind: AdapterSourceKind::Provider,
        id: PROVIDER_ID.into(),
        target,
        surface,
    })
}

fn account(
    hub: &AgentHub,
    agent_id: AgentId,
    credentials: serde_json::Value,
    extra: serde_json::Value,
    target: AgentId,
    surface: RouteDownstreamSurface,
) -> ProbeResult<SeededSource> {
    let created = hub
        .accounts()
        .create(AccountInput {
            agent_id,
            kind: AccountKind::Oauth,
            label: "External policy probe login".into(),
            credentials,
            extra,
            is_current: false,
        })
        .map_err(|_| ProbeError::Fatal("create isolated login failed"))?;
    Ok(SeededSource {
        kind: AdapterSourceKind::Account,
        id: created.id,
        target,
        surface,
    })
}

fn validate_scratch(requested: &Path) -> ProbeResult<(PathBuf, PathBuf)> {
    fs::create_dir_all(requested).map_err(|_| ProbeError::Fatal("create scratch failed"))?;
    let scratch = fs::canonicalize(requested)
        .map_err(|_| ProbeError::Fatal("canonicalize scratch failed"))?;
    let marked = scratch
        .components()
        .any(|part| part.as_os_str() == OsStr::new("agenthub-route-external-policy"));
    let under_temp = [Path::new("/tmp"), Path::new("/var/tmp")]
        .into_iter()
        .filter_map(|root| fs::canonicalize(root).ok())
        .any(|root| scratch != root && scratch.starts_with(root));
    if !marked || !under_temp {
        return Err(ProbeError::Fatal("scratch is outside the probe temp tree"));
    }
    let data = scratch.join("data");
    let skills = scratch.join("skills");
    fs::create_dir_all(&data).map_err(|_| ProbeError::Fatal("create data directory failed"))?;
    fs::create_dir_all(&skills).map_err(|_| ProbeError::Fatal("create skills directory failed"))?;
    Ok((data, skills))
}
