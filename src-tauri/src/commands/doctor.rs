//! `get_doctor_report` — returns agenthub_core::DoctorReport.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use agenthub_core::DoctorReport;
use tauri::State;

use crate::commands::with_hub_blocking;
use crate::state::AppState;

static DOCTOR_REFRESHING: AtomicBool = AtomicBool::new(false);

/// Invoke name: `get_doctor_report`.
/// Frontend: `src/lib/api/doctor.ts` → `getDoctorReport()` / `refreshDoctor()`.
///
/// `force=true` invalidates runtime + agent detect caches before probing
/// (used by「重新检测」and post-install refresh).
/// A cold start with a disk snapshot returns that snapshot immediately and
/// refreshes detect in the background.
#[tauri::command]
pub async fn get_doctor_report(
    state: State<'_, AppState>,
    force: Option<bool>,
) -> Result<DoctorReport, String> {
    let hub = state.hub_arc()?;
    let force = force.unwrap_or(false);
    if !force && !hub.agents.cache_is_warm() {
        if let Some(snapshot) = agenthub_core::services::doctor_snapshot::load(hub.data_dir()) {
            if DOCTOR_REFRESHING
                .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
            {
                let refresh_hub = Arc::clone(&hub);
                tauri::async_runtime::spawn(async move {
                    let _ = with_hub_blocking(refresh_hub, |h| {
                        let _ = h.doctor();
                        Ok(())
                    })
                    .await;
                    DOCTOR_REFRESHING.store(false, Ordering::SeqCst);
                });
            }
            return Ok(snapshot);
        }
    }
    with_hub_blocking(hub, move |hub| {
        if force {
            hub.env.invalidate_cache();
            hub.agents.invalidate_cache();
        }
        Ok(hub.doctor())
    })
    .await
}
