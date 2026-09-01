pub mod addin_install;
pub mod app_state;
pub mod arcgis_install;
pub mod arcgis_tool_client;
pub mod cleanup;
pub mod codex;
pub mod commands;
pub mod credential_store;
pub mod mcp;
pub mod mcp_status;
pub mod paths;
pub mod providers;
pub mod runtime_secret;
pub mod settings;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let local_app_data = std::env::var_os("LOCALAPPDATA")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(app_state::DesktopState::new(local_app_data))
        .invoke_handler(tauri::generate_handler![
            commands::desktop_snapshot,
            commands::rediscover_codex,
            commands::discover_arcgis,
            commands::choose_arcgis_executable,
            commands::open_addin_installer,
            commands::launch_arcgis,
            commands::addin_uninstall_guidance,
            commands::chatgpt_login_start,
            commands::chatgpt_login_cancel,
            commands::chatgpt_logout,
            commands::conversation_start,
            commands::turn_start,
            commands::turn_interrupt,
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                commands::initialize_runtime(handle).await;
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            if matches!(event, tauri::WindowEvent::Destroyed) {
                window
                    .state::<app_state::DesktopState>()
                    .poll_gate()
                    .cancel();
                let handle = window.app_handle().clone();
                tauri::async_runtime::spawn(async move {
                    commands::shutdown_runtime(handle).await;
                });
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running ArcGIS Pro intelligent assistant");
}
