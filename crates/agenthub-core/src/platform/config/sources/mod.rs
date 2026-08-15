//! Per-agent config projectors (TODO P13: integrations/agents/<key>/).

mod claude;
mod codex;
mod dsh;
mod grok;
mod kimi;
mod util;

use super::registry::ConfigProjectorRegistry;

pub(super) fn register_all(reg: &mut ConfigProjectorRegistry) {
    reg.register(std::sync::Arc::new(claude::ClaudeConfigProjector))
        .expect("builtin config projector keys must be unique");
    reg.register(std::sync::Arc::new(codex::CodexConfigProjector))
        .expect("builtin config projector keys must be unique");
    reg.register(std::sync::Arc::new(kimi::KimiConfigProjector))
        .expect("builtin config projector keys must be unique");
    reg.register(std::sync::Arc::new(grok::GrokConfigProjector))
        .expect("builtin config projector keys must be unique");
    reg.register(std::sync::Arc::new(dsh::DshConfigProjector))
        .expect("builtin config projector keys must be unique");
}
