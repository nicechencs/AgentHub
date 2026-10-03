//! Pi provider cancellation and deletion.
//!
//! Pi keeps provider entries in files shared by several pool rows.  The
//! operation therefore validates live ownership before removing a slot and
//! holds the same per-agent saga guard as provider switching.

use crate::error::{AppError, Result};
use crate::models::AgentId;
use crate::utils::atomic::with_restored_files;

use super::live::require_live_config_write;
use super::{log_provider_op, ProviderLiveSagaGuard, ProviderService};

impl ProviderService {
    /// Cancel or delete one Pi provider row while keeping the live Pi files
    /// and the pool row in one compensating saga.
    ///
    /// `delete_from_library = false` is the Connections-page "取消接入"
    /// operation. It is valid only for the current row and demotes that row
    /// while preserving it in the provider pool. `true` moves the row to the
    /// recovery bin; a non-current row is database-only, while a current row
    /// first removes its owned live Pi slot.
    pub fn disconnect_pi_provider(&self, id: &str, delete_from_library: bool) -> Result<()> {
        let started = std::time::Instant::now();
        let result = (|| {
            let guard = self.begin_live_saga(AgentId::Pi)?;
            self.disconnect_pi_provider_with_guard(&guard, id, delete_from_library)
        })();
        log_provider_op("disconnect", AgentId::Pi, started, &result);
        result
    }

    fn disconnect_pi_provider_with_guard(
        &self,
        guard: &ProviderLiveSagaGuard<'_>,
        id: &str,
        delete_from_library: bool,
    ) -> Result<()> {
        self.validate_live_saga_guard(guard, AgentId::Pi)?;
        super::validate_id(id)?;
        let provider = self
            .get_by_id(id)?
            .ok_or_else(|| AppError::NotFound(format!("provider not found: {id}")))?;
        if provider.agent_id != AgentId::Pi {
            return Err(AppError::NotFound(format!(
                "provider not found: {id} (agent filter: pi)"
            )));
        }
        if !delete_from_library && !provider.is_current {
            return Err(AppError::InvalidArg("只有当前 Pi 连接可以取消接入".into()));
        }

        // Older duplicate rows are intentionally pool-only.  Removing one of
        // them must never touch the live slot selected by a newer row.
        if !provider.is_current {
            return self.connections.delete_provider_if_revision(
                &provider.id,
                AgentId::Pi,
                &provider.updated_at,
            );
        }

        let adapter = self.adapter(AgentId::Pi)?;
        require_live_config_write(adapter.as_ref(), AgentId::Pi)?;
        let paths = adapter.live_backup_paths();
        let path_refs: Vec<_> = paths.iter().map(std::path::PathBuf::as_path).collect();
        with_restored_files(&path_refs, || {
            // Generated projections may keep only a secret reference in the
            // provider row. Resolve it for the ownership check, while the
            // original row/revision remains the database CAS token.
            let live_provider = self.secret_resolver.materialize_for_live(&provider)?;
            crate::adapters::pi::remove_pi_provider(&live_provider)?;
            if delete_from_library {
                self.connections.delete_provider_if_revision(
                    &provider.id,
                    AgentId::Pi,
                    &provider.updated_at,
                )?;
            } else {
                let mut demoted = provider.clone();
                demoted.is_current = false;
                self.connections
                    .update_provider_non_current_if_revision(&demoted, &provider.updated_at)?;
            }
            Ok(())
        })
    }
}
