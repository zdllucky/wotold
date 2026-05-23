//! Commands for pipeline re-runs (reprocess / cancel / regenerate recap).

use serde::Serialize;
use tauri::{AppHandle, State};

use crate::{db, services::pipeline_runner::PipelineRunner, state::AppState, AppError};

/// [M13.3.1] Public view над `call_chunks` row. UI рендерит ChunkProgressStrip
/// из этого payload'а — transcript/embeddings_json не включены (UI они не
/// нужны + большая нагрузка по network).
#[derive(Debug, Clone, Serialize)]
pub struct ChunkInfoView {
    pub chunk_idx: u32,
    /// pending | processing | done | failed
    pub status: String,
    pub start_ms: i64,
    pub end_ms: Option<i64>,
}

/// [M13.3.1] Список chunks для звонка — sorted by `chunk_idx asc`. Возвращает
/// пустой Vec если call не chunked (или ещё не имеет rows). Frontend
/// ChunkProgressStrip рендерится только когда не-empty.
#[tauri::command]
pub async fn list_call_chunks(
    state: State<'_, AppState>,
    call_id: String,
) -> Result<Vec<ChunkInfoView>, AppError> {
    let rows = db::chunks::list_chunks_by_call(&state.db, &call_id).await?;
    Ok(rows
        .into_iter()
        .map(|r| ChunkInfoView {
            chunk_idx: r.chunk_idx,
            status: r.status,
            start_ms: r.start_ms,
            end_ms: r.end_ms,
        })
        .collect())
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_support::fresh_db;
    use sqlx::SqlitePool;
    use std::path::PathBuf;

    async fn insert_dummy_call(pool: &SqlitePool, id: &str) {
        sqlx::query(
            "INSERT INTO calls (id, started_at, status, path_label, created_at, updated_at)
             VALUES (?1, CURRENT_TIMESTAMP, 'recording', 'managed', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)",
        )
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    }

    /// Helper: повторить логику list_call_chunks без Tauri State (для теста
    /// чистой data path функции). Если test refactor сломает signature —
    /// заметим compile-time.
    async fn run_list(pool: &SqlitePool, call_id: &str) -> Vec<ChunkInfoView> {
        let rows = db::chunks::list_chunks_by_call(pool, call_id)
            .await
            .unwrap();
        rows.into_iter()
            .map(|r| ChunkInfoView {
                chunk_idx: r.chunk_idx,
                status: r.status,
                start_ms: r.start_ms,
                end_ms: r.end_ms,
            })
            .collect()
    }

    #[tokio::test]
    async fn list_call_chunks_returns_empty_when_no_chunks() {
        let db_t = fresh_db().await;
        insert_dummy_call(&db_t.pool, "c1").await;
        let out = run_list(&db_t.pool, "c1").await;
        assert!(out.is_empty());
    }

    #[tokio::test]
    async fn list_call_chunks_returns_mixed_status_snapshot() {
        let db_t = fresh_db().await;
        insert_dummy_call(&db_t.pool, "c1").await;

        // chunk 0 done.
        db::chunks::insert_chunk(
            &db_t.pool,
            "c1",
            0,
            0,
            &PathBuf::from("/m0"),
            &PathBuf::from("/s0"),
        )
        .await
        .unwrap();
        db::chunks::mark_chunk_processing(&db_t.pool, "c1", 0)
            .await
            .unwrap();
        db::chunks::mark_chunk_done(&db_t.pool, "c1", 0, 600_000, "{}", None, None)
            .await
            .unwrap();

        // chunk 1 processing.
        db::chunks::insert_chunk(
            &db_t.pool,
            "c1",
            1,
            600_000,
            &PathBuf::from("/m1"),
            &PathBuf::from("/s1"),
        )
        .await
        .unwrap();
        db::chunks::mark_chunk_processing(&db_t.pool, "c1", 1)
            .await
            .unwrap();

        // chunk 2 pending.
        db::chunks::insert_chunk(
            &db_t.pool,
            "c1",
            2,
            1_200_000,
            &PathBuf::from("/m2"),
            &PathBuf::from("/s2"),
        )
        .await
        .unwrap();

        let out = run_list(&db_t.pool, "c1").await;
        assert_eq!(out.len(), 3);
        // Sorted by chunk_idx asc.
        assert_eq!(out[0].chunk_idx, 0);
        assert_eq!(out[0].status, "done");
        assert_eq!(out[0].end_ms, Some(600_000));
        assert_eq!(out[1].chunk_idx, 1);
        assert_eq!(out[1].status, "processing");
        assert!(out[1].end_ms.is_none());
        assert_eq!(out[2].chunk_idx, 2);
        assert_eq!(out[2].status, "pending");
    }
}
