//! ZCode integration (`zcode`).

mod adapter_facade;
mod install;
mod paths;

use crate::integrations::shared::register::{
    register_fn_detector, register_skills_from_home,
};
use crate::integrations::IntegrationContext;
use crate::models::AgentId;

pub fn register(ctx: &mut IntegrationContext<'_>) {
    paths::register(ctx);
    install::register(ctx);
    register_fn_detector(
        ctx,
        AgentId::Zcode,
        crate::adapters::detect_zcode_installation,
    );
    register_skills_from_home(ctx, AgentId::Zcode, "skills");
}
