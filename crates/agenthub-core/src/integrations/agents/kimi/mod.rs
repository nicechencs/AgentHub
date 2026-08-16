//! Kimi Code integration (`kimi`).

mod adapter_facade;
mod config;
mod install;
mod paths;
mod project;
mod stream;
mod usage;

use crate::integrations::shared::register::register_fn_detector;
use crate::integrations::IntegrationContext;
use crate::models::AgentId;

pub fn register(ctx: &mut IntegrationContext<'_>) {
    paths::register(ctx);
    install::register(ctx);
    project::register(ctx);
    config::register(ctx);
    usage::register(ctx);
    stream::register(ctx);
    register_fn_detector(
        ctx,
        AgentId::Kimi,
        crate::adapters::detect_kimi_installation,
    );
}
