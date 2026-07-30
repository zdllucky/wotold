//! [TD-41] Ручной retry упавшего чанка и авто-возобновление пайплайна.
//!
//! Выделено из `commands/recording.rs` (1426 строк при лимите 800, правило 8).
//! Соседи по домену — `commands::recovery` (восстановление всей записи) и
//! `commands::chunked_setup` (провайдеры чанка). Логика не менялась.

use std::sync::Arc;

use sqlx::SqlitePool;
use tauri::{AppHandle, State};

use crate::{
    call_id::CallId, db, pipeline::chunk_runner::ChunkRunInput,
    services::pipeline_runner::PipelineRunner, state::AppState, AppError,
};

use super::chunked_setup::{build_chunk_providers, ChunkProviders};

/// [Tech-debt P0.2] Retry одного failed chunk'а через background spawn.
///
/// Wire-up:
/// - DB `mark_chunk_pending` (FSM gate failed → pending) — sync, fail если
///   chunk не в failed.
/// - Resolve providers тем же путём что `prepare_chunked_setup` (preset →
///   LocalWhisperProvider mic + system).
/// - `tokio::spawn` background task: `chunk_runner::run_chunk` пишет
///   результат в DB + emit'ит `transcript:chunk_done` — UI обновится сам.
/// - Return Ok(()) сразу — не блокируем Tauri command поток.
#[tauri::command]
pub async fn retry_chunk(
    app: AppHandle,
    state: State<'_, AppState>,
    call_id: String,
    chunk_idx: u32,
) -> Result<(), AppError> {
    // [TD-05] call_id из webview — валидируем до любых путей.
    let parsed_id = CallId::parse(&call_id)?;
    use crate::pipeline::chunk_runner;

    // 1. Validate chunk существует + status == failed (mark_chunk_pending
    //    enforce'ит FSM, но row lookup всё равно нужен для start/end_ms).
    let rows = db::chunks::list_chunks_by_call(&state.db, &call_id).await?;
    let row = rows
        .iter()
        .find(|r| r.chunk_idx == chunk_idx)
        .ok_or_else(|| AppError::NotFound(format!("chunk {call_id}/{chunk_idx} not found")))?;
    if row.status != "failed" {
        return Err(AppError::Other(format!(
            "retry_chunk: chunk {call_id}/{chunk_idx} status={} (need 'failed')",
            row.status
        )));
    }
    let start_ms = row.start_ms.max(0) as u64;
    let end_ms = row.end_ms.unwrap_or(start_ms as i64).max(0) as u64;

    // 2. Build providers — shared helper (mirror prepare_chunked_setup).
    let ChunkProviders {
        mic: mic_provider,
        system: system_provider,
        lang: stt_lang,
        mic_diarization_num_speakers,
    } = build_chunk_providers(&state.db, &state.app_data_dir, &app, &parsed_id).await?;

    // 4. FSM gate failed → pending. После этого chunk_runner внутри сделает
    //    pending → processing → done|failed.
    db::chunks::mark_chunk_pending(&state.db, &call_id, chunk_idx).await?;

    // 5. Background spawn — не блокируем UI. Errors handled внутри
    //    chunk_runner (mark_failed + emit chunk_done event).
    let mic_path = state.store.chunk_mic_path(&parsed_id, chunk_idx);
    let system_path = state.store.chunk_system_path(&parsed_id, chunk_idx);
    let pool = state.db.clone();
    let app_data_dir = state.app_data_dir.clone();
    let app_for_task = app.clone();
    let call_id_clone = call_id.clone();
    // [P11.1] Дополнительные клоны для post-success auto-resume hook.
    let store_for_resume = state.store.clone();
    let tasks_for_resume = state.pipeline_tasks.clone();
    let app_for_resume = app.clone();
    log::info!(
        "retry_chunk: spawning run_chunk for {call_id_clone}/{chunk_idx} \
         (start_ms={start_ms}, end_ms={end_ms})"
    );
    tokio::spawn(async move {
        let input = ChunkRunInput {
            call_id: call_id_clone.clone(),
            chunk_idx,
            start_ms,
            end_ms,
            mic_path,
            system_path,
            // No prev_prompt — chunk N-1 tail может быть устаревший после
            // первого fail'а. Точность первой фразы пострадает на ~10pp,
            // acceptable trade-off для retry-сценария.
            prev_prompt: None,
            lang: stt_lang,
            app_data_dir: Some(app_data_dir),
            app_handle: Some(app_for_task),
            mic_diarization_num_speakers,
        };
        match chunk_runner::run_chunk(
            &pool,
            mic_provider.as_ref(),
            system_provider.as_ref(),
            input,
        )
        .await
        {
            Ok(out) => {
                log::info!(
                    "retry_chunk[{call_id_clone}/{chunk_idx}]: success, {} segments",
                    out.segment_count
                );
                // [P11.1] Auto-resume pipeline: если все chunks теперь done и
                // звонок не активен (не recording) — spawn reprocess через тот
                // же PipelineRunner что использует stop_recording. Idempotent
                // через `spawn_reprocess` abort+respawn для same call_id.
                if let Err(e) = maybe_resume_pipeline_after_chunk(
                    &pool,
                    &CallId::from_db(&call_id_clone),
                    store_for_resume,
                    app_for_resume,
                    tasks_for_resume,
                )
                .await
                {
                    log::warn!("retry_chunk[{call_id_clone}/{chunk_idx}]: auto-resume failed: {e}");
                }
            }
            Err(e) => log::warn!("retry_chunk[{call_id_clone}/{chunk_idx}]: failed: {e}"),
        }
    });

    Ok(())
}

// [TD-33] Восстановление сломанных/прерванных записей (recover_chunked_call,
// spawn_recover_chunked, headless- и авто-триггеры) переехало в
// `commands/recovery.rs`: там оно тестируемо, здесь файл и так сверх лимита 800.

/// [P11.1] После того как chunk_runner перевёл chunk failed→done в
/// `retry_chunk`, проверить: можно ли уже автоматически возобновить
/// downstream pipeline (diarize → audio_merger → recap).
///
/// **Гарантии auto-resume:**
/// - Все chunks для звонка `status='done'` (нет ни pending, ни failed).
/// - Звонок не recording (active recording owns orchestrator, не наш case).
/// - Звонок не уже `status='ready'` (recap уже persisted).
///
/// Если условия совпали — `PipelineRunner::spawn_reprocess` сам идемпотентен:
/// abort'ает existing task для этого `call_id` (если есть) и spawn'ит новый
/// `pipeline::run` через chunked path (load_chunked_transcripts skip STT).
async fn maybe_resume_pipeline_after_chunk(
    pool: &SqlitePool,
    call_id: &CallId,
    store: Arc<crate::call_store::CallStore>,
    app: AppHandle,
    tasks: crate::services::pipeline_runner::PipelineTasks,
) -> Result<(), AppError> {
    if !db::chunks::all_chunks_done(pool, call_id.as_str()).await? {
        log::debug!("maybe_resume_pipeline_after_chunk[{call_id}]: not all chunks done, skip");
        return Ok(());
    }
    let Some(call) = db::get_call(pool, call_id.as_str()).await? else {
        return Err(AppError::NotFound(format!("call {call_id}")));
    };
    if call.status == "recording" {
        log::debug!("maybe_resume_pipeline_after_chunk[{call_id}]: still recording, skip");
        return Ok(());
    }
    if call.status == "ready" && call.recap_failed_reason.is_none() {
        log::debug!(
            "maybe_resume_pipeline_after_chunk[{call_id}]: already ready w/o failure, skip"
        );
        return Ok(());
    }
    log::info!("maybe_resume_pipeline_after_chunk[{call_id}]: all chunks done, spawning reprocess");
    PipelineRunner::spawn_reprocess(pool.clone(), store, app, tasks, call_id.to_string()).await
}
