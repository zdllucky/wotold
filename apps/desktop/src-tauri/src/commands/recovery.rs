//! [M13 fix / B28.2] Восстановление сломанных и прерванных записей.
//!
//! Три входа в один и тот же клей `reconstruct → STT недостающих чанков →
//! finalize`: ручная команда из UI, headless-триггер `WOTOLD_RECOVER_CALL_ID`
//! и авто-восстановление на старте.
//!
//! [TD-33] Выделено из `commands::recording` (1677 строк, сверх лимита 800 —
//! правило 8): туда нельзя было добавить ни строки, а именно этот клей и
//! оставался без тестов. Решения, которые здесь можно сломать, вынесены
//! чистыми функциями: порядок и условия — в `pipeline::recovery_flow`,
//! отбор кандидатов авто-восстановления — в `plan_auto_recovery` ниже.

use std::sync::Arc;

use sqlx::SqlitePool;
use tauri::{AppHandle, State};

use crate::{
    call_id::CallId,
    call_store::CallStore,
    db,
    pipeline::{
        chunk_recovery,
        chunk_runner::{self, ChunkRunInput},
        recovery_flow,
    },
    services::pipeline_runner::PipelineRunner,
    state::AppState,
    AppError,
};

use super::chunked_setup::build_chunk_providers;

/// [M13 fix] Recovery сломанной chunked-записи (например записанной старым
/// кодом с chunk-0-path-mismatch + пропущенным финальным chunk'ом).
/// Реконструирует `call_chunks` из on-disk WAV'ов, STT'ит недостающие chunk'и,
/// затем reprocess (assembly + merge + recap). Возвращается сразу — работа
/// идёт в фоне (status=processing подтянется через list_calls).
#[tauri::command]
pub async fn recover_chunked_call(
    app: AppHandle,
    state: State<'_, AppState>,
    call_id: String,
) -> Result<(), AppError> {
    // [TD-05] call_id из webview — валидируем до любых путей.
    let parsed_id = CallId::parse(&call_id)?;
    spawn_recover_chunked(
        state.db.clone(),
        state.store.clone(),
        state.pipeline_tasks.clone(),
        state.app_data_dir.clone(),
        app,
        parsed_id,
    )
    .await
}

/// [M13 fix] Core recovery — shared by the Tauri command и headless
/// `WOTOLD_RECOVER_CALL_ID` startup trigger (см. lib.rs setup). Валидирует
/// call + engine, строит providers, spawn'ит фоновый task: reconstruct →
/// STT недостающих chunk'ов → reprocess (assembly + merge + recap).
pub(crate) async fn spawn_recover_chunked(
    pool: SqlitePool,
    store: Arc<CallStore>,
    tasks: crate::services::pipeline_runner::PipelineTasks,
    app_data_dir: std::path::PathBuf,
    app: AppHandle,
    call_id: CallId,
) -> Result<(), AppError> {
    // 1. Валидируем существование звонка.
    db::get_call(&pool, call_id.as_str())
        .await?
        .ok_or_else(|| AppError::NotFound(format!("call {call_id} not found")))?;

    // 2. Providers (fail fast если preset/модель не выбраны).
    let providers = build_chunk_providers(&pool, &app_data_dir, &app, &call_id).await?;

    // 3. Клоны для фонового task'а.
    let db_bg = pool;
    let app_bg = app;

    tokio::spawn(async move {
        // Finalize идёт через `spawn_initial` (НЕ `spawn_reprocess`):
        // reconstruct промоутит root→chunks/0, поэтому root mic.wav больше
        // нет — а `reprocess_call` требует root WAV и упал бы.
        // `spawn_initial` идёт через `run_local_inner`, который сам мержит
        // chunks→root, потом assembly (chunks уже done → STT skip) + recap.
        // Тот же путь, что и у нормальной записи после stop.
        let (db_fin, store_fin, app_fin, tasks_fin) =
            (db_bg.clone(), store.clone(), app_bg.clone(), tasks);
        let call_id_fin = call_id.as_str().to_string();
        let mic_path = store.mic_path(&call_id);
        let system_path = store.system_path(&call_id);

        // [TD-33] Порядок и условия живут в `recovery_flow` с инжектируемыми
        // шагами — только так их вообще можно покрыть тестом: здесь вокруг
        // AppHandle, пул и файловая система.
        let verdict = recovery_flow::run_recovery_flow(
            call_id.as_str(),
            chunk_recovery::reconstruct_chunk_rows(&db_bg, &store, &call_id),
            |rc| {
                let input = ChunkRunInput {
                    call_id: call_id.as_str().to_string(),
                    chunk_idx: rc.idx,
                    start_ms: rc.start_ms,
                    end_ms: rc.end_ms,
                    mic_path: store.chunk_mic_path(&call_id, rc.idx),
                    system_path: store.chunk_system_path(&call_id, rc.idx),
                    prev_prompt: None,
                    lang: providers.lang.clone(),
                    app_data_dir: Some(app_data_dir.clone()),
                    app_handle: Some(app_bg.clone()),
                    mic_diarization_num_speakers: providers.mic_diarization_num_speakers,
                };
                chunk_runner::run_chunk(
                    &db_bg,
                    providers.mic.as_ref(),
                    providers.system.as_ref(),
                    input,
                )
            },
            || async move {
                PipelineRunner::spawn_initial(
                    db_fin,
                    store_fin,
                    app_fin,
                    tasks_fin,
                    call_id_fin,
                    mic_path,
                    system_path,
                )
                .await;
            },
        )
        .await;
        // Итог — одной строкой: по логу видно, чем кончилось восстановление,
        // без склейки трёх разных warn'ов из разных мест.
        log::info!("recovery[{call_id}]: {verdict:?}");
    });

    Ok(())
}

/// [M13 fix / ops] Headless recovery trigger. Если env `WOTOLD_RECOVER_CALL_ID`
/// задан на старте — спавнит recovery для этого call_id без GUI. Dev/support-хук
/// для восстановления записей, сломанных старым chunk-0-path-mismatch кодом.
/// Prod окружение env не задаёт → no-op. Вызывается из `lib.rs::setup`.
pub(crate) async fn maybe_headless_recover(app: AppHandle) {
    let Some(call_id) = headless_recover_target() else {
        return;
    };
    log::warn!("WOTOLD_RECOVER_CALL_ID set → headless recovery for {call_id}");
    let state = tauri::Manager::state::<AppState>(&app);
    if let Err(e) = spawn_recover_chunked(
        state.db.clone(),
        state.store.clone(),
        state.pipeline_tasks.clone(),
        state.app_data_dir.clone(),
        app.clone(),
        CallId::from_db(&call_id),
    )
    .await
    {
        log::error!("headless recovery for {call_id} failed to start: {e}");
    }
}

/// call_id из `WOTOLD_RECOVER_CALL_ID`, если он вообще задан непустым.
/// Читается дважды (сам триггер + исключение из авто-восстановления), поэтому
/// живёт отдельной функцией: разъехавшийся trim означал бы двойной recovery
/// одного звонка.
fn headless_recover_target() -> Option<String> {
    parse_headless_target(std::env::var("WOTOLD_RECOVER_CALL_ID").ok())
}

/// Чистая часть: env отдельно, разбор отдельно — иначе тест пришлось бы
/// писать через `set_var`, а он гоняется параллельно с остальными.
fn parse_headless_target(raw: Option<String>) -> Option<String> {
    raw.map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

/// [B28.2] Максимум авто-восстановлений за один старт (не забивать
/// resource queue при массовом бэклоге).
const AUTO_RECOVER_MAX_PER_STARTUP: usize = 3;
/// [B28.2] Лимит попыток на звонок (маркер-файл в call-dir) — иначе
/// повторяющийся краш зациклил бы recovery навсегда.
const AUTO_RECOVER_MAX_TRIES: u32 = 2;

/// Что известно о кандидате на авто-восстановление. Собирается с диска и из
/// env; сам вердикт — чистая функция ниже.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CandidateFacts {
    /// Этот же звонок уже забрал ручной headless-триггер.
    pub is_headless_target: bool,
    /// `transcript.md` на диске — звонок уже обработан.
    pub has_transcript: bool,
    /// Есть root- или chunk-WAV.
    pub has_audio: bool,
    /// Сколько раз авто-восстановление уже пробовали (маркер-файл).
    pub tries: u32,
}

/// Вердикт по одному кандидату.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AutoRecoverPlan {
    /// Запустить восстановление.
    Recover,
    /// Лимит на старт исчерпан — остальных кандидатов не смотрим вообще.
    StopCapReached,
    /// Ручной триггер уже занимается этим звонком.
    SkipHeadlessOwned,
    /// Транскрипт есть: `failed` относится к хвосту (recap), не к аудио.
    SkipAlreadyTranscribed,
    /// Восстанавливать нечего.
    SkipNoAudio,
    /// Анти-луп: повторяющийся краш не должен крутить recovery вечно.
    SkipTriesExhausted,
}

/// [B28.2] Решение по кандидату авто-восстановления. Вынесено чистой функцией:
/// пять условий, каждое из которых при ошибке либо трогает чужой звонок, либо
/// зацикливает восстановление, — а вокруг них в проде `AppHandle` и диск.
pub(crate) fn plan_auto_recovery(started: usize, facts: &CandidateFacts) -> AutoRecoverPlan {
    if started >= AUTO_RECOVER_MAX_PER_STARTUP {
        return AutoRecoverPlan::StopCapReached;
    }
    if facts.is_headless_target {
        return AutoRecoverPlan::SkipHeadlessOwned;
    }
    if facts.has_transcript {
        return AutoRecoverPlan::SkipAlreadyTranscribed;
    }
    if !facts.has_audio {
        return AutoRecoverPlan::SkipNoAudio;
    }
    if facts.tries >= AUTO_RECOVER_MAX_TRIES {
        return AutoRecoverPlan::SkipTriesExhausted;
    }
    AutoRecoverPlan::Recover
}

/// [B28.2] Авто-восстановление прерванных звонков на старте.
///
/// Кейс 3df01365 (23.07): WKWebView crash посреди пайплайна → рестарт →
/// sweep пометил звонок failed НАВСЕГДА при целом аудио и даже готовом STT
/// chunk-0. Аудио пишется на диск всю запись — терять такой звонок нельзя.
///
/// Кандидат: `status='failed' AND failed_reason IS NULL` (помечен sweep'ом,
/// не настоящим фейлом пайплайна), на диске есть аудио, НЕТ transcript.md,
/// попыток < лимита. Восстановление — тот же путь, что ручная
/// `recover_chunked_call` (M13): reconstruct chunks, STT недостающих,
/// reprocess.
pub(crate) async fn auto_recover_interrupted_calls(app: AppHandle) {
    let state = tauri::Manager::state::<AppState>(&app);
    let headless_id = headless_recover_target();

    let candidates = match db::list_interrupted_failed_calls(&state.db).await {
        Ok(v) => v,
        Err(e) => {
            log::warn!("auto_recover: candidate query failed: {e}");
            return;
        }
    };
    let mut started = 0usize;
    // Факты собираются до вердикта, включая заведомо отсеиваемых кандидатов:
    // лишний readdir на звонок предпочтён второму месту, где продублирован
    // порядок условий. Список короткий по построению (`failed` без причины),
    // а все чтения инертны.
    for call_id in candidates {
        let recovered_id = CallId::from_db(call_id.as_str());
        let call_dir = state.store.call_dir(&recovered_id);
        let marker = call_dir.join(".auto-recover-tries");
        let tries: u32 = std::fs::read_to_string(&marker)
            .ok()
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(0);
        let facts = CandidateFacts {
            is_headless_target: headless_id.as_deref() == Some(call_id.as_str()),
            has_transcript: call_dir.join("transcript.md").exists(),
            has_audio: call_has_audio(&state.store, &recovered_id),
            tries,
        };

        match plan_auto_recovery(started, &facts) {
            AutoRecoverPlan::StopCapReached => {
                log::warn!(
                    "auto_recover: cap {AUTO_RECOVER_MAX_PER_STARTUP} reached, rest deferred"
                );
                break;
            }
            AutoRecoverPlan::SkipTriesExhausted => {
                log::warn!("auto_recover[{call_id}]: {tries} попыток исчерпано — оставляем failed");
                continue;
            }
            AutoRecoverPlan::SkipHeadlessOwned
            | AutoRecoverPlan::SkipAlreadyTranscribed
            | AutoRecoverPlan::SkipNoAudio => continue,
            AutoRecoverPlan::Recover => {}
        }

        // Анти-луп: счётчик попыток инкрементим ДО запуска — падение самого
        // recovery не должно давать бесконечные повторы.
        if let Err(e) = std::fs::write(&marker, (tries + 1).to_string()) {
            log::warn!("auto_recover[{call_id}]: marker write failed: {e}");
        }
        log::warn!(
            "auto_recover[{call_id}]: прерванный звонок (попытка {}/{AUTO_RECOVER_MAX_TRIES}) → recovery",
            tries + 1
        );
        if let Err(e) = spawn_recover_chunked(
            state.db.clone(),
            state.store.clone(),
            state.pipeline_tasks.clone(),
            state.app_data_dir.clone(),
            app.clone(),
            recovered_id,
        )
        .await
        {
            log::warn!("auto_recover[{call_id}]: не стартовал: {e}");
        } else {
            started += 1;
        }
    }
    if started > 0 {
        log::info!("auto_recover: восстановление запущено для {started} звонков");
    }
}

/// Есть ли на диске хоть какое-то аудио звонка: root WAV любой дорожки или
/// chunk-WAV. Без него восстанавливать нечего.
fn call_has_audio(store: &CallStore, call_id: &CallId) -> bool {
    use crate::pipeline::audio_merger::{list_chunk_wavs, TrackKind};
    let chunks_dir = store.chunks_dir(call_id);
    store.mic_path(call_id).exists()
        || store.system_path(call_id).exists()
        || !list_chunk_wavs(&chunks_dir, TrackKind::Mic).is_empty()
        || !list_chunk_wavs(&chunks_dir, TrackKind::System).is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts() -> CandidateFacts {
        CandidateFacts {
            is_headless_target: false,
            has_transcript: false,
            has_audio: true,
            tries: 0,
        }
    }

    #[test]
    fn plan_recovers_fresh_interrupted_call() {
        assert_eq!(plan_auto_recovery(0, &facts()), AutoRecoverPlan::Recover);
    }

    #[test]
    fn plan_stops_at_startup_cap() {
        // Кейс из B28.2: массовый бэклог не должен забить resource queue.
        assert_eq!(
            plan_auto_recovery(AUTO_RECOVER_MAX_PER_STARTUP, &facts()),
            AutoRecoverPlan::StopCapReached
        );
        assert_eq!(
            plan_auto_recovery(AUTO_RECOVER_MAX_PER_STARTUP - 1, &facts()),
            AutoRecoverPlan::Recover,
            "последний слот под лимитом ещё наш"
        );
    }

    #[test]
    fn plan_cap_wins_over_everything_else() {
        // Лимит проверяется первым: он означает break, а не continue —
        // перепутанный порядок молча превратил бы его в пропуск одного
        // кандидата и продолжил обход.
        let hopeless = CandidateFacts {
            is_headless_target: true,
            has_transcript: true,
            has_audio: false,
            tries: 99,
        };
        assert_eq!(
            plan_auto_recovery(AUTO_RECOVER_MAX_PER_STARTUP, &hopeless),
            AutoRecoverPlan::StopCapReached
        );
    }

    #[test]
    fn plan_skips_call_owned_by_headless_trigger() {
        // Двойной recovery одного звонка = две гонки за одни и те же WAV'ы.
        let f = CandidateFacts {
            is_headless_target: true,
            ..facts()
        };
        assert_eq!(
            plan_auto_recovery(0, &f),
            AutoRecoverPlan::SkipHeadlessOwned
        );
    }

    #[test]
    fn plan_skips_already_transcribed_call() {
        // transcript.md есть → failed относится к хвосту (recap), аудио цело
        // и трогать его нечем.
        let f = CandidateFacts {
            has_transcript: true,
            ..facts()
        };
        assert_eq!(
            plan_auto_recovery(0, &f),
            AutoRecoverPlan::SkipAlreadyTranscribed
        );
    }

    #[test]
    fn plan_skips_call_without_audio() {
        let f = CandidateFacts {
            has_audio: false,
            ..facts()
        };
        assert_eq!(plan_auto_recovery(0, &f), AutoRecoverPlan::SkipNoAudio);
    }

    #[test]
    fn plan_stops_retrying_after_limit() {
        // Анти-луп: повторяющийся краш пайплайна не должен крутить recovery
        // на каждом старте вечно.
        let f = CandidateFacts {
            tries: AUTO_RECOVER_MAX_TRIES,
            ..facts()
        };
        assert_eq!(
            plan_auto_recovery(0, &f),
            AutoRecoverPlan::SkipTriesExhausted
        );
        let last = CandidateFacts {
            tries: AUTO_RECOVER_MAX_TRIES - 1,
            ..facts()
        };
        assert_eq!(
            plan_auto_recovery(0, &last),
            AutoRecoverPlan::Recover,
            "последняя попытка ещё разрешена"
        );
    }

    #[test]
    fn headless_target_ignores_blank_value() {
        // Пустая/пробельная переменная не должна «занимать» несуществующий
        // звонок: сравнение с ней идёт в отборе кандидатов, и совпадение по
        // пустой строке заглушило бы авто-восстановление.
        assert_eq!(parse_headless_target(None), None);
        assert_eq!(parse_headless_target(Some("   ".into())), None);
        assert_eq!(
            parse_headless_target(Some("  call-42 ".into())).as_deref(),
            Some("call-42"),
            "trim обязан совпадать с тем, что использует сам триггер"
        );
    }
}
