//! Commands for pipeline re-runs (reprocess / cancel / regenerate recap).

use tauri::{AppHandle, State};

use crate::{services::pipeline_runner::PipelineRunner, state::AppState, AppError};

/// [V9] Количество РЕАЛЬНО работающих pipeline-задач в текущей сессии.
/// Раньше фронт считал через `list_calls().filter(status IN processing|recording)`
/// — но это давало false positives из zombie rows (старые crashed processing,
/// которые `sweep_stale_calls` ещё не пометил failed). Сейчас источник
/// правды — in-memory `pipeline_tasks` registry, который содержит только
/// активные tokio JoinHandle'ы.
#[tauri::command]
pub async fn get_active_pipeline_count(state: State<'_, AppState>) -> Result<usize, AppError> {
    let tasks = state.pipeline_tasks.lock().await;
    Ok(tasks.len())
}

/// M4.5 паспорта: пересоздать рекап + action_items без повторной транскрипции.
/// Ошибки LLM пробрасываются (UI показывает toast / error), в отличие от
/// pipeline::run где рекап silent-skip при ошибке (транскрипт важнее).
#[tauri::command]
pub async fn regenerate_recap(state: State<'_, AppState>, call_id: String) -> Result<(), AppError> {
    crate::pipeline::regenerate_recap(&state.db, &state.app_data_dir, &state.device_id, &call_id)
        .await
}

/// Перезапустить полный pipeline (STT + recap) для существующего звонка.
/// Применяется к failed | ready | processing звонкам — берёт mic.wav/system.wav
/// с диска и прогоняет заново.
///
/// [V8] Spawn'им как stop_recording — invoke возвращается сразу, фронт
/// идёт оптимистично рендерить reprocess banner и подтягивает state через
/// `pipeline:started` / `call:progress` / `pipeline:finished` события.
/// Handle регистрируется в `pipeline_tasks` чтобы `cancel_reprocess` мог
/// его abort'нуть.
#[tauri::command]
pub async fn reprocess_call(
    app: AppHandle,
    state: State<'_, AppState>,
    call_id: String,
) -> Result<(), AppError> {
    PipelineRunner::spawn_reprocess(
        state.db.clone(),
        state.store.clone(),
        state.device_id.clone(),
        app,
        state.pipeline_tasks.clone(),
        call_id,
    )
    .await
}

/// [V8] Отменить running reprocess. Идемпотент — если pipeline уже завершился
/// или не стартовал, возвращает Ok без действий.
///
/// Restoration logic:
///   - Если `transcript.md` существует на диске → старые артефакты пережили
///     старт нового run (persist_artifacts ещё не успел перезаписать) →
///     status='ready', clear pipeline_*.
///   - Иначе → status='failed' с reason «Отменено пользователем».
///
/// Эмитит `pipeline:cancelled` event чтобы фронт перечитал call + артефакты.
#[tauri::command]
pub async fn cancel_reprocess(
    app: AppHandle,
    state: State<'_, AppState>,
    call_id: String,
) -> Result<(), AppError> {
    PipelineRunner::cancel(
        &state.db,
        &state.store,
        &app,
        state.pipeline_tasks.clone(),
        &call_id,
    )
    .await?;
    Ok(())
}
