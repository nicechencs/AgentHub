use std::sync::Arc;

use crate::integrations::shared::stream::FnStreamParser;
use crate::utils::stream_parse::kiro::parse_line;

pub fn register(ctx: &mut crate::integrations::IntegrationContext<'_>) {
    ctx.stream
        .register(Arc::new(FnStreamParser::new("kiro", parse_line)))
        .expect("unique built-in stream parser key");
}
