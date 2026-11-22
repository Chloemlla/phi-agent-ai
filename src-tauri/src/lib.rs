//! Desktop shell: platform paths, Tauri sinks, commands and the application
//! entry point. All domain wiring lives in `phi-node`.

mod api;
mod app;
mod tauri_sink;

#[cfg(target_os = "android")]
mod android;

use app::{AppStateHandle, compose};
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // The certificate verifier must be ready before the first HTTPS request;
    // the model gateway can fetch models as soon as the UI is interactive.
    #[cfg(target_os = "android")]
    android::init_platform_certificates();

    // Panics on this shell are invisible to the JVM and end in `abort()`, so the
    // hook that preserves their message goes in before any node work starts.
    #[cfg(target_os = "android")]
    android::init_crash_reports();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let handle = app.handle().clone();
            let state =
                compose(handle).map_err(|error| -> Box<dyn std::error::Error> { error.into() })?;
            app.manage(AppStateHandle(state));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            api::get_bootstrap,
            api::set_language,
            api::set_workspace,
            api::create_session,
            api::delete_session,
            api::rename_session,
            api::get_session,
            api::send_message,
            api::add_image_attachment,
            api::image_asset_data_url,
            api::approve,
            api::cancel_run,
            api::list_providers,
            api::save_provider,
            api::remove_provider,
            api::set_active_provider,
            api::test_provider,
            api::preview_models,
            api::describe_models,
            api::list_memories,
            api::search_memories,
            api::get_memory_stats,
            api::add_memory,
            api::forget_memories,
            api::record_web_failure,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
