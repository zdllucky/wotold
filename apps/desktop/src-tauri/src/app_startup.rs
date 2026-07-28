//! [TD-41] Фоновые задачи старта и panic-хук.
//!
//! Выделено из `lib.rs` (876 строк при лимите 800, правило 8). Все задачи
//! здесь — fire-and-forget: ни одна не блокирует показ окна, каждая
//! деградирует в `log::warn!`. Порядок вызовов сохранён 1-в-1 с прежним
//! `setup()`: восстановление раньше прогрева, прогрев раньше индексации не
//! требуется — задачи независимы.

use std::time::Duration;

use tauri::{AppHandle, Emitter};

use crate::{commands, db, events, state, updater};

/// [B16 audit P1] panic hook: silent-kill процессу не оставляет следов.
/// Пишем backtrace в panic.log + дублируем в stderr. Поверх default hook —
/// вызываем prev_hook так чтобы dev-сборка получала console-friendly stderr.
pub(crate) fn install_panic_hook() {
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
}

/// Все фоновые задачи, которые нужно поднять сразу после `state::init`.
pub(crate) fn spawn_startup_tasks(handle: &AppHandle) {
    spawn_recovery(handle);
    #[cfg(target_os = "macos")]
    spawn_call_detect_bootstrap(handle);
    spawn_assistant_backfill(handle);
    #[cfg(target_os = "macos")]
    spawn_llm_warmup(handle);
    #[cfg(target_os = "macos")]
    spawn_model_integrity_check(handle);
    spawn_updater_poll(handle);
}

/// Первая проверка обновлений — не сразу: старт и так занят восстановлением,
/// бэкфиллом индекса и прогревом модели.
const UPDATE_FIRST_CHECK_DELAY: Duration = Duration::from_secs(30);
/// Дальше — раз в шесть часов. Приложение живёт днями, а не минутами;
/// чаще спрашивать GitHub незачем.
const UPDATE_CHECK_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);
/// Как часто перепроверять занятость, когда обязательное обновление ждёт
/// окончания записи.
const UPDATE_IDLE_RETRY: Duration = Duration::from_secs(30);

/// Периодическая проверка обновлений.
///
/// Раньше проверка жила в `useEffect` баннера: один раз за запуск и ошибка
/// молча в консоль. Приложение для записи звонков открыто сутками — узнавать
/// о новой версии только при следующем холодном старте недостаточно.
///
/// Обязательное обновление ставится само, но никогда не прерывает запись или
/// обработку: `install_when_idle` ждёт простоя сколько потребуется.
fn spawn_updater_poll(handle: &AppHandle) {
    let app = handle.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(UPDATE_FIRST_CHECK_DELAY).await;
        loop {
            match updater::check(&app).await {
                Ok(Some(update)) => {
                    let urgency = update.urgency;
                    let version = update.version.clone();
                    // Событие эмитим всегда: UI объясняет пользователю и
                    // предстоящий перезапуск, и ожидание простоя.
                    if let Err(e) = app.emit(events::UPDATER_AVAILABLE, update) {
                        log::warn!("updater: не смог отправить событие: {e}");
                    }
                    if urgency == updater::UpdateUrgency::Mandatory {
                        log::info!("updater: {version} обязательна, ставлю при первом простое");
                        let host = updater::AppUpdateHost { app: &app };
                        // При успехе не возвращается — процесс перезапускается.
                        if let Err(e) = updater::install_when_idle(&host, UPDATE_IDLE_RETRY).await {
                            log::warn!("updater: обязательное обновление не поставилось: {e}");
                        }
                    }
                }
                Ok(None) => log::debug!("updater: обновлений нет"),
                // Сеть недоступна — это норма для локального приложения.
                // Следующая попытка по расписанию, без агрессивных ретраев.
                Err(e) => log::info!("updater: проверка не удалась: {e}"),
            }
            tokio::time::sleep(UPDATE_CHECK_INTERVAL).await;
        }
    });
}

/// [M13 fix / ops] Headless recovery: если env WOTOLD_RECOVER_CALL_ID
/// задан — восстановить сломанную chunked-запись на старте (без GUI).
///
/// [B28.2] Следом — авто-восстановление прерванных звонков (краш/quit посреди
/// пайплайна → sweep пометил failed при целом аудио). Гейты и лимит
/// попыток внутри; ручной WOTOLD_RECOVER_CALL_ID главнее.
fn spawn_recovery(handle: &AppHandle) {
    let app_for_recover = handle.clone();
    tauri::async_runtime::spawn(async move {
        commands::maybe_headless_recover(app_for_recover).await;
    });

    let app_for_auto = handle.clone();
    tauri::async_runtime::spawn(async move {
        commands::auto_recover_interrupted_calls(app_for_auto).await;
    });
}

/// [S2] Если CALL_DETECT_ENABLED == "1" с прошлой сессии — поднимаем
/// probe автоматически. Иначе sidecar спит до toggle'а юзером.
#[cfg(target_os = "macos")]
fn spawn_call_detect_bootstrap(handle: &AppHandle) {
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
        let cooldown_min: u64 = match db::get_setting(&state.db, "call_detect.cooldown_min").await {
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

/// [M15.3] Backfill индекса ассистента: ready-звонки без записи в
/// assistant_index_state (миграция с до-M15 версий, headless-pipeline
/// без AppHandle). Фоном, последовательно, не блокирует окно.
fn spawn_assistant_backfill(handle: &AppHandle) {
    let app_for_backfill = handle.clone();
    tauri::async_runtime::spawn(async move {
        let (pool, store) = {
            let state = tauri::Manager::state::<state::AppState>(&app_for_backfill);
            (state.db.clone(), state.store.clone())
        };
        crate::assistant::indexer::backfill(&pool, &store).await;
        // [B25] Авто-скачивание эмбеддера (тумблер default on +
        // выбран локальный пресет) — прогресс в UI через
        // model:progress. Ошибка сети — warn, не фатал.
        if let Err(e) = crate::assistant::embedder::ensure_model_downloaded(
            &pool,
            store.app_data_dir(),
            Some(&app_for_backfill),
        )
        .await
        {
            log::warn!("assistant semantic auto-download: {e}");
        }
        // [M15.10] Следом — вектора: инвалидация по id модели +
        // добор пассажей без эмбеддинга. No-op без модели/feature.
        crate::assistant::indexer::embed_backfill(&pool, store.app_data_dir()).await;
    });
}

/// [warm-up B1] Прогрев local-LLM при старте: фоновый крошечный
/// generate компилит Metal-шейдеры + греет модель в page-cache, чтобы
/// первый рекап не ловил ~30с cold-start. No-op если движок не Local.
/// Non-fatal, фоном (не блокирует показ окна).
#[cfg(target_os = "macos")]
fn spawn_llm_warmup(handle: &AppHandle) {
    let app_for_warmup = handle.clone();
    tauri::async_runtime::spawn(async move {
        let (pool, app_data_dir) = {
            let state = tauri::Manager::state::<state::AppState>(&app_for_warmup);
            (state.db.clone(), state.app_data_dir.clone())
        };
        crate::pipeline::warm_up_local_llm(&pool, &app_data_dir, &app_for_warmup).await;
    });
}

/// [security-scan W5] Целостность моделей: полный SHA256 считается один раз на
/// версию файла и кэшируется по «размер+mtime». Быстрый путь на каждом прогоне
/// сравнивает только размер, то есть подмену файла того же размера он не
/// увидит; эта проверка её вскрывает — с задержкой до следующего старта, но
/// без 6 ГБ чтения перед каждым звонком.
#[cfg(target_os = "macos")]
fn spawn_model_integrity_check(handle: &AppHandle) {
    let app = handle.clone();
    tauri::async_runtime::spawn(async move {
        let (pool, app_data_dir) = {
            let state = tauri::Manager::state::<state::AppState>(&app);
            (state.db.clone(), state.app_data_dir.clone())
        };
        let failed =
            crate::local_engine::model_integrity::verify_present_models(&pool, &app_data_dir).await;
        if failed > 0 {
            log::error!("model_integrity: {failed} модел(ей) не прошли проверку SHA256");
        }
    });
}
