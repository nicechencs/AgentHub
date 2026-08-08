//! `get_doctor_report` — returns agenthub_core::DoctorReport.

use agenthub_core::DoctorReport;
use tauri::State;

use crate::commands::with_hub_blocking;
use crate::state::AppState;

/// Invoke name: `get_doctor_report`.
/// Frontend: `src/lib/api/doctor.ts` → `getDoctorReport()` / `refreshDoctor()`.
///
/// `force=true` invalidates runtime + agent detect caches before probing
/// (used by「重新检测」and post-install refresh).
#[tauri::command]
pub async fn get_doctor_report(
    state: State<'_, AppState>,
    force: Option<bool>,
) -> Result<DoctorReport, String> {
    let hub = state.hub_arc()?;
    let force = force.unwrap_or(false);
    with_hub_blocking(hub, move |hub| {
        if force {
            hub.env.invalidate_cache();
            hub.agents.invalidate_cache();
        }
        Ok(hub.doctor())
    })
    .await
}
