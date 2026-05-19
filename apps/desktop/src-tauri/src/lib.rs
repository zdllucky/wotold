// Скаффолд модулей: типы/трейты определены, конкретные имплементации подключатся
// в Этапах 2 (audio), 3 (transcription) и 5 (llm).
#[allow(dead_code, unused_imports)]
mod audio;
mod commands;
mod db;
mod device;
mod error;
#[allow(dead_code, unused_imports)]
mod providers;
mod state;
mod updater;

pub use error::AppError;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_log::Builder::default().build())
        .plugin(tauri_plugin_dialog::init())
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
            commands::delete_contact,
            commands::rename_owner_contact,
            commands::get_setting,
            commands::set_setting,
            commands::check_for_update,
            commands::apply_update,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
