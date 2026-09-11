//! Kiro CLI integration (`kiro`).

mod adapter_facade;
mod install;
mod paths;
mod project;
mod session_title;
mod stream;
mod usage;

pub(crate) use project::load_kiro_excerpt;

use crate::integrations::shared::register::register_fn_detector;
use crate::integrations::IntegrationContext;
use crate::models::AgentId;

pub fn register(ctx: &mut IntegrationContext<'_>) {
    paths::register(ctx);
    install::register(ctx);
    project::register(ctx);
    session_title::register(ctx);
    stream::register(ctx);
    usage::register(ctx);
    register_fn_detector(
        ctx,
        AgentId::Kiro,
        crate::adapters::detect_kiro_installation,
    );
}
