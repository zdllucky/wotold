// Скаффолд модулей: типы/трейты определены, конкретные имплементации подключатся
// в Этапах 2 (audio), 3 (transcription) и 5 (llm).
#[allow(dead_code, unused_imports)]
mod audio;
mod commands;
mod db;
mod device;
#[allow(dead_code)]
mod embeddings;
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
            commands::get_recording_state,
            commands::list_calls,
            commands::get_call,
            commands::delete_call,
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
