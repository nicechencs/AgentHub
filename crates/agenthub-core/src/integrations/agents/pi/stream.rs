use std::sync::Arc;

use crate::utils::stream_parse::pi::PiStreamParser;

pub fn register(ctx: &mut crate::integrations::IntegrationContext<'_>) {
    ctx.stream
        .register(Arc::new(PiStreamParser::new()))
        .expect("unique built-in stream parser key");
}
