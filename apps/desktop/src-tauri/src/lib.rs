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

/// [S9] Show main window + unminimise + focus. Used by tray icon click and
/// "Открыть Wotold" menu item. Idempotent — silent if window absent.
fn bring_main_to_front(app: &tauri::AppHandle) {
    let Some(main) = tauri::Manager::get_webview_window(app, "main") else {
        return;
    };
    if let Err(e) = main.unminimize() {
        log::warn!("tray bring-to-front unminimize: {e}");
    }
    if let Err(e) = main.show() {
        log::warn!("tray bring-to-front show: {e}");
    }
    if let Err(e) = main.set_focus() {
        log::warn!("tray bring-to-front set_focus: {e}");
    }
}

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

            // [S9] Shared flag: "пользователь явно нажал Выход через tray-меню /
            // ⌘Q / app menu". Без этого CloseRequested от красного крестика
            // сворачивает окно в трей; с флагом — даёт нашему graceful-stop
            // pipeline пути отработать и завершить процесс.
            let quitting = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            tauri::Manager::manage(app, quitting.clone());

            // [S2] Если CALL_DETECT_ENABLED == "1" с прошлой сессии — поднимаем
            // probe автоматически. Иначе sidecar спит до toggle'а юзером.
            #[cfg(target_os = "macos")]
            {
                let app_for_probe = handle.clone();
                tauri::async_runtime::spawn(async move {
                    let state = tauri::Manager::state::<state::AppState>(&app_for_probe);
                    let enabled = match db::get_setting(&state.db, "call_detect.enabled").await {
                        Ok(Some(v)) => v == "1",
                        _ => false,
                    };
                    if !enabled {
                        return;
                    }
                    let cooldown_min: u64 =
                        match db::get_setting(&state.db, "call_detect.cooldown_min").await {
                            Ok(Some(v)) => v.parse().unwrap_or(5),
                            _ => 5,
                        };
                    if let Err(e) = state
                        .call_detect
                        .enable(app_for_probe.clone(), cooldown_min)
                        .await
                    {
                        log::warn!("call-detect bootstrap failed: {e}");
                    }
                });
            }

            // [B16 audit P2] macOS app menu — без явного menu Tauri даёт только
            // basic App/Quit. Native Cut/Copy/Paste/SelectAll на webview без menu
            // не работают (стандартные ⌘C/⌘V). Add File/Edit/View/Window submenus.
            #[cfg(target_os = "macos")]
            {
                use tauri::menu::{MenuBuilder, MenuItemBuilder, SubmenuBuilder};

                // [S9] Custom Quit item with ⌘Q accelerator. Tauri's
                // PredefinedMenuItem::quit() calls app.exit() напрямую и
                // обходит наш CloseRequested → graceful-stop. Здесь мы вместо
                // exit ставим quitting=true и просим окно закрыться.
                let app_quit = MenuItemBuilder::with_id("app:quit", "Выход Wotold")
                    .accelerator("CmdOrCtrl+Q")
                    .build(app)?;

                let app_menu = SubmenuBuilder::new(&handle, "Wotold")
                    .about(None)
                    .separator()
                    .hide()
                    .hide_others()
                    .show_all()
                    .separator()
                    .item(&app_quit)
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

                // [S9] Catch the custom "app:quit" item. Tray menu has its own
                // handler (on_menu_event на TrayIconBuilder), но app-menu items
                // эмитят через app-level menu event.
                let quitting_for_app_menu = quitting.clone();
                app.on_menu_event(move |app, event| {
                    if event.id().as_ref() == "app:quit" {
                        quitting_for_app_menu.store(true, std::sync::atomic::Ordering::Relaxed);
                        if let Some(main) = tauri::Manager::get_webview_window(app, "main") {
                            let _ = main.show();
                            let _ = main.close();
                        } else {
                            app.exit(0);
                        }
                    }
                });
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

            // [S9] System tray icon + меню. Click по иконке (left-click на macOS)
            // показывает + поднимает main; "Выход" из меню ставит quitting=true
            // и просит окно закрыться, что прогоняет existing graceful-stop путь
            // через CloseRequested. "Открыть Wotold" вызывает то же что и tray
            // click. Tray live даже когда main скрыто — приложение остаётся
            // в фоне, recording продолжается.
            {
                use tauri::menu::{MenuBuilder, MenuItemBuilder};
                use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

                let open_item =
                    MenuItemBuilder::with_id("tray:open", "Открыть Wotold").build(app)?;
                let quit_item = MenuItemBuilder::with_id("tray:quit", "Выход").build(app)?;
                let tray_menu = MenuBuilder::new(app)
                    .items(&[&open_item, &quit_item])
                    .build()?;

                let quitting_for_menu = quitting.clone();
                let _tray = TrayIconBuilder::with_id("wotold-tray")
                    .icon(
                        app.default_window_icon()
                            .cloned()
                            .ok_or_else(|| std::io::Error::other("default_window_icon missing"))?,
                    )
                    .icon_as_template(true)
                    .menu(&tray_menu)
                    .show_menu_on_left_click(false)
                    .on_menu_event(move |app, event| match event.id.as_ref() {
                        "tray:open" => bring_main_to_front(app),
                        "tray:quit" => {
                            quitting_for_menu.store(true, std::sync::atomic::Ordering::Relaxed);
                            // Trigger close — CloseRequested handler с
                            // quitting=true прогонит graceful-stop путь.
                            if let Some(main) = tauri::Manager::get_webview_window(app, "main") {
                                if let Err(e) = main.show() {
                                    log::warn!("tray quit: main.show failed: {e}");
                                }
                                if let Err(e) = main.close() {
                                    log::warn!("tray quit: main.close failed: {e}");
                                }
                            } else {
                                // Окно уже сожжено → просто exit.
                                app.exit(0);
                            }
                        }
                        _ => {}
                    })
                    .on_tray_icon_event(|tray, event| {
                        if let TrayIconEvent::Click {
                            button: MouseButton::Left,
                            button_state: MouseButtonState::Up,
                            ..
                        } = event
                        {
                            bring_main_to_front(tray.app_handle());
                        }
                    })
                    .build(app)?;
            }

            // [B2]: graceful stop при window close. Если идёт запись —
            // префлайт-stop через pipeline, потом exit. Иначе sidecar получает
            // SIGHUP, последние ≤5s могут не успеть flush, calls row висит recording.
            if let Some(window) = tauri::Manager::get_webview_window(app, "main") {
                let app_for_event = handle.clone();
                let quitting_for_close = quitting.clone();
                // [W4 + S8] Edge-trigger widget show/hide. Originally we listened
                // only to `Resized` + is_minimized() to catch dock minimisation —
                // but on macOS that misses every other "user не смотрит на нас":
                // Cmd+Tab to other app, swipe to another Space, click on Finder,
                // hide-others. Widget should appear in all those cases.
                //
                // `WindowEvent::Focused(bool)` fires for all four scenarios above
                // (lose focus → show widget; gain focus → hide). Edge-filter via
                // `prev_focused` because Tauri also emits Focused(true) on init
                // and on each window event burst — we'd thrash the widget without
                // it.
                let prev_focused = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
                let prev_for_event = prev_focused.clone();
                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::Focused(focused) = event {
                        let was_focused =
                            prev_for_event.swap(*focused, std::sync::atomic::Ordering::Relaxed);
                        if was_focused == *focused {
                            return; // no edge
                        }
                        let name = if *focused {
                            "main-window:restored"
                        } else {
                            "main-window:minimized"
                        };
                        if let Err(e) = tauri::Emitter::emit(&app_for_event, name, ()) {
                            log::warn!("emit {name} failed: {e}");
                        }
                    }

                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        let is_quitting =
                            quitting_for_close.load(std::sync::atomic::Ordering::Relaxed);

                        // [S9] Если юзер просто нажал красный X — сворачиваем
                        // в трей, recording продолжается. Real exit идёт только
                        // через tray-menu "Выход" / ⌘Q, который ставит флаг
                        // перед закрытием.
                        if !is_quitting {
                            api.prevent_close();
                            if let Some(main) =
                                tauri::Manager::get_webview_window(&app_for_event, "main")
                            {
                                if let Err(e) = main.hide() {
                                    log::warn!("hide main on close: {e}");
                                }
                            }
                            return;
                        }

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

            // [S8] WKWebView на macOS рисует opaque белый фон даже при
            // `transparent: true` на NSWindow. Без явного set webview
            // background to RGBA(0,0,0,0) пилл-окно выглядит как
            // прозрачный pill ВНУТРИ непрозрачного 320×84 прямоугольника.
            if let Some(widget) = tauri::Manager::get_webview_window(app, "recording-widget") {
                if let Err(e) = widget.set_background_color(Some(tauri::webview::Color(0, 0, 0, 0)))
                {
                    log::warn!("widget set_background_color transparent failed: {e}");
                }
                // [Bug #4] Apply canJoinAllSpaces eagerly at window creation —
                // visibleOnAllWorkspaces в tauri.conf иногда не применяется к
                // окнам со стартовым visible:false. Без этого виджет «прибит»
                // к Space на котором был show'ен и не следует за Ctrl+Arrow.
                if let Err(e) = widget.set_visible_on_all_workspaces(true) {
                    log::warn!("widget set_visible_on_all_workspaces (setup) failed: {e}");
                }
                // [Widget v4] NSWindow.movableByWindowBackground = YES —
                // native macOS drag without IPC. Работает на тачпаде где
                // IPC-based startDragging промахивается мимо currentEvent.
                #[cfg(target_os = "macos")]
                make_widget_draggable_by_background(&widget);
            }

            // [S7] Persist floating-widget position when user drags it. We
            // debounce via a tokio task that resets a 400ms timer on each
            // `Moved` event — Tauri fires `Moved` ~per-frame during drag, and
            // we don't want to thrash SQLite. The timer captures the last
            // position seen and commits it once drag settles.
            if let Some(widget) = tauri::Manager::get_webview_window(app, "recording-widget") {
                use std::sync::atomic::{AtomicBool, Ordering};
                use std::sync::Mutex as StdMutex;
                use std::time::{Duration, Instant};

                let pending = std::sync::Arc::new(StdMutex::new(None::<(f64, f64, Instant)>));
                // Флаг чтобы snap-анимация (которая дёргает set_position и
                // фаирит Moved events) не триггерила сама себя рекурсивно.
                let is_animating = std::sync::Arc::new(AtomicBool::new(false));

                let pending_for_event = pending.clone();
                let is_animating_for_event = is_animating.clone();
                let widget_for_event = widget.clone();
                let app_for_persist = handle.clone();

                widget.on_window_event(move |event| {
                    if !matches!(event, tauri::WindowEvent::Moved(_)) {
                        return;
                    }
                    // Snap animation в процессе — Moved events это наши собственные
                    // set_position вызовы. Игнорируем, иначе recursive snap.
                    if is_animating_for_event.load(Ordering::Relaxed) {
                        return;
                    }
                    // Read scale + physical position fresh — Tauri gives us
                    // physical pixels; we persist logical so future shows
                    // на разных DPI скрытах не съезжают.
                    let scale = widget_for_event.scale_factor().unwrap_or(1.0);
                    let Ok(pos) = widget_for_event.outer_position() else {
                        return;
                    };
                    let logical_x = pos.x as f64 / scale;
                    let logical_y = pos.y as f64 / scale;

                    let was_idle;
                    {
                        let Ok(mut guard) = pending_for_event.lock() else {
                            return;
                        };
                        was_idle = guard.is_none();
                        *guard = Some((logical_x, logical_y, Instant::now()));
                    }
                    if !was_idle {
                        return;
                    }

                    // First Moved after settle — spawn debounce task that
                    // polls until 400ms pass without another Moved.
                    let pending_for_task = pending_for_event.clone();
                    let is_animating_for_task = is_animating_for_event.clone();
                    let widget_for_task = widget_for_event.clone();
                    let app_for_task = app_for_persist.clone();
                    tauri::async_runtime::spawn(async move {
                        loop {
                            tokio::time::sleep(Duration::from_millis(400)).await;
                            let snapshot = pending_for_task.lock().ok().and_then(|g| *g);
                            let Some((x, y, last_seen)) = snapshot else {
                                return;
                            };
                            if last_seen.elapsed() < Duration::from_millis(380) {
                                continue;
                            }
                            if let Ok(mut g) = pending_for_task.lock() {
                                *g = None;
                            }
                            // [Widget v3] Drag settled → snap к ближайшей
                            // вертикальной стороне с анимацией. is_animating
                            // выставляется ДО snap чтобы Moved events от наших
                            // set_position не триггерили recursive debounce.
                            is_animating_for_task.store(true, Ordering::Relaxed);
                            let state = tauri::Manager::state::<state::AppState>(&app_for_task);
                            commands::widget::snap_to_nearest_side(
                                app_for_task.clone(),
                                widget_for_task.clone(),
                                state.db.clone(),
                                x,
                                y,
                            )
                            .await;
                            is_animating_for_task.store(false, Ordering::Relaxed);
                            return;
                        }
                    });
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
            commands::regenerate_title,
            commands::reprocess_call,
            commands::cancel_reprocess,
            commands::get_active_pipeline_count,
            commands::list_call_chunks,
            commands::retry_chunk,
            commands::list_call_decisions,
            commands::list_call_open_questions,
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
            commands::local_engine_get_active_engine,
            #[cfg(target_os = "macos")]
            commands::local_engine_set_active_engine,
            #[cfg(target_os = "macos")]
            commands::local_engine_storage_list,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// [Widget v4] Включить native NSWindow drag из любой точки фона.
///
/// `data-tauri-drag-region` / `-webkit-app-region: drag` оба идут через IPC
/// или webview rendering layer, что на тачпаде ломается: палец отпускается
/// быстрее чем IPC долетает, `[NSApp currentEvent]` уже не mousedown а
/// mouseUp, `performWindowDragWithEvent:` no-op'ит.
///
/// Прямой `NSWindow.setMovableByWindowBackground:YES` — нативный путь macOS.
/// AppKit ловит mousedown в NSWindow level до того как event попадёт в
/// WKWebView. Drag начинается синхронно, без IPC.
///
/// Кнопки в widget'е переопределяют CSS `-webkit-app-region: no-drag`, что
/// блокирует window drag на этих субвью — клики работают.
///
/// # Safety
///
/// `ns_window` pointer гарантированно валиден от Tauri пока окно
/// существует. `setMovableByWindowBackground:` — простой setter без
/// эксцепшнов / side effects, поэтому unsafe block тривиален.
#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
fn make_widget_draggable_by_background(widget: &tauri::WebviewWindow) {
    use objc::msg_send;
    use objc::runtime::{Object, YES};
    use objc::sel;
    use objc::sel_impl;

    let ns_window = match widget.ns_window() {
        Ok(ptr) => ptr,
        Err(e) => {
            log::warn!("widget ns_window unavailable: {e}");
            return;
        }
    };
    if ns_window.is_null() {
        log::warn!("widget ns_window is null");
        return;
    }
    // SAFETY: NSWindow* lives as long as the widget window; setter is no-throw.
    unsafe {
        let ns_window = ns_window as *mut Object;
        let _: () = msg_send![ns_window, setMovableByWindowBackground: YES];
    }
}
