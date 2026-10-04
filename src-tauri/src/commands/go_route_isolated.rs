//! Isolated Go route start/status/stop. Not the default local gateway.

use tauri::State;

use crate::commands::GuiError;
use crate::go_route_isolated::GoRouteIsolatedStatus;
use crate::state::AppState;

#[tauri::command]
pub async fn start_go_route_isolated(
    state: State<'_, AppState>,
) -> Result<GoRouteIsolatedStatus, GuiError> {
    #[cfg(not(unix))]
    {
        let _ = state;
        return Err(isolated_unavailable());
    }
    #[cfg(unix)]
    {
        let host = state.go_route_isolated();
        tauri::async_runtime::spawn_blocking(move || host.start())
            .await
            .map_err(|err| GuiError::adapter("go.route.isolated.join", err.to_string(), None))
    }
}

#[tauri::command]
pub async fn stop_go_route_isolated(
    state: State<'_, AppState>,
) -> Result<GoRouteIsolatedStatus, GuiError> {
    #[cfg(not(unix))]
    {
        let _ = state;
        return Err(isolated_unavailable());
    }
    #[cfg(unix)]
    {
        let host = state.go_route_isolated();
        tauri::async_runtime::spawn_blocking(move || host.stop())
            .await
            .map_err(|err| GuiError::adapter("go.route.isolated.join", err.to_string(), None))
    }
}

#[tauri::command]
pub async fn get_go_route_isolated_status(
    state: State<'_, AppState>,
) -> Result<GoRouteIsolatedStatus, GuiError> {
    #[cfg(not(unix))]
    {
        let _ = state;
        return Err(isolated_unavailable());
    }
    #[cfg(unix)]
    {
        let host = state.go_route_isolated();
        tauri::async_runtime::spawn_blocking(move || host.status())
            .await
            .map_err(|err| GuiError::adapter("go.route.isolated.join", err.to_string(), None))
    }
}

#[cfg(not(unix))]
fn isolated_unavailable() -> GuiError {
    GuiError::adapter(
        "go.route.isolated.unavailable",
        "Go route is unavailable on this platform",
        None,
    )
}
