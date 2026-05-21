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

mod audio;
mod audio_io;
mod call_store;
mod commands;
mod db;
mod device;
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
    // [B16 audit P1] panic hook: silent-kill процессу не оставляет следов.
    // Пишем backtrace в panic.log + дублируем в stderr. Поверх default hook —
    // вызываем prev_hook так чтобы dev-сборка получала console-friendly stderr.
    let prev_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // Используем DATA_DIR из ENV или fallback на home/.wotold-panic.log:
        // на момент panic AppState может быть не инициализирован.
        let bt = std::backtrace::Backtrace::force_capture();
        let log_dir = std::env::var("HOME")
            .map(|h| std::path::PathBuf::from(h).join("Library/Logs/app.wotold.desktop"))
            .unwrap_or_else(|_| std::path::PathBuf::from("/tmp"));
        let _ = std::fs::create_dir_all(&log_dir);
        let entry = format!(
            "[{}] PANIC at {}:\n{}\n\nBacktrace:\n{}\n\n",
            chrono::Utc::now().to_rfc3339(),
            info.location()
                .map(|l| format!("{}:{}", l.file(), l.line()))
                .unwrap_or_else(|| "<unknown>".into()),
            info,
            bt
        );
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_dir.join("panic.log"))
        {
            use std::io::Write;
            let _ = f.write_all(entry.as_bytes());
        }
        prev_hook(info);
    }));

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
                .build(),
        )
        .plugin(tauri_plugin_dialog::init())
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

            // [B16 audit P2] macOS app menu — без явного menu Tauri даёт только
            // basic App/Quit. Native Cut/Copy/Paste/SelectAll на webview без menu
            // не работают (стандартные ⌘C/⌘V). Add File/Edit/View/Window submenus.
            #[cfg(target_os = "macos")]
            {
                use tauri::menu::{MenuBuilder, SubmenuBuilder};

                let app_menu = SubmenuBuilder::new(&handle, "Wotold")
                    .about(None)
                    .separator()
                    .hide()
                    .hide_others()
                    .show_all()
                    .separator()
                    .quit()
                    .build()?;

                let edit_menu = SubmenuBuilder::new(&handle, "Edit")
                    .undo()
                    .redo()
                    .separator()
                    .cut()
                    .copy()
                    .paste()
                    .select_all()
                    .build()?;

                let view_menu = SubmenuBuilder::new(&handle, "View").fullscreen().build()?;

                let window_menu = SubmenuBuilder::new(&handle, "Window")
                    .minimize()
                    .maximize()
                    .separator()
                    .close_window()
                    .build()?;

                let menu = MenuBuilder::new(&handle)
                    .items(&[&app_menu, &edit_menu, &view_menu, &window_menu])
                    .build()?;
                app.set_menu(menu)?;
            }

            // [B9]: подписка на wotold:// deep-link. Прокси редиректит сюда после OIDC.
            // Парсим URL → emit 'auth:deep-link' с распакованным session+account.
            // Никаких side-effects в Rust — frontend в AccountSection сам сохранит
            // токен в Keychain и обновит /me. Это держит security flow simple
            // и сосредоточенным в одном TypeScript-месте.
            {
                use tauri_plugin_deep_link::DeepLinkExt;
                let app_for_dl = handle.clone();
                handle.deep_link().on_open_url(move |event| {
                    for url in event.urls() {
                        if url.scheme() != "wotold" {
                            continue;
                        }
                        let mut payload = serde_json::Map::new();
                        payload.insert(
                            "path".into(),
                            serde_json::Value::String(url.path().to_string()),
                        );
                        for (k, v) in url.query_pairs() {
                            // session — sensitive: не логируем значение.
                            payload.insert(k.to_string(), serde_json::Value::String(v.to_string()));
                        }
                        let event_name = match url.host_str() {
                            Some("auth") => "auth:deep-link",
                            _ => "deep-link",
                        };
                        if let Err(e) = tauri::Emitter::emit(
                            &app_for_dl,
                            event_name,
                            serde_json::Value::Object(payload),
                        ) {
                            log::error!("emit {event_name} failed: {e}");
                        }
                    }
                });
            }

            // [B2]: graceful stop при window close. Если идёт запись —
            // префлайт-stop через pipeline, потом exit. Иначе sidecar получает
            // SIGHUP, последние ≤5s могут не успеть flush, calls row висит recording.
            if let Some(window) = tauri::Manager::get_webview_window(app, "main") {
                let app_for_event = handle.clone();
                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        let state = tauri::Manager::state::<state::AppState>(&app_for_event);
                        let has_active = tauri::async_runtime::block_on(async {
                            state.recording.lock().await.is_some()
                        });
                        if !has_active {
                            return;
                        }

                        // Останавливаем close, тушим запись в фоне, потом exit.
                        api.prevent_close();
                        let app_for_quit = app_for_event.clone();
                        tauri::async_runtime::spawn(async move {
                            let state = tauri::Manager::state::<state::AppState>(&app_for_quit);
                            let session = state.recording.lock().await.take();
                            if let Some(session) = session {
                                let call_id = session.call_id.clone();
                                if let Err(e) = audio::macos::stop(session).await {
                                    log::error!(
                                        "graceful stop {call_id} failed: {e}; marking failed"
                                    );
                                    let _ = db::fail_recording_with_reason(
                                        &state.db,
                                        &call_id,
                                        Some("Окно закрыто во время записи — аудио неполное."),
                                    )
                                    .await;
                                } else {
                                    log::info!("graceful stop {call_id} ok");
                                }
                            }
                            // [B16 audit P0]: дождаться pipeline-tasks (с таймаутом),
                            // чтобы избежать UB при abrupt exit во время DB write /
                            // file flush. Раньше JoinHandle dropped → tokio cancel.
                            let pending: Vec<_> = {
                                let mut guard = state.pipeline_tasks.lock().await;
                                guard.drain().collect()
                            };
                            if !pending.is_empty() {
                                log::info!(
                                    "graceful shutdown: ждём {} pipeline task(s)",
                                    pending.len()
                                );
                                for (cid, h) in pending {
                                    let waited =
                                        tokio::time::timeout(std::time::Duration::from_secs(8), h)
                                            .await;
                                    match waited {
                                        Ok(Ok(())) => log::info!("pipeline {cid} done"),
                                        Ok(Err(e)) => log::warn!("pipeline {cid} join error: {e}"),
                                        Err(_) => log::warn!("pipeline {cid} timeout — abort"),
                                    }
                                }
                            }
                            // Выход — pipeline не запускаем (юзер закрыл окно осознанно).
                            app_for_quit.exit(0);
                        });
                    }
                });
            }
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
            commands::pause_recording,
            commands::resume_recording,
            commands::get_recording_state,
            commands::list_calls,
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
            commands::set_byo_key,
            commands::delete_byo_key,
            commands::list_byo_status,
            commands::get_account_session_status,
            commands::set_account_session,
            commands::clear_account_session,
            commands::read_account_session_token,
            commands::list_call_speakers,
            commands::confirm_call_speaker,
            commands::unbind_call_speaker,
            commands::list_voice_samples,
            commands::delete_voice_sample,
            commands::regenerate_recap,
            commands::reprocess_call,
            commands::cancel_reprocess,
            commands::get_active_pipeline_count,
            commands::get_call_audio_path,
            commands::export_call_markdown,
            commands::voice_model_status,
            commands::voice_model_download,
            commands::voice_model_delete,
            commands::voice_model_info,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
