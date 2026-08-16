//! DeepSeek Harness integration (`dsh`).

mod adapter_facade;
mod config;
mod install;
mod paths;
mod project;
mod usage;

use crate::integrations::shared::register::{register_fn_detector, register_skills_from_home};
use crate::integrations::IntegrationContext;
use crate::models::AgentId;

pub fn register(ctx: &mut IntegrationContext<'_>) {
    paths::register(ctx);
    install::register(ctx);
    project::register(ctx);
    config::register(ctx);
    usage::register(ctx);
    register_fn_detector(ctx, AgentId::Dsh, crate::adapters::detect_dsh_installation);
    register_skills_from_home(ctx, AgentId::Dsh, "skills");
}
