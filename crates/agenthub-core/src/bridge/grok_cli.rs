//! Grok CLI session identity for subscription Responses upstream.
//!
//! SuperGrok / Grok Build OAuth talks to `cli-chat-proxy.grok.com`, not the
//! public `api.x.ai` Chat Completions surface. The proxy 426s without these
//! client headers.

pub const GROK_CLI_VERSION: &str = "0.2.114";
pub const GROK_CLI_PROXY_BASE_URL: &str = "https://cli-chat-proxy.grok.com/v1";
pub const GROK_CLI_DEFAULT_MODEL: &str = "grok-4.5";

const TOKEN_AUTH: &str = "xai-grok-cli";

pub fn grok_cli_user_agent() -> String {
    format!("grok-pager/{GROK_CLI_VERSION} grok-shell/{GROK_CLI_VERSION}")
}

pub fn apply_grok_cli_identity(builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
    builder
        .header("x-xai-token-auth", TOKEN_AUTH)
        .header("x-grok-client-version", GROK_CLI_VERSION)
        .header(reqwest::header::USER_AGENT, grok_cli_user_agent())
}
