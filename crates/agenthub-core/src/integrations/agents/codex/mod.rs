//! Codex integration (`codex`).

mod adapter_facade;
pub(crate) mod auth;
mod config;
mod install;
pub(crate) mod managed;
mod paths;
mod project;
mod stream;
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
    stream::register(ctx);
    register_fn_detector(
        ctx,
        AgentId::Codex,
        crate::adapters::detect_codex_installation,
    );
    register_skills_from_home(ctx, AgentId::Codex, "skills");
}

pub(crate) use auth::write_api_key_auth;
