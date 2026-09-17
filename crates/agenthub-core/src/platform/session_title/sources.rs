//! Compatibility façade — session title sources live in `integrations/agents/<key>/`.

use super::registry::SessionTitleRegistry;

/// Process-wide builtin session title sources (product AgentId::ALL order).
pub fn builtin_session_title_registry() -> &'static SessionTitleRegistry {
    &crate::integrations::production_integrations().session_titles
}
