/// Public GitHub repository. About page, User-Agent, and in-app doc links must use this.
pub const GITHUB_REPOSITORY_URL: &str = "https://github.com/nicechencs/AgentHub";

/// HTTP User-Agent for AgentHub's own update and skill-market requests.
pub fn agenthub_user_agent() -> String {
    format!(
        "AgentHub/{} (+{GITHUB_REPOSITORY_URL})",
        env!("CARGO_PKG_VERSION")
    )
}
