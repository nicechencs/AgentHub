//! Cursor Agent CLI integration (`cursor`).

mod adapter_facade;
mod install;
mod paths;
mod project;

use crate::integrations::shared::register::{register_fn_detector, register_skills_from_home};
use crate::integrations::IntegrationContext;
use crate::models::AgentId;

pub fn register(ctx: &mut IntegrationContext<'_>) {
    paths::register(ctx);
    install::register(ctx);
    project::register(ctx);
    register_fn_detector(
        ctx,
        AgentId::Cursor,
        crate::adapters::detect_cursor_installation,
    );
    register_skills_from_home(ctx, AgentId::Cursor, "skills-cursor");
}
