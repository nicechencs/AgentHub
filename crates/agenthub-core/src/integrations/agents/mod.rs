//! One submodule per agent key. Production registration order = AgentId::ALL.

pub mod claude;
pub mod codex;
pub mod cursor;
pub mod dsh;
pub mod grok;
pub mod kimi;
pub mod pi;
pub mod workbuddy;

#[cfg(test)]
pub mod demo_agent;

use super::IntegrationContext;

/// Register the eight production agents. Do not add test-only keys here.
pub fn register_production(ctx: &mut IntegrationContext<'_>) {
    claude::register(ctx);
    codex::register(ctx);
    kimi::register(ctx);
    grok::register(ctx);
    pi::register(ctx);
    workbuddy::register(ctx);
    cursor::register(ctx);
    dsh::register(ctx);
}
