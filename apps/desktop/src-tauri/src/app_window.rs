//! [TD-41] Окна, меню и трей: всё, что настраивает нативную оболочку приложения.
//!
//! Выделено из `lib.rs` (876 строк при лимите 800, правило 8). Резать `run()`
//! пришлось по естественному шву: builder-цепочка Tauri и список команд —
//! это конфигурация, а установка меню/трея/обработчиков окон — код, который
//! читают и правят отдельно от неё.
//!
//! Логика не менялась ни на строку: те же обработчики, тот же порядок вызовов
//! из `setup()`.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tauri::{App, AppHandle, WebviewWindow};

use crate::commands;

type SetupResult = Result<(), Box<dyn std::error::Error>>;

/// [S9] Show main window + unminimise + focus. Used by tray icon click and
/// "Открыть Wotold" menu item. Idempotent — silent if window absent.
pub(crate) fn bring_main_to_front(app: &AppHandle) {
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

/// [B16 audit P2] macOS app menu — без явного menu Tauri даёт только
/// basic App/Quit. Native Cut/Copy/Paste/SelectAll на webview без menu
/// не работают (стандартные ⌘C/⌘V). Add File/Edit/View/Window submenus.
#[cfg(target_os = "macos")]
pub(crate) fn install_app_menu(
    app: &App,
    handle: &AppHandle,
    quitting: &Arc<AtomicBool>,
) -> SetupResult {
    use tauri::menu::{MenuBuilder, MenuItemBuilder, SubmenuBuilder};

    // [S9] Custom Quit item with ⌘Q accelerator. Tauri's
    // PredefinedMenuItem::quit() calls app.exit() напрямую и
    // обходит наш CloseRequested → graceful-stop. Здесь мы вместо
    // exit ставим quitting=true и просим окно закрыться.
    let app_quit = MenuItemBuilder::with_id("app:quit", "Выход Wotold")
        .accelerator("CmdOrCtrl+Q")
        .build(app)?;

    let app_menu = SubmenuBuilder::new(handle, "Wotold")
        .about(None)
        .separator()
        .hide()
        .hide_others()
        .show_all()
        .separator()
        .item(&app_quit)
        .build()?;

    let edit_menu = SubmenuBuilder::new(handle, "Edit")
        .undo()
        .redo()
        .separator()
        .cut()
        .copy()
        .paste()
        .select_all()
        .build()?;

    let view_menu = SubmenuBuilder::new(handle, "View").fullscreen().build()?;

    let window_menu = SubmenuBuilder::new(handle, "Window")
        .minimize()
        .maximize()
        .separator()
        .close_window()
        .build()?;

    let menu = MenuBuilder::new(handle)
        .items(&[&app_menu, &edit_menu, &view_menu, &window_menu])
        .build()?;
    app.set_menu(menu)?;

    // [S9] Catch the custom "app:quit" item. Tray menu has its own
    // handler (on_menu_event на TrayIconBuilder), но app-menu items
    // эмитят через app-level menu event.
    let quitting_for_app_menu = quitting.clone();
    app.on_menu_event(move |app, event| {
        if event.id().as_ref() == "app:quit" {
            quitting_for_app_menu.store(true, Ordering::Relaxed);
            if let Some(main) = tauri::Manager::get_webview_window(app, "main") {
                let _ = main.show();
                let _ = main.close();
            } else {
                app.exit(0);
            }
        }
    });
    Ok(())
}

/// [S9] System tray icon + меню. Click по иконке (left-click на macOS)
/// показывает + поднимает main; "Выход" из меню ставит quitting=true
/// и просит окно закрыться, что прогоняет existing graceful-stop путь
/// через CloseRequested. "Открыть Wotold" вызывает то же что и tray
/// click. Tray live даже когда main скрыто — приложение остаётся
/// в фоне, recording продолжается.
pub(crate) fn install_tray(app: &App, quitting: &Arc<AtomicBool>) -> SetupResult {
    use tauri::menu::{MenuBuilder, MenuItemBuilder};
    use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

    let open_item = MenuItemBuilder::with_id("tray:open", "Открыть Wotold").build(app)?;
    let quit_item = MenuItemBuilder::with_id("tray:quit", "Выход").build(app)?;
    let tray_menu = MenuBuilder::new(app)
        .items(&[&open_item, &quit_item])
        .build()?;

    let quitting_for_menu = quitting.clone();
    let _tray = TrayIconBuilder::with_id("wotold-tray")
        // Dedicated monochrome TEMPLATE mark (black + alpha) — macOS tints it
        // for the light/dark menu bar. Не переиспользуем цветную bundle-иконку
        // (её альфа в template-режиме дала бы силуэт-кляксу).
        .icon(tauri::image::Image::from_bytes(include_bytes!(
            "../icons/tray.png"
        ))?)
        .icon_as_template(true)
        .menu(&tray_menu)
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, event| match event.id.as_ref() {
            "tray:open" => bring_main_to_front(app),
            "tray:quit" => {
                quitting_for_menu.store(true, Ordering::Relaxed);
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
    Ok(())
}

/// [B2]: graceful stop при window close. Если идёт запись —
/// префлайт-stop через pipeline, потом exit. Иначе sidecar получает
/// SIGHUP, последние ≤5s могут не успеть flush, calls row висит recording.
pub(crate) fn install_main_window_events(
    app: &App,
    handle: &AppHandle,
    quitting: &Arc<AtomicBool>,
) {
    let Some(window) = tauri::Manager::get_webview_window(app, "main") else {
        return;
    };
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
    let prev_focused = Arc::new(AtomicBool::new(true));
    let prev_for_event = prev_focused.clone();
    window.on_window_event(move |event| {
        if let tauri::WindowEvent::Focused(focused) = event {
            let was_focused = prev_for_event.swap(*focused, Ordering::Relaxed);
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
            let is_quitting = quitting_for_close.load(Ordering::Relaxed);

            // [S9] Если юзер просто нажал красный X — сворачиваем
            // в трей, recording продолжается. Real exit идёт только
            // через tray-menu "Выход" / ⌘Q, который ставит флаг
            // перед закрытием.
            if !is_quitting {
                api.prevent_close();
                if let Some(main) = tauri::Manager::get_webview_window(&app_for_event, "main") {
                    if let Err(e) = main.hide() {
                        log::warn!("hide main on close: {e}");
                    }
                }
                return;
            }

            // [B2] Гасим resident llama-server при реальном выходе —
            // иначе дочерний процесс осиротеет и продолжит держать
            // RAM + порт после закрытия приложения.
            #[cfg(target_os = "macos")]
            {
                let app_srv = app_for_event.clone();
                tauri::async_runtime::block_on(async move {
                    crate::pipeline::stop_resident_server(&app_srv).await;
                });
            }

            let state = tauri::Manager::state::<crate::state::AppState>(&app_for_event);
            let has_active =
                tauri::async_runtime::block_on(async { state.recording.lock().await.is_some() });
            if !has_active {
                return;
            }

            // Останавливаем close, тушим запись в фоне, потом exit.
            api.prevent_close();
            let app_for_quit = app_for_event.clone();
            tauri::async_runtime::spawn(async move {
                graceful_shutdown(app_for_quit).await;
            });
        }
    });
}

/// Хвост выхода при активной записи: дотушить сессию и дождаться фоновых
/// задач пайплайна. Вынесено из обработчика окна, чтобы вложенность
/// closure'ов не уезжала за читаемость.
async fn graceful_shutdown(app: AppHandle) {
    let state = tauri::Manager::state::<crate::state::AppState>(&app);
    let session = state.recording.lock().await.take();
    if let Some(session) = session {
        let call_id = session.call_id.clone();
        if let Err(e) = crate::audio::macos::stop(session).await {
            log::error!("graceful stop {call_id} failed: {e}; marking failed");
            let _ = crate::db::fail_recording_with_reason(
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
        log::info!("graceful shutdown: ждём {} pipeline task(s)", pending.len());
        for (cid, h) in pending {
            let waited = tokio::time::timeout(std::time::Duration::from_secs(8), h).await;
            match waited {
                Ok(Ok(())) => log::info!("pipeline {cid} done"),
                Ok(Err(e)) => log::warn!("pipeline {cid} join error: {e}"),
                Err(_) => log::warn!("pipeline {cid} timeout — abort"),
            }
        }
    }
    // Выход — pipeline не запускаем (юзер закрыл окно осознанно).
    app.exit(0);
}

/// [S8/S7/Widget v3-v4] Плавающий виджет: прозрачный фон, присутствие на всех
/// Spaces, нативный drag и запоминание позиции.
pub(crate) fn install_widget_window(app: &App, handle: &AppHandle) {
    let Some(widget) = tauri::Manager::get_webview_window(app, "recording-widget") else {
        return;
    };
    // [S8] WKWebView на macOS рисует opaque белый фон даже при
    // `transparent: true` на NSWindow. Без явного set webview
    // background to RGBA(0,0,0,0) пилл-окно выглядит как
    // прозрачный pill ВНУТРИ непрозрачного 320×84 прямоугольника.
    if let Err(e) = widget.set_background_color(Some(tauri::webview::Color(0, 0, 0, 0))) {
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

    install_widget_position_persistence(widget, handle);
}

/// [S7] Persist floating-widget position when user drags it. We
/// debounce via a tokio task that resets a 400ms timer on each
/// `Moved` event — Tauri fires `Moved` ~per-frame during drag, and
/// we don't want to thrash SQLite. The timer captures the last
/// position seen and commits it once drag settles.
fn install_widget_position_persistence(widget: WebviewWindow, handle: &AppHandle) {
    use std::sync::Mutex as StdMutex;
    use std::time::{Duration, Instant};

    let pending = Arc::new(StdMutex::new(None::<(f64, f64, Instant)>));
    // Флаг чтобы snap-анимация (которая дёргает set_position и
    // фаирит Moved events) не триггерила сама себя рекурсивно.
    let is_animating = Arc::new(AtomicBool::new(false));

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
                let state = tauri::Manager::state::<crate::state::AppState>(&app_for_task);
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

/// [B30.1] Установить Dock-иконку приложения из padded-1024 PNG (канонный
/// macOS-паддинг ~10%). setup() бежит на main thread — AppKit-инвариант
/// соблюдён; NSImage decode PNG сам. Ошибки — warn, не фатальны.
#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
pub(crate) fn set_dock_icon() {
    use objc2::AnyThread;
    use objc2_app_kit::{NSApplication, NSImage};
    use objc2_foundation::{MainThreadMarker, NSData};

    let Some(mtm) = MainThreadMarker::new() else {
        log::warn!("set_dock_icon: not on main thread");
        return;
    };
    let bytes: &[u8] = include_bytes!("../icons/source/app-icon-1024.png");
    let data = NSData::with_bytes(bytes);
    let Some(img) = NSImage::initWithData(NSImage::alloc(), &data) else {
        log::warn!("set_dock_icon: NSImage decode failed");
        return;
    };
    // SAFETY: main thread гарантирован mtm; img — валидный NSImage (decode
    // проверен выше); setApplicationIconImage — no-throw AppKit-setter.
    unsafe {
        NSApplication::sharedApplication(mtm).setApplicationIconImage(Some(&img));
    }
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
fn make_widget_draggable_by_background(widget: &WebviewWindow) {
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
    // SAFETY: (1) NSWindow* lives as long as the widget window (`widget` keeps it
    // alive for this call). (2) setMovableByWindowBackground: is a no-throw AppKit
    // setter. (3) Must run on the main thread — call sites are Tauri 2 setup() (runs
    // on main) / window-setup, never a spawned thread; moving this onto a background
    // thread would be unsound (AppKit is main-thread-only).
    unsafe {
        let ns_window = ns_window as *mut Object;
        let _: () = msg_send![ns_window, setMovableByWindowBackground: YES];
    }
}

/// Прячет/показывает три нативных macOS-кнопки main-окна (close/miniaturize/zoom).
/// hidden=true — рисуем свой кастомный светофор (hover-reveal, WindowControls.tsx);
/// hidden=false — возвращаем нативные (нужно в fullscreen, где их показывает
/// нативная авто-плашка). NSWindowButton: Close=0, Miniaturize=1, Zoom=2.
///
/// # Safety
///
/// `ns_window` валиден от Tauri пока окно живёт; `standardWindowButton:` и
/// `setHidden:` — простые AppKit-аксессоры без эксцепшнов / side effects.
#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
pub(crate) fn set_main_window_buttons_hidden(main: &WebviewWindow, hidden: bool) {
    use objc::msg_send;
    use objc::runtime::{Object, BOOL, NO, YES};
    use objc::sel;
    use objc::sel_impl;

    let ns_window = match main.ns_window() {
        Ok(ptr) => ptr,
        Err(e) => {
            log::warn!("main ns_window unavailable: {e}");
            return;
        }
    };
    if ns_window.is_null() {
        log::warn!("main ns_window is null");
        return;
    }
    let val: BOOL = if hidden { YES } else { NO };
    // SAFETY: (1) NSWindow* lives as long as the main window (`main` keeps it alive
    // for this call). (2) standardWindowButton: / setHidden: are no-throw AppKit
    // accessors. (3) Must run on the main thread — call sites are Tauri 2 setup()
    // (runs on main) and the #[tauri::command] path (wry's WKScriptMessageHandler
    // is MainThreadOnly, so IPC fires on main); moving onto a spawned thread would
    // be unsound (AppKit is main-thread-only).
    unsafe {
        let ns_window = ns_window as *mut Object;
        // NSWindowButton — NSUInteger (= usize на всех целевых платформах).
        for kind in 0usize..=2 {
            let btn: *mut Object = msg_send![ns_window, standardWindowButton: kind];
            if !btn.is_null() {
                let _: () = msg_send![btn, setHidden: val];
            }
        }
    }
}

/// Команда из фронта: переключить видимость нативных светофоров main-окна.
/// JS зовёт на смене fullscreen — в fullscreen показываем нативные (там их
/// плашка), иначе скрываем (берут верх кастомные WindowControls).
#[tauri::command]
pub fn set_main_traffic_lights_hidden(app: AppHandle, hidden: bool) {
    #[cfg(target_os = "macos")]
    if let Some(main) = tauri::Manager::get_webview_window(&app, "main") {
        set_main_window_buttons_hidden(&main, hidden);
    } else {
        log::warn!("set_main_traffic_lights_hidden: main window not found");
    }
    #[cfg(not(target_os = "macos"))]
    let _ = (app, hidden);
}
