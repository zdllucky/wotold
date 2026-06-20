//! Commands for pipeline re-runs (reprocess / cancel / regenerate recap).

use serde::Serialize;
use tauri::{AppHandle, State};

use crate::{
    db,
    services::pipeline_runner::{PipelineRunner, RegenKind},
    state::AppState,
    AppError,
};

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

/// M4.5: пересоздать рекап + action_items без повторной транскрипции.
///
/// [Global regen] Запускается как ФОНОВАЯ задача (spawn + register в
/// `pipeline_tasks`) — переживает уход со страницы, считается в бейдже у
/// «Звонки», эмитит `pipeline:started`/`pipeline:finished`. Команда возвращается
/// сразу; фронт подтягивает результат через `pipeline:finished` listener.
#[tauri::command]
pub async fn regenerate_recap(
    app: AppHandle,
    state: State<'_, AppState>,
    call_id: String,
) -> Result<(), AppError> {
    PipelineRunner::spawn_regen(
        state.db.clone(),
        state.store.clone(),
        state.device_id.clone(),
        app,
        state.pipeline_tasks.clone(),
        call_id,
        RegenKind::Recap,
    )
    .await
}

/// [M14 T-17] Lightweight title-only regen (engine-aware). Как `regenerate_recap`
/// — ФОНОВАЯ задача (survives навигацию, в бейдже). Возвращается сразу; новый
/// title подтянется через refetch на `pipeline:finished`.
#[tauri::command]
pub async fn regenerate_title(
    app: AppHandle,
    state: State<'_, AppState>,
    call_id: String,
) -> Result<(), AppError> {
    PipelineRunner::spawn_regen(
        state.db.clone(),
        state.store.clone(),
        state.device_id.clone(),
        app,
        state.pipeline_tasks.clone(),
        call_id,
        RegenKind::Title,
    )
    .await
}

/// [Global regen] Есть ли активная фон-задача (reprocess / regen) для звонка.
/// Frontend использует на mount CallDetailPage чтобы восстановить busy-состояние
/// после возврата на страницу.
#[tauri::command]
pub async fn is_call_processing(
    state: State<'_, AppState>,
    call_id: String,
) -> Result<bool, AppError> {
    Ok(state.pipeline_tasks.lock().await.contains_key(&call_id))
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

/// [Bulk recap] Прогресс одного шага массового регена.
#[derive(Debug, Clone, Serialize)]
pub struct BulkRecapProgress {
    /// Сколько звонков уже обработано (0-based текущий индекс).
    pub done: usize,
    pub total: usize,
    pub call_id: String,
}

/// [Bulk recap] Итог массового регена.
#[derive(Debug, Clone, Serialize)]
pub struct BulkRecapDone {
    pub regenerated: usize,
    pub failed: usize,
    pub cancelled: bool,
}

/// [Bulk recap] Пересоздать рекапы для всех ready-звонков с пустым/отсутствующим
/// recap.md. Чинит старый корпус (звонки обработанные до schema-fix имеют пустые
/// «# Рекап» рекапы). Возвращает кол-во звонков на обработку; сам реген идёт в
/// фоне последовательно (local LLM semaphore=1) с `recap:bulk_progress` events.
#[tauri::command]
pub async fn regenerate_empty_recaps(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<usize, AppError> {
    use crate::call_store::ArtifactKind;

    let calls = db::list_calls(&state.db).await?;
    let mut targets: Vec<String> = Vec::new();
    for c in &calls {
        if c.status != "ready" {
            continue;
        }
        // Реген требует transcript.md — без него regenerate_recap упадёт.
        let has_transcript = state
            .store
            .read_artifact(&c.id, ArtifactKind::Transcript)
            .await?
            .is_some();
        if !has_transcript {
            continue;
        }
        let recap = state
            .store
            .read_artifact(&c.id, ArtifactKind::Recap)
            .await?;
        let blank = match recap {
            None => true,
            Some(md) => crate::pipeline::recap::recap_md_is_blank(&md),
        };
        if blank {
            targets.push(c.id.clone());
        }
    }

    let total = targets.len();
    let bus = crate::events::EventBus::new(Some(&app));
    if total == 0 {
        bus.recap_bulk_done(&BulkRecapDone {
            regenerated: 0,
            failed: 0,
            cancelled: false,
        });
        return Ok(0);
    }

    state
        .bulk_recap_cancel
        .store(false, std::sync::atomic::Ordering::SeqCst);

    let pool = state.db.clone();
    let app_data_dir = state.app_data_dir.clone();
    let device_id = state.device_id.clone();
    let cancel = state.bulk_recap_cancel.clone();
    let app_for_task = app.clone();
    tauri::async_runtime::spawn(async move {
        let bus = crate::events::EventBus::new(Some(&app_for_task));
        let mut regenerated = 0usize;
        let mut failed = 0usize;
        let mut cancelled = false;
        for (i, id) in targets.iter().enumerate() {
            if cancel.load(std::sync::atomic::Ordering::SeqCst) {
                cancelled = true;
                break;
            }
            bus.recap_bulk_progress(&BulkRecapProgress {
                done: i,
                total,
                call_id: id.clone(),
            });
            match crate::pipeline::regenerate_recap(
                &pool,
                &app_data_dir,
                &device_id,
                id,
                Some(&app_for_task),
            )
            .await
            {
                Ok(()) => regenerated += 1,
                Err(e) => {
                    log::warn!("bulk recap regen {id}: {e}");
                    failed += 1;
                }
            }
        }
        bus.recap_bulk_done(&BulkRecapDone {
            regenerated,
            failed,
            cancelled,
        });
    });

    Ok(total)
}

/// [Bulk recap] Прервать активный массовый реген (флаг, проверяется между звонками).
#[tauri::command]
pub async fn cancel_bulk_recap(state: State<'_, AppState>) -> Result<(), AppError> {
    state
        .bulk_recap_cancel
        .store(true, std::sync::atomic::Ordering::SeqCst);
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
