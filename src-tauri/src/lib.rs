//! agenthub-gui — thin Tauri v2 shell over agenthub-core.
//! Business logic stays in core; this crate only wires state + commands.

mod adapter_bridge_controller;
mod adapter_control_host;
mod commands;
mod exit_coordinator;
mod file_manager;
mod skill_watch;
mod state;
mod tray;
mod tray_i18n;
mod window_policy;

use state::AppState;
use tauri::{Manager, RunEvent, WindowEvent};
#[cfg(target_os = "macos")]
use window_policy::should_show_on_reopen;
use window_policy::{decide_close_action, CloseAction};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let state = AppState::new();

    tauri::Builder::default()
        // Must be first so a second process exits before other plugins init.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            tray::show_main_window(app);
        }))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            // No extra args: start the normal GUI (tray/close-to-tray still apply).
            None::<Vec<&'static str>>,
        ))
        .plugin(tauri_plugin_dialog::init())
        .manage(state)
        .setup(|app| {
            // Desktop-only auto-update (signed release artifacts + latest.json).
            #[cfg(desktop)]
            {
                if let Err(e) = app
                    .handle()
                    .plugin(tauri_plugin_updater::Builder::new().build())
                {
                    tracing::warn!(error = %e, "updater plugin init failed");
                }
            }
            if let Err(e) = tray::setup_tray(app.handle()) {
                tracing::warn!(error = %e, "system tray setup failed");
            }
            // Best-effort skill dir watch → frontend `skills-fs-changed`.
            if let Ok(hub) = app.state::<AppState>().hub_arc() {
                skill_watch::start_skill_watcher(app.handle().clone(), hub);
            }
            // Bridge recovery is deliberately asynchronous and per-profile: a
            // stale credential or occupied fixed port must not delay GUI/tray
            // startup or prevent other auto-start bridges from restoring.
            if let Ok(hub) = app.state::<AppState>().hub_arc() {
                adapter_bridge_controller::restore_adapter_bridges(
                    hub,
                    app.state::<AppState>().bridge_host(),
                    app.state::<AppState>().bridge_saga_coordinator(),
                    app.state::<AppState>().lifecycle_shutdown_barrier(),
                    app.state::<AppState>().local_entry_restarting(),
                    app.handle().clone(),
                );
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let Some(state) = window.try_state::<AppState>() else {
                    return;
                };
                // A second close while graceful shutdown is in progress must
                // not race the bridge drain. Keep the process alive until the
                // coordinator performs its final `app.exit(0)`.
                if state.exit_coordinator().shutdown_in_progress()
                    && !state.exit_coordinator().exit_ready()
                {
                    api.prevent_close();
                    let _ = window.hide();
                    return;
                }
                let bridge_active = exit_coordinator::ExitCoordinator::requires_impact_confirmation(
                    state.exit_coordinator().prepare_exit(&state.bridge_host()),
                );
                if decide_close_action(state.should_exit(), state.close_to_tray(), bridge_active)
                    == CloseAction::HideToTray
                {
                    api.prevent_close();
                    let _ = window.hide();
                    return;
                }
                // Window close hides while a local bridge is running (above).
                // Tray 退出 still uses request_app_exit so the impact prompt stays.
            }
        })
        .invoke_handler(tauri::generate_handler![
            // Adapter route preview (read-only)
            commands::adapter::analyze_adapter,
            commands::adapter::plan_adapter,
            commands::adapter::list_ticket_wallet,
            commands::adapter::plan_ticket,
            commands::adapter::bind_ticket,
            commands::adapter::unbind_ticket,
            commands::adapter::list_adapter_profiles,
            commands::adapter::list_default_route_pools,
            commands::adapter::list_local_tokens,
            commands::adapter::ensure_source_model_catalog,
            commands::adapter::set_source_custom_models,
            commands::adapter::list_local_token_models,
            commands::adapter::refresh_local_token_models,
            commands::adapter::set_local_token_custom_models,
            commands::adapter::test_local_token,
            commands::adapter::set_local_token,
            commands::adapter::set_chat_completions_shared,
            commands::adapter::attach_pool_owned_authorization,
            commands::adapter::set_route_authorization_enabled,
            commands::adapter::set_route_authorization_priority,
            commands::adapter::remove_route_authorization,
            commands::adapter::recycle_route_membership,
            commands::adapter::sync_connection_authorizations,
            commands::adapter::enroll_native_to_gateway,
            commands::adapter::apply_adapter,
            commands::adapter::start_adapter_bridge,
            commands::adapter::stop_adapter_bridge,
            commands::adapter::get_adapter_bridge_status,
            commands::adapter::start_local_gateway,
            commands::adapter::stop_local_gateway,
            commands::adapter::get_local_gateway_status,
            commands::adapter::start_local_entry,
            commands::adapter::stop_local_entry,
            commands::adapter::get_local_entry_status,
            commands::adapter::set_adapter_bridge_auto_start,
            commands::adapter::remove_adapter,
            commands::lifecycle::request_controlled_restart,
            commands::doctor::get_doctor_report,
            // Agent catalog (read-only directory)
            commands::agent_catalog::list_agent_catalog,
            commands::agent_catalog::get_agent_catalog_entry,
            // Configuration (schema / read / validate / apply)
            commands::configuration::get_agent_config_schema,
            commands::configuration::read_agent_config,
            commands::configuration::validate_agent_config,
            commands::configuration::plan_agent_config,
            commands::configuration::apply_agent_config,
            commands::configuration::materialize_agent_config,
            commands::install::list_install_catalog,
            commands::install::list_install_catalog_cmd,
            commands::install::install_runtime,
            commands::install::install_agent,
            commands::install::upgrade_agent,
            commands::install::check_agent_updates,
            commands::install::uninstall_agent,
            commands::install::open_agent_config_dir,
            commands::install::get_agent_live_paths,
            commands::install::open_path_in_file_manager,
            commands::install::launch_agent_program,
            commands::agent_visibility::list_hidden_agents,
            commands::agent_visibility::set_agent_hidden,
            // MCP inventory (read-only)
            commands::mcp::list_mcp_inventory,
            commands::mcp::list_mcp_inventory_cmd,
            // Plugin / extension pack inventory + enable/disable (not MCP)
            commands::plugins::list_plugin_inventory,
            commands::plugins::list_plugin_inventory_cmd,
            commands::plugins::enable_plugin,
            commands::plugins::enable_plugin_cmd,
            commands::plugins::disable_plugin,
            commands::plugins::disable_plugin_cmd,
            // Provider
            commands::provider::list_provider_presets,
            commands::provider::list_providers,
            commands::provider::get_provider,
            commands::provider::upsert_provider,
            commands::provider::delete_provider,
            commands::trash::list_connection_trash,
            commands::trash::restore_connection_trash,
            commands::trash::delete_connection_trash,
            commands::provider::import_provider_live,
            commands::provider::switch_provider,
            commands::provider::switch_provider_preview,
            commands::provider::undo_switch_provider,
            commands::provider::test_provider_latency,
            commands::provider::list_remote_openai_models,
            commands::provider::detect_api_endpoint_types,
            commands::provider::list_remote_openai_models_for_provider,
            // Skill
            commands::skill::list_skills,
            commands::skill::list_installed_skills,
            commands::skill::list_skill_catalog,
            commands::skill::read_skill_markdown,
            commands::skill::sync_skill,
            commands::skill::disable_skill,
            commands::skill::sync_all_skills,
            commands::skill::install_skill,
            commands::skill::import_private_skill,
            commands::skill::uninstall_skill,
            commands::skill::update_skill,
            commands::skill::apply_skill_projection,
            commands::skill::project_skill,
            commands::skill::search_skill_market,
            commands::skill::install_market_skill,
            commands::skill::list_project_skills,
            commands::skill::install_project_skill,
            commands::skill::uninstall_project_skill,
            commands::skill::read_project_skill_markdown,
            // Backup
            commands::backup::list_backups,
            commands::backup::inspect_backup,
            commands::backup::create_backup,
            commands::backup::restore_backup,
            commands::backup::delete_backup,
            // Account
            commands::account::list_accounts,
            commands::account::reconcile_accounts,
            commands::account::probe_live_auth,
            commands::account::import_account_live,
            commands::account::add_api_key_account,
            commands::account::update_api_key_account,
            commands::account::switch_account,
            commands::account::undo_switch_account,
            commands::account::delete_account,
            commands::account::refresh_account_token,
            commands::account::refresh_account_quota,
            // OAuth PKCE
            commands::oauth::oauth_list_options,
            commands::oauth::oauth_start,
            commands::oauth::oauth_device_start,
            commands::oauth::oauth_device_poll,
            commands::oauth::oauth_device_complete,
            commands::oauth::oauth_wait,
            commands::oauth::oauth_complete,
            commands::oauth::oauth_cancel,
            commands::oauth::oauth_supported,
            // Usage
            commands::usage::usage_get_availability,
            commands::usage::usage_collect,
            commands::usage::usage_query,
            commands::usage::usage_trend,
            commands::usage::usage_overview,
            commands::usage::usage_list_models,
            commands::usage::usage_parser_health,
            commands::usage::usage_missing_pricing,
            commands::usage::gateway_usage_query,
            commands::usage::gateway_usage_overview,
            // Chat
            commands::chat::list_conversations,
            commands::chat::create_conversation,
            commands::chat::ensure_default_conversation,
            commands::chat::update_conversation,
            commands::chat::delete_conversation,
            commands::chat::list_chat_messages,
            commands::chat::chat_send,
            commands::chat::chat_cancel,
            commands::chat::set_chat_model,
            commands::chat::get_chat_model,
            // Agent projects / sessions
            commands::project::list_agent_projects,
            commands::project::list_agent_project_sessions,
            commands::project::get_project_metadata,
            commands::project::upsert_project_meta,
            commands::project::set_show_hidden_projects,
            commands::project::delete_agent_session,
            commands::project::delete_agent_sessions,
            commands::project::delete_agent_project,
            commands::project::delete_agent_projects,
            commands::project::get_agent_project_excerpts,
            // Settings / paths / logs
            commands::settings::get_app_settings,
            commands::settings::get_path_info,
            commands::settings::set_setting,
            commands::settings::log_gui_event,
            commands::settings::open_logs_dir,
            commands::settings::open_external_url,
            commands::settings::pick_directory::pick_directory,
            commands::shell_icon::set_shell_icon,
        ])
        .build(tauri::generate_context!())
        .expect("error while building AgentHub GUI")
        .run(|app_handle, event| {
            // macOS Dock click after hide-to-tray: window is still alive but not
            // visible, so the system reports no visible windows. Surface it the
            // same way the menu-bar tray "打开" action does.
            match &event {
                #[cfg(target_os = "macos")]
                RunEvent::Reopen {
                    has_visible_windows,
                    ..
                } => {
                    if should_show_on_reopen(*has_visible_windows) {
                        tray::show_main_window(app_handle);
                    }
                }
                // A window close with close-to-tray disabled, OS shutdown, or
                // another controllable Tauri exit must not bypass bridge
                // draining. The coordinator avoids a duplicate when its own
                // eventual `app.exit(0)` produces another exit event.
                RunEvent::ExitRequested { api, .. } => {
                    if let Some(state) = app_handle.try_state::<AppState>() {
                        if !state.exit_coordinator().exit_ready() {
                            api.prevent_exit();
                            if !state.exit_coordinator().shutdown_in_progress() {
                                state.request_exit();
                                let _ = state
                                    .exit_coordinator()
                                    .request_exit(app_handle.clone(), state.bridge_host());
                            }
                        }
                    }
                }
                _ => {}
            }
        });
}
