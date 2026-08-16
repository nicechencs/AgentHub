//! WorkBuddy integration (`workbuddy`).

mod adapter_facade;
mod install;
mod paths;
mod project;
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
    register_fn_detector(
        ctx,
        AgentId::WorkBuddy,
        crate::adapters::detect_workbuddy_installation,
    );
    register_skills_from_config_dir(ctx, AgentId::WorkBuddy, "skills");
}
