// Скаффолд модулей: типы/трейты определены, конкретные имплементации подключатся
// в Этапах 2 (audio), 3 (transcription) и 5 (llm).
//
// [B16 audit P2] module-level `#[allow(dead_code)]` оставлен ТОЛЬКО на модулях
// #25 voice-matching pipeline (embeddings/identify/llm_hint/matching/merge_signals).
// Эти модули будут wire-up после ONNX runtime интеграции (см. ROADMAP M3.x).
// `audio` и `providers` теперь без wide-allow — там реально вызываемый код.
//
// [CI clippy] Под `cargo clippy --all-targets -- -D warnings`:
//   - Production code lints (unwrap_used/expect_used/panic = warn в Cargo.toml)
//     становятся deny.
//   - Test code: allow глобально через `cfg_attr(test, ...)`. В тестах
//     `.unwrap()` идиоматичен и читабелен; разрешаем явно.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

mod app_startup;
mod app_window;
mod assistant;
mod audio;
mod audio_io;
mod call_id;
mod call_store;
mod commands;
mod db;
#[allow(dead_code)]
mod embeddings;
// [B3.7] OnnxEmbedder подключается только под фичей. Default build (dev)
// эту deps tree не тащит — ort + ndarray ~30s build cost экономится.
#[cfg(feature = "voice-onnx")]
mod embeddings_onnx;
mod error;
mod events;
#[allow(dead_code)]
mod identify;
#[allow(dead_code)]
mod llm_hint;
#[allow(dead_code)]
mod matching;
#[allow(dead_code)]
mod merge_signals;
// [M12] Локальный движок (PRD M12). Расположен macOS-only — на других
// платформах cfg внутри `local_engine/mod.rs` оставляет модуль пустым (R9).
#[cfg(target_os = "macos")]
mod local_engine;
mod pipeline;
mod providers;
mod secrets;
mod services;
mod state;
mod updater;
// [B3.7c] Voice embedder model — runtime download + SHA256 verify. Всегда
// компилируется (download independent от sherpa-onnx); реально использует
// модель только voice-onnx build через embeddings_onnx::OnnxEmbedder.
mod voice_model;

pub use error::AppError;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
// [CI clippy] `.expect()` на `.run()` — идиоматично для tauri::Builder
// (паника на unrecoverable startup error из-за невалидного `tauri.conf.json`
// или missing capabilities). Альтернатива — `if let Err(e) = …` + exit code,
// но stderr-логирование Tauri и так это покрывает. Локальный allow вместо
// глобального — чтобы новые `.expect()` в коде ловились clippy'м.
#[allow(clippy::expect_used)]
pub fn run() {
    app_startup::install_panic_hook();

    tauri::Builder::default()
        // [B16 audit P0]: single-instance ДОЛЖЕН быть зарегистрирован первым,
        // чтобы при попытке запустить второй процесс (например через
        // wotold:// из браузера) он передал argv в уже запущенное окно
        // и тихо завершился. Иначе две копии гоняются за app.db (corruption).
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            log::info!("single-instance: re-attach, argv={argv:?}");
            // Поднять окно на передний план если есть.
            if let Some(window) = tauri::Manager::get_webview_window(app, "main") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(
            tauri_plugin_log::Builder::default()
                // [B16 audit P2] log rotation: иначе один долгоиграющий user
                // накопит 50MB+ за месяц. 5MB cap + KeepOne — последние 5MB.
                .max_file_size(5 * 1024 * 1024)
                .rotation_strategy(tauri_plugin_log::RotationStrategy::KeepOne)
                // [security-scan W5] По умолчанию плагин пишет Trace, то есть
                // на диск попадало вообще всё, включая чужие крейты и SQL с
                // параметрами. В релизной сборке держим Info; в dev остаётся
                // Debug, потому что там лог и нужен.
                .level(if cfg!(debug_assertions) {
                    log::LevelFilter::Debug
                } else {
                    log::LevelFilter::Info
                })
                .build(),
        )
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_deep_link::init())
        .plugin(
            tauri_plugin_updater::Builder::new()
                .default_version_comparator(updater::compare_versions)
                .build(),
        )
        .setup(|app| {
            let handle = app.handle().clone();
            let state = tauri::async_runtime::block_on(state::init(handle.clone()))?;
            tauri::Manager::manage(app, state);

            // [Q] Подключить эмиссию `queue:state` (очереди stt/diarization/
            // llm) — до warm-up spawn'а, чтобы первый transition был виден
            // QueueMonitor'у.
            pipeline::resource_queue::set_app(handle.clone());

            // [B30.1] Dock-иконка runtime'ом: в dev голый бинарь без .app-бандла —
            // система берёт вшитую в бинарь иконку, а cargo не пересобирает при
            // смене icons/* (иконка «застревала» старой). Явный setApplicationIconImage
            // из padded-1024 PNG даёт корректный Dock и app-switcher в dev и проде.
            #[cfg(target_os = "macos")]
            app_window::set_dock_icon();

            // [S9] Shared flag: «пользователь явно нажал Выход через tray-меню /
            // ⌘Q / app menu». Без этого CloseRequested от красного крестика
            // сворачивает окно в трей; с флагом — даёт нашему graceful-stop
            // pipeline пути отработать и завершить процесс.
            let quitting = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            tauri::Manager::manage(app, quitting.clone());

            app_startup::spawn_startup_tasks(&handle);

            #[cfg(target_os = "macos")]
            app_window::install_app_menu(app, &handle, &quitting)?;
            app_window::install_tray(app, &quitting)?;
            app_window::install_main_window_events(app, &handle, &quitting);
            app_window::install_widget_window(app, &handle);

            // [window] Скрываем нативные macOS-светофоры main-окна — рисуем
            // свои кастомные кнопки (hover-reveal, src/ui/WindowControls.tsx).
            // В fullscreen фронт вернёт их через set_main_traffic_lights_hidden(false).
            #[cfg(target_os = "macos")]
            if let Some(main) = tauri::Manager::get_webview_window(app, "main") {
                app_window::set_main_window_buttons_hidden(&main, true);
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app_window::set_main_traffic_lights_hidden,
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
            commands::pause_recording,
            commands::resume_recording,
            commands::get_recording_state,
            commands::list_calls,
            commands::list_calls_page,
            commands::count_calls,
            commands::list_call_degraded_flags,
            commands::get_call,
            commands::delete_call,
            commands::wipe_all_data,
            commands::list_call_action_items,
            commands::read_call_artifact,
            commands::get_audio_permissions,
            commands::request_audio_permissions,
            commands::open_system_privacy_pane,
            commands::check_for_update,
            commands::apply_update,
            commands::list_call_speakers,
            commands::list_call_speakers_batch,
            commands::confirm_call_speaker,
            commands::unbind_call_speaker,
            commands::list_voice_samples,
            commands::delete_voice_sample,
            commands::get_voice_sample_audio,
            commands::regenerate_recap,
            commands::regenerate_title,
            commands::reprocess_call,
            commands::cancel_reprocess,
            commands::regenerate_empty_recaps,
            commands::cancel_bulk_recap,
            commands::is_call_processing,
            commands::get_queue_state,
            commands::list_active_call_ids,
            commands::get_active_pipeline_count,
            commands::list_call_chunks,
            commands::retry_chunk,
            commands::recover_chunked_call,
            commands::list_call_decisions,
            commands::list_call_open_questions,
            commands::assistant_index_stats,
            commands::assistant_list_chats,
            commands::assistant_get_chat,
            commands::assistant_get_call_thread,
            commands::assistant_delete_chat,
            #[cfg(target_os = "macos")]
            commands::assistant_ask,
            commands::assistant_get_semantic_search,
            commands::assistant_set_semantic_search,
            commands::assistant_get_fragment_text,
            commands::share_text,
            commands::get_call_audio_path,
            commands::export_call_markdown,
            commands::voice_model_status,
            commands::voice_model_download,
            commands::voice_model_delete,
            commands::voice_model_info,
            commands::show_recording_widget,
            commands::hide_recording_widget,
            commands::restore_main_window,
            #[cfg(target_os = "macos")]
            commands::enable_call_detect,
            #[cfg(target_os = "macos")]
            commands::disable_call_detect,
            #[cfg(target_os = "macos")]
            commands::is_call_detect_enabled,
            // [M12.4] Local engine model catalog + preset (macOS only — R9).
            #[cfg(target_os = "macos")]
            commands::local_engine_list_catalog,
            #[cfg(target_os = "macos")]
            commands::local_engine_model_status,
            #[cfg(target_os = "macos")]
            commands::local_engine_model_download,
            #[cfg(target_os = "macos")]
            commands::local_engine_model_delete,
            #[cfg(target_os = "macos")]
            commands::local_engine_get_active_preset,
            #[cfg(target_os = "macos")]
            commands::local_engine_set_active_preset,
            #[cfg(target_os = "macos")]
            commands::local_engine_hw_probe,
            #[cfg(target_os = "macos")]
            commands::local_engine_storage_list,
            #[cfg(target_os = "macos")]
            commands::local_engine_get_keep_resident,
            #[cfg(target_os = "macos")]
            commands::local_engine_set_keep_resident,
            #[cfg(target_os = "macos")]
            commands::local_engine_eval_recap,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
