//! [T2/T3/R14] Обвязка наблюдателя тишины: настройки, спавн задачи, авто-стоп.
//!
//! Решающее ядро и петля живут в [`crate::audio::silence_watch`] — здесь только
//! то, что знает про БД, `AppState` и Tauri: прочитать настройки, поднять
//! задачу на старте записи, отдать события фронту и вызвать стоп.
//!
//! Почему отдельным модулем, а не внутри `commands::recording`: тот уже 650
//! строк, а гейт `pre-write.mjs` считает итоговый размер файла (правило 8).

use tauri::{AppHandle, Manager, State};
use tokio::sync::{mpsc, oneshot};

use crate::{
    audio::silence_watch::{
        self, SilenceControl, SilenceEvent, SilenceEventFut, SilenceWatchConfig,
        SilenceWatchHandles, SUGGEST_AFTER_MS,
    },
    events::{EventBus, RecordingAutoStoppedEvent, RecordingSilencePromptEvent},
    state::AppState,
    AppError,
};

/// [T3] Подсказка «в записи тишина, остановить?» — вкл/выкл. `"0"`/`"false"`
/// выключают, отсутствие ключа = ON (новый юзер получает подсказку).
pub const SETTING_SILENCE_PROMPT: &str = "recording.silence_prompt";
/// [T3] Порог авто-стопа в минутах: `30` | `60` | `120` | `never`.
/// Отсутствие ключа = дефолт 30 (R14).
pub const SETTING_SILENCE_AUTO_STOP: &str = "recording.silence_auto_stop";

/// Дефолт авто-стопа в минутах — держать в синхроне с `SETTINGS_DEFAULTS`
/// во фронтенде (`api/settings.ts`).
const DEFAULT_AUTO_STOP_MIN: u64 = 30;
/// Допустимые значения порога. Всё остальное из БД — мусор от ручной правки,
/// откатываемся на дефолт вместо доверия входу (правило 7).
const ALLOWED_AUTO_STOP_MIN: [u64; 3] = [30, 60, 120];

/// Буфер RMS-канала наблюдателя. Сэмплы идут 10 Hz, задача разбирает их
/// мгновенно; 64 — запас на планировочные всплески. Переполнение не
/// катастрофа: `try_send` в диспатчере дропнет сэмпл, часы наблюдателя ведутся
/// по таймстемпам, а не по количеству сэмплов.
const RMS_BUFFER: usize = 64;
/// Буфер управляющего канала — burst pause/resume/snooze от рук на UI.
const CONTROL_BUFFER: usize = 8;

/// Dev-ускоритель порогов: делитель для обоих таймаутов. `WOTOLD_SILENCE_SCALE=60`
/// даёт подсказку через 15 секунд и стоп через 30 — иначе ручная проверка
/// требует получаса тишины. Логируется на WARN, чтобы случайно оставленная
/// переменная не выглядела багом продукта.
const ENV_SILENCE_SCALE: &str = "WOTOLD_SILENCE_SCALE";
/// Верхняя граница делителя. 3600 переводит минуты в секунды — дальше порог
/// схлопнется в ноль и авто-стоп сработает на первом же тихом сэмпле.
const MAX_SILENCE_SCALE: u64 = 3_600;

/// [T3] Собрать конфиг наблюдателя из настроек. Значения вне белого списка
/// (ручная правка БД, старый формат) откатываются на дефолт, а не отключают
/// защиту молча.
pub(crate) async fn load_silence_config(
    db: &sqlx::SqlitePool,
) -> Result<SilenceWatchConfig, AppError> {
    let prompt_off = matches!(
        crate::db::get_setting(db, SETTING_SILENCE_PROMPT)
            .await?
            .as_deref(),
        Some("0") | Some("false")
    );
    let auto_stop_raw = crate::db::get_setting(db, SETTING_SILENCE_AUTO_STOP).await?;
    let auto_stop_min = parse_auto_stop_min(auto_stop_raw.as_deref());

    let scale = read_silence_scale();
    Ok(SilenceWatchConfig {
        suggest_after_ms: if prompt_off {
            None
        } else {
            Some(SUGGEST_AFTER_MS / scale)
        },
        auto_stop_after_ms: auto_stop_min.map(|m| (m * 60 * 1_000) / scale),
        ..SilenceWatchConfig::default()
    })
}

/// `None` — `never` (полный opt-out, R14). Мусор и пустая строка → дефолт.
fn parse_auto_stop_min(raw: Option<&str>) -> Option<u64> {
    match raw {
        None => Some(DEFAULT_AUTO_STOP_MIN),
        Some("never") => None,
        Some(v) => match v.parse::<u64>() {
            Ok(m) if ALLOWED_AUTO_STOP_MIN.contains(&m) => Some(m),
            _ => {
                log::warn!(
                    "{SETTING_SILENCE_AUTO_STOP}={v:?} вне допустимых значений — беру дефолт {DEFAULT_AUTO_STOP_MIN} мин"
                );
                Some(DEFAULT_AUTO_STOP_MIN)
            }
        },
    }
}

fn read_silence_scale() -> u64 {
    // [T3 review] Только debug-сборки. В релизе переменная, забытая в профиле
    // оболочки или в LaunchAgent'е, ускоряла бы авто-стоп в тысячи раз — и
    // пользователь получал бы записи, останавливающиеся через секунду тишины,
    // без единого способа это понять. Отладочному ускорителю нечего делать в
    // проде вообще.
    if !cfg!(debug_assertions) {
        return 1;
    }
    let Ok(raw) = std::env::var(ENV_SILENCE_SCALE) else {
        return 1;
    };
    match raw.parse::<u64>() {
        Ok(n) if (1..=MAX_SILENCE_SCALE).contains(&n) => {
            if n > 1 {
                log::warn!(
                    "{ENV_SILENCE_SCALE}={n}: пороги тишины ускорены в {n}× — это dev-режим, не прод"
                );
            }
            n
        }
        _ => {
            log::warn!("{ENV_SILENCE_SCALE}={raw:?} игнорируется (нужно 1..={MAX_SILENCE_SCALE})");
            1
        }
    }
}

/// [T2] Поднять наблюдателя для новой записи и вернуть RMS-sender для
/// `DispatcherFanout`. `Ok(None)` — следить не за чем (обе настройки выключены),
/// диспатчер тогда даже не считает elapsed.
///
/// Ручки кладутся в `AppState` до `audio::macos::start`, потому что sender
/// нужен самому старту. Если старт провалится, `stop_silence_watch` погасит
/// задачу — иначе она осталась бы висеть с живым control-каналом.
pub(crate) async fn spawn_silence_watch(
    app: &AppHandle,
    state: &AppState,
) -> Result<Option<mpsc::Sender<(u64, f32)>>, AppError> {
    let cfg = load_silence_config(&state.db).await?;
    if !crate::audio::silence_watch::SilenceWatch::is_armed(&cfg) {
        log::debug!("silence_watch: обе настройки выключены, наблюдатель не поднимается");
        return Ok(None);
    }

    let (rms_tx, rms_rx) = mpsc::channel::<(u64, f32)>(RMS_BUFFER);
    let (control_tx, control_rx) = mpsc::channel::<SilenceControl>(CONTROL_BUFFER);
    let (stop_tx, stop_rx) = oneshot::channel::<()>();

    let app_for_events = app.clone();
    let on_event = move |event: SilenceEvent| {
        let app = app_for_events.clone();
        Box::pin(async move {
            handle_event(app, event).await;
        }) as SilenceEventFut
    };

    *state.silence_watch.lock().await = Some(SilenceWatchHandles::new(control_tx, stop_tx));
    tokio::spawn(async move {
        let summary = silence_watch::run(cfg, rms_rx, control_rx, stop_rx, on_event).await;
        log::debug!("silence_watch finished: {summary:?}");
    });

    Ok(Some(rms_tx))
}

/// Погасить наблюдателя и забыть ручки. Идемпотентно — вызывается и из стопа,
/// и из откатов старта.
pub(crate) async fn stop_silence_watch(state: &AppState) {
    if let Some(mut handles) = state.silence_watch.lock().await.take() {
        handles.stop();
    }
}

/// Отправить управляющий сигнал активному наблюдателю. Fire-and-forget: нет
/// записи — нет наблюдателя, и это не ошибка.
pub(crate) async fn signal_silence_watch(state: &AppState, control: SilenceControl) {
    if let Some(handles) = state.silence_watch.lock().await.as_ref() {
        handles.signal(control);
    }
}

/// [T7] «Продолжить» из подсказки — сбрасывает счётчик тишины, авто-стоп
/// откладывается на полный интервал заново.
#[tauri::command]
pub async fn snooze_silence_watch(state: State<'_, AppState>) -> Result<(), AppError> {
    signal_silence_watch(&state, SilenceControl::Snooze).await;
    Ok(())
}

/// Побочные эффекты решения наблюдателя. Вынесено из замыкания, чтобы читалось
/// и тестировалось отдельно от плумбинга каналов.
async fn handle_event(app: AppHandle, event: SilenceEvent) {
    match event {
        SilenceEvent::None => {}
        SilenceEvent::SuggestStop {
            silent_for_ms,
            auto_stop_in_ms,
        } => {
            let Some((call_id, _)) = active_session(&app).await else {
                // Запись уже остановлена руками, пока сэмпл летел по каналу.
                return;
            };
            log::info!("silence_watch: {silent_for_ms}ms тишины в {call_id} — предлагаем стоп");
            EventBus::new(Some(&app)).recording_silence_prompt(&RecordingSilencePromptEvent {
                call_id,
                silent_for_ms,
                auto_stop_in_ms,
            });
        }
        SilenceEvent::AutoStop {
            trim_at_ms,
            silent_for_ms,
        } => {
            // Стоп не await'им внутри наблюдателя: он держит `on_event` до
            // конца, а стоп ждёт терминального события сайдкара (до 10с).
            // Отдельная задача даёт наблюдателю выйти сразу.
            tokio::spawn(async move {
                auto_stop(app, trim_at_ms, silent_for_ms).await;
            });
        }
    }
}

/// Снимок активной сессии: id звонка и wall-clock старта. Нужен ДО стопа —
/// после него сессии уже нет, а событию нужен и id, и точка отсчёта, чтобы
/// посчитать длину отрезанного хвоста.
async fn active_session(app: &AppHandle) -> Option<(String, chrono::DateTime<chrono::Utc>)> {
    let state = app.try_state::<AppState>()?;
    let guard = state.recording.lock().await;
    guard.as_ref().map(|s| (s.call_id.clone(), s.started_at))
}

/// [T5] Остановить запись по тишине и рассказать об этом фронту.
async fn auto_stop(app: AppHandle, trim_at_ms: u64, silent_for_ms: u64) {
    let Some(state) = app.try_state::<AppState>() else {
        log::warn!("silence auto-stop: AppState недоступен");
        return;
    };
    let Some((call_id, started_at)) = active_session(&app).await else {
        log::debug!("silence auto-stop: запись уже остановлена, нечего делать");
        return;
    };

    // [T3 review] Настройка перечитывается в момент решения, а не берётся из
    // снимка на старте записи. Иначе `never`, выставленный уже во время
    // звонка — ровно то, что делает человек, увидев подсказку о тишине, —
    // не спасал бы текущую запись: она всё равно остановилась бы по порогу,
    // прочитанному полчаса назад. R14 обещает полный opt-out, и обещание
    // должно работать в тот момент, когда его дали.
    match load_silence_config(&state.db).await {
        Ok(cfg) if cfg.auto_stop_after_ms.is_none() => {
            log::info!(
                "silence auto-stop: отменён для {call_id} — настройка сменилась на «никогда»"
            );
            return;
        }
        Ok(_) => {}
        // Прочитать не удалось — действуем по решению наблюдателя. Тишина
        // реальна, а отменять стоп из-за сбоя чтения настройки значит вернуть
        // ровно ту проблему, ради которой всё это делалось.
        Err(e) => log::warn!("silence auto-stop: перечитать настройку не вышло: {e}"),
    }
    // Хвост считается от полного wall-clock, а не от `call.duration_sec`:
    // тот уже подрезан точкой реза, и разность вышла бы нулевой.
    let elapsed_ms = (chrono::Utc::now() - started_at).num_milliseconds().max(0) as u64;
    let trimmed_ms = elapsed_ms.saturating_sub(trim_at_ms);

    match crate::commands::recording::stop_recording_inner(&app, &state, Some(trim_at_ms)).await {
        Ok(Some(call)) => {
            log::info!(
                "silence auto-stop: {} остановлен после {silent_for_ms}ms тишины, \
                 длительность {}с, отрезано {trimmed_ms}ms",
                call.id,
                call.duration_sec.unwrap_or(0)
            );
            EventBus::new(Some(&app)).recording_auto_stopped(&RecordingAutoStoppedEvent {
                call_id: call.id,
                silent_for_ms,
                trimmed_ms,
            });
        }
        // Запись короче минимальной — строка удалена, звонка нет. Событие
        // об авто-стопе тут врало бы: показывать в UI нечего.
        Ok(None) => {
            log::info!("silence auto-stop: {call_id} отброшен как слишком короткий");
        }
        Err(e) => log::warn!("silence auto-stop: стоп {call_id} не удался: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_allowed_thresholds() {
        assert_eq!(parse_auto_stop_min(Some("30")), Some(30));
        assert_eq!(parse_auto_stop_min(Some("60")), Some(60));
        assert_eq!(parse_auto_stop_min(Some("120")), Some(120));
    }

    #[test]
    fn never_disables_auto_stop() {
        assert_eq!(parse_auto_stop_min(Some("never")), None);
    }

    #[test]
    fn missing_setting_falls_back_to_default() {
        assert_eq!(parse_auto_stop_min(None), Some(DEFAULT_AUTO_STOP_MIN));
    }

    #[test]
    fn garbage_falls_back_to_default_not_to_never() {
        // Ключевая разница: мусор НЕ должен молча отключать защиту.
        for raw in ["", "0", "-5", "45", "9999999999999999999999", "тридцать"] {
            assert_eq!(
                parse_auto_stop_min(Some(raw)),
                Some(DEFAULT_AUTO_STOP_MIN),
                "{raw:?}"
            );
        }
    }

    #[tokio::test]
    async fn config_defaults_when_settings_empty() {
        let db = crate::db::test_support::fresh_db().await;
        let cfg = load_silence_config(&db.pool).await.expect("config");
        assert_eq!(cfg.suggest_after_ms, Some(SUGGEST_AFTER_MS));
        assert_eq!(cfg.auto_stop_after_ms, Some(30 * 60 * 1_000));
    }

    #[tokio::test]
    async fn config_respects_prompt_off_and_never() {
        let db = crate::db::test_support::fresh_db().await;
        crate::db::set_setting(&db.pool, SETTING_SILENCE_PROMPT, "0")
            .await
            .expect("set prompt");
        crate::db::set_setting(&db.pool, SETTING_SILENCE_AUTO_STOP, "never")
            .await
            .expect("set auto stop");
        let cfg = load_silence_config(&db.pool).await.expect("config");
        assert_eq!(cfg.suggest_after_ms, None);
        assert_eq!(cfg.auto_stop_after_ms, None);
        assert!(
            !crate::audio::silence_watch::SilenceWatch::is_armed(&cfg),
            "обе настройки выключены — наблюдатель поднимать не нужно"
        );
    }

    #[tokio::test]
    async fn config_reads_custom_threshold() {
        let db = crate::db::test_support::fresh_db().await;
        crate::db::set_setting(&db.pool, SETTING_SILENCE_AUTO_STOP, "120")
            .await
            .expect("set auto stop");
        let cfg = load_silence_config(&db.pool).await.expect("config");
        assert_eq!(cfg.auto_stop_after_ms, Some(120 * 60 * 1_000));
    }
}
