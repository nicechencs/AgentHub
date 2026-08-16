use std::sync::Arc;

use crate::integrations::shared::stream::FnStreamParser;
use crate::utils::stream_parse::codex::parse_line;

pub fn register(ctx: &mut crate::integrations::IntegrationContext<'_>) {
    ctx.stream
        .register(Arc::new(FnStreamParser::new("codex", parse_line)))
        .expect("unique built-in stream parser key");
}
