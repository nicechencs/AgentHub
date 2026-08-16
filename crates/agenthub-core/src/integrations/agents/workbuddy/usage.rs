use std::sync::Arc;

use crate::integrations::shared::usage_claude_like::ClaudeLikeUsageSource;
use crate::models::AgentId;
use crate::usage::session_jsonl::discover_workbuddy_files;

pub fn register(ctx: &mut crate::integrations::IntegrationContext<'_>) {
    ctx.usage
        .register(Arc::new(ClaudeLikeUsageSource::new(
            "workbuddy",
            AgentId::WorkBuddy,
            discover_workbuddy_files,
        )))
        .expect("unique built-in usage source");
}
