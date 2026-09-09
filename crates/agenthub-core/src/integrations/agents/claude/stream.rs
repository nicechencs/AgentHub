use std::sync::Arc;

use crate::utils::stream_parse::claude::ClaudeStreamParser;

pub fn register(ctx: &mut crate::integrations::IntegrationContext<'_>) {
    ctx.stream
        .register(Arc::new(ClaudeStreamParser::new()))
        .expect("unique built-in stream parser key");
}
