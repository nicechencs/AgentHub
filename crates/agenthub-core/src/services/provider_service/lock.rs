//! Per-agent live-write lock and saga guard.

use crate::error::{AppError, Result};
use crate::models::AgentId;
use crate::services::LiveWriteGuard;

use super::ProviderService;

/// RAII guard for a cross-boundary, per-agent provider saga.
///
/// Holding this guard retains the same cross-process lock used by ordinary
/// provider switches. Guarded APIs validate both the originating service and
/// target agent, so callers cannot accidentally use a Claude saga guard for a
/// different agent or service.
pub struct ProviderLiveSagaGuard<'a> {
    service: &'a ProviderService,
    agent: AgentId,
    guard: LiveWriteGuard,
}

impl std::fmt::Debug for ProviderLiveSagaGuard<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ProviderLiveSagaGuard")
            .field("agent", &self.agent)
            .finish_non_exhaustive()
    }
}

impl ProviderLiveSagaGuard<'_> {
    pub fn agent(&self) -> AgentId {
        self.agent
    }

    /// Borrow the shared authority proof for another Core live-write service.
    /// This is intended for one larger orchestration saga and avoids nested
    /// acquisition of the same cross-process lock.
    pub fn as_live_write_guard(&self) -> &LiveWriteGuard {
        &self.guard
    }
}

impl ProviderService {
    /// Acquire the per-agent cross-process lock for an entire provider saga.
    /// The returned guard releases it on drop.
    pub fn begin_live_saga(&self, agent: AgentId) -> Result<ProviderLiveSagaGuard<'_>> {
        let guard = self.acquire_live_lock(agent)?;
        Ok(ProviderLiveSagaGuard {
            service: self,
            agent,
            guard,
        })
    }

    pub(super) fn acquire_live_lock(&self, agent: AgentId) -> Result<LiveWriteGuard> {
        self.authority.acquire(agent)
    }

    pub(super) fn validate_live_saga_guard(
        &self,
        guard: &ProviderLiveSagaGuard<'_>,
        agent: AgentId,
    ) -> Result<()> {
        if !std::ptr::eq(self, guard.service) || guard.agent != agent {
            return Err(AppError::InvalidArg(
                "provider live saga guard does not match this service and agent".into(),
            ));
        }
        self.authority.validate_guard(&guard.guard, agent)
    }
}
