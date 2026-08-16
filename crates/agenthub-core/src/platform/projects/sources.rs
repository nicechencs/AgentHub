//! Compatibility façade — project sources live in `integrations/agents/<key>/`.

use super::registry::ProjectSourceRegistry;

/// Process-wide builtin project sources (product AgentId::ALL order).
pub fn builtin_project_registry() -> &'static ProjectSourceRegistry {
    &crate::integrations::production_integrations().projects
}
