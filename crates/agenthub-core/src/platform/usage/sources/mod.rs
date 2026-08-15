//! Agent UsageSource contributions (integration side of the Usage port).
//!
//! Concrete Agent branches live only here — not in UsageService or the
//! platform collector. TODO(P13): relocate under integrations/agents/<key>/.

mod claude_like;
mod codex;
mod dsh;
mod grok;
mod kimi;
mod pi;

use std::sync::Arc;

use super::registry::UsageSourceRegistry;

pub fn build_registry() -> UsageSourceRegistry {
    let mut reg = UsageSourceRegistry::new();
    reg.register(Arc::new(claude_like::ClaudeUsageSource))
        .expect("builtin usage source keys must be unique");
    reg.register(Arc::new(claude_like::WorkBuddyUsageSource))
        .expect("builtin usage source keys must be unique");
    reg.register(Arc::new(grok::GrokUsageSource))
        .expect("builtin usage source keys must be unique");
    reg.register(Arc::new(codex::CodexUsageSource))
        .expect("builtin usage source keys must be unique");
    reg.register(Arc::new(kimi::KimiUsageSource))
        .expect("builtin usage source keys must be unique");
    reg.register(Arc::new(pi::PiUsageSource))
        .expect("builtin usage source keys must be unique");
    reg.register(Arc::new(dsh::DshUsageSource))
        .expect("builtin usage source keys must be unique");
    // Cursor intentionally omitted → unsupported / empty collect.
    reg
}
