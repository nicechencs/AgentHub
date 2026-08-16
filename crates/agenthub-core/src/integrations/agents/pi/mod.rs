//! Pi coding-agent integration (`pi`).

mod adapter_facade;
mod install;
mod paths;
mod project;
mod stream;
mod usage;

use crate::integrations::shared::register::{
    register_fn_detector, register_skills_from_config_dir,
};
use crate::integrations::IntegrationContext;
use crate::models::AgentId;

pub fn register(ctx: &mut IntegrationContext<'_>) {
    paths::register(ctx);
    install::register(ctx);
    project::register(ctx);
    usage::register(ctx);
    stream::register(ctx);
    register_fn_detector(ctx, AgentId::Pi, crate::adapters::detect_pi_installation);
    register_skills_from_config_dir(ctx, AgentId::Pi, "skills");
}
