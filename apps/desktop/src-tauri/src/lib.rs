// Скаффолд модулей: типы/трейты определены, конкретные имплементации подключатся
// в Этапах 2 (audio), 3 (transcription) и 5 (llm).
#[allow(dead_code, unused_imports)]
mod audio;
mod commands;
mod db;
mod device;
mod error;
mod pipeline;
// AnthropicProvider::new пока не вызывается из production (ждёт #28),
// Soniox/Gladia подключатся через pipeline. Скоуп allow ещё нужен.
#[allow(dead_code, unused_imports)]
mod providers;
mod secrets;
mod state;
mod updater;

pub use error::AppError;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_log::Builder::default().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(
            tauri_plugin_updater::Builder::new()
                .default_version_comparator(updater::compare_versions)
                .build(),
        )
        .setup(|app| {
            let handle = app.handle().clone();
            let state = tauri::async_runtime::block_on(state::init(handle))?;
            tauri::Manager::manage(app, state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_device_id,
            commands::get_owner_contact,
            commands::list_contacts,
            commands::create_contact,
            commands::update_contact,
            commands::delete_contact,
            commands::rename_owner_contact,
            commands::get_setting,
            commands::set_setting,
            commands::start_recording,
            commands::stop_recording,
            commands::get_recording_state,
            commands::list_calls,
            commands::get_call,
            commands::list_call_action_items,
            commands::read_call_artifact,
            commands::get_audio_permissions,
            commands::request_audio_permissions,
            commands::open_system_privacy_pane,
            commands::check_for_update,
            commands::apply_update,
            commands::set_byo_key,
            commands::delete_byo_key,
            commands::list_byo_status,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
